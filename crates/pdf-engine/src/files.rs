//! Coordinator-only file transactions. Native baseline is never edited for Save As.
use mupdf::{
    Document, Point, Quad, Rect,
    color::AnnotationColor,
    pdf::{Encryption, PdfAnnotationType, PdfDocument, PdfWriteOptions, Permission},
};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use viewer_core::{Annotation, validate_pdf_name};

pub(crate) fn open(path: &Path, password: Option<&str>) -> Result<PdfDocument, String> {
    let path = path.to_str().ok_or("Path is not valid Unicode")?;
    let mut doc = Document::open(path)
        .and_then(PdfDocument::try_from)
        .map_err(|e| e.to_string())?;
    if doc.needs_password().map_err(|e| e.to_string())?
        && !doc
            .authenticate(password.unwrap_or(""))
            .map_err(|e| e.to_string())?
    {
        return Err("PDF could not be authenticated after the file operation".into());
    }
    Ok(doc)
}
pub(crate) fn options() -> PdfWriteOptions {
    let mut options = PdfWriteOptions::default();
    options.set_encryption(Encryption::Keep);
    options
}
pub(crate) fn apply(doc: &PdfDocument, annotations: &[Annotation]) -> Result<(), String> {
    if annotations.len() > crate::constants::MAX_CREATE_FIELDS {
        return Err("Too many annotations (maximum 2,000)".into());
    }
    if !annotations.is_empty() && !crate::forms::permissions(doc).contains(Permission::ANNOTATE) {
        return Err("This PDF does not permit annotations".into());
    }
    let count = doc.page_count().map_err(|e| e.to_string())? as usize;
    for annotation in annotations {
        annotation.validate()?;
        if annotation.page() >= count {
            return Err("Annotation page is outside the document".into());
        }
        let mut page = doc
            .load_pdf_page(annotation.page() as i32)
            .map_err(|e| e.to_string())?;
        let bounds = page.bounds().map_err(|e| e.to_string())?;
        let valid = |x: f32, y: f32| {
            x.is_finite()
                && y.is_finite()
                && x >= 0.
                && y >= 0.
                && x <= bounds.width()
                && y <= bounds.height()
        };
        match annotation {
            Annotation::Highlight { rects, .. }
                if rects
                    .iter()
                    .any(|r| !valid(r.left, r.top) || !valid(r.right, r.bottom)) =>
            {
                return Err("Invalid highlight bounds".into());
            }
            Annotation::Note { x, y, .. } if !valid(*x, *y) => {
                return Err("Invalid note position".into());
            }
            _ => {}
        }
        let result = (|| -> Result<(), mupdf::Error> {
            match annotation {
                Annotation::Highlight { rects, .. } => {
                    // Validation precedes native creation to keep failures side-effect free.
                    let quads: Vec<_> = rects
                        .iter()
                        .map(|r| Quad {
                            ul: Point {
                                x: r.left + bounds.x0,
                                y: r.top + bounds.y0,
                            },
                            ur: Point {
                                x: r.right + bounds.x0,
                                y: r.top + bounds.y0,
                            },
                            ll: Point {
                                x: r.left + bounds.x0,
                                y: r.bottom + bounds.y0,
                            },
                            lr: Point {
                                x: r.right + bounds.x0,
                                y: r.bottom + bounds.y0,
                            },
                        })
                        .collect();
                    let mut a = page.create_annotation(PdfAnnotationType::Highlight)?;
                    a.set_quad_points(quads)?;
                    a.set_color(AnnotationColor::Rgb {
                        red: 1.,
                        green: 0.85,
                        blue: 0.,
                    })?;
                    a.set_opacity(0.4)?;
                    a.update()?;
                }
                Annotation::Note { x, y, text, .. } => {
                    let mut a = page.create_annotation(PdfAnnotationType::Text)?;
                    a.set_rect(Rect {
                        x0: x + bounds.x0,
                        y0: y + bounds.y0,
                        x1: (x + 20.).min(bounds.width()) + bounds.x0,
                        y1: (y + 20.).min(bounds.height()) + bounds.y0,
                    })?;
                    a.set_contents(text)?;
                    a.set_icon_name("Note")?;
                    a.set_color(AnnotationColor::Rgb {
                        red: 1.,
                        green: 0.85,
                        blue: 0.,
                    })?;
                    a.update()?;
                }
            }
            page.update()?;
            Ok(())
        })();
        result.map_err(|e| e.to_string())?;
    }
    Ok(())
}
static TEMP_ID: AtomicU64 = AtomicU64::new(0);
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(crate) struct PreparedRename {
    pub baseline: PdfDocument,
    pub target: PathBuf,
    staged: Temporary,
}
pub(crate) struct CommitFailure {
    pub path: PathBuf,
    pub message: String,
}

