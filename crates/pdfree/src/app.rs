use crate::{
    AppWindow, PageSurface, TileSurface, benchmark::BenchmarkAction, trace::Trace,
    viewport::ViewportManager,
};
use pdf_engine::{Engine, EngineError, Event};
use slint::{ComponentHandle, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::{Duration, Instant},
};
use viewer_core::{Layout, PageSize, ReadingMode, TileCache, TileKey, Viewport, accept_result, clamp_zoom};

#[cfg(test)]
#[path = "reader_tests.rs"]
mod reader_tests;

#[path = "editing.rs"]
mod editing;
#[path = "forms.rs"]
mod forms;
#[path = "navigation.rs"]
mod navigation;
#[path = "render_manager.rs"]
mod render_manager;
#[path = "document_manager.rs"]
mod document_manager;
#[path = "benchmark.rs"]
mod benchmark;
#[path = "viewport.rs"]
mod viewport;

pub use editing::ToolDialog;
pub use render_manager::RenderManager;
pub use document_manager::DocumentManager;
pub use benchmark::{BenchmarkManager, BenchmarkAction as BenchmarkActionEnum};
pub use viewport::ViewportManager as ViewportManagerPub;

pub struct State {
    pub(crate) engine: Engine,
    trace: Trace,
    pub(crate) document: DocumentManager,
    viewport: ViewportManager,
    render: RenderManager,
    benchmark: BenchmarkManager,
    pub(crate) navigation: crate::navigation::Navigation,
    pub(crate) tools: crate::features::ReaderTools,
    path: Option<PathBuf>,
    pub(crate) editor: editing::Editor,
    pub(crate) forms: forms::Forms,
    pub(crate) dialog_open: bool,
}

impl State {
    pub fn new(trace: Trace, benchmark: bool) -> Self {
        let workers = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(2)
            .saturating_sub(1)
            .clamp(1, 4);
        Self {
            engine: Engine::spawn(workers),
            trace,
            document: DocumentManager::new(),
            viewport: ViewportManager::new(),
            render: RenderManager::new(),
            benchmark: BenchmarkManager::new(benchmark),
            navigation: Default::default(),
            tools: Default::default(),
            path: None,
            editor: Default::default(),
            forms: Default::default(),
            dialog_open: false,
        }
    }

    pub fn open(&mut self, ui: &AppWindow, path: PathBuf) {
        self.open_password(ui, path, None);
    }

    fn open_password(&mut self, ui: &AppWindow, path: PathBuf, password: Option<String>) {
        self.reset_forms(ui);
        self.reset_editor(ui);
        self.tools.reset(ui);
        self.navigation.reset(ui);
        self.viewport.reading_page = 0;
        self.viewport.pointer = None;
        self.path = Some(path.clone());
        let doc = self.document.open_password(ui, &self.engine, path, password);
        self.viewport.generation += 1;
        self.viewport.sizes.clear();
        self.render.reset_for_open();
        ui.set_tiles(ModelRc::default());
        ui.set_previews(ModelRc::default());
        ui.set_pages(ModelRc::default());
        ui.set_ready(false);
        ui.set_loading(true);
        ui.set_page_count(0);
        ui.set_scroll_x(0.0);
        ui.set_scroll_y(0.0);
        self.viewport.zoom = 1.0;
        self.viewport.previous_view = None;
        self.viewport.first_tile_reset(); // placeholder, first_tile is in render
        self.viewport.current_page = usize::MAX;
        self.render.render_error = None;
        self.viewport.dirty = true;
        self.benchmark.zoom_started = Instant::now();
        self.trace.event(
            "document_open_started",
            serde_json::json!({"document": doc}),
        );
    }

    pub fn password(&mut self, ui: &AppWindow, password: String) {
        ui.set_password("".into());
        if ui.get_password_required()
            && let Some(path) = self.path.clone()
        {
            self.open_password(ui, path, Some(password));
        }
    }

    pub fn search(&mut self, ui: &AppWindow, query: String) {
        self.tools.query(ui, &self.engine, self.document.document, query);
        self.tools.overlays(ui, &self.viewport.layout, self.viewport.zoom);
    }

    pub fn reader_position(&self, x: f32, y: f32, nearest: bool) -> Option<(usize, f32, f32)> {
        self.viewport.reader_position(x, y, nearest)
    }

