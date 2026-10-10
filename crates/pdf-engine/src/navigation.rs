//! Coordinator-owned navigation extraction; native values never cross this boundary.
use crate::{Event, Shared};
use mupdf::{DestinationKind, Document};
use std::collections::VecDeque;
use viewer_core::{LinkDestination, OutlineEntry, PageLink, TextRect};

#[derive(Default)]
pub(crate) struct Navigation {
    outlines: bool,
    pages: VecDeque<usize>,
}
impl Navigation {
    pub fn request_outlines(&mut self) {
        self.outlines = true;
    }
    pub fn request_links(&mut self, pages: Vec<usize>, count: usize) {
        self.pages = pages.into_iter().filter(|p| *p < count).take(16).collect();
    }
    pub fn tick(
        &mut self,
        doc: &Document,
        document: u64,
        revision: u64,
        shared: &Shared,
    ) -> Option<Event> {
        if !shared.current_document(document) {
            return None;
        }
        let result = if std::mem::take(&mut self.outlines) {
            outlines(doc).map(|(entries, truncated)| Event::Outlines {
                document,
                revision,
                entries,
                truncated,
            })
        } else {
            let page = self.pages.pop_front()?;
            links(doc, page).map(|(links, truncated)| Event::Links {
                document,
                revision,
                page,
                links,
                truncated,
            })
        };
        if !shared.current_document(document) {
            return None;
        }
        Some(result.unwrap_or_else(|message| Event::ReaderError {
            document,
            operation: "navigation",
            page: None,
            message,
        }))
    }
}
fn limited(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}
fn destination(
    doc: &Document,
    dest: Option<mupdf::link::LinkDestination>,
    uri: &str,
) -> LinkDestination {
    if let Some(dest) = dest {
        let page = dest.loc.page_number as usize;
        let (x, y) = match dest.kind {
            DestinationKind::XYZ { left, top, .. } => (left.unwrap_or(0.), top.unwrap_or(0.)),
            DestinationKind::FitH { top } | DestinationKind::FitBH { top } => {
                (0., top.unwrap_or(0.))
            }
            DestinationKind::FitV { left } | DestinationKind::FitBV { left } => {
                (left.unwrap_or(0.), 0.)
            }
            DestinationKind::FitR { left, top, .. } => (left, top),
            _ => (0., 0.),
        };
        // Destination positions use MuPDF user space, like page link rectangles.
        if let Ok(bounds) = doc.load_page(page as i32).and_then(|p| p.bounds()) {
            return LinkDestination::Internal {
                page,
                x: (x - bounds.x0).max(0.),
                y: (y - bounds.y0).max(0.),
            };
        }
    }
    if uri.len() <= 8192 && !uri.chars().any(char::is_control) {
        let lower = uri.to_ascii_lowercase();
        if lower.starts_with("http://") || lower.starts_with("https://") {
            return LinkDestination::External(uri.into());
        }
    }
    LinkDestination::Unsupported(limited(uri, 256))
}
pub(crate) fn outlines(doc: &Document) -> Result<(Vec<OutlineEntry>, bool), String> {
    let tree = doc.outlines().map_err(|e| e.to_string())?;
    let mut stack: Vec<_> = tree.iter().rev().map(|n| (n, 0)).collect();
    let mut entries = Vec::new();
    let mut truncated = false;
    while let Some((node, depth)) = stack.pop() {
        if entries.len() == 4096 {
            truncated = true;
            break;
        }
        entries.push(OutlineEntry {
            title: limited(&node.title, 512),
            depth,
            destination: destination(doc, node.dest, node.uri.as_deref().unwrap_or("")),
        });
        if depth < 32 {
            stack.extend(node.down.iter().rev().map(|n| (n, depth + 1)));
        } else if !node.down.is_empty() {
            truncated = true;
        }
    }
    Ok((entries, truncated))
}
pub(crate) fn links(doc: &Document, page: usize) -> Result<(Vec<PageLink>, bool), String> {
    let page = doc.load_page(page as i32).map_err(|e| e.to_string())?;
    let bounds = page.bounds().map_err(|e| e.to_string())?;
    // The published iterator internally unwraps destination parsing. Contain malformed
    // link failures so a bad action cannot terminate the coordinator thread.
    let extracted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut result = Vec::new();
        let mut truncated = false;
        for (index, link) in page.links().map_err(|e| e.to_string())?.enumerate() {
            if index == 1024 {
                truncated = true;
                break;
            }
            let b = link.bounds;
            if ![b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite()) {
                continue;
            }
            let rect = TextRect {
                left: (b.x0 - bounds.x0).max(0.),
                top: (b.y0 - bounds.y0).max(0.),
                right: (b.x1 - bounds.x0).min(bounds.width()),
                bottom: (b.y1 - bounds.y0).min(bounds.height()),
            };
            if rect.right <= rect.left || rect.bottom <= rect.top {
                continue;
            }
            result.push(PageLink {
                bounds: rect,
                destination: destination(doc, link.dest, &link.uri),
            });
        }
        Ok((result, truncated))
    }));
    extracted.unwrap_or_else(|_| Err("Malformed PDF link destination".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_actions_are_restricted() {
        let doc = mupdf::pdf::PdfDocument::new();
        for uri in [
            "file:///x",
            "javascript:alert(1)",
            "mailto:x@y",
            "https://x\n",
        ] {
            assert!(matches!(
                destination(&doc, None, uri),
                LinkDestination::Unsupported(_)
            ));
        }
        assert_eq!(
            destination(&doc, None, "https://example.org/"),
            LinkDestination::External("https://example.org/".into())
        );
    }
    #[test]
    fn owned_fixture_outlines_links_and_crop_coordinates() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/corpus/navigation.pdf");
        let doc = Document::open(path.to_str().unwrap()).unwrap();
        let (entries, truncated) = outlines(&doc).unwrap();
        assert!(!truncated);
        assert_eq!(
            entries.iter().map(|e| e.depth).collect::<Vec<_>>(),
            vec![0, 1, 0]
        );
        assert!(matches!(
            entries[1].destination,
            LinkDestination::Internal { page: 1, .. }
        ));
        let (links, truncated) = links(&doc, 0).unwrap();
        assert!(!truncated);
        assert_eq!(links.len(), 3);
        assert!(
            links
                .iter()
                .any(|l| matches!(l.destination, LinkDestination::Internal { page: 1, .. }))
        );
        assert!(
            links
                .iter()
                .any(|l| matches!(l.destination, LinkDestination::External(_)))
        );
        assert!(
            links
                .iter()
                .any(|l| matches!(l.destination, LinkDestination::Unsupported(_)))
        );
        let (cropped, _) = super::links(&doc, 1).unwrap();
        assert_eq!(cropped.len(), 1);
        assert!((cropped[0].bounds.left - 10.).abs() < 0.01);
        assert!((cropped[0].bounds.top - 132.).abs() < 0.01);
    }
}
