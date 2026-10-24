use viewer_core::{FieldKind, FieldSpec, FormValue, TextRect};

fn spec(kind: FieldKind) -> FieldSpec {
    FieldSpec {
        page: 0,
        name: "name".into(),
        label: "Name".into(),
        kind,
        rect: TextRect {
            left: 20.,
            top: 20.,
            right: 180.,
            bottom: 44.,
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

#[test]
fn typed_values_reject_invalid_input_without_coercion() {
    let number = spec(FieldKind::Number);
    assert!(
        number
            .validate_value(&FormValue::Text("NaN".into()))
            .is_err()
    );
    assert!(
        number
            .validate_value(&FormValue::Text("1,23".into()))
            .is_err()
    );
    assert!(
        number
            .validate_value(&FormValue::Text("-123456789012345678.25".into()))
            .is_ok()
    );
    let date = spec(FieldKind::Date);
    assert!(
        date.validate_value(&FormValue::Text("2026-02-29".into()))
            .is_err()
    );
    assert!(
        date.validate_value(&FormValue::Text("2028-02-29".into()))
            .is_ok()
    );
    assert!(date.validate_value(&FormValue::Text(String::new())).is_ok());
}

#[test]
fn field_validation_rejects_degenerate_geometry_and_bad_options() {
    let mut f = spec(FieldKind::Text);
    f.rect.right = f.rect.left;
    assert!(f.validate().is_err());
    f = spec(FieldKind::Dropdown);
    assert!(f.validate().is_err());
    f.options = vec!["One".into(), "One".into()];
    assert!(f.validate().is_err());
    f.options = vec!["One".into(), "Two".into()];
    assert!(f.validate().is_ok());
    assert!(f.validate_value(&FormValue::Text("Three".into())).is_err());
}

#[test]
fn literal_off_is_a_real_text_value_and_image_headers_are_bounded() {
    assert!(!FormValue::Text("Off".into()).is_empty());
    assert!(
        spec(FieldKind::Text)
            .validate_value(&FormValue::Text("Off".into()))
            .is_ok()
    );
    let mut png = vec![0_u8; 24];
    png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    png[12..16].copy_from_slice(b"IHDR");
    png[16..20].copy_from_slice(&9000_u32.to_be_bytes());
    png[20..24].copy_from_slice(&10_u32.to_be_bytes());
    assert!(viewer_core::signature_image_size(&png).is_err());
}