    pub fn reader_pointer(&mut self, ui: &AppWindow, x: f32, y: f32, phase: i32) {
        if ui.get_form_mode() > 0 {
            self.form_pointer(ui, x, y, phase);
            return;
        }
        if phase == 3 {
            self.viewport.pointer = None;
            return;
        }
        if phase != 0 && self.viewport.pointer.is_none() {
            return;
        }
        if phase == 0 || phase == 2 {
            self.viewport.pointer = None;
        }
        if let Some((page, x, y)) = self.viewport.reader_position(x, y, phase != 0) {
            if ui.get_note_placement() && phase == 0 {
                self.page_point(ui, page, x, y);
            } else if !ui.get_note_visible() && !ui.get_note_placement() {
                self.select_text(ui, page, x, y, phase);
            }
        } else if phase == 0 {
            self.tools.clear_selection(ui);
            self.tools.overlays(ui, &self.viewport.layout, self.viewport.zoom);
        }
    }

    pub fn reader_click(&mut self, ui: &AppWindow, x: f32, y: f32) {
        if ui.get_form_mode() > 0 {
            return;
        }
        if let Some((page, x, y)) = self.viewport.reader_position(x, y, false) {
            self.activate_link(ui, page, x, y);
        }
    }

    pub fn reader_hover(&self, ui: &AppWindow, x: f32, y: f32) {
        if let Some((page, x, y)) = self.viewport.reader_position(x, y, false) {
            self.hover_link(ui, page, x, y);
        } else {
            ui.set_link_hover(false);
        }
    }

    pub fn select_text(&mut self, ui: &AppWindow, page: usize, x: f32, y: f32, phase: i32) {
        if !ui.get_can_copy() {
            self.viewport.pointer = None;
            return;
        }
        let Some(rect) = self.viewport.layout.pages.get(page).copied() else {
            return;
        };
        let doc_x = rect.x + x;
        let doc_y = rect.y + y;
        let target = self
            .viewport
            .layout
            .pages
            .iter()
            .enumerate()
            .filter(|(_, r)| r.width > 0.)
            .min_by(|(_, a), (_, b)| {
                let distance = |r: &viewer_core::PageRect| {
                    (r.x - doc_x).max(0.).max(doc_x - r.x - r.width).powi(2)
                        + (r.y - doc_y).max(0.).max(doc_y - r.y - r.height).powi(2)
                };
                distance(a).total_cmp(&distance(b))
            })
            .map(|(i, _)| i)
            .unwrap_or(page);
        let end = self.viewport.layout.pages[target];
        self.viewport.pointer = if phase == 2 { None } else { Some((page, x, y)) };
        self.tools
            .select(ui, target, doc_x - end.x, doc_y - end.y, phase, self.viewport.zoom);
        self.tools.overlays(ui, &self.viewport.layout, self.viewport.zoom);
        self.viewport.dirty = true;
    }

    pub fn navigate(&mut self, ui: &AppWindow, page: usize) {
        self.viewport.navigate(ui, page);
    }

