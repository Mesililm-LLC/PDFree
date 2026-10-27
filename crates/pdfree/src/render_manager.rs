//! RenderManager: tile cache, image/preview buffers, and render error state.

use crate::AppWindow;
use pdf_engine::{Engine, EngineError};
use slint::{ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::collections::HashSet;
use viewer_core::{PixelTile, TileKey};

pub struct RenderManager {
    pub cache: viewer_core::TileCache,
    pub images: std::collections::HashMap<TileKey, slint::Image>,
    pub previews: std::collections::HashMap<TileKey, slint::Image>,
    pub render_error: Option<String>,
    pub first_tile: bool,
    pub first_pixels_at: std::time::Instant,
}

impl RenderManager {
    pub fn new() -> Self {
        Self {
            cache: viewer_core::TileCache::new(128 * 1024 * 1024),
            images: std::collections::HashMap::new(),
            previews: std::collections::HashMap::new(),
            render_error: None,
            first_tile: false,
            first_pixels_at: std::time::Instant::now(),
        }
    }

    /// Reset render state for a new document open.
    pub fn reset_for_open(&mut self) {
        self.cache.clear();
        self.images.clear();
        self.previews.clear();
        self.render_error = None;
        self.first_tile = false;
        self.first_pixels_at = std::time::Instant::now();
    }

    /// Process a completed tile event from the engine. Returns true if accepted.
    pub fn insert_tile(
        &mut self,
        key: TileKey,
        pixels: PixelTile,
        generation: u64,
        document: u64,
        current_generation: u64,
    ) -> bool {
        if !viewer_core::accept_result(document, current_generation, key.document, generation) {
            return false;
        }
        self.cache.insert(key, pixels);
        true
    }

    /// Handle a render error event.
    pub fn handle_error(&mut self, error: &EngineError) {
        if matches!(error, EngineError::Render(_)) {
            self.render_error = Some(error.to_string());
        }
    }

    /// Build the UI refresh: pages, tiles, previews, and missing render requests.
    /// Returns (missing_requests, active_pages).
    pub fn refresh_ui(
        &mut self,
        ui: &AppWindow,
        engine: &Engine,
        document: u64,
        generation: u64,
        layout: &viewer_core::Layout,
        zoom: f32,
    ) -> (Vec<viewer_core::RenderRequest>, Vec<usize>) {
        let view = viewport_view(ui, zoom);
        let visible = layout.visible_pages(view);
        let first = *visible.start();
        let last = *visible.end();
        let resident: Vec<usize> = (first.saturating_sub(2)
            ..=(last + 2).min(layout.pages.len().saturating_sub(1)))
            .collect();
        let active: Vec<usize> = resident
            .iter()
            .copied()
            .filter(|&p| layout.pages[p].width > 0.)
            .collect();

        let pages = resident
            .iter()
            .filter(|&&index| layout.pages[index].width > 0.)
            .map(|&index| {
                let page = layout.pages[index];
                crate::PageSurface {
                    x: page.x,
                    y: page.y,
                    w: page.width,
                    h: page.height,
                    number: index as i32 + 1,
                }
            })
            .collect::<Vec<_>>();
        ui.set_pages(ModelRc::new(VecModel::from(pages)));

        let requests = layout.requests(document, generation, &view, 2);
        let desired_images: HashSet<TileKey> = requests
            .iter()
            .filter(|r| r.priority < 1000)
            .map(|r| r.key)
            .collect();
        self.images.retain(|key, _| desired_images.contains(key));
        let mut missing = Vec::new();
        for request in &requests {
            if self.images.contains_key(request) {
                continue;
            }
            if let Some(pixels) = self.cache.get(request) {
                if request.priority < 1000 {
                    self.images.entry(*request).or_insert_with(|| {
                        let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                            &pixels.rgba,
                            pixels.width,
                            pixels.height,
                        );
                        slint::Image::from_rgba8(buffer)
                    });
                }
            } else {
                missing.push(*request);
            }
        }
        if self.render_error.is_some() {
            missing.clear();
        }
        engine.view(document, generation, missing.clone(), resident.clone());

        let tiles = self
            .images
            .iter()
            .map(|(key, image)| self.surface(*key, image.clone(), layout, zoom))
            .collect::<Vec<_>>();
        if !tiles.is_empty() && !self.first_tile {
            self.first_tile = true;
            self.first_pixels_at = std::time::Instant::now();
        }
        ui.set_tiles(ModelRc::new(VecModel::from(tiles)));

        if self.images.len() >= desired_images.len() {
            self.previews.clear();
        }
        let previews = self
            .previews
            .iter()
            .filter(|(key, _)| visible.contains(&key.page))
            .map(|(key, image)| self.surface(*key, image.clone(), layout, zoom))
            .collect::<Vec<_>>();
        ui.set_previews(ModelRc::new(VecModel::from(previews)));
        ui.set_zoom_percent((zoom * 100.0).round() as i32);
        ui.set_page_entry((first + 1).to_string().into());
        ui.set_status(
            format!(
                "Page {} of {}   •   Raster cache {:.1} MiB / 128 MiB",
                first + 1,
                layout.pages.len(),
                self.cache.bytes() as f64 / 1048576.0
            )
            .into(),
        );
        if let Some(error) = &self.render_error {
            ui.set_status(error.as_str().into());
        }

        (missing, active)
    }

    fn surface(
        &self,
        key: TileKey,
        image: slint::Image,
        layout: &viewer_core::Layout,
        zoom: f32,
    ) -> crate::TileSurface {
        let page = layout.pages[key.page];
        let logical_per_pixel = zoom * 96.0 / 72.0 / (key.scale_milli as f32 / 1000.0);
        crate::TileSurface {
            pixels: image,
            x: page.x + key.x as f32 * logical_per_pixel,
            y: page.y + key.y as f32 * logical_per_pixel,
            w: key.width as f32 * logical_per_pixel,
            h: key.height as f32 * logical_per_pixel,
        }
    }

    pub fn cache_bytes(&self) -> usize {
        self.cache.bytes()
    }
}

fn viewport_view(ui: &AppWindow, zoom: f32) -> viewer_core::Viewport {
    viewer_core::Viewport {
        width: (ui.get_view_width() - 14.0).max(1.0),
        height: ui.get_view_height().max(1.0),
        scroll_x: (-ui.get_scroll_x()).max(0.0),
        scroll_y: (-ui.get_scroll_y()).max(0.0),
        zoom,
        dpi: ui.window().scale_factor(),
    }
}
