use crate::{Event, Shared};
use mupdf::{Document, Quad, TextPageFlags, text_page::SearchHitResponse};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, atomic::Ordering},
};
use viewer_core::{PageText, PixelTile, SearchHit, TextGlyph, TextRect, TileKey};

const MAX_GLYPHS: usize = 100_000;
const MAX_HITS: usize = 10_000;

#[derive(Default)]
pub(crate) struct Reader {
    text: VecDeque<usize>,
    text_cache: HashMap<usize, Arc<PageText>>,
    thumbnails: VecDeque<usize>,
    thumbnail_cache: HashMap<usize, (TileKey, Arc<PixelTile>)>,
    thumbnail_wanted: HashSet<usize>,
    search: Option<Search>,
}
struct Search {
    id: u64,
    query: String,
    cursor: usize,
    hits: usize,
    errors: usize,
}

impl Reader {
    pub fn text_pages(&mut self, pages: Vec<usize>, count: usize) {
        let pages: Vec<_> = pages.into_iter().filter(|p| *p < count).take(16).collect();
        self.text_cache.retain(|p, _| pages.contains(p));
        // Every latest-value command republishes its wanted snapshots. A leave/reenter
        // transition may have been coalesced away after the UI dropped its reference.
        self.text = pages.into();
    }
    pub fn thumbnails(&mut self, pages: Vec<usize>, count: usize) {
        self.thumbnail_wanted = pages.into_iter().filter(|p| *p < count).take(32).collect();
        self.thumbnail_cache
            .retain(|p, _| self.thumbnail_wanted.contains(p));
        self.thumbnails = self.thumbnail_wanted.iter().copied().collect();
        self.thumbnails.make_contiguous().sort_unstable();
    }
    pub fn next_thumbnail(&mut self) -> Option<usize> {
        self.thumbnails.pop_front()
    }
    pub fn cached_thumbnail(&self, page: usize) -> Option<(TileKey, Arc<PixelTile>)> {
        self.thumbnail_cache.get(&page).cloned()
    }
    pub fn thumbnail_wanted(&self, page: usize) -> bool {
        self.thumbnail_wanted.contains(&page)
    }
    pub fn thumbnail_finished(
        &mut self,
        key: TileKey,
        pixels: PixelTile,
    ) -> Option<Arc<PixelTile>> {
        if !self.thumbnail_wanted(key.page) {
            return None;
        }
        let pixels = Arc::new(pixels);
        self.thumbnail_cache.insert(key.page, (key, pixels.clone()));
        Some(pixels)
    }
    pub fn search(&mut self, id: u64, mut query: String) {
        if query.len() > 512 || query.contains('\0') {
            query.clear();
        }
        self.search = Some(Search {
            id,
            query,
            cursor: 0,
            hits: 0,
            errors: 0,
        });
    }
    pub fn tick(
        &mut self,
        doc: &Document,
        document: u64,
        count: usize,
        can_copy: bool,
        shared: &Shared,
    ) -> Option<Event> {
        if let Some(page) = self.text.pop_front() {
            if let Some(text) = self.text_cache.get(&page) {
                return Some(Event::Text {
                    document,
                    page,
                    text: text.clone(),
                });
            }
            let result = if can_copy {
                extract(doc, page)
            } else {
                Ok(PageText::default())
            };
            return Some(match result {
                Ok(text) => {
                    let text = Arc::new(text);
                    self.text_cache.insert(page, text.clone());
                    Event::Text {
                        document,
                        page,
                        text,
                    }
                }
                Err(error) => Event::ReaderError {
                    document,
                    operation: "text",
                    page: Some(page),
                    message: error.to_string(),
                },
            });
        }
        let task = self.search.as_mut()?;
        if shared.search.load(Ordering::Acquire) != task.id || !shared.current_document(document) {
            self.search = None;
            return None;
        }
        let page = task.cursor;
        let mut hits = Vec::new();
        if !task.query.is_empty() && page < count {
            let result = doc.load_page(page as i32).and_then(|p| {
                let bounds = p.bounds()?;
                let text = p.to_text_page(TextPageFlags::empty())?;
                text.search_cb(&task.query, &mut hits, |hits, quads| {
                    if !shared.current_document(document)
                        || shared.search.load(Ordering::Acquire) != task.id
                        || task.hits + hits.len() >= MAX_HITS
                    {
                        return SearchHitResponse::AbortSearch;
                    }
                    hits.push(SearchHit {
                        page,
                        rects: quads
                            .iter()
                            .map(|q| rect(q, bounds.x0, bounds.y0))
                            .collect(),
                    });
                    SearchHitResponse::ContinueSearch
                })?;
                Ok(())
            });
            if result.is_err() {
                task.errors += 1;
            }
            task.cursor += 1;
        }
        task.hits += hits.len();
        let truncated = task.hits >= MAX_HITS;
        let done = task.query.is_empty() || task.cursor >= count || truncated;
        let search_id = task.id;
        let event = Event::SearchPage {
            document,
            search: search_id,
            page,
            hits,
            scanned: task.cursor,
            total: count,
            done,
            truncated,
            errors: task.errors,
        };
        if done {
            self.search = None;
        }
        if shared.current_document(document) && shared.search.load(Ordering::Acquire) == search_id {
            Some(event)
        } else {
            None
        }
    }
}

