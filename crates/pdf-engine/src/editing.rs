//! Transactional native mutation helpers, used only by the coordinator.
use crate::{DocumentInfo, EditCommand, NativeAnnotation, files};
use mupdf::{
    color::AnnotationColor,
    pdf::{
        AnnotationFlags, InsertPdfOptions, InsertPosition, PageSelection, PdfAnnotation,
        PdfAnnotationType, PdfDocument,
    },
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use viewer_core::{Annotation, PageSize, TextRect};
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub(crate) fn perform(
    doc: &PdfDocument,
    journal: &mut Option<viewer_core::SnapshotHistory>,
    password: Option<&str>,
    revision: u64,
    command: &EditCommand,
    current: impl Fn() -> bool,
) -> Result<(PdfDocument, usize, u64), String> {
    if revision != journal.as_ref().map_or(0, |h| h.revision()) {
        return Err("The document changed; select the annotation or page again".into());
    }
    if matches!(command, EditCommand::Undo | EditCommand::Redo) {
        let h = journal.as_mut().ok_or("There is no edit to undo or redo")?;
        let bytes = if matches!(command, EditCommand::Undo) {
            h.previous()
        } else {
            h.next()
        }
        .ok_or("There is no edit to undo or redo")?;
        let next = restore(bytes, password)?;
        info(&next)?;
        if !current() {
            return Err("Document changed during editing".into());
        }
        if matches!(command, EditCommand::Undo) {
            h.undo();
        } else {
            h.redo();
        }
        return Ok((next, 0, h.revision()));
    }
    let original = serialize(doc)?;
    let mut next = restore(&original, password)?;
    let focus = mutate(&mut next, command)?;
    info(&next)?;
    let bytes = serialize(&next)?;
    if !current() {
        return Err("Document changed during editing".into());
    }
    let revision = if let Some(h) = journal.as_mut() {
        h.push(bytes)?;
        h.revision()
    } else {
        let mut h = viewer_core::SnapshotHistory::new(original)?;
        h.push(bytes)?;
        let revision = h.revision();
        *journal = Some(h);
        revision
    };
    Ok((next, focus, revision))
}
pub(crate) fn serialize(doc: &PdfDocument) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    doc.write_to_with_options(&mut bytes, files::options())
        .map_err(error)?;
    Ok(bytes)
}
pub(crate) fn restore(bytes: &[u8], password: Option<&str>) -> Result<PdfDocument, String> {
    let mut doc = PdfDocument::from_bytes(bytes).map_err(error)?;
    if doc.needs_password().map_err(error)?
        && !doc.authenticate(password.unwrap_or("")).map_err(error)?
    {
        return Err("Could not authenticate PDF snapshot".into());
    }
    Ok(doc)
}
pub(crate) fn info(doc: &PdfDocument) -> Result<DocumentInfo, String> {
    let pages = doc.page_count().map_err(error)?;
    if !(1..=100_000).contains(&pages) {
        return Err("Editing supports 1–100,000 pages".into());
    }
    let bounds = doc.load_page(0).and_then(|p| p.bounds()).map_err(error)?;
    let permissions = doc.permissions();
    Ok(DocumentInfo {
        pages: pages as usize,
        first_page: PageSize::new(bounds.width(), bounds.height()),
        can_copy: permissions.contains(mupdf::pdf::Permission::COPY),
        can_annotate: permissions.contains(mupdf::pdf::Permission::ANNOTATE),
        can_assemble: permissions.contains(mupdf::pdf::Permission::ASSEMBLE),
    })
}
fn unlocked(a: &PdfAnnotation) -> Result<bool, String> {
    let flags = a.flags().map_err(error)?;
    Ok(!flags.intersects(
        AnnotationFlags::IS_READ_ONLY
            | AnnotationFlags::IS_LOCKED
            | AnnotationFlags::IS_LOCKED_CONTENTS,
    ))
}
fn rgb(a: &PdfAnnotation) -> Result<[f32; 3], String> {
    Ok(match a.color().map_err(error)? {
        Some(AnnotationColor::Rgb { red, green, blue }) => [red, green, blue],
        Some(AnnotationColor::Gray(g)) => [g, g, g],
        Some(AnnotationColor::Cmyk {
            cyan,
            magenta,
            yellow,
            key,
        }) => [
            1.0 - (cyan + key).min(1.0),
            1.0 - (magenta + key).min(1.0),
            1.0 - (yellow + key).min(1.0),
        ],
        _ => [1., 0.85, 0.],
    })
}
pub(crate) fn annotations(
    doc: &PdfDocument,
    page: usize,
) -> Result<(Vec<NativeAnnotation>, bool), String> {
    let native = doc.load_pdf_page(page as i32).map_err(error)?;
    let b = native.bounds().map_err(error)?;
    let mut result = Vec::new();
    let mut bytes = 0;
    let mut truncated = false;
    for (ordinal, a) in native.annotations().enumerate() {
        if ordinal >= 10_000 || result.len() >= 256 || bytes >= 256 * 1024 {
            truncated = true;
            break;
        }
        let partial;
        let annotation = match a.r#type().map_err(error)? {
            PdfAnnotationType::Text => {
                let r = a.rect().map_err(error)?;
                let text = a.contents().map_err(error)?.unwrap_or("");
                let mut end = text.len().min(16_384).min(256 * 1024 - bytes);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                truncated |= end < text.len();
                partial = end < text.len();
                bytes += end;
                Annotation::Note {
                    page,
                    x: r.x0 - b.x0,
                    y: r.y0 - b.y0,
                    text: text[..end].into(),
                }
            }
            PdfAnnotationType::Highlight => {
                if a.quad_count().map_err(error)? > 1000 {
                    truncated = true;
                    continue;
                }
                let quads = a.quad_points().map_err(error)?;
                let remaining = (256 * 1024 - bytes) / std::mem::size_of::<TextRect>();
                let take = quads.len().min(1000).min(remaining);
                truncated |= take < quads.len();
                partial = take < quads.len();
                bytes += take * std::mem::size_of::<TextRect>();
                let rects = quads
                    .iter()
                    .take(take)
                    .map(|q| {
                        let ps = [q.ul, q.ur, q.ll, q.lr];
                        TextRect {
                            left: ps.iter().map(|p| p.x - b.x0).fold(f32::INFINITY, f32::min),
                            top: ps.iter().map(|p| p.y - b.y0).fold(f32::INFINITY, f32::min),
                            right: ps
                                .iter()
                                .map(|p| p.x - b.x0)
                                .fold(f32::NEG_INFINITY, f32::max),
                            bottom: ps
                                .iter()
                                .map(|p| p.y - b.y0)
                                .fold(f32::NEG_INFINITY, f32::max),
                        }
                    })
                    .collect();
                Annotation::Highlight { page, rects }
            }
            _ => continue,
        };
        result.push(NativeAnnotation {
            ordinal,
            annotation,
            rgb: rgb(&a)?,
            opacity: a.opacity().map_err(error)?,
            editable: !partial
                && unlocked(&a)?
                && doc.permissions().contains(mupdf::pdf::Permission::ANNOTATE),
        });
    }
    Ok((result, truncated))
}
fn selected(pages: &[usize], count: usize) -> Result<Vec<usize>, String> {
    if pages.is_empty() || pages.len() > 100_000 {
        return Err("Select at least one page".into());
    }
    let mut seen = HashSet::new();
    for &p in pages {
        if p >= count || !seen.insert(p) {
            return Err("Invalid or duplicate page selection".into());
        }
    }
    let mut result = pages.to_vec();
    result.sort_unstable();
    Ok(result)
}
pub(crate) fn mutate(doc: &mut PdfDocument, command: &EditCommand) -> Result<usize, String> {
    let info = info(doc)?;
    if matches!(command,EditCommand::UpdateNote{page,ordinal,..}|EditCommand::Color{page,ordinal,..}|EditCommand::DeleteAnnotation{page,ordinal} if *page>=info.pages||*ordinal>=10_000)
    {
        return Err("Annotation page is outside the document".into());
    }
    let annotate = matches!(
        command,
        EditCommand::Add(_)
            | EditCommand::UpdateNote { .. }
            | EditCommand::Color { .. }
            | EditCommand::DeleteAnnotation { .. }
    );
    if annotate && !info.can_annotate {
        return Err("This PDF does not permit annotations".into());
    }
    if !annotate && !info.can_assemble {
        return Err("This PDF does not permit page assembly".into());
    }
    match command {
        EditCommand::Add(annotation) => {
            files::apply(doc, std::slice::from_ref(annotation))?;
            Ok(annotation.page())
        }
        EditCommand::UpdateNote {
            page,
            ordinal,
            text,
        } => {
            if text.trim().is_empty() || text.len() > 16_384 || text.contains('\0') {
                return Err("Notes need 1–16,384 UTF-8 bytes of text".into());
            }
            let native = doc.load_pdf_page(*page as i32).map_err(error)?;
            let mut a = native
                .annotations()
                .nth(*ordinal)
                .ok_or("Annotation no longer exists")?;
            if a.r#type().map_err(error)? != PdfAnnotationType::Text || !unlocked(&a)? {
                return Err("This annotation cannot be edited".into());
            }
            a.set_contents(text).map_err(error)?;
            a.update().map_err(error)?;
            Ok(*page)
        }
        EditCommand::Color { page, ordinal, rgb } => {
            if rgb
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            {
                return Err("Invalid annotation color".into());
            }
            let native = doc.load_pdf_page(*page as i32).map_err(error)?;
            let mut a = native
                .annotations()
                .nth(*ordinal)
                .ok_or("Annotation no longer exists")?;
            if !unlocked(&a)?
                || !matches!(
                    a.r#type().map_err(error)?,
                    PdfAnnotationType::Text | PdfAnnotationType::Highlight
                )
            {
                return Err("This annotation cannot be edited".into());
            }
            a.set_color(AnnotationColor::Rgb {
                red: rgb[0],
                green: rgb[1],
                blue: rgb[2],
            })
            .map_err(error)?;
            a.update().map_err(error)?;
            Ok(*page)
        }
        EditCommand::DeleteAnnotation { page, ordinal } => {
            let mut native = doc.load_pdf_page(*page as i32).map_err(error)?;
            let a = native
                .annotations()
                .nth(*ordinal)
                .ok_or("Annotation no longer exists")?;
            if !unlocked(&a)?
                || !matches!(
                    a.r#type().map_err(error)?,
                    PdfAnnotationType::Text | PdfAnnotationType::Highlight
                )
            {
                return Err("This annotation cannot be deleted".into());
            }
            native.delete_annotation(a).map_err(error)?;
            Ok(*page)
        }
        EditCommand::Rotate { pages, degrees } => {
            if degrees % 90 != 0 {
                return Err("Rotate by a multiple of 90 degrees".into());
            }
            let pages = selected(pages, info.pages)?;
            for &p in &pages {
                let mut page = doc.load_pdf_page(p as i32).map_err(error)?;
                let angle = page
                    .rotation()
                    .map_err(error)?
                    .rem_euclid(360)
                    .saturating_add(degrees.rem_euclid(360))
                    .rem_euclid(360);
                page.set_rotation(angle).map_err(error)?;
            }
            Ok(pages[0])
        }
        EditCommand::Move { page, to } => {
            doc.move_page(*page, *to).map_err(error)?;
            Ok(*to)
        }
        EditCommand::DeletePages { pages } => {
            let pages = selected(pages, info.pages)?;
            if pages.len() == info.pages {
                return Err("A PDF must keep at least one page".into());
            }
            let focus = pages[0].min(info.pages - pages.len() - 1);
            doc.delete_pages(pages).map_err(error)?;
            Ok(focus)
        }
        EditCommand::Blank { at } => {
            if *at > info.pages || info.pages >= 100_000 {
                return Err("Invalid insertion position or page limit".into());
            }
            let b = doc
                .load_page(at.saturating_sub(1).min(info.pages - 1) as i32)
                .and_then(|p| p.bounds())
                .map_err(error)?;
            doc.new_page_at(*at as i32, (b.width(), b.height()))
                .map_err(error)?;
            Ok(*at)
        }
        EditCommand::Insert { paths, at } => {
            insert(doc, paths, *at)?;
            Ok(*at)
        }
        EditCommand::Undo | EditCommand::Redo => {
            Err("History commands must use the coordinator journal".into())
        }
    }
}
fn insert(doc: &mut PdfDocument, paths: &[PathBuf], at: usize) -> Result<(), String> {
    let count = doc.page_count().map_err(error)? as usize;
    if paths.is_empty() || paths.len() > 16 || at > count {
        return Err("Choose 1–16 PDF files and a valid insertion position".into());
    }
    let mut sources = Vec::new();
    let mut total = count;
    for path in paths {
        let source = files::open(path, None)?;
        if !source.permissions().contains(mupdf::pdf::Permission::COPY) {
            return Err("An imported PDF does not permit copying".into());
        }
        let pages = source.page_count().map_err(error)?;
        if pages <= 0 {
            return Err("Imported PDF has no pages".into());
        }
        total = total
            .checked_add(pages as usize)
            .ok_or("Page limit exceeded")?;
        if total > 100_000 {
            return Err("Editing is limited to 100,000 pages".into());
        }
        for p in 0..pages {
            let page = source.load_pdf_page(p).map_err(error)?;
            if let Some(annots) = page.object().get_dict("Annots").map_err(error)? {
                if annots.len().map_err(error)? > 2000 {
                    return Err("Imported page has too many annotations".into());
                }
                for i in 0..annots.len().map_err(error)? {
                    let a = annots
                        .get_array(i as i32)
                        .map_err(error)?
                        .ok_or("Invalid annotation array")?;
                    let subtype = a
                        .get_dict("Subtype")
                        .map_err(error)?
                        .ok_or("Annotation subtype missing")?
                        .as_name()
                        .map_err(error)?;
                    if !matches!(subtype.as_slice(), b"Text" | b"Highlight") {
                        return Err("Import supports text notes and highlights; links, forms, popups and other annotations are not supported".into());
                    }
                }
            }
            for a in page.annotations() {
                if a.quad_count().unwrap_or(0) > 1000
                    || a.contents()
                        .map_err(error)?
                        .is_some_and(|s| s.len() > 16_384)
                {
                    return Err("Imported annotation exceeds reader limits".into());
                }
            }
        }
        sources.push(source);
    }
    let mut target = at;
    for source in sources {
        let pages = source.page_count().map_err(error)?;
        for p in 0..pages {
            doc.insert_pdf(
                &source,
                InsertPdfOptions {
                    source_pages: PageSelection::Pages(vec![p as usize]),
                    target: InsertPosition::Before(target),
                    ..Default::default()
                },
            )
            .map_err(error)?;
            let original = source.load_pdf_page(p).map_err(error)?;
            let mut destination = doc.load_pdf_page(target as i32).map_err(error)?;
            for a in original.annotations() {
                let kind = a.r#type().map_err(error)?;
                let mut copy = destination.create_annotation(kind).map_err(error)?;
                match kind {
                    PdfAnnotationType::Highlight => copy
                        .set_quad_points(a.quad_points().map_err(error)?)
                        .map_err(error)?,
                    PdfAnnotationType::Text => {
                        copy.set_rect(a.rect().map_err(error)?).map_err(error)?;
                        if let Some(icon) = a.icon_name().map_err(error)? {
                            copy.set_icon_name(icon).map_err(error)?;
                        }
                    }
                    _ => return Err("Unsupported imported annotation".into()),
                };
                if let Some(text) = a.contents().map_err(error)? {
                    copy.set_contents(text).map_err(error)?;
                }
                if let Some(color) = a.color().map_err(error)? {
                    copy.set_color(color).map_err(error)?;
                }
                copy.set_opacity(a.opacity().map_err(error)?)
                    .map_err(error)?;
                if let Some(author) = a.author().map_err(error)? {
                    copy.set_author(author).map_err(error)?;
                }
                copy.set_flags(a.flags().map_err(error)?).map_err(error)?;
                copy.update().map_err(error)?;
            }
            target += 1;
        }
    }
    Ok(())
}
fn export_rights(doc: &PdfDocument) -> Result<(), String> {
    info(doc)?;
    let p = doc.permissions();
    if !p.contains(mupdf::pdf::Permission::COPY | mupdf::pdf::Permission::ASSEMBLE) {
        return Err("This PDF does not permit page extraction".into());
    }
    Ok(())
}
pub(crate) fn extract(
    doc: &PdfDocument,
    source: &Path,
    password: Option<&str>,
    target: &Path,
    pages: &[usize],
    current: impl Fn() -> bool,
) -> Result<PathBuf, String> {
    export_rights(doc)?;
    let pages = selected(pages, doc.page_count().map_err(error)? as usize)?;
    let mut copy = restore(&serialize(doc)?, password)?;
    copy.select_pages(pages).map_err(error)?;
    // Exporting a subset cannot keep outline destinations that target removed pages.
    copy.delete_outlines().map_err(error)?;
    let mut options = files::options();
    options.set_garbage(true);
    let mut bytes = Vec::new();
    copy.write_to_with_options(&mut bytes, options)
        .map_err(error)?;
    let copy = restore(&bytes, password)?;
    files::save(&copy, source, password, target, &[], current)
}
static EXPORT_ID: AtomicU64 = AtomicU64::new(0);
struct ExportStage(PathBuf);
impl Drop for ExportStage {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub(crate) fn split(
    doc: &PdfDocument,
    source: &Path,
    password: Option<&str>,
    directory: &Path,
    after: usize,
    current: impl Fn() -> bool,
) -> Result<PathBuf, String> {
    export_rights(doc)?;
    let count = doc.page_count().map_err(error)? as usize;
    if after >= count.saturating_sub(1) {
        return Err("Split must leave pages in both PDFs".into());
    }
    let directory = std::fs::canonicalize(directory).map_err(error)?;
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid PDF filename")?;
    let targets = [
        directory.join(format!("{stem}-part-1.pdf")),
        directory.join(format!("{stem}-part-2.pdf")),
    ];
    for path in &targets {
        if std::fs::symlink_metadata(path).is_ok() {
            return Err("A split output already exists; no files were created".into());
        }
    }
    let mut stages = Vec::new();
    for pages in [
        (0..=after).collect::<Vec<_>>(),
        ((after + 1)..count).collect::<Vec<_>>(),
    ] {
        let path = directory.join(format!(
            ".pdfree-export-{}-{}.pdf",
            std::process::id(),
            EXPORT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let staged = extract(doc, source, password, &path, &pages, &current)?;
        stages.push(ExportStage(staged));
    }
    let mut published = Vec::new();
    for (stage, target) in stages.iter().zip(&targets) {
        if !current() {
            for p in published {
                let _ = std::fs::remove_file(p);
            }
            return Err("Document changed before split completed".into());
        }
        if let Err(e) = files::move_no_replace(&stage.0, target) {
            let mut failed = Vec::new();
            for p in published {
                if std::fs::remove_file(&p).is_err() {
                    failed.push(p);
                }
            }
            return Err(format!(
                "Split failed: {e}. Remaining created files: {failed:?}"
            ));
        }
        published.push(target.clone());
    }
    Ok(directory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_inherits_preceding_page_dimensions_in_mixed_documents() {
        let mut doc = PdfDocument::new();
        doc.new_page((300., 400.)).unwrap();
        doc.new_page((800., 600.)).unwrap();
        mutate(&mut doc, &EditCommand::Blank { at: 1 }).unwrap();
        let bounds = doc.load_page(1).unwrap().bounds().unwrap();
        assert_eq!((bounds.width(), bounds.height()), (300., 400.));
    }
}
