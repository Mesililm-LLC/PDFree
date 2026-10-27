//! Transactional native mutation helpers, used only by the coordinator.
use crate::{DocumentInfo, EditCommand, NativeAnnotation, files};
use mupdf::{
    color::AnnotationColor,
    pdf::{
        AnnotationFlags, InsertPdfOptions, InsertPosition, PageSelection, PdfAnnotation,
        PdfAnnotationType, PdfDocument, PdfObject,
    },
};
use std::{
    collections::{HashMap, HashSet},
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
    crate::forms::ensure_writable(doc)?;
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
    if !(1..=crate::constants::MAX_EDIT_PAGES).contains(&pages) {
        return Err("Editing supports 1–100,000 pages".into());
    }
    let bounds = doc.load_page(0).and_then(|p| p.bounds()).map_err(error)?;
    let permissions = crate::forms::permissions(doc);
    let (signed, xfa, _) = crate::forms::security(doc).unwrap_or((true, true, true));
    let writable = !signed && !xfa;
    Ok(DocumentInfo {
        can_form: writable && permissions.contains(mupdf::pdf::Permission::FORM),
        can_edit: writable
            && permissions
                .contains(mupdf::pdf::Permission::MODIFY | mupdf::pdf::Permission::ANNOTATE),
        signed,
        pages: pages as usize,
        first_page: PageSize::new(bounds.width(), bounds.height()),
        can_copy: permissions.contains(mupdf::pdf::Permission::COPY),
        can_annotate: writable && permissions.contains(mupdf::pdf::Permission::ANNOTATE),
        can_assemble: writable && permissions.contains(mupdf::pdf::Permission::ASSEMBLE),
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
        if ordinal >= crate::constants::MAX_ANNOTATIONS_PER_PAGE
            || result.len() >= crate::constants::MAX_ANNOTATIONS_RETURNED
            || bytes >= crate::constants::MAX_ANNOTATION_BYTES
        {
            truncated = true;
            break;
        }
        let partial;
        let annotation = match a.r#type().map_err(error)? {
            PdfAnnotationType::Text => {
                let r = a.rect().map_err(error)?;
                let text = a.contents().map_err(error)?.unwrap_or("");
                let mut end = text
                    .len()
                    .min(crate::constants::MAX_TEXT_LENGTH)
                    .min(crate::constants::MAX_ANNOTATION_BYTES - bytes);
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
                if a.quad_count().map_err(error)? > crate::constants::MAX_QUAD_COUNT {
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
                && crate::forms::permissions(doc).contains(mupdf::pdf::Permission::ANNOTATE),
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
    if let EditCommand::Form(edit) = command {
        return crate::forms::apply(doc, edit);
    }
    let info = info(doc)?;
    if matches!(command,EditCommand::UpdateNote{page,ordinal,..}|EditCommand::Color{page,ordinal,..}|EditCommand::DeleteAnnotation{page,ordinal} if *page>=info.pages||*ordinal>=crate::constants::MAX_ANNOTATIONS_PER_PAGE)
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
        EditCommand::Form(_) => unreachable!(),
        EditCommand::Add(annotation) => {
            files::apply(doc, std::slice::from_ref(annotation))?;
            Ok(annotation.page())
        }
        EditCommand::UpdateNote {
            page,
            ordinal,
            text,
        } => {
            if text.trim().is_empty()
                || text.len() > crate::constants::MAX_TEXT_LENGTH
                || text.contains('\0')
            {
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
            if *at > info.pages || info.pages >= crate::constants::MAX_EDIT_PAGES {
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
    if paths.is_empty() || paths.len() > crate::constants::MAX_INSERT_PATHS || at > count {
        return Err("Choose 1–16 PDF files and a valid insertion position".into());
    }
    let mut sources = Vec::new();
    let mut total = count;
    for path in paths {
        let source = files::open(path, None)?;
        if !crate::forms::permissions(&source).contains(mupdf::pdf::Permission::COPY) {
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
        sources.push(source);
    }
    let mut target = at;
    for source in sources {
        let pages = source.page_count().map_err(error)?;
        let start = target;
        doc.insert_pdf(
            &source,
            InsertPdfOptions {
                source_pages: PageSelection::All,
                target: InsertPosition::Before(target),
                ..Default::default()
            },
        )
        .map_err(error)?;
        import_interactivity(doc, &source, start)?;
        target += pages as usize;
    }
    Ok(())
}
/// Copy a bounded PDF object graph, replacing source page references with the
/// already-grafted target pages. Cycles in popup/reply and field trees retain identity.
struct ImportGraph<'a> {
    target: &'a mut PdfDocument,
    objects: HashMap<i32, PdfObject>,
    nodes: usize,
    named: HashMap<String, PdfObject>,
    start: usize,
    pages: usize,
    page_refs: HashSet<i32>,
    stream_bytes: usize,
}
impl ImportGraph<'_> {
    fn destination(&mut self, value: &PdfObject, depth: usize) -> Result<PdfObject, String> {
        if depth > crate::constants::MAX_IMPORT_DEPTH {
            return Err("Destination nesting exceeds limits".into());
        }
        if value.is_name().map_err(error)? || value.is_string().map_err(error)? {
            let name = if value.is_name().map_err(error)? {
                String::from_utf8(value.as_name().map_err(error)?).map_err(error)?
            } else {
                value.as_string().map_err(error)?
            };
            let dest = self
                .named
                .get(&name)
                .ok_or("Imported link names an unknown destination")?
                .try_clone()
                .map_err(error)?;
            return self.destination(&dest, depth + 1);
        }
        if value.is_dict().map_err(error)? {
            let dest = value
                .get_dict("D")
                .map_err(error)?
                .ok_or("Invalid destination dictionary")?;
            return self.destination(&dest, depth + 1);
        }
        if !value.is_array().map_err(error)? || value.len().map_err(error)? < 2 {
            return Err("Imported link has an invalid destination".into());
        }
        let page = value
            .get_array(0)
            .map_err(error)?
            .ok_or("Missing destination page")?;
        if page.is_indirect().map_err(error)?
            && !self.page_refs.contains(&page.as_indirect().map_err(error)?)
        {
            return Err("Imported destination does not reference an imported page".into());
        }
        let mut dest = self.copy(value, depth + 1)?;
        if !page.is_indirect().map_err(error)? {
            if !page.is_int().map_err(error)? {
                return Err("Imported destination page is not an integer or page reference".into());
            }
            let index = page.as_int().map_err(error)?;
            if index < 0 || index as usize >= self.pages {
                return Err("Imported destination page is outside document".into());
            }
            dest.array_put(
                0,
                self.target
                    .find_page((self.start + index as usize) as i32)
                    .map_err(error)?,
            )
            .map_err(error)?;
        }
        Ok(dest)
    }
    fn copy(&mut self, value: &PdfObject, depth: usize) -> Result<PdfObject, String> {
        if depth > crate::constants::MAX_IMPORT_DEPTH
            || self.nodes >= crate::constants::MAX_IMPORT_NODES
        {
            return Err("Imported interactive objects exceed complexity limits".into());
        }
        self.nodes += 1;
        if value.is_indirect().map_err(error)? {
            let id = value.as_indirect().map_err(error)?;
            if let Some(existing) = self.objects.get(&id) {
                return existing.try_clone().map_err(error);
            }
            let resolved = value
                .resolve()
                .map_err(error)?
                .ok_or("Broken imported reference")?;
            let mut reserved = self
                .target
                .add_object(&self.target.new_null())
                .map_err(error)?;
            self.objects
                .insert(id, reserved.try_clone().map_err(error)?);
            let copied = self.copy(&resolved, depth + 1)?;
            reserved.write_object(&copied).map_err(error)?;
            if value.is_stream().map_err(error)? {
                if let Some(length) = resolved.get_dict("Length").map_err(error)? {
                    let length = length.as_int().map_err(error)?;
                    if length < 0 || length as usize > crate::constants::MAX_STREAM_BYTES {
                        return Err("Imported stream exceeds 64 MiB limit".into());
                    }
                }
                let bytes = value.read_raw_stream().map_err(error)?;
                self.stream_bytes = self.stream_bytes.saturating_add(bytes.len());
                if bytes.len() > crate::constants::MAX_STREAM_BYTES
                    || self.stream_bytes > crate::constants::MAX_TOTAL_STREAM_BYTES
                {
                    return Err("Imported stream exceeds 64 MiB limit".into());
                }
                reserved
                    .write_raw_stream_buffer(&mupdf::Buffer::from_bytes(&bytes).map_err(error)?)
                    .map_err(error)?;
            }
            return Ok(reserved);
        }
        if value.is_array().map_err(error)? {
            if value.len().map_err(error)? > crate::constants::MAX_IMPORT_ARRAY_LEN {
                return Err("Imported interactive array exceeds limits".into());
            }
            let mut copied = self.target.new_array().map_err(error)?;
            for item in value.array_iter().map_err(error)? {
                copied
                    .array_push(self.copy(&item.map_err(error)?, depth + 1)?)
                    .map_err(error)?;
            }
            return Ok(copied);
        }
        if value.is_dict().map_err(error)? {
            if value.dict_len().map_err(error)? > crate::constants::MAX_IMPORT_DICT_LEN {
                return Err("Imported dictionary exceeds limits".into());
            }
            if value.get_dict("Type").map_err(error)?.is_some_and(|kind| {
                kind.as_name()
                    .is_ok_and(|name| matches!(name.as_slice(), b"Page" | b"Pages" | b"Catalog"))
            }) {
                return Err(
                    "Imported interactivity references a foreign page tree or catalog".into(),
                );
            }
            if value.get_dict("XFA").map_err(error)?.is_some() {
                return Err("XFA forms cannot be merged yet".into());
            }
            if let Some(ft) = value.get_dict("FT").map_err(error)?
                && ft.as_name().map_err(error)? == b"Sig"
            {
                return Err(
                    "Signature fields cannot be merged without invalidating signatures".into(),
                );
            }
            let annotation = value
                .get_dict("Type")
                .map_err(error)?
                .is_some_and(|kind| kind.as_name().is_ok_and(|name| name == b"Annot"))
                || value
                    .get_dict("Subtype")
                    .map_err(error)?
                    .is_some_and(|kind| {
                        kind.as_name().is_ok_and(|name| {
                            matches!(
                                name.as_slice(),
                                b"Text"
                                    | b"Link"
                                    | b"FreeText"
                                    | b"Line"
                                    | b"Square"
                                    | b"Circle"
                                    | b"Polygon"
                                    | b"PolyLine"
                                    | b"Highlight"
                                    | b"Underline"
                                    | b"Squiggly"
                                    | b"StrikeOut"
                                    | b"Stamp"
                                    | b"Caret"
                                    | b"Ink"
                                    | b"Popup"
                                    | b"FileAttachment"
                                    | b"Sound"
                                    | b"Movie"
                                    | b"Widget"
                                    | b"Screen"
                                    | b"PrinterMark"
                                    | b"TrapNet"
                                    | b"Watermark"
                                    | b"3D"
                                    | b"Redact"
                                    | b"Projection"
                                    | b"RichMedia"
                            )
                        })
                    });
            let field = value.get_dict("FT").map_err(error)?.is_some()
                || (value.get_dict("T").map_err(error)?.is_some()
                    && (value.get_dict("Kids").map_err(error)?.is_some()
                        || value.get_dict("Parent").map_err(error)?.is_some()));
            let action = value
                .get_dict("Type")
                .map_err(error)?
                .is_some_and(|kind| kind.as_name().is_ok_and(|name| name == b"Action"));
            if action {
                validate_action(value, 0)?;
            }
            let goto = value
                .get_dict("S")
                .map_err(error)?
                .is_some_and(|s| s.as_name().is_ok_and(|n| n == b"GoTo"));
            let mut copied = self.target.new_dict().map_err(error)?;
            for entry in value.dict_iter().map_err(error)? {
                let (key, item) = entry.map_err(error)?;
                let name = String::from_utf8(key.as_name().map_err(error)?).map_err(error)?;
                if name == "AA" && (annotation || field) {
                    return Err(
                        "Additional actions in imported forms or annotations are not supported"
                            .into(),
                    );
                }
                if name == "A" && (annotation || field) {
                    validate_action(&item, 0)?;
                }
                if (name == "Dest" && annotation) || (name == "D" && goto) {
                    let destination = self.destination(&item, depth + 1)?;
                    copied.dict_put(name.as_str(), destination).map_err(error)?;
                    continue;
                }
                copied
                    .dict_put(name.as_str(), self.copy(&item, depth + 1)?)
                    .map_err(error)?;
            }
            return Ok(copied);
        }
        self.target.graft_object(value).map_err(error)
    }
}
fn validate_action(action: &PdfObject, depth: usize) -> Result<(), String> {
    if depth > crate::constants::MAX_IMPORT_DEPTH {
        return Err("Imported action chain exceeds limits".into());
    }
    if !action.is_dict().map_err(error)? {
        return Err("Imported action is malformed or chained as an array".into());
    }
    let kind = action
        .get_dict("S")
        .map_err(error)?
        .ok_or("Imported action has no subtype")?
        .as_name()
        .map_err(error)?;
    if !matches!(kind.as_slice(), b"URI" | b"GoTo") {
        return Err(
            "Imported action is unsupported; only URI and GoTo actions are preserved".into(),
        );
    }
    if let Some(next) = action.get_dict("Next").map_err(error)? {
        validate_action(&next, depth + 1)?;
    }
    Ok(())
}
fn named_destinations(source: &PdfDocument) -> Result<HashMap<String, PdfObject>, String> {
    let root = source
        .trailer()
        .map_err(error)?
        .get_dict("Root")
        .map_err(error)?
        .ok_or("Missing source catalog")?;
    let mut result = HashMap::new();
    if let Some(legacy) = root.get_dict("Dests").map_err(error)? {
        if legacy.dict_len().map_err(error)? > crate::constants::MAX_EDIT_PAGES {
            return Err("Legacy destination dictionary exceeds limits".into());
        }
        for entry in legacy.dict_iter().map_err(error)? {
            let (key, value) = entry.map_err(error)?;
            result.insert(
                String::from_utf8(key.as_name().map_err(error)?).map_err(error)?,
                value,
            );
        }
    }
    let mut pending = Vec::new();
    if let Some(tree) = root
        .get_dict("Names")
        .map_err(error)?
        .and_then(|n| n.get_dict("Dests").ok().flatten())
    {
        pending.push((tree, 0));
    }
    let mut nodes = 0;
    while let Some((node, depth)) = pending.pop() {
        nodes += 1;
        if nodes > 10000
            || depth > crate::constants::MAX_IMPORT_DEPTH
            || result.len() > crate::constants::MAX_EDIT_PAGES
        {
            return Err("Destination name tree exceeds limits".into());
        }
        if let Some(names) = node.get_dict("Names").map_err(error)? {
            let count = names.len().map_err(error)?;
            if count % 2 != 0 || count > 200000 {
                return Err("Malformed destination name tree".into());
            }
            for index in (0..count).step_by(2) {
                let key = names
                    .get_array(index as i32)
                    .map_err(error)?
                    .ok_or("Missing destination name")?
                    .as_string()
                    .map_err(error)?;
                let value = names
                    .get_array(index as i32 + 1)
                    .map_err(error)?
                    .ok_or("Missing named destination")?;
                result.insert(key, value);
            }
        }
        if let Some(kids) = node.get_dict("Kids").map_err(error)? {
            if kids.len().map_err(error)? > 10000 {
                return Err("Destination tree fanout exceeds limits".into());
            }
            for kid in kids.array_iter().map_err(error)? {
                pending.push((kid.map_err(error)?, depth + 1));
            }
        }
    }
    Ok(result)
}
fn import_interactivity(
    target: &mut PdfDocument,
    source: &PdfDocument,
    start: usize,
) -> Result<(), String> {
    let pages = source.page_count().map_err(error)? as usize;
    let mut objects = HashMap::new();
    for p in 0..pages {
        let original = source.find_page(p as i32).map_err(error)?;
        objects.insert(
            original.as_indirect().map_err(error)?,
            target.find_page((start + p) as i32).map_err(error)?,
        );
    }
    let page_refs = objects.keys().copied().collect();
    let mut graph = ImportGraph {
        target,
        objects,
        nodes: 0,
        page_refs,
        stream_bytes: 0,
        named: named_destinations(source)?,
        start,
        pages,
    };
    for p in 0..pages {
        let original = source.find_page(p as i32).map_err(error)?;
        if let Some(annots) = original.get_dict("Annots").map_err(error)? {
            if !annots.is_array().map_err(error)? || annots.len().map_err(error)? > 2000 {
                return Err("Imported page annotation array is invalid or too large".into());
            }
            for annot in annots.array_iter().map_err(error)? {
                let annot = annot.map_err(error)?;
                if !annot.is_dict().map_err(error)? {
                    return Err("Malformed imported annotation dictionary".into());
                }
                if let Some(action) = annot.get_dict("A").map_err(error)? {
                    validate_action(&action, 0)?;
                }
                if annot.get_dict("AA").map_err(error)?.is_some() {
                    return Err("Additional annotation actions are unsupported".into());
                }
            }
            let copied = graph.copy(&annots, 0)?;
            let mut page = graph.target.find_page((start + p) as i32).map_err(error)?;
            page.dict_put("Annots", copied).map_err(error)?;
        }
    }
    let root = source
        .trailer()
        .map_err(error)?
        .get_dict("Root")
        .map_err(error)?
        .ok_or("Missing imported catalog")?;
    if let Some(form) = root.get_dict("AcroForm").map_err(error)? {
        if form.get_dict("XFA").map_err(error)?.is_some() {
            return Err("XFA forms cannot be merged yet".into());
        }
        let copied = graph.copy(&form, 0)?;
        let mut target_root = graph
            .target
            .trailer()
            .map_err(error)?
            .get_dict("Root")
            .map_err(error)?
            .ok_or("Missing target catalog")?;
        let mut target_form = match target_root.get_dict("AcroForm").map_err(error)? {
            Some(form) => form,
            None => graph.target.new_dict().map_err(error)?,
        };
        if target_form.get_dict("XFA").map_err(error)?.is_some() {
            return Err("Cannot merge fields into an XFA form".into());
        }
        let mut fields = target_form
            .get_dict("Fields")
            .map_err(error)?
            .unwrap_or(graph.target.new_array().map_err(error)?);
        // A namespace parent preserves each source's inherited DA/DR resources and
        // keeps duplicate field names from becoming coupled across imported files.
        let mut parent = graph.target.new_dict().map_err(error)?;
        parent
            .dict_put(
                "T",
                graph
                    .target
                    .new_string(&format!(
                        "PDFree_import_{}_{}",
                        start + 1,
                        graph.target.xref_len().map_err(error)?
                    ))
                    .map_err(error)?,
            )
            .map_err(error)?;
        for name in ["DA", "Q"] {
            if let Some(value) = copied.get_dict(name).map_err(error)? {
                parent.dict_put(name, value).map_err(error)?;
            }
        }
        if let Some(resources) = copied.get_dict("DR").map_err(error)? {
            let mut target_resources = target_form
                .get_dict("DR")
                .map_err(error)?
                .unwrap_or(graph.target.new_dict().map_err(error)?);
            for entry in resources.dict_iter().map_err(error)? {
                let (category, incoming) = entry.map_err(error)?;
                let category =
                    String::from_utf8(category.as_name().map_err(error)?).map_err(error)?;
                let mut existing = target_resources
                    .get_dict(category.as_str())
                    .map_err(error)?
                    .unwrap_or(graph.target.new_dict().map_err(error)?);
                for entry in incoming.dict_iter().map_err(error)? {
                    let (name, value) = entry.map_err(error)?;
                    let name = String::from_utf8(name.as_name().map_err(error)?).map_err(error)?;
                    if existing.get_dict(name.as_str()).map_err(error)?.is_some() {
                        return Err("Imported form resource names conflict with existing forms; merge cancelled".into());
                    }
                    existing.dict_put(name.as_str(), value).map_err(error)?;
                }
                target_resources
                    .dict_put(category.as_str(), existing)
                    .map_err(error)?;
            }
            target_form
                .dict_put("DR", target_resources)
                .map_err(error)?;
        }
        let kids = copied
            .get_dict("Fields")
            .map_err(error)?
            .ok_or("Imported form has no Fields array")?;
        if !kids.is_array().map_err(error)? {
            return Err("Invalid imported form fields".into());
        }
        parent
            .dict_put("Kids", kids.try_clone().map_err(error)?)
            .map_err(error)?;
        let parent = graph.target.add_object(&parent).map_err(error)?;
        for kid in kids.array_iter().map_err(error)? {
            kid.map_err(error)?
                .dict_put("Parent", parent.try_clone().map_err(error)?)
                .map_err(error)?;
        }
        fields.array_push(parent).map_err(error)?;
        target_form.dict_put("Fields", fields).map_err(error)?;
        if let Some(need) = copied.get_dict("NeedAppearances").map_err(error)?
            && need.as_bool().map_err(error)?
        {
            target_form
                .dict_put("NeedAppearances", need)
                .map_err(error)?;
        }
        // Calculation order entails form execution; do not silently preserve a
        // partial calculation tree that this offline viewer cannot execute.
        if copied.get_dict("CO").map_err(error)?.is_some() {
            return Err("Forms with calculation order cannot be merged yet".into());
        }
        target_root
            .dict_put("AcroForm", target_form)
            .map_err(error)?;
    }
    Ok(())
}
fn export_rights(doc: &PdfDocument) -> Result<(), String> {
    info(doc)?;
    let p = crate::forms::permissions(doc);
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
fn print_rights(doc: &PdfDocument) -> Result<(), String> {
    info(doc)?;
    if !crate::forms::permissions(doc).contains(mupdf::pdf::Permission::PRINT) {
        return Err("This PDF does not permit printing".into());
    }
    Ok(())
}
/// Write a full copy of the current (edited) document for OS printing.
/// Printing only needs the print permission: it must work on documents that
/// deny extraction and on digitally signed read-only files.
pub(crate) fn print_copy(
    doc: &PdfDocument,
    source: &Path,
    password: Option<&str>,
    target: &Path,
    current: impl Fn() -> bool,
) -> Result<PathBuf, String> {
    print_rights(doc)?;
    let mut copy = restore(&serialize(doc)?, password)?;
    files::publish(doc, &mut copy, source, password, target, current)
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
    fn resource_names_are_not_actions_but_annotation_action_chains_are_validated() {
        let mut target = PdfDocument::new();
        let mut graph = ImportGraph {
            target: &mut target,
            objects: HashMap::new(),
            nodes: 0,
            named: HashMap::new(),
            start: 0,
            pages: 0,
            page_refs: HashSet::new(),
            stream_bytes: 0,
        };
        let resources = graph.target.new_object_from_str("<< /Font << /A << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> /Next << /Type /Font /Subtype /Type1 /BaseFont /Courier >> >> /CharProcs << /A << >> /AA << >> >> >>").unwrap();
        assert!(graph.copy(&resources, 0).is_ok());
        let unsupported = graph.target.new_object_from_str("<< /Type /Annot /Subtype /Link /A << /S /URI /URI (https://example.com) /Next << /S /JavaScript /JS (blocked) >> >> >>").unwrap();
        assert!(
            graph
                .copy(&unsupported, 0)
                .unwrap_err()
                .contains("unsupported")
        );
        let malformed = graph
            .target
            .new_object_from_str("<< /Subtype /Widget /FT /Tx /A << /S /URI /Next [] >> >>")
            .unwrap();
        assert!(graph.copy(&malformed, 0).unwrap_err().contains("malformed"));
    }

    #[test]
    fn cyclic_named_destinations_and_foreign_page_trees_fail_with_bounded_work() {
        let mut target = PdfDocument::new();
        target.new_page((300., 400.)).unwrap();
        let mut graph = ImportGraph {
            target: &mut target,
            objects: HashMap::new(),
            nodes: 0,
            named: HashMap::from([("cycle".into(), PdfObject::new_name("cycle").unwrap())]),
            start: 0,
            pages: 1,
            page_refs: HashSet::new(),
            stream_bytes: 0,
        };
        assert!(
            graph
                .destination(&PdfObject::new_name("cycle").unwrap(), 0)
                .unwrap_err()
                .contains("nesting")
        );
        let foreign = graph
            .target
            .new_object_from_str("<< /Type /Page >>")
            .unwrap();
        assert!(
            graph
                .copy(&foreign, 0)
                .unwrap_err()
                .contains("foreign page tree")
        );
        let invalid = graph.target.new_object_from_str("[null /Fit]").unwrap();
        assert!(graph.destination(&invalid, 0).is_err());
    }

    #[test]
    fn existing_indirect_destinations_follow_page_moves_and_insertion() {
        let mut doc = PdfDocument::new();
        doc.new_page((300., 400.)).unwrap();
        doc.new_page((300., 400.)).unwrap();
        let mut link = doc.new_dict().unwrap();
        let mut dest = doc.new_array().unwrap();
        dest.array_push(doc.find_page(1).unwrap()).unwrap();
        dest.array_push(doc.new_name("Fit").unwrap()).unwrap();
        link.dict_put("Dest", dest).unwrap();
        let mut annots = doc.new_array().unwrap();
        annots.array_push(link).unwrap();
        doc.find_page(0)
            .unwrap()
            .dict_put("Annots", annots)
            .unwrap();
        mutate(&mut doc, &EditCommand::Move { page: 1, to: 0 }).unwrap();
        mutate(&mut doc, &EditCommand::Blank { at: 0 }).unwrap();
        let destination = doc
            .find_page(2)
            .unwrap()
            .get_dict("Annots")
            .unwrap()
            .unwrap()
            .get_array(0)
            .unwrap()
            .unwrap()
            .get_dict("Dest")
            .unwrap()
            .unwrap()
            .get_array(0)
            .unwrap()
            .unwrap();
        assert_eq!(doc.lookup_page_number(&destination).unwrap(), 1);
    }

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