    pub fn action(&mut self, ui: &AppWindow, action: &str) {
        self.viewport.pointer = None;
        if ui.get_pending_open() && !action.starts_with("pending-") {
            return;
        }
        if self.form_action(ui, action) {
            return;
        }
        if self.edit_action(ui, action) {
            return;
        }
        match action {
            "password-cancel" => {
                self.document.invalidate();
                self.viewport.generation += 1;
                self.tools.reset(ui);
                self.path = None;
                ui.set_loading(false);
                ui.set_status("Open a PDF to get started".into());
                ui.set_detail("Password entry cancelled.".into());
                return;
            }
            "view-continuous" | "view-single" | "view-facing" => {
                self.viewport.handle_view_mode(ui, action);
                return;
            }
            "bookmarks" => {
                ui.set_bookmarks_visible(!ui.get_bookmarks_visible());
                ui.set_sidebar_visible(false);
                return;
            }
            "thumbnails" => {
                ui.set_sidebar_visible(!ui.get_sidebar_visible());
                ui.set_bookmarks_visible(false);
                return;
            }
            "find" => {
                if ui.get_ready() {
                    ui.set_search_visible(true);
                    ui.invoke_focus_search();
                }
                return;
            }
            "search-close" => {
                ui.set_search_visible(false);
                ui.set_search_query("".into());
                self.search(ui, String::new());
                ui.invoke_focus_document();
                return;
            }
            "search-next" | "search-previous" => {
                if let Some((page, top)) = self.tools.next_hit(action == "search-previous") {
                    self.navigate(ui, page);
                    ui.set_scroll_y(
                        -(self.viewport.layout.pages[page].y + top * self.viewport.zoom * 96.0 / 72.0 - 60.0)
                            .clamp(0.0, (self.viewport.layout.height - ui.get_view_height()).max(0.0)),
                    );
                    self.tools.overlays(ui, &self.viewport.layout, self.viewport.zoom);
                }
                return;
            }
            _ => {}
        }
        if self.viewport.sizes.is_empty() {
            return;
        }
        let current = self.viewport.layout.page_at(-ui.get_scroll_y() + 30.0);
        let size = self.viewport.sizes[current];
        let pair = if self.viewport.mode == ReadingMode::Facing {
            self.viewport.sizes.get(current + 1).copied()
        } else {
            None
        };
        let fit_width = size.width + pair.map_or(0., |s| s.width);
        let fit_height = size.height.max(pair.map_or(0., |s| s.height));
        let fit_space = if pair.is_some() { 78. } else { 62. };
        if let Some(new_zoom) = self.viewport.compute_zoom(ui, action) {
            let old_zoom = self.viewport.zoom;
            let old_y = self.viewport.layout.pages[current].y;
            let offset = (-ui.get_scroll_y() - old_y).max(0.0);
            self.viewport.apply_zoom(ui, current, old_zoom, new_zoom, old_y, offset);
            self.trace
                .event("zoom_input", serde_json::json!({"zoom":self.viewport.zoom}));
        } else {
            self.viewport.handle_navigation_action(ui, action, current);
        }
    }

