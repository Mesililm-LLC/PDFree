//! AcroForm schema adapter. All entry points run on the native coordinator.
use mupdf::{
    Buffer, Rect,
    pdf::{PdfDocument, PdfObject, Permission, WidgetType},
};
use std::collections::HashSet;
use viewer_core::{FieldKind, FieldRef, FieldSpec, FormEdit, FormField, FormValue, TextRect};
type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub(crate) fn root(doc: &PdfDocument) -> Result<PdfObject> {
    doc.trailer()
        .map_err(err)?
        .get_dict("Root")
        .map_err(err)?
        .ok_or("Missing PDF catalog".into())
}
pub(crate) fn string(o: &PdfObject, key: &str) -> Result<String> {
    Ok(o.get_dict_inheritable(key)
        .map_err(err)?
        .map(|v| v.as_string().map_err(err))
        .transpose()?
        .unwrap_or_default())
}
pub(crate) fn integer(o: &PdfObject, key: &str) -> Result<i32> {
    Ok(o.get_dict_inheritable(key)
        .map_err(err)?
        .map(|v| v.as_int().map_err(err))
        .transpose()?
        .unwrap_or(0))
}
fn name(o: &PdfObject, key: &str) -> Result<Vec<u8>> {
    Ok(o.get_dict_inheritable(key)
        .map_err(err)?
        .map(|v| v.as_name().map_err(err))
        .transpose()?
        .unwrap_or_default())
}
fn objects(doc: &PdfDocument) -> Result<Vec<PdfObject>> {
    let mut stack = Vec::new();
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    if let Some(form) = root(doc)?.get_dict("AcroForm").map_err(err)?
        && let Some(fields) = form.get_dict("Fields").map_err(err)?
    {
        stack.push((fields, 0));
    }
    while let Some((o, depth)) = stack.pop() {
        if depth > crate::constants::MAX_IMPORT_DEPTH || found.len() + stack.len() > 10_000 {
            return Err("Form tree exceeds the safe inspection limit.".into());
        }
        if o.is_array().map_err(err)? {
            for i in 0..o.len().map_err(err)?.min(10_001) {
                if let Some(c) = o.get_array(i as i32).map_err(err)? {
                    stack.push((c, depth + 1));
                }
            }
            continue;
        }
        if !o.is_dict().map_err(err)? {
            return Err("Invalid form field tree.".into());
        }
        if o.is_indirect().map_err(err)? && !seen.insert(o.as_indirect().map_err(err)?) {
            return Err("Cyclic or shared form tree.".into());
        }
        if let Some(kids) = o.get_dict("Kids").map_err(err)? {
            stack.push((kids, depth + 1));
        }
        found.push(o);
    }
    Ok(found)
}
pub(crate) fn security(doc: &PdfDocument) -> Result<(bool, bool, bool)> {
    let r = root(doc)?;
    let form = r.get_dict("AcroForm").map_err(err)?;
    let xfa = form
        .as_ref()
        .map(|f| f.get_dict("XFA"))
        .transpose()
        .map_err(err)?
        .flatten()
        .is_some();
    let calculated = form
        .as_ref()
        .map(|f| f.get_dict("CO"))
        .transpose()
        .map_err(err)?
        .flatten()
        .is_some();
    let mut signed = r.get_dict("Perms").map_err(err)?.is_some();
    for o in objects(doc)? {
        if name(&o, "FT")? == b"Sig"
            && let Some(value) = o.get_dict_inheritable("V").map_err(err)?
            && !value.is_null().map_err(err)?
        {
            signed = true;
        }
    }
    Ok((signed, xfa, calculated))
}
/// Read declared encryption permissions without the binding's unknown-bit fallback.
/// Reserved PDF bits must never turn a restricted document into an unrestricted one.
pub(crate) fn permissions(doc: &PdfDocument) -> Permission {
    let read = || -> Result<Permission> {
        let Some(encryption) = doc
            .trailer()
            .map_err(err)?
            .get_dict("Encrypt")
            .map_err(err)?
        else {
            return Ok(Permission::all());
        };
        let bits = encryption
            .get_dict("P")
            .map_err(err)?
            .ok_or("Missing encryption permissions")?
            .as_int()
            .map_err(err)?;
        Ok(Permission::from_bits_truncate(bits as u32))
    };
    read().unwrap_or_else(|_| Permission::empty())
}
pub(crate) fn ensure_writable(doc: &PdfDocument) -> Result<()> {
    let (signed, xfa, _) = security(doc)?;
    if signed {
        return Err("This document contains a digital signature and is read-only. Rewriting would invalidate it.".into());
    }
    if xfa {
        return Err("XFA documents are read-only in PDFree.".into());
    }
    Ok(())
}
fn terminal(widget: &PdfObject) -> Result<PdfObject> {
    let mut o = widget.try_clone().map_err(err)?;
    let mut seen = HashSet::new();
    for _ in 0..64 {
        if o.is_indirect().map_err(err)? && !seen.insert(o.as_indirect().map_err(err)?) {
            return Err("Cyclic form parents.".into());
        }
        if o.get_dict("T").map_err(err)?.is_some() {
            return Ok(o);
        }
        if let Some(p) = o.get_dict("Parent").map_err(err)? {
            o = p;
        } else {
            return Ok(o);
        }
    }
    Err("Form parent depth exceeded.".into())
}
fn options(o: &PdfObject) -> Result<(Vec<String>, Vec<String>)> {
    let mut labels = vec![];
    let mut exports = vec![];
    if let Some(opts) = o.get_dict_inheritable("Opt").map_err(err)? {
        if opts.len().map_err(err)? > crate::constants::MAX_FIELD_OPTIONS {
            return Err("Field exceeds 256 options.".into());
        }
        for i in 0..opts.len().map_err(err)? {
            let v = opts
                .get_array(i as i32)
                .map_err(err)?
                .ok_or("Invalid choice option")?;
            if v.is_array().map_err(err)? {
                exports.push(
                    v.get_array(0)
                        .map_err(err)?
                        .ok_or("Invalid option")?
                        .as_string()
                        .map_err(err)?,
                );
                labels.push(
                    v.get_array(1)
                        .map_err(err)?
                        .ok_or("Invalid option label")?
                        .as_string()
                        .map_err(err)?,
                );
            } else {
                let s = v.as_string().map_err(err)?;
                labels.push(s.clone());
                exports.push(s);
            }
        }
    }
    Ok((labels, exports))
}
fn managed_radio(o: &PdfObject) -> Result<bool> {
    if integer(o, "PDFreeManaged")? != 1
        || o.get_dict("Parent").map_err(err)?.is_some()
        || name(o, "FT")? != b"Btn"
        || integer(o, "Ff")? & (1 << 15) == 0
    {
        return Ok(false);
    }
    let Some(kids) = o.get_dict("Kids").map_err(err)? else {
        return Ok(false);
    };
    if kids.len().map_err(err)? > crate::constants::MAX_RADIO_KIDS {
        return Ok(false);
    }
    for i in 0..kids.len().map_err(err)? {
        let k = kids
            .get_array(i as i32)
            .map_err(err)?
            .ok_or("Invalid radio group")?;
        if name(&k, "Subtype")? != b"Widget"
            || k.get_dict("Kids").map_err(err)?.is_some()
            || k.get_dict("Parent")
                .map_err(err)?
                .is_none_or(|p| p.as_indirect().ok() != o.as_indirect().ok())
        {
            return Ok(false);
        }
    }
    Ok(true)
}
fn button_values(doc: &PdfDocument, field: i32) -> Result<HashSet<String>> {
    let o = doc.new_indirect(field, 0).map_err(err)?;
    let mut widgets = vec![o.try_clone().map_err(err)?];
    if let Some(kids) = o.get_dict("Kids").map_err(err)? {
        if kids.len().map_err(err)? > crate::constants::MAX_BUTTON_KIDS {
            return Err("Button group exceeds the safe limit.".into());
        }
        widgets.clear();
        for i in 0..kids.len().map_err(err)? {
            widgets.push(
                kids.get_array(i as i32)
                    .map_err(err)?
                    .ok_or("Invalid button group")?,
            );
        }
    }
    let mut values = HashSet::from(["Off".to_string()]);
    for w in widgets {
        if let Some(ap) = w.get_dict("AP").map_err(err)?
            && let Some(states) = ap.get_dict("N").map_err(err)?
            && states.is_dict().map_err(err)?
        {
            for i in 0..states
                .dict_len()
                .map_err(err)?
                .min(crate::constants::MAX_APPEARANCE_STATES)
            {
                let k = states
                    .get_dict_key(i as i32)
                    .map_err(err)?
                    .ok_or("Invalid appearance")?
                    .as_name()
                    .map_err(err)?;
                values.insert(
                    String::from_utf8(k).map_err(|_| "Unsupported button export-name encoding")?,
                );
            }
        }
    }
    Ok(values)
}
fn remove_root_field(doc: &mut PdfDocument, id: i32) -> Result<()> {
    let mut form = font_resources(doc)?;
    let mut fields = form
        .get_dict("Fields")
        .map_err(err)?
        .ok_or("Missing fields")?;
    for i in (0..fields.len().map_err(err)?).rev() {
        if fields
            .get_array(i as i32)
            .map_err(err)?
            .ok_or("Invalid field")?
            .as_indirect()
            .map_err(err)?
            == id
        {
            fields.array_delete(i as i32).map_err(err)?;
        }
    }
    form.dict_put("Fields", fields).map_err(err)?;
    Ok(())
}
pub(crate) fn read_confidence(o: &PdfObject) -> Result<Option<viewer_core::DetectionConfidence>> {
    let Some(score) = o.get_dict("PDFreeConfidence").map_err(err)? else {
        return Ok(None);
    };
    let r = o
        .get_dict("PDFreeOrigin")
        .map_err(err)?
        .ok_or("Missing detected field origin")?;
    let n = |i| -> Result<f32> {
        r.get_array(i)
            .map_err(err)?
            .ok_or("Invalid field origin")?
            .as_float()
            .map_err(err)
    };
    Ok(Some(viewer_core::DetectionConfidence {
        score: score.as_float().map_err(err)?,
        origin: TextRect {
            left: n(0)?,
            top: n(1)?,
            right: n(2)?,
            bottom: n(3)?,
        },
    }))
}
pub(crate) fn suppressed(doc: &PdfDocument, page: usize) -> Result<Vec<TextRect>> {
    let mut result = vec![];
    let p = doc
        .find_page(page as i32)
        .map_err(err)?
        .as_indirect()
        .map_err(err)?;
    if let Some(list) = root(doc)?.get_dict("PDFreeSuppressedFields").map_err(err)? {
        for i in 0..list
            .len()
            .map_err(err)?
            .min(crate::constants::MAX_SUPPRESSED_FIELDS)
        {
            let item = list
                .get_array(i as i32)
                .map_err(err)?
                .ok_or("Invalid detection exclusions")?;
            if item
                .get_dict("P")
                .map_err(err)?
                .is_some_and(|o| o.as_indirect().ok() == Some(p))
                && let Some(c) = read_confidence(&item)?
            {
                result.push(c.origin);
            }
        }
    }
    Ok(result)
}
fn suppress(doc: &PdfDocument, f: &FormField) -> Result<()> {
    if let Some(c) = &f.detection {
        let mut r = root(doc)?;
        let mut list = r
            .get_dict("PDFreeSuppressedFields")
            .map_err(err)?
            .unwrap_or(doc.new_array().map_err(err)?);
        if list.len().map_err(err)? >= crate::constants::MAX_SUPPRESSED_FIELDS {
            return Err("Detection exclusion limit reached.".into());
        }
        let mut item = doc.new_dict().map_err(err)?;
        item.dict_put("P", doc.find_page(f.spec.page as i32).map_err(err)?)
            .map_err(err)?;
        item.dict_put("PDFreeConfidence", doc.new_real(c.score).map_err(err)?)
            .map_err(err)?;
        let mut a = doc.new_array().map_err(err)?;
        for n in [c.origin.left, c.origin.top, c.origin.right, c.origin.bottom] {
            a.array_push(doc.new_real(n).map_err(err)?).map_err(err)?;
        }
        item.dict_put("PDFreeOrigin", a).map_err(err)?;
        list.array_push(item).map_err(err)?;
        r.dict_put("PDFreeSuppressedFields", list).map_err(err)?;
    }
    Ok(())
}
pub(crate) fn page(doc: &PdfDocument, index: usize) -> Result<Vec<FormField>> {
    let (signed, xfa, calculated) = security(doc)?;
    let native = doc.load_pdf_page(index as i32).map_err(err)?;
    let b = native.bounds().map_err(err)?;
    let mut result = vec![];
    for (n, w) in native.widgets().enumerate() {
        if n >= crate::constants::MAX_WIDGETS_PER_PAGE {
            return Err("This page exceeds the 512-widget preview limit.".into());
        }
        let a = w.annotation();
        let o = a.object();
        let f = terminal(&o)?;
        let flags = integer(&f, "Ff")?;
        let kind = match w.r#type().map_err(err)? {
            WidgetType::Text => match string(&f, "PDFreeType")?.as_str() {
                "Date" => FieldKind::Date,
                "Number" => FieldKind::Number,
                _ => {
                    if flags & (1 << 12) != 0 {
                        FieldKind::Multiline
                    } else {
                        FieldKind::Text
                    }
                }
            },
            WidgetType::Checkbox => FieldKind::Checkbox,
            WidgetType::RadioButton => FieldKind::Radio,
            WidgetType::Combobox => FieldKind::Dropdown,
            WidgetType::Listbox => FieldKind::List,
            WidgetType::Signature => FieldKind::DigitalSignature,
            _ => continue,
        };
        let rect = a.rect().map_err(err)?;
        let (opts, exports) = options(&f)?;
        let value = if let Some(v) = f.get_dict_inheritable("V").map_err(err)? {
            if v.is_array().map_err(err)? {
                let mut s = vec![];
                for i in 0..v
                    .len()
                    .map_err(err)?
                    .min(crate::constants::MAX_SELECTION_VALUES)
                {
                    s.push(
                        v.get_array(i as i32)
                            .map_err(err)?
                            .ok_or("Invalid selection")?
                            .as_string()
                            .map_err(err)?,
                    );
                }
                FormValue::Selection(s)
            } else if v.is_name().map_err(err)? {
                FormValue::Text(String::from_utf8_lossy(&v.as_name().map_err(err)?).into_owned())
            } else {
                FormValue::Text(v.as_string().map_err(err)?)
            }
        } else {
            FormValue::Text(String::new())
        };
        let scripted = o.get_dict_inheritable("AA").map_err(err)?.is_some()
            || o.get_dict_inheritable("A").map_err(err)?.is_some();
        let unsupported = flags & ((1 << 20) | (1 << 25)) != 0; // file-select / rich text
        let locked = flags & 1 != 0
            || a.flags().map_err(err)?.bits() & ((1 << 6) | (1 << 7) | (1 << 9)) != 0;
        let reason = if signed {
            "Signed document"
        } else if xfa {
            "XFA form"
        } else if calculated || scripted {
            "Scripted or calculated field"
        } else if unsupported {
            "Unsupported text field behavior"
        } else if locked {
            "Read-only field"
        } else if !permissions(doc).contains(Permission::FORM) {
            "Form filling is not permitted"
        } else if kind == FieldKind::DigitalSignature {
            "Digital signature field"
        } else {
            ""
        };
        let simple = f.as_indirect().map_err(err)? == o.as_indirect().map_err(err)?
            && f.get_dict("Kids").map_err(err)?.is_none()
            && f.get_dict("Parent").map_err(err)?.is_none();
        let spec = FieldSpec {
            page: index,
            name: w.name().map_err(err)?.unwrap_or_default(),
            label: w.label().map_err(err)?.unwrap_or_default(),
            kind,
            rect: TextRect {
                left: rect.x0 - b.x0,
                top: rect.y0 - b.y0,
                right: rect.x1 - b.x0,
                bottom: rect.y1 - b.y0,
            },
            required: flags & 2 != 0,
            read_only: flags & 1 != 0,
            options: opts,
            group: if kind == FieldKind::Radio {
                w.name().map_err(err)?.unwrap_or_default()
            } else {
                String::new()
            },
            review: integer(&o, "PDFreeReview")? != 0,
            date_format: match string(&f, "PDFreeDate")? {
                s if !s.is_empty() => s,
                _ => crate::constants::DEFAULT_DATE_FORMAT.into(),
            },
            decimals: f
                .get_dict("PDFreeDecimals")
                .map_err(err)?
                .map(|v| {
                    v.as_int()
                        .unwrap_or(crate::constants::DEFAULT_DECIMALS)
                        .clamp(0, crate::constants::MAX_DECIMALS) as u8
                })
                .unwrap_or(crate::constants::DEFAULT_DECIMALS),
        };
        let mut export_value = "Yes".to_string();
        if let Some(ap) = o.get_dict("AP").map_err(err)?
            && let Some(states) = ap.get_dict("N").map_err(err)?
            && states.is_dict().map_err(err)?
        {
            for i in 0..states
                .dict_len()
                .map_err(err)?
                .min(crate::constants::MAX_APPEARANCE_STATES)
            {
                let k = states
                    .get_dict_key(i as i32)
                    .map_err(err)?
                    .ok_or("Invalid appearance state")?
                    .as_name()
                    .map_err(err)?;
                if k != b"Off" {
                    export_value = String::from_utf8_lossy(&k).into_owned();
                    break;
                }
            }
        }
        result.push(FormField {
            id: FieldRef {
                field: f.as_indirect().map_err(err)?,
                widget: w.xref().map_err(err)?,
                page: index,
            },
            spec,
            value,
            export_values: exports,
            export_value,
            signable: kind == FieldKind::DigitalSignature
                && reason == "Digital signature field"
                && f.get_dict_inheritable("SV").map_err(err)?.is_none()
                && f.get_dict("Lock").map_err(err)?.is_none(),
            editable: reason.is_empty(),
            authorable: (simple || managed_radio(&f)?)
                && !signed
                && !xfa
                && !calculated
                && !scripted
                && (kind != FieldKind::DigitalSignature
                    || (f.get_dict_inheritable("SV").map_err(err)?.is_none()
                        && f.get_dict("Lock").map_err(err)?.is_none()))
                && permissions(doc).contains(Permission::MODIFY | Permission::ANNOTATE)
                && a.flags().map_err(err)?.bits() & ((1 << 6) | (1 << 7) | (1 << 9)) == 0,
            reason: reason.into(),
            detection: read_confidence(&o)?,
            password: flags & (1 << 13) != 0,
            multi_select: flags & (1 << 21) != 0,
        });
    }
    result.extend(crate::visual_signatures::fields(doc, index)?);
    Ok(result)
}
pub(crate) fn locate(doc: &PdfDocument, id: FieldRef) -> Result<FormField> {
    if id.field <= 0 || id.widget <= 0 || id.page >= doc.page_count().map_err(err)? as usize {
        return Err("Invalid field reference.".into());
    }
    page(doc, id.page)?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or("Field no longer exists; select it again.".into())
}
fn put_text(doc: &PdfDocument, o: &mut PdfObject, key: &str, value: &str) -> Result<()> {
    o.dict_put(key, doc.new_string(value).map_err(err)?)
        .map_err(err)
}
fn put_int(doc: &PdfDocument, o: &mut PdfObject, key: &str, value: i32) -> Result<()> {
    o.dict_put(key, doc.new_int(value).map_err(err)?)
        .map_err(err)
}
fn put_name(doc: &PdfDocument, o: &mut PdfObject, key: &str, value: &str) -> Result<()> {
    o.dict_put(key, doc.new_name(value).map_err(err)?)
        .map_err(err)
}
pub(crate) fn metadata(doc: &PdfDocument, o: &mut PdfObject, s: &FieldSpec) -> Result<()> {
    put_text(doc, o, "TU", &s.label)?;
    put_text(doc, o, "PDFreeType", s.kind.label())?;
    put_text(doc, o, "PDFreeDate", &s.date_format)?;
    put_int(doc, o, "PDFreeDecimals", i32::from(s.decimals))?;
    put_int(doc, o, "PDFreeReview", i32::from(s.review))?;
    Ok(())
}
pub(crate) fn pdf_rect(doc: &PdfDocument, s: &FieldSpec) -> Result<PdfObject> {
    let p = doc.find_page(s.page as i32).map_err(err)?;
    let b = doc
        .load_page(s.page as i32)
        .and_then(|p| p.bounds())
        .map_err(err)?;
    if s.rect.right > b.width() + 0.01 || s.rect.bottom > b.height() + 0.01 {
        return Err("Field must stay inside its page.".into());
    }
    let inv = p
        .page_ctm()
        .map_err(err)?
        .invert()
        .ok_or("Invalid page transform")?;
    let r = Rect {
        x0: s.rect.left + b.x0,
        y0: s.rect.top + b.y0,
        x1: s.rect.right + b.x0,
        y1: s.rect.bottom + b.y0,
    }
    .transform(&inv);
    let mut a = doc.new_array().map_err(err)?;
    for n in [r.x0, r.y0, r.x1, r.y1] {
        a.array_push(doc.new_real(n).map_err(err)?).map_err(err)?;
    }
    Ok(a)
}
fn font_resources(doc: &mut PdfDocument) -> Result<PdfObject> {
    let mut r = root(doc)?;
    let mut form = r
        .get_dict("AcroForm")
        .map_err(err)?
        .unwrap_or(doc.new_dict().map_err(err)?);
    let mut dr = form
        .get_dict("DR")
        .map_err(err)?
        .unwrap_or(doc.new_dict().map_err(err)?);
    let mut fonts = dr
        .get_dict("Font")
        .map_err(err)?
        .unwrap_or(doc.new_dict().map_err(err)?);
    if fonts.get_dict("PDFreeHelv").map_err(err)?.is_none() {
        let font = doc
            .new_object_from_str(
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
            )
            .map_err(err)?;
        fonts
            .dict_put("PDFreeHelv", doc.add_object(&font).map_err(err)?)
            .map_err(err)?;
    }
    dr.dict_put("Font", fonts).map_err(err)?;
    form.dict_put("DR", dr).map_err(err)?;
    if form.get_dict("DA").map_err(err)?.is_none() {
        put_text(doc, &mut form, "DA", "/PDFreeHelv 11 Tf 0 g")?;
    }
    form.dict_put("NeedAppearances", doc.new_bool(false))
        .map_err(err)?;
    if form.get_dict("Fields").map_err(err)?.is_none() {
        form.dict_put("Fields", doc.new_array().map_err(err)?)
            .map_err(err)?;
    }
    // try_clone deep-copies direct dictionaries. Install an indirect form and return
    // another reference to that same object, so later Fields edits reach the catalog.
    let form = if form.is_indirect().map_err(err)? {
        form
    } else {
        doc.add_object(&form).map_err(err)?
    };
    r.dict_put(
        "AcroForm",
        doc.new_indirect(form.as_indirect().map_err(err)?, 0)
            .map_err(err)?,
    )
    .map_err(err)?;
    Ok(form)
}
fn appearance(doc: &mut PdfDocument, w: f32, h: f32, on: bool) -> Result<PdfObject> {
    let dict = doc
        .new_object_from_str(&format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 {w} {h}] /Resources << >> >>"
        ))
        .map_err(err)?;
    let mut content = format!(
        "q 0.25 0.35 0.45 RG 0.7 w 0.5 0.5 {} {} re S ",
        w - 1.,
        h - 1.
    );
    if on {
        content.push_str(&format!(
            "0 0 0 RG 1.5 w {} {} m {} {} l {} {} l S ",
            w * 0.18,
            h * 0.5,
            w * 0.42,
            h * 0.22,
            w * 0.83,
            h * 0.8
        ));
    }
    content.push('Q');
    doc.add_stream(
        &Buffer::from_bytes(content.as_bytes()).map_err(err)?,
        Some(&dict),
        false,
    )
    .map_err(err)
}
fn configure(doc: &mut PdfDocument, o: &mut PdfObject, s: &FieldSpec) -> Result<()> {
    s.validate()?;
    o.dict_put("Rect", pdf_rect(doc, s)?).map_err(err)?;
    metadata(doc, o, s)?;
    put_int(doc, o, "F", 4)?;
    let mut flags = i32::from(s.read_only) | (i32::from(s.required) << 1);
    let ft = match s.kind {
        FieldKind::Checkbox | FieldKind::Radio => "Btn",
        FieldKind::Dropdown | FieldKind::List => "Ch",
        FieldKind::DigitalSignature => "Sig",
        _ => "Tx",
    };
    put_name(doc, o, "FT", ft)?;
    match s.kind {
        FieldKind::Multiline => flags |= 1 << 12,
        FieldKind::Radio => flags |= 1 << 15,
        FieldKind::Dropdown => flags |= 1 << 17,
        FieldKind::List => flags |= 1 << 21,
        _ => {}
    }
    put_int(doc, o, "Ff", flags)?;
    put_text(doc, o, "DA", "/PDFreeHelv 11 Tf 0 g")?;
    let bs = doc.new_object_from_str("<< /W 0.7 /S /S >>").map_err(err)?;
    o.dict_put("BS", bs).map_err(err)?;
    if matches!(s.kind, FieldKind::Dropdown | FieldKind::List) {
        let mut opts = doc.new_array().map_err(err)?;
        for option in &s.options {
            opts.array_push(doc.new_string(option).map_err(err)?)
                .map_err(err)?;
        }
        o.dict_put("Opt", opts).map_err(err)?;
    }
    if matches!(s.kind, FieldKind::Checkbox | FieldKind::Radio) {
        let export = if s.kind == FieldKind::Radio {
            s.name.as_str()
        } else {
            "Yes"
        };
        let mut states = doc.new_dict().map_err(err)?;
        states
            .dict_put(
                "Off",
                appearance(
                    doc,
                    s.rect.right - s.rect.left,
                    s.rect.bottom - s.rect.top,
                    false,
                )?,
            )
            .map_err(err)?;
        states
            .dict_put(
                export,
                appearance(
                    doc,
                    s.rect.right - s.rect.left,
                    s.rect.bottom - s.rect.top,
                    true,
                )?,
            )
            .map_err(err)?;
        let mut ap = doc.new_dict().map_err(err)?;
        ap.dict_put("N", states).map_err(err)?;
        o.dict_put("AP", ap).map_err(err)?;
        put_name(doc, o, "AS", "Off")?;
        put_name(doc, o, "V", "Off")?;
    }
    Ok(())
}
fn create(doc: &mut PdfDocument, s: &FieldSpec, all_names: &mut HashSet<String>) -> Result<()> {
    if s.kind == FieldKind::VisualSignature {
        if !all_names.insert(s.name.clone()) {
            return Err("Field name already exists.".into());
        }
        return crate::visual_signatures::create(doc, s);
    }
    if !all_names.insert(s.name.clone()) {
        return Err(format!("A field named '{}' already exists.", s.name));
    }
    let mut form = font_resources(doc)?;
    let mut o = doc.new_dict().map_err(err)?;
    put_name(doc, &mut o, "Type", "Annot")?;
    put_name(doc, &mut o, "Subtype", "Widget")?;
    put_text(doc, &mut o, "T", &s.name)?;
    o.dict_put("P", doc.find_page(s.page as i32).map_err(err)?)
        .map_err(err)?;
    configure(doc, &mut o, s)?;
    let object = doc.add_object(&o).map_err(err)?;
    let mut fields = form
        .get_dict("Fields")
        .map_err(err)?
        .ok_or("Missing fields")?;
    if s.kind == FieldKind::Radio {
        let mut group = None;
        for i in 0..fields.len().map_err(err)? {
            let candidate = fields
                .get_array(i as i32)
                .map_err(err)?
                .ok_or("Invalid field")?;
            if string(&candidate, "T")? == s.group {
                group = Some(candidate);
                break;
            }
        }
        let mut parent = if let Some(g) = group {
            if !managed_radio(&g)?
                || integer(&g, "Ff")? & 1 != 0
                || g.get_dict("AA").map_err(err)?.is_some()
                || g.get_dict("A").map_err(err)?.is_some()
            {
                return Err("Only writable PDFree-created radio groups accept new members.".into());
            }
            if let Some(kids) = g.get_dict("Kids").map_err(err)? {
                for i in 0..kids.len().map_err(err)? {
                    let child = kids
                        .get_array(i as i32)
                        .map_err(err)?
                        .ok_or("Invalid radio member")?;
                    if integer(&child, "F")? & (64 | 128 | 512) != 0
                        || child.get_dict("AA").map_err(err)?.is_some()
                        || child.get_dict("A").map_err(err)?.is_some()
                    {
                        return Err("This radio group contains a locked or scripted member.".into());
                    }
                }
            }
            g
        } else {
            if all_names.contains(&s.group) {
                return Err("Group name already exists.".into());
            }
            let mut p = doc.new_dict().map_err(err)?;
            put_text(doc, &mut p, "T", &s.group)?;
            put_int(doc, &mut p, "PDFreeManaged", 1)?;
            put_name(doc, &mut p, "FT", "Btn")?;
            put_int(
                doc,
                &mut p,
                "Ff",
                (1 << 15) | i32::from(s.read_only) | (i32::from(s.required) << 1),
            )?;
            p.dict_put("Kids", doc.new_array().map_err(err)?)
                .map_err(err)?;
            let p = doc.add_object(&p).map_err(err)?;
            fields
                .array_push(p.try_clone().map_err(err)?)
                .map_err(err)?;
            all_names.insert(s.group.clone());
            p
        };
        let mut child = object.try_clone().map_err(err)?;
        child.dict_delete("T").map_err(err)?;
        child.dict_delete("V").map_err(err)?;
        child.dict_delete("FT").map_err(err)?;
        child.dict_delete("Ff").map_err(err)?;
        child
            .dict_put("Parent", parent.try_clone().map_err(err)?)
            .map_err(err)?;
        let mut kids = parent
            .get_dict("Kids")
            .map_err(err)?
            .ok_or("Invalid radio group")?;
        kids.array_push(object.try_clone().map_err(err)?)
            .map_err(err)?;
        parent.dict_put("Kids", kids).map_err(err)?;
    } else {
        fields
            .array_push(object.try_clone().map_err(err)?)
            .map_err(err)?;
    }
    form.dict_put("Fields", fields).map_err(err)?;
    let mut p = doc.find_page(s.page as i32).map_err(err)?;
    let mut annots = p
        .get_dict("Annots")
        .map_err(err)?
        .unwrap_or(doc.new_array().map_err(err)?);
    let object_id = object.as_indirect().map_err(err)?;
    annots.array_push(object).map_err(err)?;
    p.dict_put("Annots", annots).map_err(err)?;
    dirty_widget(doc, s.page, object_id)?;
    Ok(())
}
fn choice_appearance(doc: &PdfDocument, page: usize, widget: i32) -> Result<()> {
    let native = doc.load_pdf_page(page as i32).map_err(err)?;
    let w = native
        .load_widget(widget)
        .map_err(err)?
        .ok_or("Missing list widget")?;
    let mut field = terminal(&w.annotation().object())?;
    let (labels, exports) = options(&field)?;
    let mut selected = vec![];
    if let Some(value) = field.get_dict_inheritable("V").map_err(err)? {
        if value.is_array().map_err(err)? {
            for i in 0..value.len().map_err(err)?.min(256) {
                selected.push(
                    value
                        .get_array(i as i32)
                        .map_err(err)?
                        .ok_or("Invalid list value")?
                        .as_string()
                        .map_err(err)?,
                );
            }
        } else {
            selected.push(value.as_string().map_err(err)?);
        }
    }
    let is_list = integer(&field, "Ff")? & (1 << 17) == 0;
    let text = if is_list {
        labels
            .iter()
            .zip(&exports)
            .map(|(label, export)| {
                format!(
                    "[{}] {label}",
                    if selected.contains(export) { "x" } else { " " }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        selected
            .first()
            .map(|value| {
                exports
                    .iter()
                    .position(|export| export == value)
                    .and_then(|i| labels.get(i))
                    .cloned()
                    .unwrap_or_else(|| value.clone())
            })
            .unwrap_or_default()
    };
    let saved = ["FT", "Ff", "V"]
        .into_iter()
        .map(|key| Ok((key, field.get_dict(key).map_err(err)?)))
        .collect::<Result<Vec<_>>>()?;
    // MuPDF's list renderer does not highlight selections. Generate a Unicode
    // text appearance through its public widget API, then restore the original
    // choice schema and values. Standard readers retain a real list widget.
    let result = (|| -> Result<()> {
        put_name(doc, &mut field, "FT", "Tx")?;
        let flags = integer(&field, "Ff")?;
        put_int(
            doc,
            &mut field,
            "Ff",
            (flags & 3) | if is_list { 1 << 12 } else { 0 },
        )?;
        put_text(doc, &mut field, "V", &text)?;
        let mut annotation = w.into_annotation();
        let rect = annotation.rect().map_err(err)?;
        annotation.set_rect(rect).map_err(err)?;
        annotation.update().map_err(err)?;
        Ok(())
    })();
    for (key, value) in saved {
        if let Some(value) = value {
            field.dict_put(key, value).map_err(err)?;
        } else {
            field.dict_delete(key).map_err(err)?;
        }
    }
    result
}
fn dirty_widget(doc: &PdfDocument, page: usize, widget: i32) -> Result<()> {
    let native = doc.load_pdf_page(page as i32).map_err(err)?;
    let widget = native
        .load_widget(widget)
        .map_err(err)?
        .ok_or("Missing authored widget")?;
    if matches!(
        widget.r#type().map_err(err)?,
        WidgetType::Listbox | WidgetType::Combobox
    ) {
        return choice_appearance(doc, page, widget.xref().map_err(err)?);
    }
    if matches!(
        widget.r#type().map_err(err)?,
        WidgetType::Text | WidgetType::Combobox
    ) {
        // Public setters notify MuPDF's appearance scheduler; raw dictionary
        // writes alone do not set resynth_required. Geometry remains unchanged.
        let mut annotation = widget.into_annotation();
        let rect = annotation.rect().map_err(err)?;
        annotation.set_rect(rect).map_err(err)?;
    }
    Ok(())
}
fn regenerate(doc: &mut PdfDocument) -> Result<()> {
    // Reload page caches so dictionary-created widgets are visible to the public widget API.
    for p in 0..doc.page_count().map_err(err)? {
        let mut page = doc.load_pdf_page(p).map_err(err)?;
        for mut w in page.widgets() {
            if w.r#type().map_err(err)? != WidgetType::Signature {
                w.update().map_err(err)?;
            }
        }
        page.update().map_err(err)?;
    }
    Ok(())
}
fn set_value(
    doc: &mut PdfDocument,
    id: FieldRef,
    value: &FormValue,
    check_permissions: bool,
) -> Result<usize> {
    let f = locate(doc, id)?;
    if check_permissions && !f.editable {
        return Err(f.reason);
    }
    let mut semantic = f.spec.clone();
    semantic.options = f.export_values.clone();
    semantic.validate_value(value)?;
    if f.spec.kind.is_text() {
        let maximum = integer(&doc.new_indirect(id.field, 0).map_err(err)?, "MaxLen")?;
        if maximum > 0 && value.text().chars().count() > maximum as usize {
            return Err(format!("This field allows at most {maximum} characters."));
        }
    }
    if matches!(f.spec.kind, FieldKind::Checkbox | FieldKind::Radio)
        && !button_values(doc, id.field)?.contains(&value.text())
    {
        return Err("Select an available button value.".into());
    }
    if f.spec.kind == FieldKind::List
        && !f.multi_select
        && matches!(value,FormValue::Selection(v) if v.len()>1)
    {
        return Err("This list accepts one selection.".into());
    }
    let p = doc.load_pdf_page(f.spec.page as i32).map_err(err)?;
    let mut w = p
        .load_widget(id.widget)
        .map_err(err)?
        .ok_or("Missing widget")?;
    if let FormValue::Selection(values) = value {
        if f.spec.kind != FieldKind::List {
            return Err("Only list fields accept multiple values.".into());
        }
        let mut field = doc.new_indirect(id.field, 0).map_err(err)?;
        let mut array = doc.new_array().map_err(err)?;
        let mut indices = doc.new_array().map_err(err)?;
        let mut ordered = values
            .iter()
            .map(|v| {
                f.export_values
                    .iter()
                    .position(|x| x == v)
                    .ok_or("Invalid selection")
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ordered.sort_unstable();
        ordered.dedup();
        for index in ordered {
            let v = &f.export_values[index];
            array
                .array_push(doc.new_string(v).map_err(err)?)
                .map_err(err)?;
            indices
                .array_push(
                    doc.new_int(
                        f.export_values
                            .iter()
                            .position(|x| x == v)
                            .ok_or("Invalid selection")? as i32,
                    )
                    .map_err(err)?,
                )
                .map_err(err)?;
        }
        field.dict_put("V", array).map_err(err)?;
        field.dict_put("I", indices).map_err(err)?;
    } else if !w.set_value(doc, &value.text(), true).map_err(err)? {
        return Err("The PDF field rejected this value.".into());
    }
    // Refresh every widget sharing the field, including appearances on other pages.
    for index in 0..doc.page_count().map_err(err)? {
        let mut p = doc.load_pdf_page(index).map_err(err)?;
        for mut linked in p.widgets() {
            let o = terminal(&linked.annotation().object())?;
            if o.as_indirect().map_err(err)? == id.field {
                if matches!(f.spec.kind, FieldKind::List | FieldKind::Dropdown) {
                    choice_appearance(doc, index as usize, linked.xref().map_err(err)?)?;
                } else {
                    linked.update().map_err(err)?;
                }
            }
        }
        p.update().map_err(err)?;
    }
    Ok(f.spec.page)
}
pub(crate) fn apply(doc: &mut PdfDocument, edit: &FormEdit) -> Result<usize> {
    ensure_writable(doc)?;
    match edit {
        FormEdit::SetVisualSignature { id, mark } => {
            let f = locate(doc, *id)?;
            if f.spec.kind != FieldKind::VisualSignature || !f.editable {
                return Err("Select an editable visual signature slot.".into());
            }
            crate::visual_signatures::mark(doc, *id, &f.spec, mark)?;
            Ok(f.spec.page)
        }
        FormEdit::CreateDetected(detected) => {
            let specs = detected.iter().map(|d| d.spec.clone()).collect::<Vec<_>>();
            let page = apply(doc, &FormEdit::Create(specs))?;
            for d in detected {
                let f = crate::forms::page(doc, d.spec.page)?
                    .into_iter()
                    .find(|f| {
                        f.spec.name == d.spec.name
                            || (d.spec.kind == FieldKind::Radio
                                && f.spec.group == d.spec.group
                                && f.export_value == d.spec.name)
                    })
                    .ok_or("Detected field was not created")?;
                let mut o = doc.new_indirect(f.id.widget, 0).map_err(err)?;
                o.dict_put(
                    "PDFreeConfidence",
                    doc.new_real(d.confidence.score).map_err(err)?,
                )
                .map_err(err)?;
                let mut origin = doc.new_array().map_err(err)?;
                for n in [
                    d.confidence.origin.left,
                    d.confidence.origin.top,
                    d.confidence.origin.right,
                    d.confidence.origin.bottom,
                ] {
                    origin
                        .array_push(doc.new_real(n).map_err(err)?)
                        .map_err(err)?;
                }
                o.dict_put("PDFreeOrigin", origin).map_err(err)?;
            }
            Ok(page)
        }
        FormEdit::Create(specs) => {
            if security(doc)?.2 {
                return Err("Calculated forms are read-only for field authoring.".into());
            }
            if !permissions(doc).contains(Permission::MODIFY | Permission::ANNOTATE) {
                return Err("This PDF does not permit creating form fields.".into());
            }
            if specs.is_empty() || specs.len() > crate::constants::MAX_CREATE_FIELDS {
                return Err("Create 1–2,000 fields per operation.".into());
            }
            let mut names = HashSet::new();
            for p in 0..doc.page_count().map_err(err)? {
                for f in crate::visual_signatures::fields(doc, p as usize)? {
                    names.insert(f.spec.name);
                }
            }
            for o in objects(doc)? {
                let t = string(&o, "T")?;
                if !t.is_empty() {
                    names.insert(t);
                }
            }
            for s in specs {
                create(doc, s, &mut names)?;
            }
            regenerate(doc)?;
            Ok(specs[0].page)
        }
        FormEdit::SetValue { id, value } => set_value(doc, *id, value, true),
        FormEdit::Confirm(id) => {
            let f = locate(doc, *id)?;
            if !f.authorable {
                return Err("This field cannot be changed.".into());
            }
            let mut o = doc.new_indirect(id.widget, 0).map_err(err)?;
            put_int(doc, &mut o, "PDFreeReview", 0)?;
            Ok(f.spec.page)
        }
        FormEdit::Update { id, spec } => {
            let f = locate(doc, *id)?;
            if !f.authorable {
                return Err("Type and geometry changes require a simple editable field.".into());
            }
            if f.spec.kind == FieldKind::VisualSignature && spec.kind == FieldKind::VisualSignature
            {
                crate::visual_signatures::update(doc, &f, spec)?;
                return Ok(spec.page);
            }
            if spec.page != f.spec.page || spec.name != f.spec.name {
                return Err("Keep this field's name and page when updating.".into());
            }
            let compatible_choices = matches!(f.spec.kind, FieldKind::Dropdown | FieldKind::List)
                && matches!(spec.kind, FieldKind::Dropdown | FieldKind::List)
                && spec.options == f.spec.options;
            if !f.is_empty() {
                let mut s = spec.clone();
                if matches!(
                    spec.kind,
                    FieldKind::Checkbox
                        | FieldKind::Radio
                        | FieldKind::VisualSignature
                        | FieldKind::DigitalSignature
                ) && spec.kind != f.spec.kind
                {
                    return Err("Clear the field's value before changing its type.".into());
                }
                s.options = if compatible_choices {
                    f.export_values.clone()
                } else {
                    spec.options.clone()
                };
                s.validate_value(&f.value)?;
            }
            let mut o = doc.new_indirect(id.field, 0).map_err(err)?;
            let preserved_options = if compatible_choices {
                o.get_dict_inheritable("Opt").map_err(err)?
            } else {
                None
            };
            if f.spec.kind == FieldKind::Radio && spec.kind == FieldKind::Radio {
                if spec.group != f.spec.group {
                    return Err("Keep this radio button in its existing group.".into());
                }
                spec.validate()?;
                let mut widget = doc.new_indirect(id.widget, 0).map_err(err)?;
                widget.dict_put("Rect", pdf_rect(doc, spec)?).map_err(err)?;
                metadata(doc, &mut widget, spec)?;
                let mut parent = doc.new_indirect(id.field, 0).map_err(err)?;
                let flags = (integer(&parent, "Ff")? & !3)
                    | i32::from(spec.read_only)
                    | (i32::from(spec.required) << 1);
                put_int(doc, &mut parent, "Ff", flags)?;
                let mut states = doc.new_dict().map_err(err)?;
                states
                    .dict_put(
                        "Off",
                        appearance(
                            doc,
                            spec.rect.right - spec.rect.left,
                            spec.rect.bottom - spec.rect.top,
                            false,
                        )?,
                    )
                    .map_err(err)?;
                states
                    .dict_put(
                        f.export_value.as_str(),
                        appearance(
                            doc,
                            spec.rect.right - spec.rect.left,
                            spec.rect.bottom - spec.rect.top,
                            true,
                        )?,
                    )
                    .map_err(err)?;
                let mut ap = doc.new_dict().map_err(err)?;
                ap.dict_put("N", states).map_err(err)?;
                widget.dict_put("AP", ap).map_err(err)?;
                regenerate(doc)?;
                return Ok(spec.page);
            }
            if spec.kind == FieldKind::Radio || f.spec.kind == FieldKind::Radio {
                return Err(
                    "Radio group members keep their type; draw another field to replace one."
                        .into(),
                );
            }
            if f.spec.kind == FieldKind::VisualSignature && spec.kind != FieldKind::VisualSignature
            {
                if !f.is_empty() {
                    return Err("Clear the signature mark before converting its slot.".into());
                }
                if !permissions(doc).contains(Permission::MODIFY | Permission::ANNOTATE) {
                    return Err("Field creation is not permitted.".into());
                }
                let mut o = doc.new_indirect(id.widget, 0).map_err(err)?;
                o.dict_delete("PDFreeVisual").map_err(err)?;
                o.dict_delete("PDFreeMark").map_err(err)?;
                o.dict_delete("PDFreeName").map_err(err)?;
                put_name(doc, &mut o, "Subtype", "Widget")?;
                put_text(doc, &mut o, "T", &spec.name)?;
                configure(doc, &mut o, spec)?;
                let mut form = font_resources(doc)?;
                let mut fields = form
                    .get_dict("Fields")
                    .map_err(err)?
                    .ok_or("Missing fields")?;
                fields
                    .array_push(doc.new_indirect(id.widget, 0).map_err(err)?)
                    .map_err(err)?;
                form.dict_put("Fields", fields).map_err(err)?;
                dirty_widget(doc, spec.page, id.widget)?;
                regenerate(doc)?;
                return Ok(spec.page);
            }
            if spec.kind == FieldKind::VisualSignature {
                if !permissions(doc).contains(Permission::ANNOTATE) {
                    return Err("Visual signature annotations are not permitted.".into());
                }
                let mut form = font_resources(doc)?;
                let mut fields = form
                    .get_dict("Fields")
                    .map_err(err)?
                    .ok_or("Missing fields")?;
                for i in (0..fields.len().map_err(err)?).rev() {
                    if fields
                        .get_array(i as i32)
                        .map_err(err)?
                        .ok_or("Invalid field")?
                        .as_indirect()
                        .map_err(err)?
                        == id.field
                    {
                        fields.array_delete(i as i32).map_err(err)?;
                    }
                }
                form.dict_put("Fields", fields).map_err(err)?;
                for key in ["FT", "T", "V", "DV", "Opt", "I", "AP", "AS", "DA"] {
                    o.dict_delete(key).map_err(err)?;
                }
                put_name(doc, &mut o, "Subtype", "Stamp")?;
                put_text(doc, &mut o, "PDFreeName", &spec.name)?;
                put_int(doc, &mut o, "PDFreeVisual", 1)?;
                metadata(doc, &mut o, spec)?;
                o.dict_put("Rect", pdf_rect(doc, spec)?).map_err(err)?;
                crate::visual_signatures::mark(doc, *id, spec, &viewer_core::SignatureMark::Empty)?;
                return Ok(spec.page);
            }
            if spec.kind == FieldKind::DigitalSignature
                || (f.is_empty() && spec.kind != f.spec.kind)
            {
                o.dict_delete("V").map_err(err)?;
                o.dict_delete("DV").map_err(err)?;
            }
            for key in ["AP", "AS", "Opt", "I", "DA", "Ff", "FT"] {
                o.dict_delete(key).map_err(err)?;
            }
            let mut unlocked = spec.clone();
            unlocked.read_only = false;
            configure(doc, &mut o, &unlocked)?;
            if let Some(options) = preserved_options {
                o.dict_put("Opt", options).map_err(err)?;
            }
            dirty_widget(doc, spec.page, id.widget)?;
            regenerate(doc)?;
            if !f.is_empty() {
                set_value(doc, *id, &f.value, false)?;
            }
            let flags = integer(&o, "Ff")?;
            put_int(doc, &mut o, "Ff", (flags & !1) | i32::from(spec.read_only))?;
            Ok(spec.page)
        }
        FormEdit::Delete(id) => {
            let f = locate(doc, *id)?;
            if !f.authorable {
                return Err("Only simple editable fields can be deleted.".into());
            }
            suppress(doc, &f)?;
            if f.spec.kind == FieldKind::Radio {
                let mut parent = doc.new_indirect(id.field, 0).map_err(err)?;
                let mut kids = parent
                    .get_dict("Kids")
                    .map_err(err)?
                    .ok_or("Invalid radio group")?;
                for i in (0..kids.len().map_err(err)?).rev() {
                    if kids
                        .get_array(i as i32)
                        .map_err(err)?
                        .ok_or("Invalid radio member")?
                        .as_indirect()
                        .map_err(err)?
                        == id.widget
                    {
                        kids.array_delete(i as i32).map_err(err)?;
                    }
                }
                if f.value.text() == f.export_value {
                    put_name(doc, &mut parent, "V", "Off")?;
                    for i in 0..kids.len().map_err(err)? {
                        let mut k = kids
                            .get_array(i as i32)
                            .map_err(err)?
                            .ok_or("Invalid radio member")?;
                        put_name(doc, &mut k, "AS", "Off")?;
                    }
                }
                parent
                    .dict_put("Kids", kids.try_clone().map_err(err)?)
                    .map_err(err)?;
                if kids.len().map_err(err)? == 0 {
                    remove_root_field(doc, id.field)?;
                }
                crate::visual_signatures::delete(doc, &f)?;
                regenerate(doc)?;
                return Ok(f.spec.page);
            }
            if f.spec.kind == FieldKind::VisualSignature {
                crate::visual_signatures::delete(doc, &f)?;
                return Ok(f.spec.page);
            }
            let mut form = font_resources(doc)?;
            let mut fields = form
                .get_dict("Fields")
                .map_err(err)?
                .ok_or("Missing fields")?;
            for i in (0..fields.len().map_err(err)?).rev() {
                if fields
                    .get_array(i as i32)
                    .map_err(err)?
                    .ok_or("Invalid field")?
                    .as_indirect()
                    .map_err(err)?
                    == id.field
                {
                    fields.array_delete(i as i32).map_err(err)?;
                }
            }
            form.dict_put("Fields", fields).map_err(err)?;
            let mut p = doc.find_page(f.spec.page as i32).map_err(err)?;
            let mut annots = p
                .get_dict("Annots")
                .map_err(err)?
                .ok_or("Missing page annotations")?;
            for i in (0..annots.len().map_err(err)?).rev() {
                if annots
                    .get_array(i as i32)
                    .map_err(err)?
                    .ok_or("Invalid annotation")?
                    .as_indirect()
                    .map_err(err)?
                    == id.widget
                {
                    annots.array_delete(i as i32).map_err(err)?;
                }
            }
            p.dict_put("Annots", annots).map_err(err)?;
            regenerate(doc)?;
            Ok(f.spec.page)
        }
    }
}
fn appearances(doc: &PdfDocument, id: FieldRef) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
    let widget = doc.new_indirect(id.widget, 0).map_err(err)?;
    let Some(ap) = widget.get_dict("AP").map_err(err)? else {
        return Ok(vec![]);
    };
    let Some(normal) = ap.get_dict("N").map_err(err)? else {
        return Ok(vec![]);
    };
    if normal.is_stream().map_err(err)? {
        return Ok(vec![(vec![], normal.read_stream().map_err(err)?)]);
    }
    let mut streams = vec![];
    for i in 0..normal
        .dict_len()
        .map_err(err)?
        .min(crate::constants::MAX_APPEARANCE_STATES)
    {
        let key = normal
            .get_dict_key(i as i32)
            .map_err(err)?
            .ok_or("Missing appearance state")?
            .as_name()
            .map_err(err)?;
        let value = normal
            .get_dict(std::str::from_utf8(&key).map_err(err)?)
            .map_err(err)?
            .ok_or("Missing appearance stream")?;
        streams.push((key, value.read_stream().map_err(err)?));
    }
    streams.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(streams)
}
pub(crate) fn verify(source: &PdfDocument, saved: &PdfDocument) -> Result<()> {
    for p in 0..source.page_count().map_err(err)? {
        let before = page(source, p as usize)?;
        let after = page(saved, p as usize)?;
        if before.len() != after.len() {
            return Err("Saved PDF failed form-field count verification.".into());
        }
        for (a, b) in before.iter().zip(&after) {
            if appearances(source, a.id)? != appearances(saved, b.id)? {
                return Err("Saved PDF failed form appearance verification.".into());
            }
            if a.spec != b.spec
                || a.value != b.value
                || a.export_values != b.export_values
                || a.password != b.password
                || a.multi_select != b.multi_select
            {
                return Err("Saved PDF failed form value/properties verification.".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "forms_tests.rs"]
mod tests;
