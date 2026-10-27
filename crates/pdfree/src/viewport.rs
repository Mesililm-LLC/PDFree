//! ViewportManager: zoom, layout, page geometry, scroll, and pointer state.

use crate::AppWindow;
use viewer_core::{Layout, PageSize, ReadingMode, clamp_zoom};

pub struct ViewportManager {
    pub zoom: f32,
    pub layout: Layout,
    pub sizes: Vec<PageSize>,
    pub current_page: usize,
    pub reading_page: usize,
    pub mode: ReadingMode,
    pub pointer: Option<(usize, f32, f32)>,
    pub previous_view: Option<[f32; 6]>,
    pub generation: u64,
    pub dirty: bool,
}

impl ViewportManager {
    pub fn new() -> Self {
        Self {
            zoom: 1.0,
            layout: Layout::new(&[], 1.0, 1.0),
            sizes: Vec::new(),
            current_page: usize::MAX,
            reading_page: 0,
            mode: ReadingMode::Continuous,
            pointer: None,
            previous_view: None,
            generation: 0,
            dirty: true,
        }
    }

    /// Reset viewport state for a new document open.
    pub fn reset_for_open(&mut self) {
        self.sizes.clear();
        self.zoom = 1.0;
        self.reading_page = 0;
        self.pointer = None;
        self.previous_view = None;
        self.current_page = usize::MAX;
        self.layout = Layout::new(&[], 1.0, 1.0);
        self.dirty = true;
    }