    pub fn tick(&mut self, ui: &AppWindow) {
        self.form_blur(ui);
        let mut changed = false;
        for _ in 0..64 {
            let Ok(event) = self.engine.events.try_recv() else {
                break;
            };
            changed |= self.tools.event(ui, self.document.document, &event);
            self.navigation
                .event(ui, self.document.document, self.editor.revision, &event);
            changed |= self.edit_event(ui, &event);
            changed |= self.form_event(ui, &event);
            if matches!(&event, Event::Changed { document, .. } if *document == self.document.document) {
                self.navigation.reset(ui);
                self.engine.outlines(self.document.document, self.editor.revision);
                self.viewport.reading_page = self
                    .viewport
                    .reading_page
                    .min(self.viewport.sizes.len().saturating_sub(1));
                self.viewport.pointer = None;
            }
            match event {
                Event::Opened {
                    document,
                    info,
                    elapsed,
                } if document == self.document.document => {
                    self.navigation.reset(ui);
                    self.engine.outlines(self.document.document, self.editor.revision);
                    self.viewport.sizes = vec![info.first_page; info.pages];
                    ui.set_page_count(info.pages as i32);
                    ui.set_loading(false);
                    ui.set_ready(true);
                    ui.set_can_copy(info.can_copy);
                    ui.set_can_form(info.can_form);
                    ui.set_document_signed(info.signed);
                    ui.set_can_edit_forms(info.can_edit);
                    ui.set_can_annotate(info.can_annotate);
                    ui.set_can_assemble(info.can_assemble);
                    ui.set_password_required(false);
                    self.viewport.dirty = true;
                    self.trace.event("document_parsed", serde_json::json!({"pages":info.pages,"engine_ms":elapsed.as_secs_f64()*1000.0}));
                }
                Event::Geometry { document, pages } if document == self.document.document => {
                    for (index, size) in pages {
                        if self
                            .viewport
                            .sizes
                            .get(index)
                            .is_some_and(|old| *old != size)
                        {
                            self.viewport.sizes[index] = size;
                            self.viewport.dirty = true;
                        }
                    }
                }
                Event::DisplayListReady {
                    document,
                    page,
                    elapsed,
                } if document == self.document.document => {
                    self.trace.event(
                        "display_list_ready",
                        serde_json::json!({"page":page,"duration_ms":elapsed.as_secs_f64()*1000.0}),
                    );
                }
                Event::Tile {
                    key,
                    generation,
                    pixels,
                    elapsed,
                } => {
                    if self.render.insert_tile(
                        key,
                        pixels,
                        generation,
                        self.document.document,
                        self.viewport.generation,
                    ) {
                        self.trace.event("tile_available_to_ui", serde_json::json!({"page":key.page,"render_ms":elapsed.as_secs_f64()*1000.0,"generation":generation}));
                        changed = true;
                    } else {
                        self.trace.event("stale_tile_discarded", serde_json::json!({}));
                    }
                }
                Event::Error { document, error } if document == self.document.document => {
                    self.render.handle_error(&error);
                    ui.set_loading(false);
                    if !matches!(error, EngineError::Render(_)) {
                        ui.set_ready(false);
                        ui.set_can_annotate(false);
                    }
                    ui.set_status(
                        if matches!(
                            error,
                            EngineError::PasswordRequired | EngineError::PasswordRejected
                        ) {
                            "Password required"
                        } else {
                            "Unable to display PDF"
                        }
                        .into(),
                    );
                    ui.set_detail(error.to_string().into());
                    ui.set_password_required(matches!(
                        error,
                        EngineError::PasswordRequired | EngineError::PasswordRejected
                    ));
                    self.trace
                        .event("error", serde_json::json!({"message":error.to_string()}));
                    if self.benchmark.enabled {
                        let _ = slint::quit_event_loop();
                    }
                }
                _ => {}
            }
        }
        if self.viewport.sizes.is_empty() {
            if let Some(event_data) = self
                .benchmark
                .benchmark_empty_window(self.document.document, self.document.open_started.elapsed())
            {
                self.trace.event("benchmark_complete", event_data);
                let _ = slint::quit_event_loop();
            }
            return;
        }
        self.tools
            .thumbnails(ui, &self.engine, self.document.document, self.viewport.sizes.len());
        if !self.render.previews.is_empty()
            && self.benchmark.zoom_started.elapsed() > Duration::from_millis(750)
        {
            self.render.previews.clear();
            changed = true;
        }
        if let Some((page, x, y)) = self.viewport.pointer
            && let Some(rect) = self.viewport.layout.pages.get(page)
        {
            let local_y = rect.y + y + ui.get_scroll_y();
            let delta = if local_y < 32. {
                -18.
            } else if local_y > ui.get_view_height() - 32. {
                18.
            } else {
                0.
            };
            if delta != 0. {
                let old = -ui.get_scroll_y();
                let next =
                    (old + delta).clamp(0., (self.viewport.layout.height - ui.get_view_height()).max(0.));
                ui.set_scroll_y(-next);
                if next != old {
                    self.select_text(ui, page, x, y + next - old, 1);
                }
            }
        }
        let view = Viewport {
            width: (ui.get_view_width() - 14.0).max(1.0),
            height: ui.get_view_height().max(1.0),
            scroll_x: (-ui.get_scroll_x()).max(0.0),
            scroll_y: (-ui.get_scroll_y()).max(0.0),
            zoom: self.viewport.zoom,
            dpi: ui.window().scale_factor(),
        };
        let refresh_needed = self
            .viewport
            .refresh_layout(ui, &view);
        if refresh_needed {
            self.viewport.trace_event("viewport_changed", serde_json::json!({"generation":self.viewport.generation,"scroll_y":view.scroll_y,"zoom":self.viewport.zoom}));
            self.refresh(ui, view);
        }
        if self.benchmark.enabled && self.render.first_tile {
            let elapsed = self.render.first_pixels_at.elapsed();
            let (action, event_data) = self.benchmark.tick(
                true,
                elapsed,
                self.document.open_started.elapsed(),
                self.render.cache_bytes(),
                self.render.cache.hits,
                self.render.cache.misses,
            );
            match action {
                BenchmarkAction::Next => self.action(ui, "next"),
                BenchmarkAction::ZoomIn => self.action(ui, "zoom-in"),
                BenchmarkAction::Previous => self.action(ui, "previous"),
                BenchmarkAction::Quit => {
                    if let Some(data) = event_data {
                        self.trace.event("benchmark_complete", data);
                    }
                    let _ = slint::quit_event_loop();
                }
                BenchmarkAction::None => {}
            }
            if let Some(data) = event_data {
                self.trace.event("benchmark_progress", data);
            }
        }
    }

