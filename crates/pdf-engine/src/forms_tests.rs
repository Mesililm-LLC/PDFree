use super::*;
use mupdf::{
    Colorspace, Matrix,
    pdf::{Encryption, PdfWriteOptions},
};
use viewer_core::SignatureMark;

fn spec(kind: FieldKind, name: &str, top: f32) -> FieldSpec {
    FieldSpec {
        page: 0,
        name: name.into(),
        label: name.into(),
        kind,
        rect: TextRect {
            left: 40.,
            top,
            right: 260.,
            bottom: top + 35.,
        },
        required: false,
        read_only: false,
        options: vec![],
        group: String::new(),
        review: false,
        date_format: "YYYY-MM-DD".into(),
        decimals: 2,
    }
}
fn blank() -> PdfDocument {
    let mut d = PdfDocument::new();
    d.new_page((500., 500.)).unwrap();
    d
}
fn bytes(doc: &PdfDocument) -> Vec<u8> {
    let mut b = vec![];
    doc.write_to_with_options(&mut b, crate::files::options())
        .unwrap();
    b
}
fn export(doc: &PdfDocument, name: &str) {
    if let Some(dir) = std::env::var_os("PDFREE_FORM_EVIDENCE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        doc.save_with_options(
            std::path::Path::new(&dir).join(name).to_str().unwrap(),
            crate::files::options(),
        )
        .unwrap();
    }
}
#[test]
fn shared_widgets_keep_field_identity_values_and_appearances_across_pages() {
    let mut d = blank();
    d.new_page((500., 500.)).unwrap();
    let first = spec(FieldKind::Text, "first", 40.);
    let mut second = spec(FieldKind::Text, "second", 80.);
    second.page = 1;
    apply(&mut d, &FormEdit::Create(vec![first, second])).unwrap();
    let a = page(&d, 0).unwrap().remove(0);
    let b = page(&d, 1).unwrap().remove(0);
    let mut parent = d
        .new_object_from_str("<< /FT /Tx /T (linked) /DA (/Helv 12 Tf 0 g) >>")
        .unwrap();
    let mut kids = d.new_array().unwrap();
    for id in [a.id, b.id] {
        kids.array_push(d.new_indirect(id.widget, 0).unwrap())
            .unwrap();
    }
    parent.dict_put("Kids", kids).unwrap();
    let parent = d.add_object(&parent).unwrap();
    for id in [a.id, b.id] {
        let mut child = d.new_indirect(id.widget, 0).unwrap();
        for key in ["T", "FT", "V"] {
            child.dict_delete(key).unwrap();
        }
        child
            .dict_put("Parent", parent.try_clone().unwrap())
            .unwrap();
        child.dict_delete("P").unwrap();
    }
    let mut form = root(&d).unwrap().get_dict("AcroForm").unwrap().unwrap();
    let mut fields = d.new_array().unwrap();
    fields.array_push(parent).unwrap();
    form.dict_put("Fields", fields).unwrap();
    let a = page(&d, 0).unwrap().remove(0);
    let b = page(&d, 1).unwrap().remove(0);
    assert_eq!(a.id.field, b.id.field);
    assert_ne!(a.id.widget, b.id.widget);
    assert!(!a.authorable);
    apply(
        &mut d,
        &FormEdit::SetValue {
            id: a.id,
            value: FormValue::Text("Zoë 東京".into()),
        },
    )
    .unwrap();
    for p in 0..2 {
        assert_eq!(page(&d, p).unwrap()[0].value.text(), "Zoë 東京");
        assert!(
            !appearances(&d, page(&d, p).unwrap()[0].id)
                .unwrap()
                .is_empty()
        );
    }
    let saved = PdfDocument::from_bytes(&bytes(&d)).unwrap();
    verify(&d, &saved).unwrap();
    export(&saved, "linked-widgets.pdf");
}
#[test]
fn encrypted_form_filling_is_separate_from_authoring_and_keeps_encryption() {
    let mut d = blank();
    apply(
        &mut d,
        &FormEdit::Create(vec![spec(FieldKind::Text, "restricted", 40.)]),
    )
    .unwrap();
    let mut options = PdfWriteOptions::default();
    options
        .set_encryption(Encryption::Aes256)
        .set_owner_password("owner")
        .set_user_password("reader")
        .set_permissions(Permission::FORM | Permission::PRINT);
    let mut data = vec![];
    d.write_to_with_options(&mut data, options).unwrap();
    let mut encrypted = PdfDocument::from_bytes(&data).unwrap();
    assert!(encrypted.authenticate("reader").unwrap());
    assert!(permissions(&encrypted).contains(Permission::FORM));
    assert!(!permissions(&encrypted).contains(Permission::ANNOTATE));
    let f = page(&encrypted, 0).unwrap().remove(0);
    assert!(f.editable);
    assert!(!f.authorable);
    apply(
        &mut encrypted,
        &FormEdit::SetValue {
            id: f.id,
            value: FormValue::Text("Off".into()),
        },
    )
    .unwrap();
    assert!(
        apply(
            &mut encrypted,
            &FormEdit::Create(vec![spec(FieldKind::Text, "blocked", 90.)])
        )
        .is_err()
    );
    let data = bytes(&encrypted);
    let mut saved = PdfDocument::from_bytes(&data).unwrap();
    assert!(saved.needs_password().unwrap());
    assert!(!saved.authenticate("wrong").unwrap());
    assert!(saved.authenticate("reader").unwrap());
    verify(&encrypted, &saved).unwrap();
    export(&saved, "encrypted-filled.pdf");
}
#[test]
fn scripted_xfa_and_malformed_trees_fail_closed() {
    let mut d = blank();
    apply(
        &mut d,
        &FormEdit::Create(vec![spec(FieldKind::Text, "scripted", 40.)]),
    )
    .unwrap();
    let f = page(&d, 0).unwrap().remove(0);
    let mut o = d.new_indirect(f.id.widget, 0).unwrap();
    o.dict_put(
        "AA",
        d.new_object_from_str("<< /K << /S /JavaScript /JS (app.alert\\(1\\)) >> >>")
            .unwrap(),
    )
    .unwrap();
    assert!(!page(&d, 0).unwrap()[0].editable);
    assert!(
        apply(
            &mut d,
            &FormEdit::SetValue {
                id: f.id,
                value: FormValue::Text("x".into())
            }
        )
        .is_err()
    );
    o.dict_delete("AA").unwrap();
    let mut form = root(&d).unwrap().get_dict("AcroForm").unwrap().unwrap();
    form.dict_put("XFA", d.new_string("unsupported").unwrap())
        .unwrap();
    assert!(ensure_writable(&d).is_err());
    form.dict_delete("XFA").unwrap();
    let mut kids = d.new_array().unwrap();
    kids.array_push(d.new_indirect(f.id.field, 0).unwrap())
        .unwrap();
    o.dict_put("Kids", kids).unwrap();
    assert!(security(&d).is_err());
}
#[test]
fn conversion_preserves_literal_off_and_managed_radio_delete_preserves_tree() {
    let mut d = blank();
    apply(
        &mut d,
        &FormEdit::Create(vec![spec(FieldKind::Text, "literal", 40.)]),
    )
    .unwrap();
    let f = page(&d, 0).unwrap().remove(0);
    apply(
        &mut d,
        &FormEdit::SetValue {
            id: f.id,
            value: FormValue::Text("Off".into()),
        },
    )
    .unwrap();
    let mut date = f.spec;
    date.kind = FieldKind::Date;
    assert!(
        apply(
            &mut d,
            &FormEdit::Update {
                id: f.id,
                spec: date
            }
        )
        .is_err()
    );
    assert_eq!(page(&d, 0).unwrap()[0].value.text(), "Off");
    let mut one = spec(FieldKind::Radio, "one", 100.);
    one.group = "group".into();
    let mut two = one.clone();
    two.name = "two".into();
    two.rect.top += 40.;
    two.rect.bottom += 40.;
    apply(&mut d, &FormEdit::Create(vec![one, two])).unwrap();
    let radio = page(&d, 0)
        .unwrap()
        .into_iter()
        .find(|f| f.spec.kind == FieldKind::Radio)
        .unwrap();
    assert!(radio.authorable);
    let mut moved = radio.spec.clone();
    moved.rect.left += 20.;
    moved.rect.right += 20.;
    apply(
        &mut d,
        &FormEdit::Update {
            id: radio.id,
            spec: moved,
        },
    )
    .unwrap();
    apply(&mut d, &FormEdit::Delete(radio.id)).unwrap();
    assert_eq!(
        page(&d, 0)
            .unwrap()
            .iter()
            .filter(|f| f.spec.kind == FieldKind::Radio)
            .count(),
        1
    );
    verify(&d, &PdfDocument::from_bytes(&bytes(&d)).unwrap()).unwrap();
}
#[test]
fn visual_signatures_render_after_crop_rotation_resize_and_round_trip() {
    let mut d = blank();
    let mut p = d.find_page(0).unwrap();
    p.dict_put("CropBox", d.new_object_from_str("[10 20 490 480]").unwrap())
        .unwrap();
    p.dict_put("Rotate", d.new_int(90).unwrap()).unwrap();
    let marks = [
        SignatureMark::Typed("Michael".into()),
        SignatureMark::Strokes(vec![vec![[0.1, 0.8], [0.3, 0.2], [0.6, 0.7], [0.9, 0.3]]]),
    ];
    for (i, mark) in marks.into_iter().enumerate() {
        let s = spec(
            FieldKind::VisualSignature,
            &format!("visual_{i}"),
            40. + i as f32 * 100.,
        );
        apply(&mut d, &FormEdit::Create(vec![s])).unwrap();
        let f = page(&d, 0)
            .unwrap()
            .into_iter()
            .find(|f| f.spec.name == format!("visual_{i}"))
            .unwrap();
        apply(&mut d, &FormEdit::SetVisualSignature { id: f.id, mark }).unwrap();
    }
    let mut png = vec![];
    {
        let mut encoder = png::Encoder::new(&mut png, 40, 20);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[20, 40, 80].repeat(800))
            .unwrap();
    }
    apply(
        &mut d,
        &FormEdit::Create(vec![spec(FieldKind::VisualSignature, "image", 240.)]),
    )
    .unwrap();
    let f = page(&d, 0)
        .unwrap()
        .into_iter()
        .find(|f| f.spec.name == "image")
        .unwrap();
    apply(
        &mut d,
        &FormEdit::SetVisualSignature {
            id: f.id,
            mark: SignatureMark::Image(png),
        },
    )
    .unwrap();
    let saved = PdfDocument::from_bytes(&bytes(&d)).unwrap();
    verify(&d, &saved).unwrap();
    for scale in [1., 2.] {
        let pix = saved
            .load_page(0)
            .unwrap()
            .to_pixmap(
                &Matrix::new_scale(scale, scale),
                &Colorspace::device_rgb(),
                true,
                false,
            )
            .unwrap();
        for f in page(&saved, 0).unwrap() {
            let r = f.spec.rect;
            let mut ink = 0;
            for y in (r.top * scale) as usize..(r.bottom * scale) as usize {
                for x in (r.left * scale) as usize..(r.right * scale) as usize {
                    if pix.samples()[y * pix.stride() as usize + x * 3] < 180 {
                        ink += 1;
                    }
                }
            }
            assert!(ink > 20, "{} did not render at {scale}", f.spec.name);
        }
    }
    export(&saved, "visual-signatures-rotated.pdf");
}

