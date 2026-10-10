use crate::{DocumentInfo, EngineError, Event, FileAction, FileCommandAction, Shared, raster};
use crossbeam_channel::{Receiver, Sender, bounded};
use mupdf::{
    DisplayList, Document,
    pdf::{PdfDocument, Permission},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};
use viewer_core::{PageSize, PixelTile, RenderQueue, RenderRequest};

struct Job {
    content: u64,
    request: RenderRequest,
    list: Arc<DisplayList>,
    thumbnail: bool,
}
struct Completed {
    content: u64,
    thumbnail: bool,
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
                        let valid = if job.thumbnail {
                            shared.current_document(job.request.key.document)
                        } else {
                            shared.current(job.request.key.document, job.request.generation)
                        };
                        let valid =
                            valid && shared.current_content(job.request.key.document, job.content);
                        let pixels = valid.then(|| raster::render(&job.list, job.request.key));
                        let result = Completed {
                            content: job.content,
                            thumbnail: job.thumbnail,
                            request: job.request,
                            pixels,
                            elapsed: started.elapsed(),
                        };
                        // A completion also acknowledges release of file-backed image resources.
                        drop(job.list);
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
    let mut document: Option<PdfDocument> = None;
    let mut can_copy = false;
    let mut journal: Option<viewer_core::SnapshotHistory> = None;
    let mut content: u64 = 0;
    let mut annotation_queue = std::collections::VecDeque::new();
    let mut annotation_cache = HashMap::new();
    let mut source_path = std::path::PathBuf::new();
    let mut session_password: Option<String> = None;
    let mut notes_queue = std::collections::VecDeque::new();
    let mut notes_cache = HashMap::new();
    let mut reader = crate::reader::Reader::default();
    let mut navigation = crate::navigation::Navigation::default();
    let mut document_id = 0;
    let mut geometry: Vec<Option<PageSize>> = Vec::new();
    let mut cursor = 0;
    let mut lists: HashMap<usize, Arc<DisplayList>> = HashMap::new();
    let mut inflight = HashSet::new();
    let mut deferred_results = std::collections::VecDeque::new();
    let mut queue = RenderQueue::new(128);

