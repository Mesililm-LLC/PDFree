//! Rendering subsystem: render queue, tile dispatch, and geometry discovery.
//! pdfium-render does not cache display lists; pages are rendered on demand
//! with PdfRenderConfig specifying the tile region and scale.

use crate::{Event, raster, Shared, View};
use crossbeam_channel::Sender;
use pdfium_render::prelude::*;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
use viewer_core::{PageSize, PixelTile, RenderQueue, RenderRequest, TileKey};

/// A render job handed to the rendering subsystem.
pub(crate) struct Job {
    pub content: u64,
    pub request: RenderRequest,
    pub thumbnail: bool,
}

/// A completed render result.
pub(crate) struct Completed {
    pub content: u64,
    pub thumbnail: bool,
    pub request: RenderRequest,
    pub pixels: Option<Result<PixelTile, crate::EngineError>>,
    pub elapsed: Duration,
}

/// Emits an event through the bounded channel, respecting shutdown.
fn emit(shared: &Shared, events: &Sender<Event>, mut event: Event) -> bool {
    loop {
        match events.send_timeout(event, Duration::from_millis(crate::constants::CHANNEL_SEND_TIMEOUT_MILLIS)) {
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

/// The rendering subsystem owns the render queue and tile dispatch.
/// Pages are held by the coordinator and passed in on each tick.
pub(crate) struct Rendering {
    shared: Arc<Shared>,
    events: Sender<Event>,
    document_id: u64,
    queue: RenderQueue,
    geometry: Vec<Option<PageSize>>,
    cursor: usize,
    inflight: HashSet<(TileKey, u64, bool, u64)>,
    deferred_results: VecDeque<Completed>,
    content: u64,
}

impl Rendering {
    pub(crate) fn new(
        shared: Arc<Shared>,
        events: Sender<Event>,
        document_id: u64,
    ) -> Self {
        Self {
            shared,
            events,
            document_id,
            queue: RenderQueue::new(crate::constants::RENDER_QUEUE_CAPACITY),
            geometry: Vec::new(),
            cursor: 0,
            inflight: HashSet::new(),
            deferred_results: VecDeque::new(),
            content: 0,
        }
    }

    /// Reset all rendering state for a newly opened document.
    pub(crate) fn reset(&mut self, pages: usize, first_page: PageSize) {
        self.geometry = vec![None; pages];
        self.geometry[0] = Some(first_page);
        self.cursor = 1;
        self.queue.replace(Vec::new());
        self.inflight.clear();
        self.deferred_results.clear();
    }

    /// Reset rendering state after an edit (content changed).
    pub(crate) fn reset_after_edit(&mut self, pages: usize, first_page: PageSize, content: u64) {
        self.content = content;
        self.geometry = vec![None; pages];
        self.geometry[0] = Some(first_page);
        self.cursor = 1;
        self.queue.replace(vec![]);
    }

    /// Process a view request: replace the render queue with the requested tiles.
    pub(crate) fn handle_view(&mut self, view: View, content: u64) {
        self.queue.replace(
            view.requests
                .into_iter()
                .filter(|r| !self.inflight.contains(&(r.key, r.generation, false, content)))
                .collect(),
        );
    }

    /// Process completed render results. Returns events to emit.
    pub(crate) fn process_results(&mut self) -> Vec<Event> {
        let mut events = Vec::new();

        while let Some(result) = self
            .deferred_results
            .pop_front()
        {
            self.inflight.remove(&(
                result.request.key,
                result.request.generation,
                result.thumbnail,
                result.content,
            ));
            if !self
                .shared
                .current_content(result.request.key.document, result.content)
            {
                continue;
            }

            if result.thumbnail {
                if self.shared.current_document(result.request.key.document) {
                    match result.pixels {
                        Some(Ok(pixels)) => {
                            events.push(Event::Thumbnail {
                                key: result.request.key,
                                pixels: Arc::new(pixels),
                                elapsed: result.elapsed,
                            });
                        }
                        Some(Err(error)) => {
                            events.push(Event::ReaderError {
                                document: self.document_id,
                                operation: "thumbnail",
                                page: Some(result.request.key.page),
                                message: error.to_string(),
                            });
                        }
                        None => {}
                    }
                }
                continue;
            }

            if !self
                .shared
                .current(result.request.key.document, result.request.generation)
            {
                continue;
            }
            match result.pixels {
                Some(Ok(pixels)) => {
                    events.push(Event::Tile {
                        key: result.request.key,
                        generation: result.request.generation,
                        pixels: Arc::new(pixels),
                        elapsed: result.elapsed,
                    });
                }
                Some(Err(error)) => {
                    events.push(Event::Error {
                        document: self.document_id,
                        error: error,
                    });
                }
                None => {}
            }
        }

        events
    }

    /// Main rendering tick: renders tiles from the queue synchronously.
    /// pdfium-render serializes all calls internally, so parallel workers add no benefit.
    pub(crate) fn tick(
        &mut self,
        doc: &PdfDocument,
        can_copy: bool,
        shared: &Shared,
    ) -> Vec<Event> {
        let mut events = Vec::new();

        // Dispatch render jobs from the queue.
        while self.inflight.is_empty() {
            let Some(request) = self.queue.pop() else {
                break;
            };
            if !shared.current(request.key.document, request.generation) {
                continue;
            }
            let page = request.key.page;
            if page >= self.geometry.len() {
                continue;
            }

            let started = Instant::now();
            let pages = doc.pages();
            let pdf_page = match pages.get(page) {
                Some(p) => p,
                None => continue,
            };

            // Cache geometry on first render of this page.
            let bounds = pdf_page.boundaries().map_err(|e| e.to_string());
            match bounds {
                Ok(b) => {
                    let size = PageSize::new(b.width(), b.height());
                    if self.geometry[page] != Some(size) {
                        self.geometry[page] = Some(size);
                        events.push(Event::Geometry {
                            document: self.document_id,
                            pages: vec![(page, size)],
                        });
                    }
                }
                Err(error) => {
                    events.push(Event::Error {
                        document: self.document_id,
                        error: crate::EngineError::Render(error),
                    });
                    continue;
                }
            }

            self.inflight.insert((request.key, request.generation, false, self.content));
            let result = raster::render(pdf_page, request.key);
            self.inflight.remove(&(request.key, request.generation, false, self.content));

            match result {
                Ok(pixels) => {
                    events.push(Event::Tile {
                        key: request.key,
                        generation: request.generation,
                        pixels: Arc::new(pixels),
                        elapsed: started.elapsed(),
                    });
                }
                Err(error) => {
                    events.push(Event::Error {
                        document: self.document_id,
                        error,
                    });
                }
            }
        }

        // Handle thumbnail rendering.
        if self.queue.is_empty() && self.inflight.is_empty() {
            // Thumbnails are rendered via the coordinator's reader subsystem.
            // The rendering tick only handles main tile rendering.
        }

        events
    }

    /// Advance geometry discovery: one page per call.
    pub(crate) fn advance_geometry(&mut self, doc: &PdfDocument) -> Vec<Event> {
        let mut events = Vec::new();

        while self.cursor < self.geometry.len() && self.geometry[self.cursor].is_some() {
            self.cursor += 1;
        }
        if self.cursor < self.geometry.len() && self.queue.is_empty() {
            let pages = doc.pages();
            if let Some(pdf_page) = pages.get(self.cursor) {
                if let Ok(bounds) = pdf_page.boundaries() {
                    let size = PageSize::new(bounds.width(), bounds.height());
                    self.geometry[self.cursor] = Some(size);
                    events.push(Event::Geometry {
                        document: self.document_id,
                        pages: vec![(self.cursor, size)],
                    });
                }
            }
            self.cursor += 1;
        }

        events
    }

    /// Access the render queue for the coordinator to inspect.
    pub(crate) fn queue(&self) -> &RenderQueue {
        &self.queue
    }

    /// Access the inflight set for the coordinator to inspect.
    pub(crate) fn inflight(&self) -> &HashSet<(TileKey, u64, bool, u64)> {
        &self.inflight
    }

    /// Access the geometry vector.
    pub(crate) fn geometry(&self) -> &Vec<Option<PageSize>> {
        &self.geometry
    }

    /// Set the content epoch.
    pub(crate) fn set_content(&mut self, content: u64) {
        self.content = content;
    }

    pub(crate) fn spawn_workers(
        &mut self,
        _shared: Arc<Shared>,
        _job_rx: crossbeam_channel::Receiver<Job>,
        _completed: crossbeam_channel::Sender<Completed>,
    ) -> Vec<std::thread::JoinHandle<()>> {
        // pdfium-render serializes all calls internally, so no worker threads are needed.
        Vec::new()
    }

    pub(crate) fn shutdown(&mut self) {
        // No threads to join.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendering_module_compiles() {
        let _ = std::ptr::null::<Rendering>();
    }
}
