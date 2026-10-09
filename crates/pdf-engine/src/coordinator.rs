use crate::{DocumentInfo, EngineError, Event, Shared, raster};
use crossbeam_channel::{Receiver, Sender, bounded};
use mupdf::{DisplayList, Document};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};
use viewer_core::{PageSize, PixelTile, RenderQueue, RenderRequest};

struct Job {
    request: RenderRequest,
    list: Arc<DisplayList>,
}
struct Completed {
    request: RenderRequest,
    pixels: Option<Result<PixelTile, EngineError>>,
    elapsed: Duration,
}

fn emit(shared: &Shared, events: &Sender<Event>, mut event: Event) -> bool {
    loop {
        match events.send_timeout(event, Duration::from_millis(10)) {
            Ok(()) => return true,
            Err(crossbeam_channel::SendTimeoutError::Timeout(value)) => {
                if shared.stopping.load(Ordering::Acquire) {
                    return false;
                }
                event = value;
            }
            Err(_) => return false,
        }
    }
}

pub(super) fn run(shared: Arc<Shared>, wake: Receiver<()>, events: Sender<Event>, workers: usize) {
    let (jobs, job_rx) = bounded::<Job>(workers);
    let (completed, results) = bounded::<Completed>(workers);
    let mut threads = Vec::new();
    for index in 0..workers {
        let receive = job_rx.clone();
        let send = completed.clone();
        let shared = shared.clone();
        threads.push(
            thread::Builder::new()
                .name(format!("pdf-raster-{index}"))
                .spawn(move || {
                    // mupdf 0.8.0 lazily clones one shared, locked base context per thread.
                    while let Ok(job) = receive.recv() {
                        if shared.stopping.load(Ordering::Acquire) {
                            break;
                        }
                        let started = Instant::now();
                        let pixels = shared
                            .current(job.request.key.document, job.request.generation)
                            .then(|| raster::render(&job.list, job.request.key));
                        let result = Completed {
                            request: job.request,
                            pixels,
                            elapsed: started.elapsed(),
                        };
                        if send.send(result).is_err() {
                            break;
                        }
                    }
                })
                .expect("create raster worker"),
        );
    }
    drop(completed);
    drop(job_rx);
    let mut document: Option<Document> = None;
    let mut document_id = 0;
    let mut geometry: Vec<Option<PageSize>> = Vec::new();
    let mut cursor = 0;
    let mut lists: HashMap<usize, Arc<DisplayList>> = HashMap::new();
    let mut inflight = HashSet::new();
    let mut queue = RenderQueue::new(128);

    while !shared.stopping.load(Ordering::Acquire) {
        let desired = {
            let mut slot = shared.desired.lock().unwrap();
            std::mem::take(&mut *slot)
        };
        if let Some(open) = desired.open {
            lists.clear();
            document = None;
            geometry.clear();
            queue.replace(Vec::new());
            document_id = open.document;
            let started = Instant::now();
            let load = || -> Result<(Document, DocumentInfo), EngineError> {
                let path = open.path.to_str().ok_or_else(|| {
                    EngineError::InvalidDocument("Path is not valid Unicode".into())
                })?;
                let doc = Document::open(path)
                    .map_err(|e| EngineError::InvalidDocument(e.to_string()))?;
                if doc
                    .needs_password()
                    .map_err(|e| EngineError::InvalidDocument(e.to_string()))?
                {
                    return Err(EngineError::PasswordRequired);
                }
                let count = doc
                    .page_count()
                    .map_err(|e| EngineError::InvalidDocument(e.to_string()))?;
                if count <= 0 || count > 1_000_000 {
                    return Err(EngineError::InvalidDocument(
                        "Page count must be between 1 and 1,000,000".into(),
                    ));
                }
                let bounds = doc
                    .load_page(0)
                    .and_then(|p| p.bounds())
                    .map_err(|e| EngineError::InvalidDocument(e.to_string()))?;
                let info = DocumentInfo {
                    pages: count as usize,
                    first_page: PageSize::new(bounds.width(), bounds.height()),
                };
                Ok((doc, info))
            };
            match load() {
                Ok((doc, info)) if shared.current(open.document, open.generation) => {
                    geometry = vec![None; info.pages];
                    geometry[0] = Some(info.first_page);
                    cursor = 1;
                    document = Some(doc);
                    emit(
                        &shared,
                        &events,
                        Event::Opened {
                            document: document_id,
                            info,
                            elapsed: started.elapsed(),
                        },
                    );
                }
                Ok(_) => {}
                Err(error) => {
                    emit(
                        &shared,
                        &events,
                        Event::Error {
                            document: document_id,
                            error,
                        },
                    );
                }
            }
        }
        if let Some(view) = desired.view
            && view.document == document_id
            && shared.current(view.document, view.generation)
        {
            lists.retain(|page, _| view.resident.contains(page));
            queue.replace(
                view.requests
                    .into_iter()
                    .filter(|r| !inflight.contains(&(r.key, r.generation)))
                    .collect(),
            );
        }
        while let Ok(result) = results.try_recv() {
            inflight.remove(&(result.request.key, result.request.generation));
            if !shared.current(result.request.key.document, result.request.generation) {
                continue;
            }
            match result.pixels {
                Some(Ok(pixels)) => {
                    emit(
                        &shared,
                        &events,
                        Event::Tile {
                            key: result.request.key,
                            generation: result.request.generation,
                            pixels: Arc::new(pixels),
                            elapsed: result.elapsed,
                        },
                    );
                }
                Some(Err(error)) => {
                    emit(
                        &shared,
                        &events,
                        Event::Error {
                            document: document_id,
                            error,
                        },
                    );
                }
                None => {}
            }
        }
        if let Some(doc) = &document {
            while inflight.len() < workers {
                let Some(request) = queue.pop() else {
                    break;
                };
                if !shared.current(request.key.document, request.generation) {
                    continue;
                }
                let page = request.key.page;
                if page >= geometry.len() {
                    continue;
                }
                if let std::collections::hash_map::Entry::Vacant(entry) = lists.entry(page) {
                    let started = Instant::now();
                    match doc.load_page(page as i32).and_then(|p| {
                        let bounds = p.bounds()?;
                        let list = p.to_display_list(true)?;
                        Ok((PageSize::new(bounds.width(), bounds.height()), list))
                    }) {
                        Ok((size, list)) => {
                            if geometry[page] != Some(size) {
                                geometry[page] = Some(size);
                                emit(
                                    &shared,
                                    &events,
                                    Event::Geometry {
                                        document: document_id,
                                        pages: vec![(page, size)],
                                    },
                                );
                            }
                            entry.insert(Arc::new(list));
                            emit(
                                &shared,
                                &events,
                                Event::DisplayListReady {
                                    document: document_id,
                                    page,
                                    elapsed: started.elapsed(),
                                },
                            );
                        }
                        Err(error) => {
                            emit(
                                &shared,
                                &events,
                                Event::Error {
                                    document: document_id,
                                    error: EngineError::Render(error.to_string()),
                                },
                            );
                            continue;
                        }
                    }
                }
                if let Some(list) = lists.get(&page) {
                    inflight.insert((request.key, request.generation));
                    if jobs
                        .send(Job {
                            request,
                            list: list.clone(),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
            // One page per iteration bounds background work between user commands.
            while cursor < geometry.len() && geometry[cursor].is_some() {
                cursor += 1;
            }
            if cursor < geometry.len() && queue.is_empty() {
                if let Ok(bounds) = doc.load_page(cursor as i32).and_then(|p| p.bounds()) {
                    let size = PageSize::new(bounds.width(), bounds.height());
                    geometry[cursor] = Some(size);
                    emit(
                        &shared,
                        &events,
                        Event::Geometry {
                            document: document_id,
                            pages: vec![(cursor, size)],
                        },
                    );
                }
                cursor += 1;
            }
        }
        let _ = wake.recv_timeout(Duration::from_millis(2));
    }
    drop(jobs);
    // Dropping the receiver releases workers even if shutdown finds completed tiles queued.
    drop(results);
    for worker in threads {
        let _ = worker.join();
    }
    drop(lists);
    drop(document);
}