    while !shared.stopping.load(Ordering::Acquire) {
        let desired = {
            let mut slot = shared.desired.lock().unwrap();
            std::mem::take(&mut *slot)
        };
        if let Some(open) = desired.open {
            journal = None;
            content = 0;
            shared.content.store(0, Ordering::Release);
            annotation_queue.clear();
            annotation_cache.clear();
            reader = crate::reader::Reader::default();
            navigation = crate::navigation::Navigation::default();
            lists.clear();
            document = None;
            geometry.clear();
            queue.replace(Vec::new());
            document_id = open.document;
            source_path = open.path.clone();
            session_password = open.password.clone();
            notes_queue.clear();
            notes_cache.clear();
            let started = Instant::now();
            let load = || -> Result<(PdfDocument, DocumentInfo), EngineError> {
                let path = open.path.to_str().ok_or_else(|| {
                    EngineError::InvalidDocument("Path is not valid Unicode".into())
                })?;
                let mut doc = Document::open(path)
                    .and_then(PdfDocument::try_from)
                    .map_err(|e| EngineError::InvalidDocument(e.to_string()))?;
                if doc
                    .needs_password()
                    .map_err(|e| EngineError::InvalidDocument(e.to_string()))?
                {
                    let password = open
                        .password
                        .as_deref()
                        .ok_or(EngineError::PasswordRequired)?;
                    if !doc
                        .authenticate(password)
                        .map_err(|e| EngineError::InvalidDocument(e.to_string()))?
                    {
                        return Err(EngineError::PasswordRejected);
                    }
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
                    can_copy: doc.permissions().contains(Permission::COPY),
                    can_annotate: doc.permissions().contains(Permission::ANNOTATE),
                    can_assemble: doc.permissions().contains(Permission::ASSEMBLE),
                    pages: count as usize,
                    first_page: PageSize::new(bounds.width(), bounds.height()),
                };
                Ok((doc, info))
            };
            match load() {
                Ok((doc, info)) if shared.current(open.document, open.generation) => {
                    can_copy = info.can_copy;
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
        if let Some(command) = desired.file {
            let direct_save = matches!(&command.action, FileCommandAction::Save);
            if let FileCommandAction::Edit {
                revision,
                generation,
                command: edit,
            } = &command.action
            {
                let result = if command.document != document_id
                    || !shared.current_document(command.document)
                {
                    Err("The document changed before editing started".into())
                } else if let Some(doc) = document.as_ref() {
                    crate::editing::perform(
                        doc,
                        &mut journal,
                        session_password.as_deref(),
                        *revision,
                        edit,
                        || shared.current_document(command.document),
                    )
                } else {
                    Err("No PDF is open".into())
                };
                match result {
                    Ok((doc, focus, revision)) => {
                        let info = crate::editing::info(&doc).expect("validated edited PDF");
                        content = content.wrapping_add(1);
                        shared.content.store(content, Ordering::Release);
                        shared.generation.store(*generation, Ordering::Release);
                        lists.clear();
                        queue.replace(vec![]);
                        reader = crate::reader::Reader::default();
                        navigation = crate::navigation::Navigation::default();
                        notes_queue.clear();
                        notes_cache.clear();
                        annotation_queue.clear();
                        annotation_cache.clear();
                        geometry = vec![None; info.pages];
                        geometry[0] = Some(info.first_page);
                        cursor = 1;
                        can_copy = info.can_copy;
                        document = Some(doc);
                        shared.file_busy.store(false, Ordering::Release);
                        let h = journal.as_ref().unwrap();
                        emit(
                            &shared,
                            &events,
                            Event::Changed {
                                document: document_id,
                                operation: command.operation,
                                revision,
                                generation: *generation,
                                info,
                                dirty: h.dirty(),
                                can_undo: h.can_undo(),
                                can_redo: h.can_redo(),
                                focus_page: focus,
                            },
                        );
                    }
                    Err(message) => {
                        shared.file_busy.store(false, Ordering::Release);
                        emit(
                            &shared,
                            &events,
                            Event::FileError {
                                document: command.document,
                                operation: command.operation,
                                message,
                            },
                        );
                    }
                }
                continue;
            }
            let result = if command.document != document_id
                || !shared.current_document(command.document)
            {
                Err("The document changed before the file operation started".into())
            } else if let Some(doc) = document.as_ref() {
                let action = match command.action {
                    FileCommandAction::Save => FileCommandAction::RenameAndSave {
                        name: source_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .into(),
                        annotations: vec![],
                    },
                    FileCommandAction::Rename(name) if journal.is_some() => {
                        FileCommandAction::RenameAndSave {
                            name,
                            annotations: vec![],
                        }
                    }
                    action => action,
                };
                match action {
                    FileCommandAction::Save => unreachable!(),
                    FileCommandAction::Edit { .. } => unreachable!(),
                    FileCommandAction::Extract {
                        revision,
                        target,
                        pages,
                    } => {
                        if revision != journal.as_ref().map_or(0, |h| h.revision()) {
                            Err("Document revision changed".into())
                        } else {
                            crate::editing::extract(
                                doc,
                                &source_path,
                                session_password.as_deref(),
                                &target,
                                &pages,
                                || shared.current_document(command.document),
                            )
                            .map(|p| (p, FileAction::Extract))
                        }
                    }
                    FileCommandAction::Split {
                        revision,
                        directory,
                        after,
                    } => {
                        if revision != journal.as_ref().map_or(0, |h| h.revision()) {
                            Err("Document revision changed".into())
                        } else {
                            crate::editing::split(
                                doc,
                                &source_path,
                                session_password.as_deref(),
                                &directory,
                                after,
                                || shared.current_document(command.document),
                            )
                            .map(|p| (p, FileAction::Split))
                        }
                    }
                    FileCommandAction::SaveAs {
                        target,
                        annotations,
                    } => {
                        let switch = journal.is_some() || annotations.is_empty();
                        let baseline = if switch {
                            crate::editing::serialize(doc).and_then(|bytes| {
                                let initial = if journal.is_none() {
                                    Some(viewer_core::SnapshotHistory::new(bytes.clone())?)
                                } else {
                                    None
                                };
                                Ok((
                                    Some(crate::editing::restore(
                                        &bytes,
                                        session_password.as_deref(),
                                    )?),
                                    initial,
                                ))
                            })
                        } else {
                            Ok((None, None))
                        };
                        match baseline {
                            Err(message)=>Err(message),
                            Ok((baseline,initial))=>{
                            let saved=crate::files::save(
                                doc,
                                &source_path,
                                session_password.as_deref(),
                                &target,
                                if journal.is_some() { &[] } else { &annotations },
                                || shared.current_document(command.document),
                            );
                            match saved {
                                Err(message)=>Err(message),
                                Ok(path)=>{
                                if switch {
                                    source_path = path.clone();
                                    document = baseline;
                                    if let Some(h) = initial {
                                        journal = Some(h);
                                    }
                                    if let Some(h) = journal.as_mut() {
                                        h.mark_saved();
                                    }
                                }
                                Ok((path, FileAction::SaveAs))
                                }
                            }
                            }
                        }
                    }
                    FileCommandAction::RenameAndSave { ref annotations,.. } if journal.is_some()&&!annotations.is_empty()=>Err("Use native editing commands before saving; external drafts cannot bypass undo history".into()),
                    FileCommandAction::RenameAndSave { name, annotations } => {
                        match crate::files::prepare_rename(
                            doc,
                            &source_path,
                            session_password.as_deref(),
                            &name,
                            &annotations,
                            || shared.current_document(command.document),
                        ) {
                            Err(e) => Err(e),
                            Ok(prepared) => {
                                while deferred_results.len() < inflight.len()
                                    && !shared.stopping.load(Ordering::Acquire)
                                {
                                    match results.recv_timeout(Duration::from_millis(10)) {
                                        Ok(result) => deferred_results.push_back(result),
                                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                                        Err(_) => break,
                                    }
                                }
                                if !shared.current_document(command.document) {
                                    Err("Document changed before rename completed".into())
                                } else {
                                    lists.clear();
                                    drop(document.take());
                                    let (baseline, result) = prepared.commit(&source_path);
                                    document = Some(baseline);
                                    match result {
                                        Ok(path) => {
                                            source_path = path.clone();
                                            if let Some(h) = journal.as_mut() {
                                                h.mark_saved();
                                            }
                                            Ok((
                                                path,
                                                if direct_save {
                                                    FileAction::Save
                                                } else {
                                                    FileAction::Rename
                                                },
                                            ))
                                        }
                                        Err(failure) => {
                                            source_path = failure.path;
                                            Err(failure.message)
                                        }
                                    }
                                }
                            }
                        }
                    }
                    FileCommandAction::Rename(name) => {
                        match crate::files::renamed_path(&source_path, &name) {
                            Err(e) => Err(e),
                            Ok(target) if target == source_path => {
                                Ok((source_path.clone(), FileAction::Rename))
                            }
                            Ok(target) => {
                                // Display lists on workers can retain image streams from the
                                // source file. Finish bounded raster work before releasing it.
                                while deferred_results.len() < inflight.len()
                                    && !shared.stopping.load(Ordering::Acquire)
                                {
                                    match results.recv_timeout(Duration::from_millis(10)) {
                                        Ok(result) => deferred_results.push_back(result),
                                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                                        Err(_) => break,
                                    }
                                }
                                if !shared.current_document(command.document) {
                                    Err("Document changed before rename completed".into())
                                } else {
                                    // Release all page/document handles before Windows moves the open file.
                                    lists.clear();
                                    notes_cache.clear();
                                    document = None;
                                    let moved =
                                        crate::files::move_no_replace(&source_path, &target);
                                    let reopen_path = if moved.is_ok() {
                                        target.clone()
                                    } else {
                                        source_path.clone()
                                    };
                                    let reopened = crate::files::open(
                                        &reopen_path,
                                        session_password.as_deref(),
                                    );
                                    match reopened {
                                        Ok(doc) => {
                                            document = Some(doc);
                                            match moved {
                                                Ok(()) => {
                                                    source_path = target.clone();
                                                    Ok((target, FileAction::Rename))
                                                }
                                                Err(e) => Err(format!(
                                                    "Could not rename PDF: {e}. Original remains open."
                                                )),
                                            }
                                        }
                                        Err(e) => {
                                            if moved.is_err() {
                                                Err(format!(
                                                    "Rename failed and the original PDF could not be reopened: {e}. File remains at {}. Use Open to recover.",
                                                    source_path.display()
                                                ))
                                            } else {
                                                match crate::files::move_no_replace(
                                                    &target,
                                                    &source_path,
                                                ) {
                                                    Ok(()) => match crate::files::open(
                                                        &source_path,
                                                        session_password.as_deref(),
                                                    ) {
                                                        Ok(doc) => {
                                                            document = Some(doc);
                                                            Err(format!(
                                                                "Renamed PDF could not be reopened ({e}); rename was rolled back. Original remains open."
                                                            ))
                                                        }
                                                        Err(rollback_error) => Err(format!(
                                                            "Rename was rolled back but reopening failed: {rollback_error}. File remains at {}. Use Open to recover.",
                                                            source_path.display()
                                                        )),
                                                    },
                                                    Err(rollback_error) => {
                                                        source_path = target;
                                                        Err(format!(
                                                            "Rename succeeded but reopening failed ({e}) and rollback failed ({rollback_error}). File is at {}. Use Open to recover.",
                                                            source_path.display()
                                                        ))
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                Err("No PDF is open".into())
            };
            shared.file_busy.store(false, Ordering::Release);
            let event = match result {
                Ok((path, kind)) => Event::FileDone {
                    document: command.document,
                    operation: command.operation,
                    path,
                    kind,
                },
                Err(message) => Event::FileError {
                    document: command.document,
                    operation: command.operation,
                    message,
                },
            };
            emit(&shared, &events, event);
            if document.is_none() && command.document == document_id {
                emit(
                    &shared,
                    &events,
                    Event::Error {
                        document: document_id,
                        error: EngineError::InvalidDocument(format!(
                            "The PDF at {} could not be reopened. Use Open to recover.",
                            source_path.display()
                        )),
                    },
                );
            }
        }
        if let Some((id, revision, pages)) = desired.annotations
            && id == document_id
            && revision == journal.as_ref().map_or(0, |h| h.revision())
        {
            let pages: Vec<_> = pages
                .into_iter()
                .filter(|p| *p < geometry.len())
                .take(16)
                .collect();
            annotation_cache.retain(|p, _| pages.contains(p));
            annotation_queue = pages.into();
        }
        if let Some((id, pages)) = desired.notes
            && id == document_id
        {
            let pages: Vec<_> = pages
                .into_iter()
                .filter(|p| *p < geometry.len())
                .take(16)
                .collect();
            notes_cache.retain(|p, _| pages.contains(p));
            notes_queue = pages.into();
        }
        let revision = journal.as_ref().map_or(0, |h| h.revision());
        if let Some((id, rev)) = desired.outlines
            && id == document_id
            && rev == revision
        {
            navigation.request_outlines();
        }
        if let Some((id, rev, pages)) = desired.links
            && id == document_id
            && rev == revision
        {
            navigation.request_links(pages, geometry.len());
        }
        if let Some((id, pages)) = desired.text
            && id == document_id
        {
            reader.text_pages(pages, geometry.len());
        }
        if let Some((id, pages)) = desired.thumbnails
            && id == document_id
        {
            reader.thumbnails(pages, geometry.len());
        }
        if let Some((id, search, query)) = desired.search
            && id == document_id
        {
            reader.search(search, query);
        }
        if let Some(view) = desired.view
            && view.document == document_id
            && shared.current(view.document, view.generation)
        {
            lists.retain(|page, _| view.resident.contains(page));
            queue.replace(
                view.requests
                    .into_iter()
                    .filter(|r| !inflight.contains(&(r.key, r.generation, false, content)))
                    .collect(),
            );
        }
        while let Some(result) = deferred_results
            .pop_front()
            .or_else(|| results.try_recv().ok())
        {
            inflight.remove(&(
                result.request.key,
                result.request.generation,
                result.thumbnail,
                result.content,
            ));
            if !shared.current_content(result.request.key.document, result.content) {
                continue;
            }
            if result.thumbnail {
                if shared.current_document(result.request.key.document)
                    && reader.thumbnail_wanted(result.request.key.page)
                {
                    match result.pixels {
                        Some(Ok(pixels)) => {
                            let Some(pixels) =
                                reader.thumbnail_finished(result.request.key, pixels)
                            else {
                                continue;
                            };
                            emit(
                                &shared,
                                &events,
                                Event::Thumbnail {
                                    key: result.request.key,
                                    pixels,
                                    elapsed: result.elapsed,
                                },
                            );
                        }
                        Some(Err(error)) => {
                            emit(
                                &shared,
                                &events,
                                Event::ReaderError {
                                    document: document_id,
                                    operation: "thumbnail",
                                    page: Some(result.request.key.page),
                                    message: error.to_string(),
                                },
                            );
                        }
                        None => {}
                    }
                }
                continue;
            }
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
            if let Some(page) = annotation_queue.pop_front() {
                let result = annotation_cache
                    .get(&page)
                    .cloned()
                    .map(Ok)
                    .unwrap_or_else(|| crate::editing::annotations(doc, page));
                match result {
                    Ok((annotations, truncated)) => {
                        annotation_cache.insert(page, (annotations.clone(), truncated));
                        emit(
                            &shared,
                            &events,
                            Event::Annotations {
                                document: document_id,
                                revision: journal.as_ref().map_or(0, |h| h.revision()),
                                page,
                                annotations,
                                truncated,
                            },
                        );
                    }
                    Err(message) => {
                        emit(
                            &shared,
                            &events,
                            Event::ReaderError {
                                document: document_id,
                                operation: "annotations",
                                page: Some(page),
                                message,
                            },
                        );
                    }
                }
            }
            if let Some(page) = notes_queue.pop_front() {
                let result = notes_cache
                    .get(&page)
                    .cloned()
                    .map(Ok)
                    .unwrap_or_else(|| crate::files::notes(doc, page));
                match result {
                    Ok((notes, truncated)) => {
                        notes_cache.insert(page, (notes.clone(), truncated));
                        emit(
                            &shared,
                            &events,
                            Event::Notes {
                                document: document_id,
                                page,
                                notes,
                                truncated,
                            },
                        );
                    }
                    Err(message) => {
                        emit(
                            &shared,
                            &events,
                            Event::ReaderError {
                                document: document_id,
                                operation: "notes",
                                page: Some(page),
                                message,
                            },
                        );
                    }
                }
            }
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
                    inflight.insert((request.key, request.generation, false, content));
                    if jobs
                        .send(Job {
                            content,
                            thumbnail: false,
                            request,
                            list: list.clone(),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
            if queue.is_empty()
                && inflight.len() < workers
                && let Some(page) = reader.next_thumbnail()
            {
                if let Some((key, pixels)) = reader.cached_thumbnail(page) {
                    emit(
                        &shared,
                        &events,
                        Event::Thumbnail {
                            key,
                            pixels,
                            elapsed: Duration::ZERO,
                        },
                    );
                    continue;
                }
                let result = doc.load_page(page as i32).and_then(|p| {
                    let bounds = p.bounds()?;
                    let scale = (140.0 / bounds.width())
                        .min(160.0 / bounds.height())
                        .min(1.0);
                    let scale_milli = (scale * 1000.0).floor().max(1.0) as u32;
                    let scale = scale_milli as f32 / 1000.0;
                    let key = viewer_core::TileKey {
                        document: document_id,
                        page,
                        scale_milli,
                        rotation: 0,
                        x: 0,
                        y: 0,
                        width: (bounds.width() * scale).ceil().clamp(1.0, 140.0) as u32,
                        height: (bounds.height() * scale).ceil().clamp(1.0, 160.0) as u32,
                    };
                    Ok((key, p.to_display_list(true)?))
                });
                match result {
                    Ok((key, list)) => {
                        let request = RenderRequest {
                            key,
                            generation: 0,
                            priority: u32::MAX,
                        };
                        inflight.insert((key, 0, true, content));
                        let _ = jobs.send(Job {
                            content,
                            request,
                            list: Arc::new(list),
                            thumbnail: true,
                        });
                    }
                    Err(error) => {
                        emit(
                            &shared,
                            &events,
                            Event::ReaderError {
                                document: document_id,
                                operation: "thumbnail",
                                page: Some(page),
                                message: error.to_string(),
                            },
                        );
                    }
                }
            }
            if let Some(event) = navigation.tick(
                doc,
                document_id,
                journal.as_ref().map_or(0, |h| h.revision()),
                &shared,
            ) {
                emit(&shared, &events, event);
            }
            if let Some(event) = reader.tick(doc, document_id, geometry.len(), can_copy, &shared) {
                emit(&shared, &events, event);
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
