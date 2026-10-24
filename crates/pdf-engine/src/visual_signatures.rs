//! Visual marks are ordinary Stamp annotations, never cryptographic /Sig values.
use mupdf::{
    Buffer, Font, Image,
    pdf::{PdfAnnotationType, PdfDocument, PdfObject, Permission},
};
use viewer_core::{FieldKind, FieldRef, FieldSpec, FormField, FormValue, SignatureMark, TextRect};
type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn put(doc: &PdfDocument, o: &mut PdfObject, k: &str, v: &str) -> Result<()> {
    o.dict_put(k, doc.new_string(v).map_err(err)?).map_err(err)
}
pub(crate) fn create(doc: &mut PdfDocument, s: &FieldSpec) -> Result<()> {
    if !crate::forms::permissions(doc).contains(Permission::ANNOTATE) {
        return Err("This PDF does not permit visual signature annotations.".into());
    }
    s.validate()?;
    let mut o = doc
        .new_object_from_str("<< /Type /Annot /Subtype /Stamp /F 4 /PDFreeVisual 1 >>")
        .map_err(err)?;
    o.dict_put("Rect", crate::forms::pdf_rect(doc, s)?)
        .map_err(err)?;
    o.dict_put("P", doc.find_page(s.page as i32).map_err(err)?)
        .map_err(err)?;
    put(doc, &mut o, "PDFreeName", &s.name)?;
    crate::forms::metadata(doc, &mut o, s)?;
    o.dict_put(
        "Ff",
        doc.new_int(i32::from(s.read_only) | (i32::from(s.required) << 1))
            .map_err(err)?,
    )
    .map_err(err)?;
    let object = doc.add_object(&o).map_err(err)?;
    let id = object.as_indirect().map_err(err)?;
    let mut p = doc.find_page(s.page as i32).map_err(err)?;
    let mut annots = p
        .get_dict("Annots")
        .map_err(err)?
        .unwrap_or(doc.new_array().map_err(err)?);
    annots.array_push(object).map_err(err)?;
    p.dict_put("Annots", annots).map_err(err)?;
    mark(
        doc,
        FieldRef {
            field: id,
            widget: id,
            page: s.page,
        },
        s,
        &SignatureMark::Empty,
    )?;
    Ok(())
}
pub(crate) fn fields(doc: &PdfDocument, page: usize) -> Result<Vec<FormField>> {
    let native = doc.load_pdf_page(page as i32).map_err(err)?;
    let b = native.bounds().map_err(err)?;
    let mut result = vec![];
    let (signed, xfa, _) = crate::forms::security(doc)?;
    for a in native.annotations().take(10_000) {
        if a.r#type().map_err(err)? != PdfAnnotationType::Stamp {
            continue;
        }
        let o = a.object();
        if crate::forms::integer(&o, "PDFreeVisual")? != 1 {
            continue;
        }
        if result.len() >= 512 {
            return Err("Too many visual signature slots on this page.".into());
        }
        let r = a.rect().map_err(err)?;
        let flags = crate::forms::integer(&o, "Ff")?;
        let annotation_locked = a.flags().map_err(err)?.bits() & (64 | 128 | 512) != 0;
        let reason = if signed {
            "Signed document"
        } else if xfa {
            "XFA document"
        } else if flags & 1 != 0 || annotation_locked {
            "Read-only signature slot"
        } else if !crate::forms::permissions(doc).contains(Permission::ANNOTATE) {
            "Annotations are not permitted"
        } else {
            ""
        };
        let name = crate::forms::string(&o, "PDFreeName")?;
        let spec = FieldSpec {
            page,
            name,
            label: crate::forms::string(&o, "TU")?,
            kind: FieldKind::VisualSignature,
            rect: TextRect {
                left: r.x0 - b.x0,
                top: r.y0 - b.y0,
                right: r.x1 - b.x0,
                bottom: r.y1 - b.y0,
            },
            required: flags & 2 != 0,
            read_only: flags & 1 != 0,
            options: vec![],
            group: String::new(),
            review: crate::forms::integer(&o, "PDFreeReview")? != 0,
            date_format: "YYYY-MM-DD".into(),
            decimals: 2,
        };
        result.push(FormField {
            id: FieldRef {
                field: a.xref().map_err(err)?,
                widget: a.xref().map_err(err)?,
                page,
            },
            spec,
            value: FormValue::Text(crate::forms::string(&o, "PDFreeMark")?),
            export_values: vec![],
            export_value: String::new(),
            signable: false,
            editable: reason.is_empty(),
            authorable: !signed
                && !xfa
                && !annotation_locked
                && crate::forms::permissions(doc).contains(Permission::ANNOTATE),
            reason: reason.into(),
            detection: crate::forms::read_confidence(&o)?,
            password: false,
            multi_select: false,
        });
    }
    Ok(result)
}
pub(crate) fn mark(
    doc: &mut PdfDocument,
    id: FieldRef,
    s: &FieldSpec,
    mark: &SignatureMark,
) -> Result<()> {
    if !crate::forms::permissions(doc).contains(Permission::ANNOTATE) {
        return Err("Annotations are not permitted.".into());
    }
    let w = s.rect.right - s.rect.left;
    let h = s.rect.bottom - s.rect.top;
    let mut resources = doc.new_dict().map_err(err)?;
    let mut description = String::new();
    let content = match mark {
        SignatureMark::Empty => String::new(),
        SignatureMark::Typed(text) => {
            if text.trim().is_empty() || text.len() > 512 || text.chars().any(char::is_control) {
                return Err("Enter a signature of 1–512 bytes without control characters.".into());
            }
            let font = Font::from_bytes(
                "Caveat",
                include_bytes!("../../../assets/signatures/Caveat.ttf"),
            )
            .map_err(err)?;
            let mut encoded = String::new();
            let mut advance = 0.;
            for c in text.chars() {
                let glyph = font.encode_character(c as i32).map_err(err)?;
                if glyph == 0 {
                    return Err("The signature font does not contain this character. Draw or import the signature instead.".into());
                }
                advance += font.advance_glyph(glyph).map_err(err)?;
                encoded.push_str(&format!("{glyph:04X}"));
            }
            let mut fonts = doc.new_dict().map_err(err)?;
            fonts
                .dict_put("Signature", doc.add_font(&font).map_err(err)?)
                .map_err(err)?;
            resources.dict_put("Font", fonts).map_err(err)?;
            let size = ((w - 4.) / advance.max(0.1)).min(h * 0.7);
            description = text.clone();
            format!(
                "q 0.03 0.10 0.22 rg BT /Signature {size} Tf 1 0 0 1 2 {} Tm <{encoded}> Tj ET Q",
                h * 0.22
            )
        }
        SignatureMark::Strokes(lines) => {
            if lines.is_empty()
                || lines.len() > 128
                || lines.iter().map(Vec::len).sum::<usize>() > 16_384
            {
                return Err("Draw a signature within the stroke limit.".into());
            }
            let mut stream = "q 0.03 0.10 0.22 RG 1.4 w 1 J 1 j ".to_string();
            let mut count = 0;
            for line in lines {
                if line.len() < 2 {
                    continue;
                }
                for (i, p) in line.iter().enumerate() {
                    if !p.iter().all(|n| n.is_finite() && *n >= 0. && *n <= 1.) {
                        return Err("Invalid signature stroke coordinates.".into());
                    }
                    stream.push_str(&format!(
                        "{} {} {} ",
                        p[0] * w,
                        (1. - p[1]) * h,
                        if i == 0 { "m" } else { "l" }
                    ));
                }
                stream.push_str("S ");
                count += 1;
            }
            if count == 0 {
                return Err("Draw at least one signature stroke.".into());
            }
            stream.push('Q');
            description = "Drawn visual signature".into();
            stream
        }
        SignatureMark::Image(bytes) => {
            if bytes.is_empty() || bytes.len() > 4 * 1024 * 1024 {
                return Err("Signature images must be at most 4 MiB.".into());
            }
            viewer_core::signature_image_size(bytes)?;
            let image = Image::from_bytes(bytes).map_err(err)?;
            if image.width() == 0
                || image.height() == 0
                || image.width() > 8192
                || image.height() > 8192
                || image.width() as u64 * image.height() as u64 > 16_000_000
            {
                return Err("Signature image dimensions exceed the safe limit.".into());
            }
            let aspect = image.width() as f32 / image.height() as f32;
            let iw = w.min(h * aspect);
            let ih = iw / aspect;
            let mut xobjects = doc.new_dict().map_err(err)?;
            xobjects
                .dict_put("SignatureImage", doc.add_image(&image).map_err(err)?)
                .map_err(err)?;
            resources.dict_put("XObject", xobjects).map_err(err)?;
            description = "Imported visual signature".into();
            format!(
                "q {iw} 0 0 {ih} {} {} cm /SignatureImage Do Q",
                (w - iw) / 2.,
                (h - ih) / 2.
            )
        }
    };
    let mut dict = doc
        .new_object_from_str(&format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 {w} {h}] >>"
        ))
        .map_err(err)?;
    dict.dict_put("Resources", resources).map_err(err)?;
    let inv = doc
        .find_page(s.page as i32)
        .map_err(err)?
        .page_ctm()
        .map_err(err)?
        .invert()
        .ok_or("Invalid page transform")?;
    dict.dict_put(
        "Matrix",
        doc.new_object_from_str(&format!("[{} {} {} {} 0 0]", inv.a, inv.b, -inv.c, -inv.d))
            .map_err(err)?,
    )
    .map_err(err)?;
    let stream = doc
        .add_stream(
            &Buffer::from_bytes(content.as_bytes()).map_err(err)?,
            Some(&dict),
            false,
        )
        .map_err(err)?;
    let mut ap = doc.new_dict().map_err(err)?;
    ap.dict_put("N", stream).map_err(err)?;
    let mut o = doc.new_indirect(id.widget, 0).map_err(err)?;
    o.dict_put("AP", ap).map_err(err)?;
    put(doc, &mut o, "PDFreeMark", &description)?;
    Ok(())
}
pub(crate) fn update(doc: &mut PdfDocument, f: &FormField, s: &FieldSpec) -> Result<()> {
    if s.page != f.spec.page || s.name != f.spec.name {
        return Err("Keep the signature slot's page and name.".into());
    }
    s.validate()?;
    let mut o = doc.new_indirect(f.id.widget, 0).map_err(err)?;
    o.dict_put("Rect", crate::forms::pdf_rect(doc, s)?)
        .map_err(err)?;
    crate::forms::metadata(doc, &mut o, s)?;
    o.dict_put(
        "Ff",
        doc.new_int(i32::from(s.read_only) | (i32::from(s.required) << 1))
            .map_err(err)?,
    )
    .map_err(err)?;
    Ok(())
}
pub(crate) fn delete(doc: &PdfDocument, f: &FormField) -> Result<()> {
    let mut page = doc.find_page(f.spec.page as i32).map_err(err)?;
    let mut annots = page
        .get_dict("Annots")
        .map_err(err)?
        .ok_or("Missing annotations")?;
    for i in (0..annots.len().map_err(err)?).rev() {
        if annots
            .get_array(i as i32)
            .map_err(err)?
            .ok_or("Invalid annotation")?
            .as_indirect()
            .map_err(err)?
            == f.id.widget
        {
            annots.array_delete(i as i32).map_err(err)?;
        }
    }
    page.dict_put("Annots", annots).map_err(err)?;
    Ok(())
}
