use pdf_engine::{EditCommand, Engine, Event, FileAction};
use std::{path::PathBuf, time::Duration};
use viewer_core::Annotation;
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus")
        .join(name)
}
fn wait(e: &Engine, p: impl Fn(&Event) -> bool) -> Event {
    loop {
        let event = e.events.recv_timeout(Duration::from_secs(30)).unwrap();
        if p(&event) {
            return event;
        }
    }
}
fn changed(e: &Engine, revision: u64, operation: u64, command: EditCommand) -> u64 {
    assert!(e.edit(1, revision, operation, operation + 100, command));
    match wait(
        e,
        |event| matches!(event,Event::Changed{operation:op,..}|Event::FileError{operation:op,..} if *op==operation),
    ) {
        Event::Changed { revision, .. } => revision,
        event => panic!("native edit failed: {event:?}"),
    }
}
static FOLDER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn folder() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "pdfree-pages-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        FOLDER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    path
}
fn cleanup(path: PathBuf) {
    let path = std::fs::canonicalize(path).unwrap();
    assert!(path.starts_with(std::fs::canonicalize(std::env::temp_dir()).unwrap()));
    assert!(
        path.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("pdfree-pages-")
    );
    if let Some(output) = std::env::var_os("PDFREE_PAGE_TEST_OUTPUT_DIR") {
        let allowed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/runs/page-tools-native");
        std::fs::create_dir_all(&allowed).unwrap();
        let output = std::fs::canonicalize(PathBuf::from(output)).unwrap();
        assert_eq!(output, std::fs::canonicalize(allowed).unwrap());
        let retained = output.join(path.file_name().unwrap());
        std::fs::create_dir(&retained).unwrap();
        for entry in std::fs::read_dir(&path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                std::fs::copy(entry.path(), retained.join(entry.file_name())).unwrap();
            }
        }
    }
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn extraction_garbage_collects_unreferenced_excluded_annotations() {
    let dir = folder();
    let source = dir.join("two-pages.pdf");
    let mut native = mupdf::pdf::PdfDocument::new();
    native.new_page((300., 400.)).unwrap();
    let mut second = native.new_page((300., 400.)).unwrap();
    let mut note = second
        .create_annotation(mupdf::pdf::PdfAnnotationType::Text)
        .unwrap();
    note.set_rect(mupdf::Rect {
        x0: 20.,
        y0: 20.,
        x1: 40.,
        y1: 40.,
    })
    .unwrap();
    note.set_contents("EXCLUDED_PAGE_PRIVATE_MARKER").unwrap();
    note.update().unwrap();
    drop(note);
    drop(second);
    native.save(source.to_str().unwrap()).unwrap();
    drop(native);
    assert!(
        std::fs::read(&source)
            .unwrap()
            .windows(b"EXCLUDED_PAGE_PRIVATE_MARKER".len())
            .any(|w| w == b"EXCLUDED_PAGE_PRIVATE_MARKER")
    );
    let e = Engine::spawn(1);
    e.open(1, 1, source.clone());
    wait(&e, |x| matches!(x, Event::Opened { .. }));
    let target = dir.join("only-first.pdf");
    assert!(e.extract(1, 0, 1, target.clone(), vec![0]));
    assert!(matches!(
        wait(&e, |x| matches!(
            x,
            Event::FileDone { operation: 1, .. } | Event::FileError { operation: 1, .. }
        )),
        Event::FileDone { .. }
    ));
    let bytes = std::fs::read(&target).unwrap();
    assert!(
        !bytes
            .windows(b"EXCLUDED_PAGE_PRIVATE_MARKER".len())
            .any(|w| w == b"EXCLUDED_PAGE_PRIVATE_MARKER")
    );
    let pdf = mupdf::pdf::PdfDocument::open(target.to_str().unwrap()).unwrap();
    assert_eq!(pdf.page_count().unwrap(), 1);
    assert_eq!(pdf.load_pdf_page(0).unwrap().annotations().count(), 0);
    drop(pdf);
    drop(e);
    cleanup(dir);
}

