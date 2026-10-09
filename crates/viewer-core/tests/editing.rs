use viewer_core::{Annotation, EditHistory, TextRect, validate_pdf_name};

fn note(text: &str) -> Annotation {
    Annotation::Note {
        page: 0,
        x: 20.0,
        y: 30.0,
        text: text.into(),
    }
}

#[test]
fn journal_undo_redo_checkpoint_and_branch_preserve_annotation_contents() {
    let mut history = EditHistory::default();
    assert!(!history.dirty());
    history.push(vec![note("Hello 😀\nSecond line")]).unwrap();
    history.mark_saved();
    history.push(vec![note("Changed")]).unwrap();
    assert!(history.dirty());
    assert!(history.undo());
    assert!(!history.dirty());
    assert_eq!(history.annotations(), &[note("Hello 😀\nSecond line")]);
    assert!(history.redo());
    assert!(history.undo());
    history.push(vec![note("New branch")]).unwrap();
    assert!(!history.can_redo());
    assert!(history.dirty());
}

#[test]
fn history_limits_and_invalid_annotations_do_not_corrupt_the_current_state() {
    let mut history = EditHistory::default();
    assert!(history.push(vec![note(&"x".repeat(16_385))]).is_err());
    assert!(
        history
            .push(vec![Annotation::Highlight {
                page: 0,
                rects: vec![TextRect {
                    left: f32::NAN,
                    top: 0.0,
                    right: 10.0,
                    bottom: 10.0
                }]
            }])
            .is_err()
    );
    assert!(history.push(vec![note("x"); 2001]).is_err());
    for i in 0..250 {
        history.push(vec![note(&i.to_string())]).unwrap();
    }
    let mut undos = 0;
    while history.undo() {
        undos += 1;
    }
    assert_eq!(undos, 200);
    assert!(!history.annotations().is_empty());
}

#[test]
fn filename_rules_preserve_unicode_and_pdf_extension_and_reject_windows_hazards() {
    assert_eq!(validate_pdf_name("Résumé 😀"), Ok("Résumé 😀.pdf".into()));
    assert_eq!(validate_pdf_name("Report.PDF"), Ok("Report.PDF".into()));
    for bad in [
        "",
        " ",
        ".",
        "..",
        "report.",
        "report ",
        "CON",
        "nul.pdf",
        "COM1.pdf",
        "LPT9.notes.pdf",
        "a/b",
        "a\\b",
        "C:report",
        "a?b",
        "a\0b",
    ] {
        assert!(validate_pdf_name(bad).is_err(), "accepted {bad:?}");
    }
}