    pub fn reader_position(&self, x: f32, y: f32, nearest: bool) -> Option<(usize, f32, f32)> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let (page, rect) = self
            .layout
            .pages
            .iter()
            .enumerate()
            .filter(|(_, r)| r.width > 0.)
            .min_by(|(_, a), (_, b)| {
                let d = |r: &viewer_core::PageRect| {
                    (r.x - x).max(0.).max(x - r.x - r.width).powi(2)
                        + (r.y - y).max(0.).max(y - r.y - r.height).powi(2)
                };
                d(a).total_cmp(&d(b))
            })?;
        if !nearest
            && (x < rect.x || x > rect.x + rect.width || y < rect.y || y > rect.y + rect.height)
        {
            return None;
        }
        Some((page, x - rect.x, y - rect.y))
    }

    pub fn navigate(&mut self, ui: &AppWindow, page: usize) {
        self.pointer = None;
        if self.sizes.is_empty() {
            return;
        }
        let page = page.min(self.sizes.len() - 1);
        self.reading_page = page;
        if self.mode != ReadingMode::Continuous {
            self.layout = Layout::with_mode(
                &self.sizes,
                self.zoom,
                (ui.get_view_width() - 14.).max(1.),
                self.mode,
                page,
            );
            ui.set_document_height(self.layout.height);
            ui.set_document_width(self.layout.width);
            self.dirty = true;
        }
        ui.set_page_entry((page + 1).to_string().into());
        ui.set_scroll_y(
            -self.layout.pages[page]
                .y
                .min((self.layout.height - ui.get_view_height()).max(0.0)),
        );
    }

    /// Handle view-mode actions (continuous/single/facing). Returns true if handled.
    pub fn handle_view_mode(&mut self, ui: &AppWindow, action: &str) -> bool {
        self.reading_page = self.current_page.min(self.sizes.len().saturating_sub(1));
        self.mode = match action {
            "view-single" => ReadingMode::Single,
            "view-facing" => ReadingMode::Facing,
            _ => ReadingMode::Continuous,
        };
        ui.set_reading_mode(match self.mode {
            ReadingMode::Continuous => 0,
            ReadingMode::Single => 1,
            ReadingMode::Facing => 2,
        });
        self.pointer = None;
        self.navigate(ui, self.reading_page);
        self.dirty = true;
        true
    }

    /// Compute a new zoom value from a zoom action. Returns None if not a zoom action.
    pub fn compute_zoom(&self, ui: &AppWindow, action: &str) -> Option<f32> {
        if self.sizes.is_empty() {
            return None;
        }
        let current = self.layout.page_at(-ui.get_scroll_y() + 30.0);
        let size = self.sizes[current];
        let pair = if self.mode == ReadingMode::Facing {
            self.sizes.get(current + 1).copied()
        } else {
            None
        };
        let fit_width = size.width + pair.map_or(0., |s| s.width);
        let fit_height = size.height.max(pair.map_or(0., |s| s.height));
        let fit_space = if pair.is_some() { 78. } else { 62. };
        match action {
            "zoom-in" => Some(self.zoom * 1.25),
            "zoom-out" => Some(self.zoom / 1.25),
            "fit-width" => Some((ui.get_view_width() - fit_space) / (fit_width * 96.0 / 72.0)),
            "fit-page" => Some(
                ((ui.get_view_width() - fit_space) / (fit_width * 96.0 / 72.0))
                    .min((ui.get_view_height() - 48.0) / (fit_height * 96.0 / 72.0)),
            ),
            _ => None,
        }
    }

    /// Apply a new zoom value, rebuilding layout and adjusting scroll.
    pub fn apply_zoom(
        &mut self,
        ui: &AppWindow,
        current: usize,
        old_zoom: f32,
        new_zoom: f32,
        offset: f32,
    ) {
        self.zoom = clamp_zoom(new_zoom);
        if self.zoom == old_zoom {
            return;
        }
        self.layout = Layout::with_mode(
            &self.sizes,
            self.zoom,
            (ui.get_view_width() - 14.).max(1.),
            self.mode,
            self.reading_page,
        );
        ui.set_document_height(self.layout.height);
        ui.set_document_width(self.layout.width);
        ui.set_scroll_y(
            -(self.layout.pages[current].y + offset * self.zoom / old_zoom)
                .clamp(0.0, (self.layout.height - ui.get_view_height()).max(0.0)),
        );
        self.dirty = true;
    }

    /// Handle navigation actions (next/previous/first/last/down/up).
    pub fn handle_navigation_action(&mut self, ui: &AppWindow, action: &str, current: usize) {
        match action {
            "next" => self.navigate(
                ui,
                current
                    + if self.mode == ReadingMode::Facing {
                        2
                    } else {
                        1
                    },
            ),
            "previous" => self.navigate(
                ui,
                current.saturating_sub(if self.mode == ReadingMode::Facing {
                    2
                } else {
                    1
                }),
            ),
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

    /// Update layout based on viewport changes. Returns whether a UI refresh is needed.
    pub fn refresh_layout(&mut self, ui: &AppWindow, view: &viewer_core::Viewport) -> bool {
        let signature = [
            view.width,
            view.height,
            view.scroll_x,
            view.scroll_y,
            view.zoom,
            view.dpi,
        ];
        let moved = self.previous_view != Some(signature);
        let changed = moved || self.dirty;
        if !changed {
            return false;
        }
        if moved || self.dirty {
            let anchor = self.layout.anchor(view.scroll_y);
            let refined = self.dirty && !self.layout.pages.is_empty();
            if moved {
                self.generation += 1;
            }
            self.previous_view = Some(signature);
            self.layout = Layout::with_mode(
                &self.sizes,
                self.zoom,
                view.width,
                self.mode,
                self.reading_page,
            );
            ui.set_document_width(self.layout.width);
            ui.set_document_height(self.layout.height);
            if refined {
                let new_scroll_y = self
                    .layout
                    .resolve_anchor(anchor)
                    .clamp(0.0, (self.layout.height - view.height).max(0.0));
                ui.set_scroll_y(-new_scroll_y);
                self.previous_view = Some([
                    view.width,
                    view.height,
                    view.scroll_x,
                    new_scroll_y,
                    view.zoom,
                    view.dpi,
                ]);
            }
            self.dirty = false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_new_has_defaults() {
        let v = ViewportManager::new();
        assert_eq!(v.zoom, 1.0);
        assert_eq!(v.mode, ReadingMode::Continuous);
        assert!(v.pointer.is_none());
        assert!(v.dirty);
    }
}
