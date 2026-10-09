use pdf_engine::{Engine, Event, FileAction};
use std::{path::PathBuf, time::Duration};
use viewer_core::{Annotation, TextRect};

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn directory() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "pdfree-edit-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    path
}
fn cleanup(path: PathBuf) {
    let resolved = std::fs::canonicalize(&path).unwrap();
    let temporary = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    assert!(resolved.starts_with(&temporary));
    assert!(
        resolved
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("pdfree-edit-")
    );
    if let Some(output) = std::env::var_os("PDFREE_EDIT_TEST_OUTPUT_DIR") {
        let output = PathBuf::from(output);
        let allowed = workspace().join("benchmarks/runs/editing-native");
        std::fs::create_dir_all(&allowed).unwrap();
        assert_eq!(
            std::fs::canonicalize(&output).unwrap(),
            std::fs::canonicalize(&allowed).unwrap()
        );
        for name in ["encrypted-copy.pdf", "restricted-copy.pdf"] {
            let source = resolved.join(name);
            if source.exists() {
                std::fs::copy(source, output.join(name)).unwrap();
            }
        }
    }
    std::fs::remove_dir_all(resolved).unwrap();
}
fn wait(engine: &Engine, predicate: impl Fn(&Event) -> bool) -> Event {
    loop {
        let e = engine.events.recv_timeout(Duration::from_secs(20)).unwrap();
        if predicate(&e) {
            return e;
        }
    }
}
fn opened(engine: &Engine, path: PathBuf) {
    engine.open(1, 1, path);
    wait(engine, |e| matches!(e, Event::Opened { document: 1, .. }));
}
fn edits() -> Vec<Annotation> {
    vec![
        Annotation::Highlight {
            page: 0,
            rects: vec![TextRect {
                left: 50.,
                top: 40.,
                right: 220.,
                bottom: 60.,
            }],
        },
        Annotation::Note {
            page: 0,
            x: 75.,
            y: 100.,
            text: "Saved note — café 日本語".into(),
        },
    ]
}
#[test]
fn rename_commits_annotations_without_duplicating_retained_session_drafts() {
    let dir = directory();
    let source = dir.join("source.pdf");
    std::fs::copy(workspace().join("tests/corpus/encrypted.pdf"), &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let engine = Engine::spawn(1);
    engine.open_with_password(1, 1, source.clone(), Some("pdfree-test".into()));
    wait(&engine, |e| matches!(e, Event::Opened { .. }));
    let occupied = dir.join("occupied.pdf");
    std::fs::write(&occupied, b"keep").unwrap();
    assert!(engine.rename_and_save(1, 1, "occupied.pdf".into(), edits()));
    assert!(matches!(
        wait(&engine, |e| matches!(
            e,
            Event::FileError { operation: 1, .. }
        )),
        Event::FileError { .. }
    ));
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert_eq!(std::fs::read(&occupied).unwrap(), b"keep");
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
            .open(&source)
            .unwrap();
        assert!(engine.rename_and_save(1, 10, "source.pdf".into(), edits()));
        assert!(matches!(
            wait(&engine, |e| matches!(
                e,
                Event::FileError { operation: 10, .. }
            )),
            Event::FileError { .. }
        ));
        assert_eq!(std::fs::read(&source).unwrap(), original);
        drop(lock);
    }
    for (op, name, annotations, count) in [
        (2, "renamed.pdf", edits(), 2),
        (3, "renamed.pdf", edits(), 2),
        (4, "renamed.pdf", vec![], 0),
    ] {
        assert!(engine.rename_and_save(1, op, name.into(), annotations));
        assert!(matches!(
            wait(
                &engine,
                |e| matches!(e,Event::FileDone{operation,..}|Event::FileError{operation,..} if *operation==op)
            ),
            Event::FileDone { .. }
        ));
        let mut doc = mupdf::pdf::PdfDocument::try_from(
            mupdf::Document::open(dir.join(name).to_str().unwrap()).unwrap(),
        )
        .unwrap();
        assert!(doc.needs_password().unwrap());
        assert!(doc.authenticate("pdfree-test").unwrap());
        assert_eq!(doc.load_pdf_page(0).unwrap().annotations().count(), count);
    }
    assert!(!source.exists());
    let copy = dir.join("another-copy.pdf");
    assert!(engine.save_as(1, 5, copy.clone(), edits()));
    assert!(matches!(
        wait(&engine, |e| matches!(
            e,
            Event::FileDone { operation: 5, .. } | Event::FileError { operation: 5, .. }
        )),
        Event::FileDone { .. }
    ));
    let mut doc =
        mupdf::pdf::PdfDocument::try_from(mupdf::Document::open(copy.to_str().unwrap()).unwrap())
            .unwrap();
    assert!(doc.authenticate("pdfree-test").unwrap());
    assert_eq!(doc.load_pdf_page(0).unwrap().annotations().count(), 2);
    drop(doc);
    drop(engine);
    cleanup(dir);
}