#[test]
fn authoring_read_only_on_a_populated_field_keeps_its_value() {
    let mut d = blank();
    apply(
        &mut d,
        &FormEdit::Create(vec![spec(FieldKind::Text, "filled", 40.)]),
    )
    .unwrap();
    let f = page(&d, 0).unwrap().remove(0);
    apply(
        &mut d,
        &FormEdit::SetValue {
            id: f.id,
            value: FormValue::Text("Keep me".into()),
        },
    )
    .unwrap();
    let mut s = f.spec;
    s.read_only = true;
    apply(&mut d, &FormEdit::Update { id: f.id, spec: s }).unwrap();
    let f = page(&d, 0).unwrap().remove(0);
    assert_eq!(f.value.text(), "Keep me");
    assert!(!f.editable);
    verify(&d, &PdfDocument::from_bytes(&bytes(&d)).unwrap()).unwrap();
}
#[test]
fn imported_or_locked_radio_groups_cannot_gain_new_members() {
    let mut d = blank();
    let mut s = spec(FieldKind::Radio, "first", 40.);
    s.group = "existing".into();
    apply(&mut d, &FormEdit::Create(vec![s.clone()])).unwrap();
    let f = page(&d, 0).unwrap().remove(0);
    let mut group = d.new_indirect(f.id.field, 0).unwrap();
    group.dict_delete("PDFreeManaged").unwrap();
    s.name = "second".into();
    assert!(apply(&mut d, &FormEdit::Create(vec![s])).is_err());
}

