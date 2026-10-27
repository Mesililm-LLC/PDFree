use crate::{AppWindow, BookmarkItem, app::State};
use pdf_engine::{Engine, Event};
use slint::{ModelRc, VecModel};
use std::collections::HashMap;
use viewer_core::{LinkDestination, OutlineEntry, PageLink};

#[derive(Default)]
pub struct Navigation {
    entries: Vec<OutlineEntry>,
    links: HashMap<usize, Vec<PageLink>>,
    wanted: Vec<usize>,
}
impl Navigation {
    #[cfg(test)]
    pub fn test_links(&self, page: usize) -> Option<Vec<PageLink>> {
        self.links.get(&page).cloned()
    }
    pub fn reset(&mut self, ui: &AppWindow) {
        *self = Self::default();
        ui.set_link_hover(false);
        ui.set_bookmarks(ModelRc::default());
        ui.set_bookmark_status("Loading bookmarks…".into());
    }
    pub fn refresh(&mut self, engine: &Engine, document: u64, revision: u64, pages: &[usize]) {
        let wanted = pages.iter().copied().take(16).collect::<Vec<_>>();
        if self.wanted != wanted {
            self.wanted = wanted;
            self.links.retain(|p, _| self.wanted.contains(p));
            engine.links_pages(document, revision, self.wanted.clone());
        }
    }
    pub fn event(&mut self, ui: &AppWindow, document: u64, revision: u64, event: &Event) {
        match event {
            Event::Outlines {
                document: id,
                revision: rev,
                entries,
                truncated,
            } if *id == document && *rev == revision => {
                self.entries = entries.clone();
                ui.set_bookmarks(ModelRc::new(VecModel::from(
                    entries
                        .iter()
                        .enumerate()
                        .map(|(index, e)| BookmarkItem {
                            index: index as i32,
                            title: format!("{}{}", "  ".repeat(e.depth.min(16)), e.title).into(),
                        })
                        .collect::<Vec<_>>(),
                )));
                ui.set_bookmark_status(
                    if *truncated {
                        "Bookmark limit reached"
                    } else if entries.is_empty() {
                        "No bookmarks in this PDF"
                    } else {
                        ""
                    }
                    .into(),
                );
            }
            Event::Links {
                document: id,
                revision: rev,
                page,
                links,
                ..
            } if *id == document && *rev == revision && self.wanted.contains(page) => {
                self.links.insert(*page, links.clone());
            }
            _ => {}
        }
    }
}
impl State {
    pub fn hover_link(&self, ui: &AppWindow, page: usize, x: f32, y: f32) {
        let scale = self.viewport.zoom * 96. / 72.;
        let (x, y) = (x / scale, y / scale);
        ui.set_link_hover(self.navigation.links.get(&page).is_some_and(|links| {
            links.iter().any(|l| {
                x >= l.bounds.left
                    && x <= l.bounds.right
                    && y >= l.bounds.top
                    && y <= l.bounds.bottom
            })
        }));
    }

    pub fn activate_bookmark(&mut self, ui: &AppWindow, index: usize) {
        if let Some(entry) = self.navigation.entries.get(index) {
            self.follow_destination(ui, entry.destination.clone());
        }
    }
    pub fn activate_link(&mut self, ui: &AppWindow, page: usize, x: f32, y: f32) {
        if ui.get_note_placement()
            || ui.get_note_visible()
            || ui.get_pending_open()
            || ui.get_file_busy()
        {
            return;
        }
        let scale = self.viewport.zoom * 96. / 72.;
        let (x, y) = (x / scale, y / scale);
        if let Some(link) = self.navigation.links.get(&page).and_then(|links| {
            links.iter().find(|l| {
                x >= l.bounds.left
                    && x <= l.bounds.right
                    && y >= l.bounds.top
                    && y <= l.bounds.bottom
            })
        }) {
            self.follow_destination(ui, link.destination.clone());
        }
    }
    fn follow_destination(&mut self, ui: &AppWindow, destination: LinkDestination) {
        match destination {
            LinkDestination::Internal { page, x, y } => {
                self.tools.clear_selection(ui);
                self.navigate(ui, page);
                if let Some(rect) = self.viewport.layout.pages.get(page) {
                    ui.set_scroll_y(-(rect.y + y * self.viewport.zoom * 96. / 72.).clamp(
                        0.,
                        (self.viewport.layout.height - ui.get_view_height()).max(0.),
                    ));
                    ui.set_scroll_x(-(rect.x + x * self.viewport.zoom * 96. / 72.).clamp(
                        0.,
                        (self.viewport.layout.width - ui.get_view_width()).max(0.),
                    ));
                }
            }
            LinkDestination::External(url) => {
                // Only web links may invoke the browser. No PDF launch/file actions.
                if url.to_ascii_lowercase().starts_with("https://")
                    || url.to_ascii_lowercase().starts_with("http://")
                {
                    std::thread::spawn(move || {
                        let _ = std::process::Command::new("rundll32.exe")
                            .arg("url.dll,FileProtocolHandler")
                            .arg(url)
                            .spawn();
                    });
                }
            }
            LinkDestination::Unsupported(reason) => {
                ui.set_file_message(format!("This link cannot be opened: {reason}").into())
            }
        }
    }
}
