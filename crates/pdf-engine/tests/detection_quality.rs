//! A separate qualification set: layouts and raster degradations are not the
//! small development fixtures in detection.rs. Ground truth is authored here.
use mupdf::{Buffer, Colorspace, Image, Matrix, pdf::PdfDocument};
use pdf_engine::{Engine, Event};
use std::{path::PathBuf, time::Duration};
use viewer_core::{FieldKind, TextRect};
fn transform(r: TextRect, angle: f32, cx: f32, cy: f32) -> TextRect {
    let (s, c) = angle.sin_cos();
    let mut b = TextRect {
        left: f32::INFINITY,
        top: f32::INFINITY,
        right: f32::NEG_INFINITY,
        bottom: f32::NEG_INFINITY,
    };
    for (x, y) in [
        (r.left, r.top),
        (r.right, r.top),
        (r.right, r.bottom),
        (r.left, r.bottom),
    ] {
        let (x, y) = (x - cx, y - cy);
        let (x, y) = (c * x - s * y + cx, s * x + c * y + cy);
        b.left = b.left.min(x);
        b.right = b.right.max(x);
        b.top = b.top.min(y);
        b.bottom = b.bottom.max(y);
    }
    b
}
fn fixture(index: usize) -> (PathBuf, Vec<(TextRect, FieldKind)>) {
    let category = index % 6;
    let shift = (index / 6) as f32 * 5.;
    let height = 620.;
    let width = 440.;
    let mut pdf = PdfDocument::new();
    pdf.new_page((width, height)).unwrap();
    let font = pdf
        .new_object_from_str("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
        .unwrap();
    let font = pdf.add_object(&font).unwrap();
    let mut fonts = pdf.new_dict().unwrap();
    fonts.dict_put("F1", font).unwrap();
    let mut resources = pdf.new_dict().unwrap();
    resources.dict_put("Font", fonts).unwrap();
    let mut text = "0.8 w ".to_string();
    let mut truth = vec![];
    for (i, (label, kind)) in [
        ("Name", FieldKind::Text),
        ("Date", FieldKind::Date),
        ("Amount", FieldKind::Number),
        ("Signature", FieldKind::VisualSignature),
    ]
    .into_iter()
    .enumerate()
    {
        let top = 48. + i as f32 * 52. + shift;
        let left = 150. + shift;
        let r = TextRect {
            left,
            top,
            right: 390.,
            bottom: top + 24.,
        };
        truth.push((r, kind));
        text.push_str(&format!(
            "BT /F1 11 Tf 20 {} Td ({label}:) Tj ET {left} {} {} 24 re S ",
            height - top - 16.,
            height - r.bottom,
            r.right - r.left
        ));
    }
    let top = 266. + shift;
    let left = 150. + shift;
    truth.push((
        TextRect {
            left,
            top,
            right: left + 12.,
            bottom: top + 12.,
        },
        FieldKind::Checkbox,
    ));
    text.push_str(&format!(
        "{left} {} 12 12 re S BT /F1 11 Tf {} {} Td (I agree to terms) Tj ET ",
        height - top - 12.,
        left + 26.,
        height - top - 10.
    ));
    let line = 333. + shift;
    truth.push((
        TextRect {
            left,
            top: line - 18.,
            right: 390.,
            bottom: line - 1.,
        },
        FieldKind::Number,
    ));
    text.push_str(&format!(
        "{left} {} m 390 {} l S BT /F1 11 Tf 20 {} Td (Quantity:) Tj ET ",
        height - line,
        height - line,
        height - line + 1.
    ));
    // Decorative table, including blank rows, is not ground-truth form input.
    for row in 0..4 {
        for col in 0..3 {
            let x = 25. + col as f32 * 120.;
            let top = 400. + row as f32 * 22.;
            text.push_str(&format!("{x} {} 120 22 re S ", height - top - 22.));
            if row == 0 {
                text.push_str(&format!(
                    "BT /F1 10 Tf {} {} Td (Item) Tj ET ",
                    x + 8.,
                    height - top - 15.
                ));
            }
        }
    }
    let stream = pdf
        .add_stream(&Buffer::from_bytes(text.as_bytes()).unwrap(), None, false)
        .unwrap();
    let mut page = pdf.find_page(0).unwrap();
    page.dict_put("Resources", resources).unwrap();
    page.dict_put("Contents", stream).unwrap();
    if category != 0 {
        let pix = pdf
            .load_page(0)
            .unwrap()
            .to_pixmap(
                &Matrix::new_scale(3., 3.),
                &Colorspace::device_rgb(),
                false,
                false,
            )
            .unwrap();
        let (w, h) = (pix.width() as usize, pix.height() as usize);
        let angle = match category {
            2 => 2.25_f32.to_radians(),
            3 => (-3_f32).to_radians(),
            _ => 0.,
        };
        let (s, c) = angle.sin_cos();
        let mut rgb = vec![255; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = (x as f32 - w as f32 / 2., y as f32 - h as f32 / 2.);
                let sx = (c * dx + s * dy + w as f32 / 2.).round() as i32;
                let sy = (-s * dx + c * dy + h as f32 / 2.).round() as i32;
                if sx >= 0 && sy >= 0 && (sx as usize) < w && (sy as usize) < h {
                    let value =
                        pix.samples()[sy as usize * pix.stride() as usize + sx as usize * 3];
                    let value = if category == 4 {
                        (166. + f32::from(value) * 0.32) as u8
                    } else {
                        value
                    };
                    rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].fill(value);
                }
            }
        }
        let mut encoded = vec![];
        {
            let mut encoder = png::Encoder::new(&mut encoded, w as u32, h as u32);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&rgb)
                .unwrap();
        }
        let image = Image::from_bytes(&encoded).unwrap();
        let mut scan = PdfDocument::new();
        scan.new_page((width, height)).unwrap();
        let image = scan.add_image(&image).unwrap();
        let mut x = scan.new_dict().unwrap();
        x.dict_put("Scan", image).unwrap();
        let mut resources = scan.new_dict().unwrap();
        resources.dict_put("XObject", x).unwrap();
        let mut content = format!("q {width} 0 0 {height} 0 0 cm /Scan Do Q ");
        if category == 5 {
            let font = scan
                .new_object_from_str("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
                .unwrap();
            let font = scan.add_object(&font).unwrap();
            let mut fonts = scan.new_dict().unwrap();
            fonts.dict_put("F1", font).unwrap();
            resources.dict_put("Font", fonts).unwrap();
            content.push_str("BT /F1 10 Tf 20 600 Td (Scan copy) Tj ET");
        }
        let stream = scan
            .add_stream(
                &Buffer::from_bytes(content.as_bytes()).unwrap(),
                None,
                false,
            )
            .unwrap();
        let mut p = scan.find_page(0).unwrap();
        p.dict_put("Resources", resources).unwrap();
        p.dict_put("Contents", stream).unwrap();
        pdf = scan;
        for (r, _) in &mut truth {
            *r = transform(*r, angle, width / 2., height / 2.);
        }
    }
    let path = std::env::temp_dir().join(format!(
        "pdfree-qualification-{}-{index}.pdf",
        std::process::id()
    ));
    assert!(!path.exists());
    pdf.save(path.to_str().unwrap()).unwrap();
    (path, truth)
}
fn iou(a: TextRect, b: TextRect) -> f32 {
    let i = (a.right.min(b.right) - a.left.max(b.left)).max(0.)
        * (a.bottom.min(b.bottom) - a.top.max(b.top)).max(0.);
    i / ((a.right - a.left) * (a.bottom - a.top) + (b.right - b.left) * (b.bottom - b.top) - i)
        .max(1.)
}
#[test]
fn held_out_english_forms_meet_detection_precision_and_recall_gate() {
    let mut true_positive = 0;
    let mut predicted = 0;
    let mut expected = 0;
    let mut high = 0;
    let mut high_correct = 0;
    let mut correct_types = 0;
    let mut cases = vec![];
    let development = std::env::var("PDFREE_FORM_DEVELOPMENT_CASES").ok();
    let indices = development
        .as_ref()
        .map(|s| {
            s.split(',')
                .map(|v| v.parse::<usize>().unwrap())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| (96..120).collect());
    for index in indices.iter().copied() {
        let (path, truth) = fixture(index);
        let e = Engine::spawn(1);
        e.open(1, 1, path.clone());
        loop {
            if matches!(
                e.events.recv_timeout(Duration::from_secs(30)).unwrap(),
                Event::Opened { .. }
            ) {
                break;
            }
        }
        assert!(e.detect_fields(1, 0, 1, 2, vec![0]));
        let mut revision = 0;
        loop {
            match e.events.recv_timeout(Duration::from_secs(90)).unwrap() {
                Event::Changed { revision: r, .. } => revision = r,
                Event::DetectionDone { .. } => break,
                Event::FileError { message, .. } => panic!("case {index}: {message}"),
                _ => {}
            }
        }
        e.form_pages(1, revision, vec![0]);
        let fields = loop {
            if let Event::Forms { fields, .. } =
                e.events.recv_timeout(Duration::from_secs(30)).unwrap()
            {
                break fields;
            }
        };
        let mut used = std::collections::HashSet::new();
        let mut matched = 0;
        let mut typed = 0;
        for field in &fields {
            let best = truth
                .iter()
                .enumerate()
                .filter(|(i, _)| !used.contains(i))
                .map(|(i, (r, k))| (i, *k, iou(field.spec.rect, *r)))
                .max_by(|a, b| a.2.total_cmp(&b.2));
            let is_high = field.detection.as_ref().is_some_and(|c| c.score >= 0.9);
            if is_high {
                high += 1;
            }
            if let Some((i, kind, score)) = best
                && score >= 0.5
            {
                used.insert(i);
                matched += 1;
                if field.spec.kind == kind {
                    typed += 1;
                }
                if is_high {
                    high_correct += 1;
                }
            }
        }
        true_positive += matched;
        correct_types += typed;
        predicted += fields.len();
        expected += truth.len();
        cases.push(serde_json::json!({"case":index,"category":(["native","scan","skew_clockwise","skew_counterclockwise","low_contrast","mixed"][index%6]),"expected":truth.len(),"predicted":fields.len(),"matched":matched,"correct_types":typed,"detections":fields.iter().map(|f|serde_json::json!({"label":f.spec.label,"kind":format!("{:?}",f.spec.kind),"rect":[f.spec.rect.left,f.spec.rect.top,f.spec.rect.right,f.spec.rect.bottom],"score":f.detection.as_ref().map(|c|c.score)})).collect::<Vec<_>>()}));
        drop(e);
        std::fs::remove_file(path).unwrap();
    }
    let precision = true_positive as f64 / predicted.max(1) as f64;
    let recall = true_positive as f64 / expected.max(1) as f64;
    let high_precision = high_correct as f64 / high.max(1) as f64;
    let type_accuracy = correct_types as f64 / true_positive.max(1) as f64;
    let report = serde_json::json!({"dataset":if development.is_some(){"development"}else{"held_out"},"forms":indices.len(),"fields":expected,"matched":true_positive,"predicted":predicted,"precision":precision,"recall":recall,"high_confidence_precision":high_precision,"type_accuracy":type_accuracy,"cases":cases});
    println!("{report}");
    if let Some(folder) = std::env::var_os("PDFREE_FORM_EVIDENCE_DIR") {
        let folder = PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("detection-quality.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    assert!(
        precision >= 0.9 && recall >= 0.8 && high_precision >= 0.95 && type_accuracy >= 0.9,
        "detection quality gate failed: {report}"
    );
}
