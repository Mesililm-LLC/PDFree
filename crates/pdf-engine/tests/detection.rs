use mupdf::{Buffer, Colorspace, Image, Matrix, pdf::PdfDocument};
use pdf_engine::{Engine, Event};
use std::{path::PathBuf, time::Duration};
fn sample(scanned: bool) -> PathBuf {
    let mut doc = PdfDocument::new();
    doc.new_page((400., 300.)).unwrap();
    let font = doc
        .new_object_from_str("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
        .unwrap();
    let font = doc.add_object(&font).unwrap();
    let mut fonts = doc.new_dict().unwrap();
    fonts.dict_put("F1", font).unwrap();
    let mut resources = doc.new_dict().unwrap();
    resources.dict_put("Font", fonts).unwrap();
    let content = "BT /F1 12 Tf 20 242 Td (Name:) Tj 0 -60 Td (Date:) Tj ET 0.8 w 100 230 220 26 re S 100 170 220 26 re S";
    let stream = doc
        .add_stream(
            &Buffer::from_bytes(content.as_bytes()).unwrap(),
            None,
            false,
        )
        .unwrap();
    let mut p = doc.find_page(0).unwrap();
    p.dict_put("Resources", resources).unwrap();
    p.dict_put("Contents", stream).unwrap();
    if scanned {
        let pix = doc
            .load_page(0)
            .unwrap()
            .to_pixmap(
                &Matrix::new_scale(3., 3.),
                &Colorspace::device_rgb(),
                false,
                false,
            )
            .unwrap();
        let image = Image::from_pixmap(&pix).unwrap();
        let mut scan = PdfDocument::new();
        scan.new_page((400., 300.)).unwrap();
        let image = scan.add_image(&image).unwrap();
        let mut x = scan.new_dict().unwrap();
        x.dict_put("Scan", image).unwrap();
        let mut res = scan.new_dict().unwrap();
        res.dict_put("XObject", x).unwrap();
        let stream = scan
            .add_stream(
                &Buffer::from_bytes(b"q 400 0 0 300 0 0 cm /Scan Do Q").unwrap(),
                None,
                false,
            )
            .unwrap();
        let mut p = scan.find_page(0).unwrap();
        p.dict_put("Resources", res).unwrap();
        p.dict_put("Contents", stream).unwrap();
        doc = scan;
    }
    let p = std::env::temp_dir().join(format!(
        "pdfree-detect-{}-{scanned}.pdf",
        std::process::id()
    ));
    assert!(!p.exists());
    doc.save(p.to_str().unwrap()).unwrap();
    p
}
fn run(scanned: bool) {
    let path = sample(scanned);
    let e = Engine::spawn(1);
    e.open(1, 1, path.clone());
    loop {
        if matches!(
            e.events.recv_timeout(Duration::from_secs(20)).unwrap(),
            Event::Opened { .. }
        ) {
            break;
        }
    }
    if scanned {
        assert!(e.detect_fields(1, 0, 99, 2, vec![0]));
        e.cancel_detection();
        loop {
            match e.events.recv_timeout(Duration::from_secs(60)).unwrap() {
                Event::FileError { operation: 99, .. } => break,
                Event::Changed { .. } => panic!("Cancellation installed fields"),
                _ => {}
            }
        }
    }
    assert!(e.detect_fields(1, 0, 1, 2, vec![0]));
    let mut revision = 0;
    let count = loop {
        match e.events.recv_timeout(Duration::from_secs(60)).unwrap() {
            Event::Changed { revision: r, .. } => revision = r,
            Event::DetectionDone { count: c, .. } => {
                break c;
            }
            Event::FileError { message, .. } => panic!("{message}"),
            _ => {}
        }
    };
    assert!(revision > 0);
    e.form_pages(1, revision, vec![0]);
    loop {
        if let Event::Forms { fields, .. } = e.events.recv_timeout(Duration::from_secs(20)).unwrap()
        {
            assert_eq!(
                count,
                2,
                "detected: {:?}",
                fields
                    .iter()
                    .map(|f| (&f.spec.label, f.spec.rect))
                    .collect::<Vec<_>>()
            );
            assert_eq!(fields.len(), 2);
            assert!(
                fields
                    .iter()
                    .any(|f| f.spec.kind == viewer_core::FieldKind::Date)
            );
            break;
        }
    }
    assert!(e.detect_fields(1, revision, 2, 3, vec![0]));
    loop {
        match e.events.recv_timeout(Duration::from_secs(60)).unwrap() {
            Event::DetectionDone { count, .. } => {
                assert_eq!(count, 0, "repeat detection must preserve existing fields");
                break;
            }
            Event::FileError { message, .. } => panic!("{message}"),
            _ => {}
        }
    }
    drop(e);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn native_form_detection_creates_fields_as_one_revision() {
    run(false);
}
#[test]
fn scanned_form_detection_uses_offline_ocr() {
    run(true);
}