#[test]
fn imports_annotation_deletion_stale_guards_and_split_conflicts_are_transactional() {
    let dir = folder();
    let source = dir.join("source.pdf");
    std::fs::copy(fixture("text.pdf"), &source).unwrap();
    let e = Engine::spawn(2);
    e.open(1, 1, source.clone());
    wait(&e, |x| matches!(x, Event::Opened { .. }));
    let revision = changed(
        &e,
        0,
        1,
        EditCommand::Add(Annotation::Highlight {
            page: 0,
            rects: vec![viewer_core::TextRect {
                left: 50.,
                top: 40.,
                right: 160.,
                bottom: 60.,
            }],
        }),
    );
    e.annotation_pages(1, revision, vec![0]);
    let Event::Annotations { annotations, .. } =
        wait(&e, |x| matches!(x, Event::Annotations { .. }))
    else {
        unreachable!()
    };
    let ordinal = annotations[0].ordinal;
    let revision = changed(
        &e,
        revision,
        2,
        EditCommand::Color {
            page: 0,
            ordinal,
            rgb: [0., 1., 0.],
        },
    );
    assert!(e.edit(
        1,
        0,
        3,
        103,
        EditCommand::DeleteAnnotation { page: 0, ordinal }
    ));
    assert!(matches!(
        wait(&e, |x| matches!(x, Event::FileError { operation: 3, .. })),
        Event::FileError { .. }
    ));
    let revision = changed(
        &e,
        revision,
        4,
        EditCommand::DeleteAnnotation { page: 0, ordinal },
    );
    let revision = changed(
        &e,
        revision,
        5,
        EditCommand::Insert {
            paths: vec![fixture("text.pdf"), fixture("text.pdf")],
            at: 1,
        },
    );
    let revision = changed(&e, revision, 6, EditCommand::DeletePages { pages: vec![2] });
    let occupied = dir.join("source-part-2.pdf");
    std::fs::write(&occupied, b"keep").unwrap();
    assert!(e.split(1, revision, 7, dir.clone(), 0));
    assert!(matches!(
        wait(&e, |x| matches!(x, Event::FileError { operation: 7, .. })),
        Event::FileError { .. }
    ));
    assert!(!dir.join("source-part-1.pdf").exists());
    assert_eq!(std::fs::read(occupied).unwrap(), b"keep");
    let unsupported = dir.join("unsupported.pdf");
    let mut foreign = mupdf::pdf::PdfDocument::new();
    let mut page = foreign.new_page((300., 400.)).unwrap();
    let mut a = page
        .create_annotation(mupdf::pdf::PdfAnnotationType::Square)
        .unwrap();
    a.set_rect(mupdf::Rect {
        x0: 10.,
        y0: 10.,
        x1: 40.,
        y1: 40.,
    })
    .unwrap();
    a.update().unwrap();
    a.object()
        .dict_put(
            "A",
            foreign
                .new_object_from_str("<< /S /JavaScript /JS (blocked) >>")
                .unwrap(),
        )
        .unwrap();
    drop(a);
    drop(page);
    foreign.save(unsupported.to_str().unwrap()).unwrap();
    drop(foreign);
    assert!(e.edit(
        1,
        revision,
        8,
        108,
        EditCommand::Insert {
            paths: vec![fixture("text.pdf"), unsupported],
            at: 0
        }
    ));
    assert!(matches!(
        wait(&e, |x| matches!(x, Event::FileError { operation: 8, .. })),
        Event::FileError { .. }
    ));
    // Failed import leaves both history revision and page count unchanged.
    let target = dir.join("after-failed-import.pdf");
    assert!(e.extract(1, revision, 9, target.clone(), vec![0, 1]));
    assert!(matches!(
        wait(&e, |x| matches!(
            x,
            Event::FileDone { operation: 9, .. } | Event::FileError { operation: 9, .. }
        )),
        Event::FileDone { .. }
    ));
    assert_eq!(
        mupdf::Document::open(target.to_str().unwrap())
            .unwrap()
            .page_count()
            .unwrap(),
        2
    );
    drop(e);
    cleanup(dir);
}