#[test]
fn saved_crop_origin_annotations_keep_page_local_positions() {
    let dir = directory();
    let source = dir.join("crop-source.pdf");
    std::fs::copy(workspace().join("tests/corpus/crop-origin.pdf"), &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let baseline =
        mupdf::pdf::PdfDocument::try_from(mupdf::Document::open(source.to_str().unwrap()).unwrap())
            .unwrap();
    let bounds = baseline.load_pdf_page(0).unwrap().bounds().unwrap();
    drop(baseline);
    let engine = Engine::spawn(1);
    opened(&engine, source.clone());
    let target = dir.join("crop-annotated.pdf");
    assert!(engine.save_as(1, 1, target.clone(), edits()));
    assert!(matches!(
        wait(&engine, |e| matches!(
            e,
            Event::FileDone { .. } | Event::FileError { .. }
        )),
        Event::FileDone { .. }
    ));
    let saved =
        mupdf::pdf::PdfDocument::try_from(mupdf::Document::open(target.to_str().unwrap()).unwrap())
            .unwrap();
    let page = saved.load_pdf_page(0).unwrap();
    let annotations: Vec<_> = page.annotations().collect();
    assert_eq!(annotations.len(), 2);
    let highlight = annotations
        .iter()
        .find(|a| a.r#type().unwrap() == mupdf::pdf::PdfAnnotationType::Highlight)
        .unwrap();
    let quads = highlight.quad_points().unwrap();
    assert_eq!(quads.len(), 1);
    let quad = &quads[0];
    let close = |actual: f32, expected: f32| {
        assert!(
            (actual - expected).abs() < 0.05,
            "expected {expected}, got {actual}"
        )
    };
    for (point, x, y) in [
        (&quad.ul, 50., 40.),
        (&quad.ur, 220., 40.),
        (&quad.ll, 50., 60.),
        (&quad.lr, 220., 60.),
    ] {
        close(point.x, bounds.x0 + x);
        close(point.y, bounds.y0 + y);
    }
    let note = annotations
        .iter()
        .find(|a| a.r#type().unwrap() == mupdf::pdf::PdfAnnotationType::Text)
        .unwrap();
    let rect = note.rect().unwrap();
    close(rect.x0, bounds.x0 + 75.);
    close(rect.y0, bounds.y0 + 100.);
    assert_eq!(note.contents().unwrap(), Some("Saved note — café 日本語"));
    assert_eq!(std::fs::read(source).unwrap(), original);
    drop(annotations);
    drop(page);
    drop(saved);
    drop(engine);
    cleanup(dir);
}

#[test]
fn saved_annotations_reopen_original_unchanged_and_no_duplicate_on_repeat() {
    let dir = directory();
    let source = dir.join("source.pdf");
    std::fs::copy(workspace().join("tests/corpus/text.pdf"), &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let engine = Engine::spawn(1);
    opened(&engine, source.clone());
    for operation in 1..=2 {
        let target = dir.join(format!("saved-{operation}.pdf"));
        assert!(engine.save_as(1, operation, target.clone(), edits()));
        assert!(matches!(
            wait(&engine, |e| matches!(
                e,
                Event::FileDone { .. } | Event::FileError { .. }
            )),
            Event::FileDone {
                kind: FileAction::SaveAs,
                ..
            }
        ));
        let doc = mupdf::pdf::PdfDocument::try_from(
            mupdf::Document::open(target.to_str().unwrap()).unwrap(),
        )
        .unwrap();
        let page = doc.load_pdf_page(0).unwrap();
        let annotations: Vec<_> = page.annotations().collect();
        assert_eq!(annotations.len(), 2);
        assert!(
            annotations
                .iter()
                .any(|a| a.contents().unwrap() == Some("Saved note — café 日本語"))
        );
        assert_eq!(std::fs::read(&source).unwrap(), original);
    }
    engine.open(2, 2, dir.join("saved-2.pdf"));
    wait(&engine, |e| matches!(e, Event::Opened { document: 2, .. }));
    engine.note_pages(2, vec![0]);
    assert!(
        matches!(wait(&engine,|e|matches!(e,Event::Notes{document:2,page:0,..})),Event::Notes{notes,..} if notes.iter().any(|note|matches!(note,Annotation::Note{text,..} if text=="Saved note — café 日本語")))
    );
    engine.note_pages(2, vec![]);
    engine.note_pages(2, vec![0]);
    assert!(
        matches!(wait(&engine,|e|matches!(e,Event::Notes{document:2,page:0,..})),Event::Notes{notes,..} if notes.len()==1)
    );
    drop(engine);
    cleanup(dir);
}
#[test]
fn rename_is_real_and_conflicts_never_overwrite_or_lose_source() {
    let dir = directory();
    let source = dir.join("source.pdf");
    std::fs::copy(workspace().join("tests/corpus/text.pdf"), &source).unwrap();
    let engine = Engine::spawn(1);
    opened(&engine, source.clone());
    let occupied = dir.join("occupied.pdf");
    std::fs::write(&occupied, b"keep").unwrap();
    assert!(engine.rename(1, 1, "occupied.pdf".into()));
    assert!(matches!(
        wait(&engine, |e| matches!(e, Event::FileError { .. })),
        Event::FileError { .. }
    ));
    assert!(source.exists());
    assert_eq!(std::fs::read(&occupied).unwrap(), b"keep");
    assert!(engine.rename(1, 2, "new name".into()));
    let Event::FileDone { path, .. } = wait(&engine, |e| {
        matches!(
            e,
            Event::FileDone { operation: 2, .. } | Event::FileError { operation: 2, .. }
        )
    }) else {
        panic!("rename failed")
    };
    assert_eq!(path.file_name().unwrap(), "new name.pdf");
    assert!(!source.exists());
    assert!(path.exists());
    assert!(engine.rename(1, 20, "NEW NAME.PDF".into()));
    assert!(matches!(
        wait(&engine, |e| matches!(
            e,
            Event::FileDone { operation: 20, .. } | Event::FileError { operation: 20, .. }
        )),
        Event::FileDone { .. }
    ));
    assert!(
        std::fs::read_dir(&dir)
            .unwrap()
            .any(|entry| entry.unwrap().file_name() == "NEW NAME.PDF")
    );
    assert!(engine.rename(1, 21, "CON.pdf".into()));
    assert!(matches!(
        wait(&engine, |e| matches!(
            e,
            Event::FileError { operation: 21, .. }
        )),
        Event::FileError { .. }
    ));
    assert!(engine.save_as(1, 3, occupied.clone(), vec![]));
    assert!(matches!(
        wait(&engine, |e| matches!(
            e,
            Event::FileError { operation: 3, .. }
        )),
        Event::FileError { .. }
    ));
    assert_eq!(std::fs::read(occupied).unwrap(), b"keep");
    engine.note_pages(1, vec![0]);
    assert!(
        matches!(wait(&engine,|e|matches!(e,Event::Notes{..})),Event::Notes{notes,..} if notes.is_empty())
    );
    drop(engine);
    cleanup(dir);
}

#[test]
fn encryption_permissions_and_invalid_geometry_are_preserved() {
    let dir = directory();
    let source = dir.join("encrypted.pdf");
    std::fs::copy(workspace().join("tests/corpus/encrypted.pdf"), &source).unwrap();
    let engine = Engine::spawn(1);
    engine.open_with_password(1, 1, source.clone(), Some("pdfree-test".into()));
    wait(&engine, |e| matches!(e, Event::Opened { .. }));
    let target = dir.join("encrypted-copy.pdf");
    assert!(engine.save_as(1, 1, target.clone(), edits()));
    assert!(matches!(
        wait(&engine, |e| matches!(
            e,
            Event::FileDone { .. } | Event::FileError { .. }
        )),
        Event::FileDone { .. }
    ));
    let mut doc = mupdf::Document::open(target.to_str().unwrap()).unwrap();
    assert!(doc.needs_password().unwrap());
    assert!(!doc.authenticate("wrong").unwrap());
    assert!(doc.authenticate("pdfree-test").unwrap());
    drop(doc);
    assert!(engine.rename(1, 10, "encrypted renamed.pdf".into()));
    assert!(matches!(
        wait(&engine, |e| matches!(
            e,
            Event::FileDone { operation: 10, .. } | Event::FileError { operation: 10, .. }
        )),
        Event::FileDone { .. }
    ));
    let invalid = dir.join("invalid.pdf");
    assert!(engine.save_as(
        1,
        2,
        invalid.clone(),
        vec![Annotation::Note {
            page: 0,
            x: f32::NAN,
            y: 3.,
            text: "bad".into()
        }]
    ));
    assert!(matches!(
        wait(&engine, |e| matches!(
            e,
            Event::FileError { operation: 2, .. }
        )),
        Event::FileError { .. }
    ));
    assert!(!invalid.exists());
    engine.open_with_password(
        2,
        2,
        workspace().join("tests/corpus/restricted-copy.pdf"),
        Some("pdfree-test".into()),
    );
    assert!(
        matches!(wait(&engine,|e|matches!(e,Event::Opened{document:2,..})),Event::Opened{info,..} if !info.can_annotate)
    );
    assert!(engine.save_as(2, 3, dir.join("restricted-edit.pdf"), edits()));
    assert!(matches!(
        wait(&engine, |e| matches!(
            e,
            Event::FileError { operation: 3, .. }
        )),
        Event::FileError { .. }
    ));
    assert!(engine.save_as(2, 4, dir.join("restricted-copy.pdf"), vec![]));
    assert!(matches!(
        wait(&engine, |e| matches!(
            e,
            Event::FileDone { operation: 4, .. } | Event::FileError { operation: 4, .. }
        )),
        Event::FileDone { .. }
    ));
    let mut restricted = mupdf::pdf::PdfDocument::try_from(
        mupdf::Document::open(dir.join("restricted-copy.pdf").to_str().unwrap()).unwrap(),
    )
    .unwrap();
    assert!(restricted.needs_password().unwrap());
    assert!(restricted.authenticate("pdfree-test").unwrap());
    assert!(
        !restricted
            .permissions()
            .contains(mupdf::pdf::Permission::ANNOTATE)
    );
    assert!(
        !restricted
            .permissions()
            .contains(mupdf::pdf::Permission::COPY)
    );
    drop(restricted);
    drop(engine);
    cleanup(dir);
}
