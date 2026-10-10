use crate::{RenderRequest, TileKey};

pub const TILE_SIZE: u32 = 512;
const MARGIN: f32 = 24.0;
const GAP: f32 = 16.0;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReadingMode {
    #[default]
    Continuous,
    Single,
    Facing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

impl PageSize {
    pub fn new(width: f32, height: f32) -> Self {
        let valid = |v: f32| {
            if v.is_finite() && v > 0.0 {
                v.min(100_000.0)
            } else {
                612.0
            }
        };
        Self {
            width: valid(width),
            height: valid(height),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub zoom: f32,
    pub dpi: f32,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub pages: Vec<PageRect>,
    pub width: f32,
    pub height: f32,
    active: std::ops::Range<usize>,
    mode: ReadingMode,
}

pub fn clamp_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() {
        zoom.clamp(0.25, 4.0)
    } else {
        1.0
    }
}

impl Layout {
    pub fn anchor(&self, y: f32) -> (usize, f32) {
        let page = self.page_at(y);
        let fraction = self
            .pages
            .get(page)
            .filter(|p| p.height > 0.0)
            .map(|p| ((y - p.y) / p.height).clamp(0.0, 1.0))
            .unwrap_or(0.0);
        (page, fraction)
    }

    pub fn resolve_anchor(&self, (page, fraction): (usize, f32)) -> f32 {
        self.pages
            .get(page)
            .map(|p| p.y + p.height * fraction)
            .unwrap_or(0.0)
    }

    pub fn new(sizes: &[PageSize], zoom: f32, viewport_width: f32) -> Self {
        let scale = clamp_zoom(zoom) * 96.0 / 72.0;
        let width = sizes
            .iter()
            .map(|s| s.width * scale + MARGIN * 2.0)
            .fold(viewport_width.max(1.0), f32::max);
        let mut y = MARGIN;
        let pages = sizes
            .iter()
            .map(|size| {
                let page = PageRect {
                    x: (width - size.width * scale) / 2.0,
                    y,
                    width: size.width * scale,
                    height: size.height * scale,
                };
                y += page.height + GAP;
                page
            })
            .collect();
        Self {
            pages,
            width,
            height: (y - GAP + MARGIN).max(MARGIN * 2.0),
            active: 0..sizes.len(),
            mode: ReadingMode::Continuous,
        }
    }

    /// Keep document page indexes stable while laying out only the active page or spread.
    pub fn with_mode(
        sizes: &[PageSize],
        zoom: f32,
        viewport_width: f32,
        mode: ReadingMode,
        current_page: usize,
    ) -> Self {
        if mode == ReadingMode::Continuous || sizes.is_empty() {
            return Self::new(sizes, zoom, viewport_width);
        }
        let current = current_page.min(sizes.len() - 1);
        let first = if mode == ReadingMode::Facing {
            current / 2 * 2
        } else {
            current
        };
        let end = (first + if mode == ReadingMode::Facing { 2 } else { 1 }).min(sizes.len());
        let scale = clamp_zoom(zoom) * 96.0 / 72.0;
        let row_width = sizes[first..end]
            .iter()
            .map(|s| s.width * scale)
            .sum::<f32>()
            + GAP * (end - first - 1) as f32;
        let width = viewport_width.max(1.0).max(row_width + 2.0 * MARGIN);
        let mut pages = vec![
            PageRect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0
            };
            sizes.len()
        ];
        let mut x = (width - row_width) / 2.0;
        let mut row_height: f32 = 0.0;
        for page in first..end {
            let size = sizes[page];
            pages[page] = PageRect {
                x,
                y: MARGIN,
                width: size.width * scale,
                height: size.height * scale,
            };
            x += pages[page].width + GAP;
            row_height = row_height.max(pages[page].height);
        }
        Self {
            pages,
            width,
            height: row_height + 2.0 * MARGIN,
            active: first..end,
            mode,
        }
    }

    pub fn page_at(&self, y: f32) -> usize {
        if self.active.is_empty() {
            return 0;
        }
        if self.mode != ReadingMode::Continuous {
            return self.active.start;
        }
        self.pages[self.active.clone()]
            .partition_point(|p| p.y + p.height < y)
            .min(self.active.len().saturating_sub(1))
            + self.active.start
    }

    pub fn visible_pages(&self, view: Viewport) -> std::ops::RangeInclusive<usize> {
        if self.mode != ReadingMode::Continuous && !self.active.is_empty() {
            return self.active.start..=self.active.end - 1;
        }
        self.page_at(view.scroll_y)..=self.page_at(view.scroll_y + view.height)
    }

    pub fn requests(
        &self,
        document: u64,
        generation: u64,
        view: Viewport,
        neighbors: usize,
    ) -> Vec<RenderRequest> {
        if self.pages.is_empty() {
            return Vec::new();
        }
        let dpi = if view.dpi.is_finite() {
            view.dpi.clamp(0.5, 8.0)
        } else {
            1.0
        };
        let scale_milli = (clamp_zoom(view.zoom) * 96.0 / 72.0 * dpi * 1000.0).round() as u32;
        let physical_per_logical =
            scale_milli as f32 / 1000.0 / (clamp_zoom(view.zoom) * 96.0 / 72.0);
        let visible = self.visible_pages(view);
        let first = *visible.start();
        let last = *visible.end();
        let mut requests = Vec::new();
        let visible_first = (first..=last)
            .chain(first.saturating_sub(neighbors)..first)
            .chain((last + 1)..=(last + neighbors).min(self.pages.len() - 1));
        for page in visible_first {
            let rect = self.pages[page];
            if rect.width <= 0.0 || rect.height <= 0.0 {
                continue;
            }
            let prefetch = !visible.contains(&page);
            // Prefetch only the leading screen of a neighbor, never its whole raster.
            let (top, bottom) = if prefetch {
                if page < first {
                    ((rect.height - view.height).max(0.0), rect.height)
                } else {
                    (0.0, view.height.min(rect.height))
                }
            } else {
                (
                    (view.scroll_y - rect.y).max(0.0),
                    (view.scroll_y + view.height - rect.y).clamp(0.0, rect.height),
                )
            };
            let left = (view.scroll_x - rect.x).max(0.0);
            let right = (view.scroll_x + view.width - rect.x).clamp(0.0, rect.width);
            if right <= left || bottom <= top {
                continue;
            }
            let full_w = (rect.width * physical_per_logical).ceil() as u32;
            let full_h = (rect.height * physical_per_logical).ceil() as u32;
            let start_x = (left * physical_per_logical) as u32 / TILE_SIZE;
            let end_x = ((right * physical_per_logical).ceil() as u32).div_ceil(TILE_SIZE);
            let start_y = (top * physical_per_logical) as u32 / TILE_SIZE;
            let end_y = ((bottom * physical_per_logical).ceil() as u32).div_ceil(TILE_SIZE);
            for ty in start_y..end_y {
                for tx in start_x..end_x {
                    let x = tx * TILE_SIZE;
                    let y = ty * TILE_SIZE;
                    if x >= full_w || y >= full_h {
                        continue;
                    }
                    requests.push(RenderRequest {
                        key: TileKey {
                            document,
                            page,
                            scale_milli,
                            rotation: 0,
                            x,
                            y,
                            width: TILE_SIZE.min(full_w - x),
                            height: TILE_SIZE.min(full_h - y),
                        },
                        generation,
                        priority: if prefetch {
                            1000 + page.abs_diff(first) as u32
                        } else {
                            page.abs_diff(first) as u32
                        },
                    });
                    if requests.len() == 256 {
                        return requests;
                    }
                }
            }
        }
        requests
    }
}
