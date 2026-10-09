use crate::{AppWindow, PageSurface, TileSurface, trace::Trace};
use pdf_engine::{Engine, EngineError, Event};
use slint::{ComponentHandle, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::{Duration, Instant},
};
use viewer_core::{Layout, PageSize, TileCache, TileKey, Viewport, clamp_zoom};

#[cfg(test)]
#[path = "reader_tests.rs"]
mod reader_tests;

#[path = "editing.rs"]
mod editing;
pub use editing::ToolDialog;

pub struct State {
    engine: Engine,
    trace: Trace,
    document: u64,
    generation: u64,
    sizes: Vec<PageSize>,
    layout: Layout,
    zoom: f32,
    cache: TileCache,
    images: HashMap<TileKey, slint::Image>,
    previews: HashMap<TileKey, slint::Image>,
    previous_view: Option<[f32; 6]>,
    dirty: bool,
    first_tile: bool,
    first_pixels_at: Instant,
    current_page: usize,
    open_started: Instant,
    zoom_started: Instant,
    render_error: Option<String>,
    benchmark: bool,
    benchmark_stage: u8,
    pub dialog_open: bool,
    tools: crate::features::ReaderTools,
    path: Option<PathBuf>,
    editor: editing::Editor,
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
            document: 0,
            generation: 0,
            sizes: Vec::new(),
            layout: Layout::new(&[], 1.0, 1.0),
            zoom: 1.0,
            cache: TileCache::new(128 * 1024 * 1024),
            images: HashMap::new(),
            previews: HashMap::new(),
            previous_view: None,
            dirty: true,
            first_tile: false,
            first_pixels_at: Instant::now(),
            current_page: usize::MAX,
            open_started: Instant::now(),
            zoom_started: Instant::now(),
            render_error: None,
            benchmark,
            benchmark_stage: 0,
            dialog_open: false,
            tools: Default::default(),
            path: None,
            editor: Default::default(),
        }
    }

    pub fn open(&mut self, ui: &AppWindow, path: PathBuf) {
        self.open_password(ui, path, None);
    }

    fn open_password(&mut self, ui: &AppWindow, path: PathBuf, password: Option<String>) {
        self.reset_editor(ui);
        self.tools.reset(ui);
        self.path = Some(path.clone());
        self.document += 1;
        self.generation += 1;
        self.sizes.clear();
        self.cache.clear();
        self.images.clear();
        self.previews.clear();
        ui.set_tiles(ModelRc::default());
        ui.set_previews(ModelRc::default());
        ui.set_pages(ModelRc::default());
        ui.set_ready(false);
        ui.set_loading(true);
        ui.set_page_count(0);
        ui.set_scroll_x(0.0);
        ui.set_scroll_y(0.0);
        ui.set_status("Opening PDF…".into());
        ui.set_detail("Preparing the first page".into());
        ui.set_document_name(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref()
                .into(),
        );
        self.zoom = 1.0;
        self.previous_view = None;
        self.first_tile = false;
        self.render_error = None;
        self.current_page = usize::MAX;
        self.benchmark_stage = 0;
        self.open_started = Instant::now();
        self.trace.event(
            "document_open_started",
            serde_json::json!({"document":self.document}),
        );
        self.engine
            .open_with_password(self.document, self.generation, path, password);
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
        self.tools.query(ui, &self.engine, self.document, query);
        self.tools.overlays(ui, &self.layout, self.zoom);
    }

    pub fn select_text(&mut self, ui: &AppWindow, page: usize, x: f32, y: f32, phase: i32) {
        self.tools.select(ui, page, x, y, phase, self.zoom);
        self.tools.overlays(ui, &self.layout, self.zoom);
    }

    pub fn navigate(&mut self, ui: &AppWindow, page: usize) {
        if self.sizes.is_empty() {
            return;
        }
        let page = page.min(self.sizes.len() - 1);
        ui.set_page_entry((page + 1).to_string().into());
        ui.set_scroll_y(
            -self.layout.pages[page]
                .y
                .min((self.layout.height - ui.get_view_height()).max(0.0)),
        );
    }

    pub fn action(&mut self, ui: &AppWindow, action: &str) {
        if ui.get_pending_open() && !action.starts_with("pending-") {
            return;
        }
        if self.edit_action(ui, action) {
            return;
        }
        match action {
            "password-cancel" => {
                // Replace any pending authentication with an invalidated document identity.
                self.document += 1;
                self.generation += 1;
                self.tools.reset(ui);
                self.path = None;
                ui.set_loading(false);
                ui.set_status("Open a PDF to get started".into());
                ui.set_detail("Password entry cancelled.".into());
                return;
            }
            "thumbnails" => {
                ui.set_sidebar_visible(!ui.get_sidebar_visible());
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
                        -(self.layout.pages[page].y + top * self.zoom * 96.0 / 72.0 - 60.0)
                            .clamp(0.0, (self.layout.height - ui.get_view_height()).max(0.0)),
                    );
                    self.tools.overlays(ui, &self.layout, self.zoom);
                }
                return;
            }
            _ => {}
        }
        if self.sizes.is_empty() {
            return;
        }
        let current = self.layout.page_at(-ui.get_scroll_y() + 30.0);
        let size = self.sizes[current];
        let new_zoom = match action {
            "zoom-in" => Some(self.zoom * 1.25),
            "zoom-out" => Some(self.zoom / 1.25),
            "fit-width" => Some((ui.get_view_width() - 62.0) / (size.width * 96.0 / 72.0)),
            "fit-page" => Some(
                ((ui.get_view_width() - 62.0) / (size.width * 96.0 / 72.0))
                    .min((ui.get_view_height() - 48.0) / (size.height * 96.0 / 72.0)),
            ),
            _ => None,
        };
        if let Some(zoom) = new_zoom {
            let old_zoom = self.zoom;
            let old_y = self.layout.pages[current].y;
            let offset = (-ui.get_scroll_y() - old_y).max(0.0);
            self.zoom = clamp_zoom(zoom);
            if self.zoom == old_zoom {
                return;
            }
            self.previews = std::mem::take(&mut self.images);
            self.zoom_started = Instant::now();
            self.layout = Layout::new(
                &self.sizes,
                self.zoom,
                (ui.get_view_width() - 14.0).max(1.0),
            );
            ui.set_document_height(self.layout.height);
            ui.set_document_width(self.layout.width);
            ui.set_scroll_y(
                -(self.layout.pages[current].y + offset * self.zoom / old_zoom)
                    .clamp(0.0, (self.layout.height - ui.get_view_height()).max(0.0)),
            );
            self.dirty = true;
            self.trace
                .event("zoom_input", serde_json::json!({"zoom":self.zoom}));
        } else {
            match action {
                "next" => self.navigate(ui, current + 1),
                "previous" => self.navigate(ui, current.saturating_sub(1)),
                "first" => self.navigate(ui, 0),
                "last" => self.navigate(ui, self.sizes.len() - 1),
                "down" => ui.set_scroll_y(
                    (ui.get_scroll_y() - 80.0)
                        .max(-(self.layout.height - ui.get_view_height()).max(0.0)),
                ),
                "up" => ui.set_scroll_y((ui.get_scroll_y() + 80.0).min(0.0)),
                _ => {}
            }
        }
    }

    pub fn tick(&mut self, ui: &AppWindow) {
        let mut changed = false;
        for _ in 0..64 {
            let Ok(event) = self.engine.events.try_recv() else {
                break;
            };
            changed |= self.tools.event(ui, self.document, &event);
            changed |= self.edit_event(ui, &event);
            match event {
                Event::Opened {
                    document,
                    info,
                    elapsed,
                } if document == self.document => {
                    self.sizes = vec![info.first_page; info.pages];
                    ui.set_page_count(info.pages as i32);
                    ui.set_loading(false);
                    ui.set_ready(true);
                    ui.set_can_copy(info.can_copy);
                    ui.set_can_annotate(info.can_annotate);
                    ui.set_can_assemble(info.can_assemble);
                    ui.set_password_required(false);
                    self.dirty = true;
                    self.trace.event("document_parsed", serde_json::json!({"pages":info.pages,"engine_ms":elapsed.as_secs_f64()*1000.0}));
                }
                Event::Geometry { document, pages } if document == self.document => {
                    for (index, size) in pages {
                        if self.sizes.get(index).is_some_and(|old| *old != size) {
                            self.sizes[index] = size;
                            self.dirty = true;
                        }
                    }
                }
                Event::DisplayListReady {
                    document,
                    page,
                    elapsed,
                } if document == self.document => {
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
                    if !viewer_core::accept_result(
                        self.document,
                        self.generation,
                        key.document,
                        generation,
                    ) {
                        self.trace
                            .event("stale_tile_discarded", serde_json::json!({}));
                        continue;
                    }
                    self.cache.insert(key, pixels);
                    self.trace.event("tile_available_to_ui", serde_json::json!({"page":key.page,"render_ms":elapsed.as_secs_f64()*1000.0,"generation":generation}));
                    changed = true;
                }
                Event::Error { document, error } if document == self.document => {
                    if matches!(error, EngineError::Render(_)) {
                        self.render_error = Some(error.to_string());
                    }
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
                    if self.benchmark {
                        let _ = slint::quit_event_loop();
                    }
                }
                _ => {}
            }
        }
        if self.sizes.is_empty() {
            if self.benchmark
                && self.document == 0
                && self.open_started.elapsed() > Duration::from_millis(600)
            {
                self.trace.event("benchmark_complete", serde_json::json!({"empty_window":true,"cache_bytes":0,"cache_hits":0,"cache_misses":0}));
                let _ = slint::quit_event_loop();
            }
            return;
        }
        self.tools
            .thumbnails(ui, &self.engine, self.document, self.sizes.len());
        if !self.previews.is_empty() && self.zoom_started.elapsed() > Duration::from_millis(750) {
            self.previews.clear();
            changed = true;
        }
        let mut view = Viewport {
            width: (ui.get_view_width() - 14.0).max(1.0),
            height: ui.get_view_height().max(1.0),
            scroll_x: (-ui.get_scroll_x()).max(0.0),
            scroll_y: (-ui.get_scroll_y()).max(0.0),
            zoom: self.zoom,
            dpi: ui.window().scale_factor(),
        };
        let signature = [
            view.width,
            view.height,
            view.scroll_x,
            view.scroll_y,
            view.zoom,
            view.dpi,
        ];
        let moved = self.previous_view != Some(signature);
        if moved || self.dirty || changed {
            if moved || self.dirty {
                let anchor = self.layout.anchor(view.scroll_y);
                let refined = self.dirty && !self.layout.pages.is_empty();
                if moved {
                    self.generation += 1;
                    self.render_error = None;
                }
                self.previous_view = Some(signature);
                self.layout = Layout::new(&self.sizes, self.zoom, view.width);
                ui.set_document_width(self.layout.width);
                ui.set_document_height(self.layout.height);
                if refined {
                    view.scroll_y = self
                        .layout
                        .resolve_anchor(anchor)
                        .clamp(0.0, (self.layout.height - view.height).max(0.0));
                    ui.set_scroll_y(-view.scroll_y);
                    self.previous_view = Some([
                        view.width,
                        view.height,
                        view.scroll_x,
                        view.scroll_y,
                        view.zoom,
                        view.dpi,
                    ]);
                }
                self.dirty = false;
                self.trace.event("viewport_changed", serde_json::json!({"generation":self.generation,"scroll_y":view.scroll_y,"zoom":self.zoom}));
            }
            self.refresh(ui, view);
        }
        if self.benchmark && self.first_tile {
            let elapsed = self.first_pixels_at.elapsed();
            if self.benchmark_stage == 0 && elapsed > Duration::from_millis(600) {
                let result = crate::capture::snapshot(ui, "first-page");
                self.trace.event("snapshot", serde_json::json!({"stage":"first-page","success":result.is_ok(),"error":result.err().map(|e|e.to_string())}));
                self.action(ui, "next");
                self.benchmark_stage = 1;
            }
            if self.benchmark_stage == 1 && elapsed > Duration::from_millis(1000) {
                self.action(ui, "zoom-in");
                self.benchmark_stage = 2;
            }
            if self.benchmark_stage == 2 && elapsed > Duration::from_millis(1500) {
                self.action(ui, "previous");
                self.benchmark_stage = 3;
            }
            if elapsed > Duration::from_millis(2200) {
                let _ = crate::capture::snapshot(ui, "after-zoom");
                self.trace.event("benchmark_complete", serde_json::json!({"cache_bytes":self.cache.bytes(),"cache_hits":self.cache.hits,"cache_misses":self.cache.misses,"native_store_limit_mib":pdf_engine::NATIVE_STORE_LIMIT_MIB}));
                let _ = slint::quit_event_loop();
            }
        }
    }

    fn refresh(&mut self, ui: &AppWindow, view: Viewport) {
        let visible = self.layout.visible_pages(view);
        let first = *visible.start();
        let last = *visible.end();
        let resident: Vec<usize> =
            (first.saturating_sub(2)..=(last + 2).min(self.sizes.len() - 1)).collect();
        self.edit_refresh(ui, &resident);
        self.tools.refresh(
            ui,
            &self.engine,
            self.document,
            &self.layout,
            self.zoom,
            &resident,
        );
        let pages = resident
            .iter()
            .map(|&index| {
                let page = self.layout.pages[index];
                PageSurface {
                    x: page.x,
                    y: page.y,
                    w: page.width,
                    h: page.height,
                    number: index as i32 + 1,
                }
            })
            .collect::<Vec<_>>();
        ui.set_pages(ModelRc::new(VecModel::from(pages)));
        let requests = self
            .layout
            .requests(self.document, self.generation, view, 2);
        let desired_images: HashSet<TileKey> = requests
            .iter()
            .filter(|r| r.priority < 1000)
            .map(|r| r.key)
            .collect();
        self.images.retain(|key, _| desired_images.contains(key));
        let mut missing = Vec::new();
        for request in requests {
            if self.images.contains_key(&request.key) {
                continue;
            }
            if let Some(pixels) = self.cache.get(&request.key) {
                if request.priority < 1000 {
                    self.images.entry(request.key).or_insert_with(|| {
                        let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                            &pixels.rgba,
                            pixels.width,
                            pixels.height,
                        );
                        slint::Image::from_rgba8(buffer)
                    });
                }
            } else {
                missing.push(request);
            }
        }
        if self.render_error.is_some() {
            missing.clear();
        }
        self.engine
            .view(self.document, self.generation, missing, resident);
        let tiles = self
            .images
            .iter()
            .map(|(key, image)| self.surface(*key, image.clone()))
            .collect::<Vec<_>>();
        if !tiles.is_empty() && !self.first_tile {
            self.first_tile = true;
            self.first_pixels_at = Instant::now();
            self.trace.event("first_page_pixels_published", serde_json::json!({"open_to_pixels_ms":self.open_started.elapsed().as_secs_f64()*1000.0,"presentation_verified":false}));
        }
        ui.set_tiles(ModelRc::new(VecModel::from(tiles)));
        if self.images.len() >= desired_images.len()
            || self.zoom_started.elapsed() > Duration::from_millis(750)
        {
            self.previews.clear();
        }
        let previews = self
            .previews
            .iter()
            .filter(|(key, _)| visible.contains(&key.page))
            .map(|(key, image)| self.surface(*key, image.clone()))
            .collect::<Vec<_>>();
        ui.set_previews(ModelRc::new(VecModel::from(previews)));
        ui.set_zoom_percent((self.zoom * 100.0).round() as i32);
        if self.current_page != first {
            ui.set_page_entry((first + 1).to_string().into());
            self.current_page = first;
        }
        ui.set_status(
            format!(
                "Page {} of {}   •   Raster cache {:.1} MiB / 128 MiB",
                first + 1,
                self.sizes.len(),
                self.cache.bytes() as f64 / 1048576.0
            )
            .into(),
        );
        if let Some(error) = &self.render_error {
            ui.set_status(error.as_str().into());
        }
    }

    fn surface(&self, key: TileKey, pixels: slint::Image) -> TileSurface {
        let page = self.layout.pages[key.page];
        let logical_per_pixel = self.zoom * 96.0 / 72.0 / (key.scale_milli as f32 / 1000.0);
        TileSurface {
            pixels,
            x: page.x + key.x as f32 * logical_per_pixel,
            y: page.y + key.y as f32 * logical_per_pixel,
            w: key.width as f32 * logical_per_pixel,
            h: key.height as f32 * logical_per_pixel,
        }
    }
}