#[test]
fn native_journal_save_as_rename_and_undo_preserve_encryption() {
    let dir = folder();
    let source = dir.join("encrypted.pdf");
    std::fs::copy(fixture("encrypted.pdf"), &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let e = Engine::spawn(1);
    e.open_with_password(1, 1, source.clone(), Some("pdfree-test".into()));
    wait(&e, |x| matches!(x, Event::Opened { .. }));
    let revision = changed(
        &e,
        0,
        1,
        EditCommand::Add(Annotation::Note {
            page: 0,
            x: 50.,
            y: 50.,
            text: "Persist and undo".into(),
        }),
    );
    let copy = dir.join("saved-copy.pdf");
    assert!(e.save_as(1, 2, copy.clone(), vec![]));
    assert!(matches!(
        wait(&e, |x| matches!(
            x,
            Event::FileDone { operation: 2, .. } | Event::FileError { operation: 2, .. }
        )),
        Event::FileDone { .. }
    ));
    assert_eq!(std::fs::read(source).unwrap(), original);
    assert!(e.rename(1, 3, "renamed.pdf".into()));
    assert!(matches!(
        wait(&e, |x| matches!(
            x,
            Event::FileDone { operation: 3, .. } | Event::FileError { operation: 3, .. }
        )),
        Event::FileDone { .. }
    ));
    assert!(!copy.exists());
    let renamed = dir.join("renamed.pdf");
    let mut native = mupdf::pdf::PdfDocument::open(renamed.to_str().unwrap()).unwrap();
    assert!(native.needs_password().unwrap());
    assert!(native.authenticate("pdfree-test").unwrap());
    assert_eq!(native.load_pdf_page(0).unwrap().annotations().count(), 1);
    drop(native);
    changed(&e, revision, 4, EditCommand::Undo);
    assert!(e.save(1, 5));
    assert!(matches!(
        wait(&e, |x| matches!(
            x,
            Event::FileDone { operation: 5, .. } | Event::FileError { operation: 5, .. }
        )),
        Event::FileDone { .. }
    ));
    let mut native = mupdf::pdf::PdfDocument::open(renamed.to_str().unwrap()).unwrap();
    assert!(native.authenticate("pdfree-test").unwrap());
    assert_eq!(native.load_pdf_page(0).unwrap().annotations().count(), 0);
    drop(native);
    drop(e);
    cleanup(dir);
}
#[test]
fn coordinator_native_edits_history_save_and_exports() {
    let folder = folder();
    let source = folder.join("source.pdf");
    std::fs::copy(fixture("text.pdf"), &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let e = Engine::spawn(2);
    e.open(1, 1, source.clone());
    wait(&e, |x| matches!(x, Event::Opened { .. }));
    assert!(e.edit(
        1,
        0,
        1,
        2,
        EditCommand::Add(Annotation::Note {
            page: 0,
            x: 50.,
            y: 50.,
            text: "Editable saved note".into()
        })
    ));
    let Event::Changed {
        revision,
        dirty,
        can_undo,
        ..
    } = wait(&e, |x| {
        matches!(
            x,
            Event::Changed { operation: 1, .. } | Event::FileError { operation: 1, .. }
        )
    })
    else {
        panic!("edit failed")
    };
    assert!(dirty && can_undo);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    e.annotation_pages(1, revision, vec![0]);
    let Event::Annotations { annotations, .. } =
        wait(&e, |x| matches!(x, Event::Annotations { .. }))
    else {
        unreachable!()
    };
    assert_eq!(annotations.len(), 1);
    assert!(e.edit(
        1,
        revision,
        2,
        3,
        EditCommand::UpdateNote {
            page: 0,
            ordinal: annotations[0].ordinal,
            text: "Revised café".into()
        }
    ));
    let Event::Changed { revision, .. } =
        wait(&e, |x| matches!(x, Event::Changed { operation: 2, .. }))
    else {
        unreachable!()
    };
    assert!(e.edit(1, revision, 3, 4, EditCommand::Blank { at: 1 }));
    let Event::Changed { revision, info, .. } =
        wait(&e, |x| matches!(x, Event::Changed { operation: 3, .. }))
    else {
        unreachable!()
    };
    assert_eq!(info.pages, 2);
    assert!(e.edit(
        1,
        revision,
        4,
        5,
        EditCommand::Rotate {
            pages: vec![0],
            degrees: 90
        }
    ));
    let Event::Changed { revision, .. } =
        wait(&e, |x| matches!(x, Event::Changed { operation: 4, .. }))
    else {
        unreachable!()
    };
    assert!(e.edit(1, revision, 5, 6, EditCommand::Move { page: 0, to: 1 }));
    let Event::Changed { revision, .. } =
        wait(&e, |x| matches!(x, Event::Changed { operation: 5, .. }))
    else {
        unreachable!()
    };
    assert!(e.edit(1, revision, 6, 7, EditCommand::Undo));
    let Event::Changed {
        revision, can_redo, ..
    } = wait(&e, |x| matches!(x, Event::Changed { operation: 6, .. }))
    else {
        unreachable!()
    };
    assert!(can_redo);
    assert!(e.edit(1, revision, 7, 8, EditCommand::Redo));
    let Event::Changed { revision, .. } =
        wait(&e, |x| matches!(x, Event::Changed { operation: 7, .. }))
    else {
        unreachable!()
    };
    assert!(e.save(1, 8));
    assert!(matches!(
        wait(&e, |x| matches!(
            x,
            Event::FileDone { operation: 8, .. } | Event::FileError { operation: 8, .. }
        )),
        Event::FileDone {
            kind: FileAction::Save,
            ..
        }
    ));
    assert!(e.extract(1, revision, 9, folder.join("extracted.pdf"), vec![1]));
    assert!(matches!(
        wait(&e, |x| matches!(
            x,
            Event::FileDone { operation: 9, .. } | Event::FileError { operation: 9, .. }
        )),
        Event::FileDone {
            kind: FileAction::Extract,
            ..
        }
    ));
    assert!(e.split(1, revision, 10, folder.clone(), 0));
    assert!(matches!(
        wait(&e, |x| matches!(
            x,
            Event::FileDone { operation: 10, .. } | Event::FileError { operation: 10, .. }
        )),
        Event::FileDone {
            kind: FileAction::Split,
            ..
        }
    ));
    drop(e);
    cleanup(folder);
}
#[test]
fn imports_links_popup_appearance_and_namespaced_form_tree() {
    let dir = folder();
    let source = dir.join("source.pdf");
    std::fs::copy(fixture("text.pdf"), &source).unwrap();
    let engine = Engine::spawn(1);
    engine.open(1, 1, source);
    wait(&engine, |event| matches!(event, Event::Opened { .. }));
    let revision = changed(
        &engine,
        0,
        1,
        EditCommand::Insert {
            paths: vec![fixture("interactive-merge.pdf")],
            at: 1,
        },
    );
    let output = dir.join("interactive-merged.pdf");
    assert!(engine.save_as(1, 2, output.clone(), vec![]));
    assert!(matches!(
        wait(&engine, |event| matches!(
            event,
            Event::FileDone { operation: 2, .. } | Event::FileError { operation: 2, .. }
        )),
        Event::FileDone { .. }
    ));
    let pdf = mupdf::pdf::PdfDocument::open(output.to_str().unwrap()).unwrap();
    assert_eq!(pdf.page_count().unwrap(), 3);
    let page = pdf.find_page(1).unwrap();
    let annots = page.get_dict("Annots").unwrap().unwrap();
    assert_eq!(annots.len().unwrap(), 7);
    let internal = annots.get_array(1).unwrap().unwrap();
    let dest = internal.get_dict("Dest").unwrap().unwrap();
    assert_eq!(
        pdf.lookup_page_number(&dest.get_array(0).unwrap().unwrap())
            .unwrap(),
        2
    );
    let named = annots
        .get_array(6)
        .unwrap()
        .unwrap()
        .get_dict("A")
        .unwrap()
        .unwrap()
        .get_dict("D")
        .unwrap()
        .unwrap();
    assert_eq!(
        pdf.lookup_page_number(&named.get_array(0).unwrap().unwrap())
            .unwrap(),
        2
    );
    let note = annots.get_array(2).unwrap().unwrap();
    let popup = annots.get_array(3).unwrap().unwrap();
    assert_eq!(
        note.get_dict("Popup")
            .unwrap()
            .unwrap()
            .as_indirect()
            .unwrap(),
        popup.as_indirect().unwrap()
    );
    assert_eq!(
        popup
            .get_dict("Parent")
            .unwrap()
            .unwrap()
            .as_indirect()
            .unwrap(),
        note.as_indirect().unwrap()
    );
    let square = annots.get_array(4).unwrap().unwrap();
    let appearance = square
        .get_dict("AP")
        .unwrap()
        .unwrap()
        .get_dict("N")
        .unwrap()
        .unwrap();
    assert!(appearance.read_stream().unwrap().starts_with(b"q 1 0 0 RG"));
    assert_eq!(
        appearance
            .get_dict("Resources")
            .unwrap()
            .unwrap()
            .get_dict("Font")
            .unwrap()
            .unwrap()
            .get_dict("A")
            .unwrap()
            .unwrap()
            .get_dict("BaseFont")
            .unwrap()
            .unwrap()
            .as_name()
            .unwrap(),
        b"Helvetica"
    );
    assert_eq!(
        appearance.as_indirect().unwrap(),
        square
            .get_dict("AP")
            .unwrap()
            .unwrap()
            .get_dict("R")
            .unwrap()
            .unwrap()
            .as_indirect()
            .unwrap()
    );
    let widget = annots.get_array(5).unwrap().unwrap();
    assert_eq!(
        pdf.lookup_page_number(&widget.get_dict("P").unwrap().unwrap())
            .unwrap(),
        1
    );
    let root = pdf.trailer().unwrap().get_dict("Root").unwrap().unwrap();
    let form = root.get_dict("AcroForm").unwrap().unwrap();
    let parent = form
        .get_dict("Fields")
        .unwrap()
        .unwrap()
        .get_array(0)
        .unwrap()
        .unwrap();
    assert!(
        parent
            .get_dict("T")
            .unwrap()
            .unwrap()
            .as_string()
            .unwrap()
            .starts_with("PDFree_import_")
    );
    assert_eq!(
        parent
            .get_dict("Kids")
            .unwrap()
            .unwrap()
            .get_array(0)
            .unwrap()
            .unwrap()
            .as_indirect()
            .unwrap(),
        widget.as_indirect().unwrap()
    );
    assert_eq!(
        widget.get_dict("V").unwrap().unwrap().as_string().unwrap(),
        "Preserved café"
    );
    assert!(
        form.get_dict("DR")
            .unwrap()
            .unwrap()
            .get_dict("Font")
            .unwrap()
            .unwrap()
            .get_dict("Helv")
            .unwrap()
            .is_some()
    );
    drop(pdf);
    // A second form with conflicting global font resource names fails atomically.
    assert!(engine.edit(
        1,
        revision,
        3,
        103,
        EditCommand::Insert {
            paths: vec![fixture("interactive-merge.pdf")],
            at: 0,
        }
    ));
    assert!(matches!(
        wait(&engine, |event| matches!(
            event,
            Event::Changed { operation: 3, .. } | Event::FileError { operation: 3, .. }
        )),
        Event::FileError { .. }
    ));
    changed(&engine, revision, 4, EditCommand::Undo);
    drop(engine);
    cleanup(dir);
}
