use crate::{AppWindow, OverlaySurface, ThumbnailSurface};
use pdf_engine::{Engine, Event};
use slint::{ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::{collections::HashMap, sync::Arc};
use viewer_core::{Layout, PageText, SearchHit, TextRect, TextSelection};

#[derive(Default)]
pub struct ReaderTools {
    text: HashMap<usize, Arc<PageText>>,
    resident: Vec<usize>,
    selection: TextSelection,
    drag: Option<(usize, (f32, f32))>,
    selection_page: usize,
    selections: std::collections::BTreeMap<usize, TextSelection>,
    endpoint: Option<(usize, (f32, f32))>,
    hits: Vec<SearchHit>,
    active_hit: Option<usize>,
    search_id: u64,
    thumbs: HashMap<usize, slint::Image>,
    thumb_pages: Vec<usize>,
}

impl ReaderTools {
    pub fn selected_highlight(&self) -> Option<viewer_core::Annotation> {
        (self.selections.len() <= 1 && !self.selection.rects.is_empty()).then(|| {
            viewer_core::Annotation::Highlight {
                page: self.selection_page,
                rects: self.selection.rects.clone(),
            }
        })
    }
    pub fn clear_selection(&mut self, ui: &AppWindow) {
        if matches!(
            ui.get_file_message().as_str(),
            "Preparing selected text…" | "Select up to 16 pages at a time."
        ) {
            ui.set_file_message("".into());
        }
        self.selection = TextSelection::default();
        self.selections.clear();
        self.endpoint = None;
        self.drag = None;
        ui.set_selection_text("".into());
        ui.set_selection_single_page(false);
    }
    #[cfg(test)]
    pub fn test_text(&self, page: usize) -> Option<Arc<PageText>> {
        self.text.get(&page).cloned()
    }
    pub fn reset(&mut self, ui: &AppWindow) {
        let next = self.search_id.wrapping_add(1);
        *self = Self::default();
        self.search_id = next;
        ui.set_selection_text("".into());
        ui.set_selection_single_page(false);
        ui.set_search_query("".into());
        ui.set_search_status("".into());
        ui.set_can_copy(false);
        ui.set_password_required(false);
        ui.set_password("".into());
        ui.set_thumbnail_scroll(0.0);
        ui.set_thumbnails(ModelRc::default());
        ui.set_overlays(ModelRc::default());
    }

    pub fn query(&mut self, ui: &AppWindow, engine: &Engine, document: u64, query: String) {
        self.search_id = self.search_id.wrapping_add(1);
        self.hits.clear();
        self.active_hit = None;
        if query.len() > 512 || query.contains('\0') {
            // Cancel the previous native query before reporting the input limit.
            engine.search(document, self.search_id, String::new());
            self.search_id = self.search_id.wrapping_add(1);
            ui.set_search_status(
                "Search accepts at most 512 UTF-8 bytes and no null characters.".into(),
            );
        } else {
            ui.set_search_status(if query.is_empty() { "" } else { "Searching…" }.into());
            engine.search(document, self.search_id, query);
        }
    }

    pub fn next_hit(&mut self, previous: bool) -> Option<(usize, f32)> {
        if self.hits.is_empty() {
            return None;
        }
        let count = self.hits.len();
        let index = match (self.active_hit, previous) {
            (Some(index), true) => (index + count - 1) % count,
            (Some(index), false) => (index + 1) % count,
            (None, true) => count - 1,
            (None, false) => 0,
        };
        self.active_hit = Some(index);
        let hit = &self.hits[index];
        Some((hit.page, hit.rects.first().map_or(0.0, |r| r.top)))
    }

    pub fn select(&mut self, ui: &AppWindow, page: usize, x: f32, y: f32, phase: i32, zoom: f32) {
        if !ui.get_can_copy() {
            return;
        }
        let point = (x / (zoom * 96.0 / 72.0), y / (zoom * 96.0 / 72.0));
        if phase == 0 {
            self.drag = Some((page, point));
            self.selection_page = page;
            self.selections.clear();
        }
        if self.drag.is_some() {
            self.endpoint = Some((page, point));
            self.reselect(ui);
        }
    }

    fn selection_pages(&self) -> Vec<usize> {
        match (self.drag, self.endpoint) {
            (Some((a, _)), Some((b, _))) if a.abs_diff(b) < 16 => (a.min(b)..=a.max(b)).collect(),
            _ => Vec::new(),
        }
    }

    fn reselect(&mut self, ui: &AppWindow) {
        let (Some((a, start)), Some((b, end))) = (self.drag, self.endpoint) else {
            return;
        };
        self.selections.clear();
        self.selection = TextSelection::default();
        ui.set_selection_text("".into());
        ui.set_selection_single_page(false);
        if a.abs_diff(b) >= 16 {
            ui.set_file_message("Select up to 16 pages at a time.".into());
            return;
        }
        if (a.min(b)..=a.max(b)).any(|p| !self.text.contains_key(&p)) {
            ui.set_file_message("Preparing selected text…".into());
            return;
        }
        let ((first, from), (last, to)) = if a <= b {
            ((a, start), (b, end))
        } else {
            ((b, end), (a, start))
        };
        let mut copied = Vec::new();
        for page in first..=last {
            let text = &self.text[&page];
            let selected = if first == last {
                text.select(from, to)
            } else {
                let lo = if page == first {
                    text.nearest_index(from).unwrap_or(0)
                } else {
                    0
                };
                let hi = if page == last {
                    text.nearest_index(to).map_or(0, |i| i + 1)
                } else {
                    text.glyphs.len()
                };
                text.select_indices(lo, hi)
            };
            if !selected.text.is_empty() {
                copied.push(selected.text.clone());
            }
            self.selections.insert(page, selected);
        }
        if first == last {
            self.selection = self.selections[&first].clone();
            self.selection_page = first;
        }
        ui.set_selection_text(copied.join("\n\n").into());
        if matches!(
            ui.get_file_message().as_str(),
            "Preparing selected text…" | "Select up to 16 pages at a time."
        ) {
            ui.set_file_message("".into());
        }
        ui.set_selection_single_page(first == last);
    }

    pub fn event(&mut self, ui: &AppWindow, document: u64, event: &Event) -> bool {
        match event {
            Event::Text {
                document: id,
                page,
                text,
                ..
            } if *id == document => {
                if self.resident.contains(page) && ui.get_can_copy() {
                    self.text.insert(*page, text.clone());
                    self.reselect(ui);
                    if text.truncated {
                        ui.set_search_status(
                            "Selectable text on this page is limited to 100,000 characters.".into(),
                        );
                    }
                }
                true
            }
            Event::SearchPage {
                document: id,
                search,
                hits,
                scanned,
                total,
                done,
                truncated,
                errors,
                ..
            } if *id == document && *search == self.search_id => {
                self.hits.extend(
                    hits.iter()
                        .take(10_000usize.saturating_sub(self.hits.len()))
                        .cloned(),
                );
                ui.set_search_status(
                    format!(
                        "{} matches · {scanned}/{total} pages{}{}{}",
                        self.hits.len(),
                        if *done { "" } else { " · searching" },
                        if *truncated {
                            " · result limit reached"
                        } else {
                            ""
                        },
                        if *errors > 0 {
                            " · some pages could not be searched"
                        } else {
                            ""
                        }
                    )
                    .into(),
                );
                true
            }
            Event::Thumbnail { key, pixels, .. } if key.document == document => {
                if self.thumb_pages.contains(&key.page) {
                    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                        &pixels.rgba,
                        pixels.width,
                        pixels.height,
                    );
                    self.thumbs
                        .insert(key.page, slint::Image::from_rgba8(buffer));
                    self.publish_thumbnails(ui);
                }
                false
            }
            Event::ReaderError {
                document: id,
                message,
                ..
            } if *id == document => {
                ui.set_search_status(message.as_str().into());
                false
            }
            _ => false,
        }
    }

    pub fn refresh(
        &mut self,
        ui: &AppWindow,
        engine: &Engine,
        document: u64,
        layout: &Layout,
        zoom: f32,
        resident: &[usize],
    ) {
        let mut wanted = if ui.get_can_copy() {
            self.selection_pages()
        } else {
            Vec::new()
        };
        for page in resident.iter().copied() {
            if wanted.len() == 16 {
                break;
            }
            if !wanted.contains(&page) && ui.get_can_copy() {
                wanted.push(page);
            }
        }
        wanted.sort_unstable();
        if self.resident != wanted {
            self.resident = wanted;
            self.text.retain(|page, _| self.resident.contains(page));
            engine.text_pages(document, self.resident.clone());
        }
        self.overlays(ui, layout, zoom);
    }

    pub fn overlays(&self, ui: &AppWindow, layout: &Layout, zoom: f32) {
        let scale = zoom * 96.0 / 72.0;
        let mut rectangles = Vec::new();
        let mut append = |page: usize, rect: &TextRect, selected: bool| {
            let Some(surface) = layout.pages.get(page) else {
                return;
            };
            if surface.width <= 0. || surface.height <= 0. {
                return;
            }
            if self.resident.contains(&page)
                || (ui.get_search_visible()
                    && surface.y + surface.height >= -ui.get_scroll_y()
                    && surface.y <= -ui.get_scroll_y() + ui.get_view_height())
            {
                rectangles.push(OverlaySurface {
                    x: surface.x + rect.left * scale,
                    y: surface.y + rect.top * scale,
                    w: (rect.right - rect.left).max(0.0) * scale,
                    h: (rect.bottom - rect.top).max(0.0) * scale,
                    selected,
                });
            }
        };
        if ui.get_search_visible() {
            for (index, hit) in self.hits.iter().enumerate() {
                for rect in &hit.rects {
                    append(hit.page, rect, self.active_hit == Some(index));
                }
            }
        }
        for (page, selection) in &self.selections {
            for rect in &selection.rects {
                append(*page, rect, true);
            }
        }
        ui.set_overlays(ModelRc::new(VecModel::from(rectangles)));
    }

    pub fn thumbnails(&mut self, ui: &AppWindow, engine: &Engine, document: u64, count: usize) {
        let pages = if ui.get_sidebar_visible() && count > 0 {
            let first = ((-ui.get_thumbnail_scroll()).max(0.0) / 196.0) as usize;
            let last = ((-ui.get_thumbnail_scroll() + ui.get_thumbnail_height()).max(0.0) / 196.0)
                as usize;
            (first.saturating_sub(2)..=(last + 2).min(count - 1))
                .take(32)
                .collect()
        } else {
            Vec::new()
        };
        if self.thumb_pages != pages {
            self.thumb_pages = pages;
            self.thumbs
                .retain(|page, _| self.thumb_pages.contains(page));
            engine.thumbnails(document, self.thumb_pages.clone());
            self.publish_thumbnails(ui);
        }
    }

    fn publish_thumbnails(&self, ui: &AppWindow) {
        ui.set_thumbnails(ModelRc::new(VecModel::from(
            self.thumb_pages
                .iter()
                .map(|page| ThumbnailSurface {
                    number: *page as i32 + 1,
                    pixels: self.thumbs.get(page).cloned().unwrap_or_default(),
                })
                .collect::<Vec<_>>(),
        )));
    }
}
