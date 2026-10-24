//! Owned form contracts. Native object identities are valid only in their stamped revision.
use crate::TextRect;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Multiline,
    Date,
    Number,
    Checkbox,
    Radio,
    Dropdown,
    List,
    VisualSignature,
    DigitalSignature,
}
impl FieldKind {
    pub const ALL: [Self; 10] = [
        Self::Text,
        Self::Multiline,
        Self::Date,
        Self::Number,
        Self::Checkbox,
        Self::Radio,
        Self::Dropdown,
        Self::List,
        Self::VisualSignature,
        Self::DigitalSignature,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Multiline => "Multiline text",
            Self::Date => "Date",
            Self::Number => "Number",
            Self::Checkbox => "Checkbox",
            Self::Radio => "Radio button",
            Self::Dropdown => "Dropdown",
            Self::List => "List",
            Self::VisualSignature => "Visual signature",
            Self::DigitalSignature => "Digital signature",
        }
    }
    pub fn is_text(self) -> bool {
        matches!(
            self,
            Self::Text | Self::Multiline | Self::Date | Self::Number
        )
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct FieldSpec {
    pub page: usize,
    pub name: String,
    pub label: String,
    pub kind: FieldKind,
    pub rect: TextRect,
    pub required: bool,
    pub read_only: bool,
    pub options: Vec<String>,
    pub group: String,
    pub review: bool,
    pub date_format: String,
    pub decimals: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormValue {
    Text(String),
    Selection(Vec<String>),
}
impl FormValue {
    pub fn text(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Selection(v) => v.join("\n"),
        }
    }
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Text(s) => s.is_empty(),
            Self::Selection(v) => v.is_empty(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldRef {
    pub field: i32,
    pub widget: i32,
    pub page: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FormField {
    pub id: FieldRef,
    pub spec: FieldSpec,
    pub value: FormValue,
    pub export_values: Vec<String>,
    pub export_value: String,
    pub editable: bool,
    pub signable: bool,
    pub authorable: bool,
    pub reason: String,
    pub detection: Option<DetectionConfidence>,
    pub password: bool,
    pub multi_select: bool,
}
impl FormField {
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
            || (matches!(self.spec.kind, FieldKind::Checkbox | FieldKind::Radio)
                && self.value.text() == "Off")
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct DetectionConfidence {
    pub score: f32,
    pub origin: TextRect,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DetectedField {
    pub spec: FieldSpec,
    pub confidence: DetectionConfidence,
}
#[derive(Clone, Debug, PartialEq)]
pub enum FormEdit {
    SetVisualSignature { id: FieldRef, mark: SignatureMark },
    SetValue { id: FieldRef, value: FormValue },
    Create(Vec<FieldSpec>),
    CreateDetected(Vec<DetectedField>),
    Update { id: FieldRef, spec: FieldSpec },
    Delete(FieldRef),
    Confirm(FieldRef),
}
#[derive(Clone, Debug, PartialEq)]
pub enum SignatureMark {
    Empty,
    Typed(String),
    Strokes(Vec<Vec<[f32; 2]>>),
    Image(Vec<u8>),
}
pub fn signature_image_size(data: &[u8]) -> Result<(u32, u32), String> {
    let size =
        if data.len() >= 24 && data.starts_with(b"\x89PNG\r\n\x1a\n") && &data[12..16] == b"IHDR" {
            Some((
                u32::from_be_bytes(data[16..20].try_into().unwrap()),
                u32::from_be_bytes(data[20..24].try_into().unwrap()),
            ))
        } else if data.starts_with(&[0xff, 0xd8]) {
            let mut at = 2;
            let mut size = None;
            while at + 4 <= data.len() {
                if data[at] != 0xff {
                    break;
                }
                let marker = data[at + 1];
                at += 2;
                if marker == 0xff {
                    at -= 1;
                    continue;
                }
                if matches!(marker, 0xd8..=0xda) {
                    break;
                }
                let length = u16::from_be_bytes([data[at], data[at + 1]]) as usize;
                if length < 2 || at + length > data.len() {
                    break;
                }
                if matches!(marker,0xc0..=0xc3|0xc5..=0xc7|0xc9..=0xcb|0xcd..=0xcf) && length >= 8 {
                    size = Some((
                        u16::from_be_bytes([data[at + 5], data[at + 6]]) as u32,
                        u16::from_be_bytes([data[at + 3], data[at + 4]]) as u32,
                    ));
                    break;
                }
                at += length;
            }
            size
        } else {
            None
        };
    let (w, h) = size.ok_or("Choose a valid PNG or JPEG signature image.")?;
    if w == 0 || h == 0 || w > 8192 || h > 8192 || u64::from(w) * u64::from(h) > 16_000_000 {
        return Err("Signature image dimensions exceed the safe limit.".into());
    }
    Ok((w, h))
}
impl FieldSpec {
    pub fn validate(&self) -> Result<(), String> {
        let r = &self.rect;
        if ![r.left, r.top, r.right, r.bottom]
            .iter()
            .all(|x| x.is_finite() && *x >= 0. && *x <= 100_000.)
            || r.right - r.left < 3.
            || r.bottom - r.top < 3.
        {
            return Err("Draw a field at least 3 PDF points wide and high.".into());
        }
        if self.name.trim().is_empty()
            || self.name.len() > 512
            || self.name.chars().any(|c| c.is_control() || c == '.')
            || self.label.len() > 4096
            || self.label.contains('\0')
        {
            return Err(
                "Use a field name of 1–512 bytes without dots or control characters.".into(),
            );
        }
        if self.decimals > 12
            || !["YYYY-MM-DD", "MM/DD/YYYY", "DD/MM/YYYY"].contains(&self.date_format.as_str())
        {
            return Err("Choose a supported date format and 0–12 decimal places.".into());
        }
        if self.options.len() > 256
            || self
                .options
                .iter()
                .any(|s| s.is_empty() || s.len() > 1024 || s.contains('\0'))
            || self.options.iter().collect::<HashSet<_>>().len() != self.options.len()
        {
            return Err("Choice options must be distinct nonempty values (maximum 256).".into());
        }
        if matches!(self.kind, FieldKind::Dropdown | FieldKind::List) && self.options.is_empty() {
            return Err("Add at least one choice option.".into());
        }
        if self.kind == FieldKind::Radio
            && (self.group.trim().is_empty()
                || self.group.len() > 512
                || self.group.contains(['.', '\0']))
        {
            return Err("Radio buttons need a group name without dots.".into());
        }
        Ok(())
    }
    pub fn validate_value(&self, value: &FormValue) -> Result<(), String> {
        let text = value.text();
        if text.len() > 16_384 || text.contains('\0') {
            return Err("Field values are limited to 16,384 UTF-8 bytes.".into());
        }
        if text.is_empty() {
            return Ok(());
        }
        match self.kind {
            FieldKind::Number => {
                let s = text
                    .strip_prefix('-')
                    .or_else(|| text.strip_prefix('+'))
                    .unwrap_or(&text);
                let mut parts = s.split('.');
                let whole = parts.next().unwrap_or("");
                let decimal = parts.next();
                if whole.is_empty()
                    || !whole.bytes().all(|b| b.is_ascii_digit())
                    || decimal.is_some_and(|s| {
                        s.is_empty()
                            || s.len() > self.decimals as usize
                            || !s.bytes().all(|b| b.is_ascii_digit())
                    })
                    || parts.next().is_some()
                {
                    return Err(format!(
                        "Enter a decimal number with at most {} decimal places.",
                        self.decimals
                    ));
                }
            }
            FieldKind::Date => {
                let separator = if self.date_format == "YYYY-MM-DD" {
                    '-'
                } else {
                    '/'
                };
                let p: Vec<_> = text.split(separator).collect();
                if p.len() != 3 {
                    return Err(format!("Enter a date as {}.", self.date_format));
                }
                let (y, m, d) = match self.date_format.as_str() {
                    "MM/DD/YYYY" => (p[2], p[0], p[1]),
                    "DD/MM/YYYY" => (p[2], p[1], p[0]),
                    _ => (p[0], p[1], p[2]),
                };
                let parse = |s: &str| {
                    s.bytes()
                        .all(|b| b.is_ascii_digit())
                        .then(|| s.parse::<u32>().ok())
                        .flatten()
                };
                let (Some(year), Some(month), Some(day)) = (parse(y), parse(m), parse(d)) else {
                    return Err("Enter a valid date.".into());
                };
                let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
                let days = match month {
                    4 | 6 | 9 | 11 => 30,
                    2 => {
                        if leap {
                            29
                        } else {
                            28
                        }
                    }
                    1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                    _ => 0,
                };
                if y.len() != 4
                    || m.len() != 2
                    || d.len() != 2
                    || year == 0
                    || day == 0
                    || day > days
                {
                    return Err("Enter a valid calendar date.".into());
                }
            }
            FieldKind::Dropdown | FieldKind::List => {
                let selected = match value {
                    FormValue::Text(s) => vec![s.clone()],
                    FormValue::Selection(v) => v.clone(),
                };
                if selected.iter().any(|s| !self.options.contains(s)) {
                    return Err("Select values from the field's options.".into());
                }
            }
            _ => {}
        }
        Ok(())
    }
}
