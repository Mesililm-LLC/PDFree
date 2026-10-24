use pdf_engine::{EditCommand, Engine, Event};
use std::{path::PathBuf, time::Duration};
use viewer_core::{FieldKind, FieldSpec, FormEdit, FormValue, TextRect};
fn wait(e: &Engine, f: impl Fn(&Event) -> bool) -> Event {
    loop {
        let event = e
            .events
            .recv_timeout(Duration::from_secs(30))
            .expect("engine event");
        if let Event::FileError { message, .. } = &event {
            panic!("{message}");
        }
        if f(&event) {
            return event;
        }
    }
}
fn changed(e: &Engine) -> u64 {
    match wait(e, |v| matches!(v, Event::Changed { .. })) {
        Event::Changed { revision, .. } => revision,
        _ => unreachable!(),
    }
}
fn fields(e: &Engine, revision: u64) -> Vec<viewer_core::FormField> {
    e.form_pages(1, revision, vec![0]);
    match wait(e, |v| matches!(v, Event::Forms { .. })) {
        Event::Forms { fields, .. } => fields,
        _ => unreachable!(),
    }
}
fn field(kind: FieldKind, name: &str, y: f32) -> FieldSpec {
    FieldSpec {
        page: 0,
        name: name.into(),
        label: name.into(),
        kind,
        rect: TextRect {
            left: 40.,
            top: y,
            right: 240.,
            bottom: y + 24.,
        },
        required: false,
        read_only: false,
        options: if matches!(kind, FieldKind::Dropdown | FieldKind::List) {
            vec!["Red".into(), "Green".into(), "Blue".into()]
        } else {
            vec![]
        },
        group: if kind == FieldKind::Radio {
            "radios".into()
        } else {
            String::new()
        },
        review: false,
        date_format: "YYYY-MM-DD".into(),
        decimals: 2,
    }
}
#[test]
fn choices_checkboxes_radio_groups_and_conversions_are_transactional() {
    let e = Engine::spawn(1);
    e.open(
        1,
        1,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/blank.pdf"),
    );
    wait(&e, |v| matches!(v, Event::Opened { .. }));
    let specs = vec![
        field(FieldKind::Checkbox, "check", 40.),
        field(FieldKind::Dropdown, "color", 80.),
        field(FieldKind::List, "colors", 120.),
        field(FieldKind::Radio, "first", 180.),
        field(FieldKind::Radio, "second", 220.),
        field(FieldKind::Text, "date", 260.),
    ];
    assert!(e.edit(1, 0, 1, 2, EditCommand::Form(FormEdit::Create(specs))));
    let mut rev = changed(&e);
    for (i, name, value) in [
        (2, "check", FormValue::Text("Yes".into())),
        (3, "color", FormValue::Text("Green".into())),
        (
            4,
            "colors",
            FormValue::Selection(vec!["Red".into(), "Blue".into()]),
        ),
        (5, "radios", FormValue::Text("second".into())),
    ] {
        let f = fields(&e, rev)
            .into_iter()
            .find(|f| f.spec.name == name)
            .unwrap();
        assert!(e.edit(
            1,
            rev,
            i,
            i + 1,
            EditCommand::Form(FormEdit::SetValue {
                id: f.id,
                value: value.clone()
            })
        ));
        rev = changed(&e);
        let updated = fields(&e, rev);
        assert!(
            updated
                .iter()
                .filter(|f| f.spec.name == name)
                .all(|f| f.value == value)
        );
    }
    let before = fields(&e, rev);
    assert_eq!(before.iter().filter(|f| f.spec.name == "radios").count(), 2);
    let f = before.iter().find(|f| f.spec.name == "date").unwrap();
    let mut spec = f.spec.clone();
    spec.kind = FieldKind::Date;
    assert!(e.edit(
        1,
        rev,
        6,
        7,
        EditCommand::Form(FormEdit::Update { id: f.id, spec })
    ));
    rev = changed(&e);
    assert!(
        fields(&e, rev)
            .iter()
            .any(|f| f.spec.kind == FieldKind::Date)
    );
    assert!(e.edit(1, rev, 7, 8, EditCommand::Undo));
    rev = changed(&e);
    assert!(
        !fields(&e, rev)
            .iter()
            .any(|f| f.spec.kind == FieldKind::Date)
    );
}
#[test]
fn visual_signature_slot_and_mark_are_distinct_from_digital_signatures() {
    let e = Engine::spawn(1);
    e.open(
        1,
        1,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/blank.pdf"),
    );
    wait(&e, |v| matches!(v, Event::Opened { .. }));
    assert!(e.edit(
        1,
        0,
        1,
        2,
        EditCommand::Form(FormEdit::Create(vec![field(
            FieldKind::VisualSignature,
            "sign_here",
            40.
        )]))
    ));
    let rev = changed(&e);
    let f = fields(&e, rev).remove(0);
    assert_eq!(f.spec.kind, FieldKind::VisualSignature);
    assert!(e.edit(
        1,
        rev,
        2,
        3,
        EditCommand::Form(FormEdit::SetVisualSignature {
            id: f.id,
            mark: viewer_core::SignatureMark::Typed("Michael".into())
        })
    ));
    let rev = changed(&e);
    assert!(!fields(&e, rev)[0].value.is_empty());
    assert!(e.edit(1, rev, 3, 4, EditCommand::Undo));
    let rev = changed(&e);
    assert!(fields(&e, rev)[0].value.is_empty());
}
#[test]
fn fields_create_fill_undo_save_and_reopen() {
    let e = Engine::spawn(1);
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/blank.pdf");
    e.open(1, 1, fixture);
    wait(&e, |v| matches!(v, Event::Opened { .. }));
    let spec = FieldSpec {
        page: 0,
        name: "contact".into(),
        label: "Full name".into(),
        kind: FieldKind::Text,
        rect: TextRect {
            left: 40.,
            top: 40.,
            right: 250.,
            bottom: 66.,
        },
        required: true,
        read_only: false,
        options: vec![],
        group: String::new(),
        review: false,
        date_format: "YYYY-MM-DD".into(),
        decimals: 2,
    };
    assert!(e.edit(1, 0, 1, 2, EditCommand::Form(FormEdit::Create(vec![spec]))));
    let rev = match wait(&e, |v| matches!(v, Event::Changed { .. })) {
        Event::Changed { revision, .. } => revision,
        _ => unreachable!(),
    };
    e.form_pages(1, rev, vec![0]);
    let fields = match wait(&e, |v| matches!(v, Event::Forms { .. })) {
        Event::Forms { fields, .. } => fields,
        _ => unreachable!(),
    };
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].spec.label, "Full name");
    assert!(e.edit(
        1,
        rev,
        2,
        3,
        EditCommand::Form(FormEdit::SetValue {
            id: fields[0].id,
            value: FormValue::Text("Zoë 東京".into())
        })
    ));
    let rev = match wait(&e, |v| matches!(v, Event::Changed { .. })) {
        Event::Changed { revision, .. } => revision,
        _ => unreachable!(),
    };
    let output =
        std::env::temp_dir().join(format!("pdfree-forms-{}-{}.pdf", std::process::id(), rev));
    assert!(!output.exists());
    assert!(e.save_as(1, 3, output.clone(), vec![]));
    wait(&e, |v| matches!(v, Event::FileDone { .. }));
    let pdf = mupdf::Document::open(output.to_str().unwrap())
        .and_then(mupdf::pdf::PdfDocument::try_from)
        .unwrap();
    assert_eq!(
        pdf.trailer()
            .unwrap()
            .get_dict("Root")
            .unwrap()
            .unwrap()
            .get_dict("AcroForm")
            .unwrap()
            .unwrap()
            .get_dict("Fields")
            .unwrap()
            .unwrap()
            .len()
            .unwrap(),
        1,
        "widget must belong to the catalog field tree"
    );
    drop(pdf);
    e.open(2, 4, output.clone());
    wait(&e, |v| matches!(v, Event::Opened { document: 2, .. }));
    e.form_pages(2, 0, vec![0]);
    let fields = match wait(&e, |v| matches!(v, Event::Forms { document: 2, .. })) {
        Event::Forms { fields, .. } => fields,
        _ => unreachable!(),
    };
    assert_eq!(fields[0].value.text(), "Zoë 東京");
    drop(e);
    std::fs::remove_file(output).unwrap();
}