#[test]
fn locked_visual_marks_and_digital_widgets_cannot_be_edited() {
    let mut d = blank();
    apply(
        &mut d,
        &FormEdit::Create(vec![
            spec(FieldKind::VisualSignature, "visual", 40.),
            spec(FieldKind::DigitalSignature, "digital", 100.),
        ]),
    )
    .unwrap();
    for f in page(&d, 0).unwrap() {
        let mut widget = d.new_indirect(f.id.widget, 0).unwrap();
        widget
            .dict_put("F", d.new_int(4 | 128 | 512).unwrap())
            .unwrap();
    }
    let fields = page(&d, 0).unwrap();
    for f in &fields {
        assert!(!f.editable && !f.authorable, "locked {}", f.spec.name);
    }
    let visual = fields
        .iter()
        .find(|f| f.spec.kind == FieldKind::VisualSignature)
        .unwrap();
    assert!(
        apply(
            &mut d,
            &FormEdit::SetVisualSignature {
                id: visual.id,
                mark: SignatureMark::Typed("Blocked".into())
            }
        )
        .is_err()
    );
    assert!(apply(&mut d, &FormEdit::Delete(visual.id)).is_err());
}
#[test]
fn list_selection_indices_are_unique_and_sorted_in_option_order() {
    let mut d = blank();
    let mut s = spec(FieldKind::List, "list", 40.);
    s.options = vec!["Red".into(), "Green".into(), "Blue".into()];
    apply(&mut d, &FormEdit::Create(vec![s])).unwrap();
    let f = page(&d, 0).unwrap().remove(0);
    apply(
        &mut d,
        &FormEdit::SetValue {
            id: f.id,
            value: FormValue::Selection(vec!["Blue".into(), "Red".into(), "Blue".into()]),
        },
    )
    .unwrap();
    let o = d.new_indirect(f.id.field, 0).unwrap();
    let indices = o.get_dict("I").unwrap().unwrap();
    assert_eq!(indices.len().unwrap(), 2);
    assert_eq!(indices.get_array(0).unwrap().unwrap().as_int().unwrap(), 0);
    assert_eq!(indices.get_array(1).unwrap().unwrap().as_int().unwrap(), 2);
    assert_eq!(
        page(&d, 0).unwrap()[0].value,
        FormValue::Selection(vec!["Red".into(), "Blue".into()])
    );
}

#[test]
fn authoring_preserves_existing_inherited_default_appearance() {
    let mut d = blank();
    apply(
        &mut d,
        &FormEdit::Create(vec![spec(FieldKind::Text, "original", 40.)]),
    )
    .unwrap();
    let mut form = root(&d).unwrap().get_dict("AcroForm").unwrap().unwrap();
    put_text(&d, &mut form, "DA", "/PDFreeHelv 18 Tf 1 0 0 rg").unwrap();
    apply(
        &mut d,
        &FormEdit::Create(vec![spec(FieldKind::Text, "new", 100.)]),
    )
    .unwrap();
    assert_eq!(
        string(
            &root(&d).unwrap().get_dict("AcroForm").unwrap().unwrap(),
            "DA"
        )
        .unwrap(),
        "/PDFreeHelv 18 Tf 1 0 0 rg"
    );
}