fn rect(quad: &Quad, x: f32, y: f32) -> TextRect {
    let points = [quad.ul, quad.ur, quad.ll, quad.lr];
    TextRect {
        left: points.iter().map(|p| p.x - x).fold(f32::INFINITY, f32::min),
        top: points.iter().map(|p| p.y - y).fold(f32::INFINITY, f32::min),
        right: points
            .iter()
            .map(|p| p.x - x)
            .fold(f32::NEG_INFINITY, f32::max),
        bottom: points
            .iter()
            .map(|p| p.y - y)
            .fold(f32::NEG_INFINITY, f32::max),
    }
}

fn extract(doc: &Document, page: usize) -> Result<PageText, mupdf::Error> {
    let page = doc.load_page(page as i32)?;
    let bounds = page.bounds()?;
    let text = page.to_text_page(TextPageFlags::empty())?;
    let mut output = PageText::default();
    let mut line_index = 0;
    for block in text.blocks() {
        for line in block.lines() {
            for ch in line.chars() {
                if output.glyphs.len() == MAX_GLYPHS {
                    output.truncated = true;
                    return Ok(output);
                }
                if let Some(ch_value) = ch.char() {
                    output.glyphs.push(TextGlyph {
                        ch: ch_value,
                        line: line_index,
                        bounds: rect(&ch.quad(), bounds.x0, bounds.y0),
                    });
                }
            }
            line_index += 1;
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_snapshot_caches_retain_only_bounded_wanted_pages() {
        let mut reader = Reader::default();
        for page in 0..100 {
            reader
                .text_cache
                .insert(page, Arc::new(PageText::default()));
            let key = TileKey {
                document: 1,
                page,
                scale_milli: 100,
                rotation: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            };
            reader.thumbnail_cache.insert(
                page,
                (
                    key,
                    Arc::new(PixelTile {
                        width: 1,
                        height: 1,
                        rgba: vec![255; 4],
                    }),
                ),
            );
        }
        reader.text_pages((0..100).collect(), 100);
        reader.thumbnails((0..100).collect(), 100);
        assert_eq!(reader.text_cache.len(), 16);
        assert_eq!(reader.thumbnail_cache.len(), 32);
        assert_eq!(reader.text.len(), 16);
        assert_eq!(reader.thumbnails.len(), 32);
        reader.text_pages(vec![0], 100);
        reader.thumbnails(vec![0], 100);
        assert_eq!(reader.text_cache.len(), 1);
        assert_eq!(reader.thumbnail_cache.len(), 1);
        assert_eq!(reader.text.pop_front(), Some(0));
        assert_eq!(reader.next_thumbnail(), Some(0));
        assert!(reader.cached_thumbnail(0).is_some());
    }
}