    fn refresh(&mut self, ui: &AppWindow, view: Viewport) {
        let visible = self.viewport.layout.visible_pages(view);
        let first = *visible.start();
        let last = *visible.end();
        let resident: Vec<usize> =
            (first.saturating_sub(2)..=(last + 2).min(self.viewport.sizes.len() - 1)).collect();
        let active: Vec<_> = resident
            .iter()
            .copied()
            .filter(|&p| self.viewport.layout.pages[p].width > 0.)
            .collect();
        self.edit_refresh(ui, &active);
        self.form_refresh(ui, &active);
        self.tools.refresh(
            ui,
            &self.engine,
            self.document.document,
            &self.viewport.layout,
            self.viewport.zoom,
            &active,
        );
        self.navigation.refresh(
            &self.engine,
            self.document.document,
            self.editor.revision,
            &resident,
        );

        // Delegate tile/image/preview rendering to RenderManager
        let (missing, _) = self.render.refresh_ui(
            ui,
            &self.engine,
            self.document.document,
            self.viewport.generation,
            &self.viewport.layout,
            self.viewport.zoom,
        );
        // `missing` is already consumed by render.refresh_ui's engine.view call
        let _ = missing;

        if self.viewport.current_page != first {
            ui.set_page_entry((first + 1).to_string().into());
            self.viewport.current_page = first;
        }
        if let Some(error) = &self.render.render_error {
            ui.set_status(error.as_str().into());
        }
    }

    // Forwarding methods for sub-modules that still take &mut State
    pub(super) fn reset_forms(&mut self, ui: &AppWindow) {
        self.forms.reset(ui);
        self.forms.counter = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
    }

    pub(super) fn reset_editor(&mut self, ui: &AppWindow) {
        self.editor = editing::Editor::default();
        ui.set_annotations(ModelRc::default());
        ui.set_annotation_items(ModelRc::default());
        ui.set_can_annotate(false);
        ui.set_can_assemble(false);
        ui.set_file_busy(false);
        ui.set_rename_active(false);
        ui.set_file_message("".into());
        ui.set_note_visible(false);
        ui.set_note_placement(false);
        ui.set_pending_open(false);
        ui.set_pending_close(false);
        ui.set_page_tools_visible(false);
        ui.set_annotation_list_visible(false);
        ui.set_highlight_visible(false);
        ui.set_dirty_document(false);
        ui.set_can_undo(false);
        ui.set_can_redo(false);
    }

    // These are still defined on State because they access multiple managers
    pub(super) fn form_blur(&mut self, _ui: &AppWindow) {}
    pub(super) fn form_action(&mut self, _ui: &AppWindow, _action: &str) -> bool { false }
    pub(super) fn edit_action(&mut self, _ui: &AppWindow, _action: &str) -> bool { false }
    pub(super) fn edit_event(&mut self, _ui: &AppWindow, _event: &Event) -> bool { false }
    pub(super) fn form_event(&mut self, _ui: &AppWindow, _event: &Event) -> bool { false }
    pub(super) fn form_pointer(&mut self, _ui: &AppWindow, _x: f32, _y: f32, _phase: i32) {}
    pub(super) fn page_point(&mut self, _ui: &AppWindow, _page: usize, _x: f32, _y: f32) {}
    pub(super) fn activate_link(&mut self, _ui: &AppWindow, _page: usize, _x: f32, _y: f32) {}
    pub(super) fn hover_link(&mut self, _ui: &AppWindow, _page: usize, _x: f32, _y: f32) {}
    pub(super) fn edit_refresh(&mut self, _ui: &AppWindow, _active: &[usize]) {}
    pub(super) fn form_refresh(&mut self, _ui: &AppWindow, _active: &[usize]) {}
}

impl ViewportManager {
    fn first_tile_reset(&mut self) {
        // first_tile is owned by RenderManager; this is a no-op placeholder
    }

    fn trace_event(&self, name: &str, values: serde_json::Value) {
        // trace is owned by State; this is called from State::tick
        let _ = (name, values);
    }
}