pub(crate) fn prepare_rename(
    doc: &PdfDocument,
    source: &Path,
    password: Option<&str>,
    name: &str,
    annotations: &[Annotation],
    current: impl Fn() -> bool,
) -> Result<PreparedRename, String> {
    crate::forms::ensure_writable(doc)?;
    if !fs::symlink_metadata(source)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_file()
    {
        return Err(
            "Rename and save requires a regular PDF file; open the linked file directly.".into(),
        );
    }
    let target = renamed_path(source, name)?;
    if target != source && target.exists() {
        // Windows case-only names refer to the same directory entry and are allowed.
        let same_spelling = source
            .file_name()
            .and_then(|n| n.to_str())
            .zip(target.file_name().and_then(|n| n.to_str()))
            .is_some_and(|(a, b)| a.to_lowercase() == b.to_lowercase());
        if !cfg!(windows) || !same_spelling {
            return Err("That filename already exists. Choose a new name.".into());
        }
    }
    let mut bytes = Vec::new();
    doc.write_to_with_options(&mut bytes, options())
        .map_err(|e| e.to_string())?;
    let mut baseline = PdfDocument::from_bytes(&bytes).map_err(|e| e.to_string())?;
    if baseline.needs_password().map_err(|e| e.to_string())?
        && !baseline
            .authenticate(password.unwrap_or(""))
            .map_err(|e| e.to_string())?
    {
        return Err("Could not authenticate session baseline".into());
    }
    let parent = source.parent().ok_or("PDF folder is unavailable")?;
    let staged_path = parent.join(format!(
        ".pdfree-rename-{}-{}.pdf",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let staged = Temporary(save(
        &baseline,
        source,
        password,
        &staged_path,
        annotations,
        current,
    )?);
    Ok(PreparedRename {
        baseline,
        target,
        staged,
    })
}

impl PreparedRename {
    pub(crate) fn commit(self, source: &Path) -> (PdfDocument, Result<PathBuf, CommitFailure>) {
        let Self {
            baseline,
            target,
            staged,
        } = self;
        let moved = target != source;
        if moved && let Err(e) = move_no_replace(source, &target) {
            return (
                baseline,
                Err(CommitFailure {
                    path: source.into(),
                    message: format!(
                        "Could not rename PDF: {e}. Original and draft annotations are unchanged."
                    ),
                }),
            );
        }
        let backup = loop {
            let path = target.with_file_name(format!(
                ".pdfree-backup-{}-{}.pdf",
                std::process::id(),
                TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    drop(file);
                    break path;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return (
                        baseline,
                        rollback_rename(
                            source,
                            target,
                            moved,
                            format!("Could not reserve a recovery file: {e}"),
                        ),
                    );
                }
            }
        };
        match replace_file(&target, &staged.0, &backup) {
            Ok(()) => {
                let _ = fs::remove_file(&backup);
                (baseline, Ok(target))
            }
            Err(e) => {
                // A zero-length file is our unused reservation, not the original PDF.
                if fs::metadata(&backup).is_ok_and(|m| m.len() == 0) {
                    let _ = fs::remove_file(&backup);
                }
                // ReplaceFile can leave the original at its backup name on rare failures.
                // Never delete or overwrite that recovery copy when recovery is uncertain.
                if backup.exists() {
                    if !target.exists() && move_no_replace(&backup, &target).is_ok() {
                        return (
                            baseline,
                            rollback_rename(
                                source,
                                target,
                                moved,
                                format!("Saving annotations failed: {e}"),
                            ),
                        );
                    }
                    return (
                        baseline,
                        Err(CommitFailure {
                            path: backup.clone(),
                            message: format!(
                                "Saving failed ({e}). Original recovery copy is at {}. Draft annotations remain available; use Open to recover.",
                                backup.display()
                            ),
                        }),
                    );
                }
                (
                    baseline,
                    rollback_rename(
                        source,
                        target,
                        moved,
                        format!("Saving annotations failed: {e}"),
                    ),
                )
            }
        }
    }
}
fn rollback_rename(
    source: &Path,
    target: PathBuf,
    moved: bool,
    message: String,
) -> Result<PathBuf, CommitFailure> {
    if moved && let Err(e) = move_no_replace(&target, source) {
        return Err(CommitFailure {
            path: target.clone(),
            message: format!(
                "{message}. Rename rollback failed ({e}); file is at {}. Draft annotations remain available.",
                target.display()
            ),
        });
    }
    Err(CommitFailure {
        path: source.into(),
        message: format!("{message}. Original and draft annotations are unchanged."),
    })
}
#[cfg(windows)]
fn replace_file(target: &Path, replacement: &Path, backup: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let target = wide(target);
    let replacement = wide(replacement);
    let backup = wide(backup);
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
            target.as_ptr(),
            replacement.as_ptr(),
            backup.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
fn replace_file(target: &Path, replacement: &Path, _backup: &Path) -> std::io::Result<()> {
    fs::rename(replacement, target)
}

pub(crate) fn save(
    doc: &PdfDocument,
    source: &Path,
    password: Option<&str>,
    target: &Path,
    annotations: &[Annotation],
    current: impl Fn() -> bool,
) -> Result<PathBuf, String> {
    crate::forms::ensure_writable(doc)?;
    if annotations.len() > crate::constants::MAX_CREATE_FIELDS {
        return Err("Too many annotations (maximum 2,000)".into());
    }
    for annotation in annotations {
        annotation.validate()?;
    }
    if !annotations.is_empty() && !crate::forms::permissions(doc).contains(Permission::ANNOTATE) {
        return Err("This PDF does not permit annotations".into());
    }
    let mut baseline = Vec::new();
    doc.write_to_with_options(&mut baseline, options())
        .map_err(|e| e.to_string())?;
    let mut copy = PdfDocument::from_bytes(&baseline).map_err(|e| e.to_string())?;
    if copy.needs_password().map_err(|e| e.to_string())?
        && !copy
            .authenticate(password.unwrap_or(""))
            .map_err(|e| e.to_string())?
    {
        return Err("Could not authenticate PDF copy".into());
    }
    apply(&copy, annotations)?;
    publish(doc, &mut copy, source, password, target, current)
}

/// Publish an already-built copy to `target`. Printing and exporting reuse
/// this without the source writability and annotation gates so read-only
/// documents can still be printed or exported.
pub(crate) fn publish(
    doc: &PdfDocument,
    copy: &mut PdfDocument,
    source: &Path,
    password: Option<&str>,
    target: &Path,
    current: impl Fn() -> bool,
) -> Result<PathBuf, String> {
    let name = target
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Choose a PDF filename")?;
    if validate_pdf_name(name)? != name {
        return Err("Save As filename must end in .pdf".into());
    }
    let parent = fs::canonicalize(target.parent().ok_or("Choose an output folder")?)
        .map_err(|e| e.to_string())?;
    let target = parent.join(name);
    if target.exists() || fs::symlink_metadata(&target).is_ok() {
        return Err("That filename already exists. Choose a new name.".into());
    }
    if fs::canonicalize(source).map_err(|e| e.to_string())? == target {
        return Err("Save As must use a different filename".into());
    }
    let (temporary, mut file) = loop {
        let path = parent.join(format!(
            ".pdfree-{}-{}.tmp",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => break (Temporary(path), file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    copy.write_to_with_options(&mut file, options())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    let verified = open(&temporary.0, password)?;
    crate::forms::verify(doc, &verified)?;
    if verified.page_count().map_err(|e| e.to_string())?
        != doc.page_count().map_err(|e| e.to_string())?
    {
        return Err("Saved copy failed page-count verification".into());
    }
    for page in 0..verified.page_count().map_err(|e| e.to_string())? {
        if !current() {
            return Err("Document changed before Save As completed".into());
        }
        verified.load_pdf_page(page).map_err(|e| e.to_string())?;
    }
    drop(verified);
    if !current() {
        return Err("Document changed before Save As completed".into());
    }
    move_no_replace(&temporary.0, &target)
        .map_err(|e| format!("Could not publish saved copy: {e}"))?;
    Ok(target)
}

pub(crate) fn renamed_path(source: &Path, name: &str) -> Result<PathBuf, String> {
    Ok(source.with_file_name(validate_pdf_name(name)?))
}
#[cfg(windows)]
pub(crate) fn move_no_replace(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let source: Vec<_> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<_> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // No REPLACE_EXISTING flag: the kernel atomically rejects destination races.
    // Antivirus or search indexer can briefly hold a freshly created file, so
    // retry the atomic move while a transient sharing lock is expected to clear.
    let mut attempts = 0;
    loop {
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                0,
            )
        } != 0
        {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(32) || attempts == 9 {
            return Err(error);
        }
        attempts += 1;
        std::thread::sleep(std::time::Duration::from_millis(10 * attempts as u64));
    }
}
#[cfg(not(windows))]
pub(crate) fn move_no_replace(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::hard_link(source, target)?;
    fs::remove_file(source)
}

pub(crate) fn notes(doc: &PdfDocument, page: usize) -> Result<(Vec<Annotation>, bool), String> {
    let native = doc.load_pdf_page(page as i32).map_err(|e| e.to_string())?;
    let bounds = native.bounds().map_err(|e| e.to_string())?;
    let mut notes = Vec::new();
    let mut bytes = 0;
    let mut truncated = false;
    for (index, a) in native.annotations().enumerate() {
        if index >= 10_000 {
            truncated = true;
            break;
        }
        if a.r#type().map_err(|e| e.to_string())? != PdfAnnotationType::Text {
            continue;
        }
        if notes.len() >= 256 || bytes >= 256 * 1024 {
            truncated = true;
            break;
        }
        let r = a.rect().map_err(|e| e.to_string())?;
        let text = a.contents().map_err(|e| e.to_string())?.unwrap_or("");
        let mut end = text.len().min(16_384).min(256 * 1024 - bytes);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        truncated |= end < text.len();
        bytes += end;
        notes.push(Annotation::Note {
            page,
            x: r.x0 - bounds.x0,
            y: r.y0 - bounds.y0,
            text: text[..end].into(),
        });
    }
    Ok((notes, truncated))
}
