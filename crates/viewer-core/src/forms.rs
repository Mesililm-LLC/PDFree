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

fn valid_calendar_date(year: u32, month: u32, day: u32) -> bool {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
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
    year >= 1 && day >= 1 && day <= days
}

/// Parse a date written in a common variant format and render it in the
/// field's expected `format` (one of YYYY-MM-DD, MM/DD/YYYY, DD/MM/YYYY).
/// Accepts any of `-`, `/`, `.`, space, or comma separators, one- or
/// two-digit months and days, two-digit years (00-69 → 2000s, 70-99 →
/// 1900s), English month names, and compact YYYYMMDD / MMDDYYYY digits.
/// When month/day order is ambiguous, the target format's order decides
/// (month-first for YYYY-MM-DD). Returns None for anything that is not a
/// real calendar date.
pub fn normalize_date(input: &str, format: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    let text = input.trim();
    if text.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = text
        .split(|c: char| matches!(c, '-' | '/' | '.' | ',') || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .collect();
    let full_year = |token: &str, value: u32| -> u32 {
        if token.len() == 2 {
            if value <= 69 {
                2000 + value
            } else {
                1900 + value
            }
        } else {
            value
        }
    };
    let numeric = |token: &str| -> Option<u32> {
        token
            .bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| token.parse().ok())
            .flatten()
    };
    let (year, month, day) = if tokens.len() == 1 {
        // Compact digits: YYYYMMDD or MMDDYYYY.
        let t = tokens[0];
        if t.len() != 8 || !t.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let (a, b, c) = (
            t[..2].parse::<u32>().ok()?,
            t[2..4].parse::<u32>().ok()?,
            t[4..].parse::<u32>().ok()?,
        );
        if (1900..=2099).contains(&(a * 100 + b)) {
            (a * 100 + b, c / 100, c % 100)
        } else if (1900..=2099).contains(&c) {
            (c, a, b)
        } else {
            return None;
        }
    } else if tokens.len() == 3 {
        let alpha: Vec<usize> = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.bytes().all(|b| b.is_ascii_alphabetic()))
            .map(|(i, _)| i)
            .collect();
        if alpha.len() == 1 {
            // "24 Nov 1997" or "Nov 24, 1997".
            let name = tokens[alpha[0]].to_ascii_lowercase();
            let month = MONTHS
                .iter()
                .position(|m| *m == name || m.starts_with(&name[..3.min(name.len())]))
                .filter(|_| name.len() >= 3)? as u32
                + 1;
            let rest: Vec<&str> = tokens
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != alpha[0])
                .map(|(_, t)| *t)
                .collect();
            let (a, b) = (numeric(rest[0])?, numeric(rest[1])?);
            let (day, year) = if rest[1].len() == 4 || b > 31 {
                (a, full_year(rest[1], b))
            } else if rest[0].len() == 4 || a > 31 {
                (b, full_year(rest[0], a))
            } else {
                (a, full_year(rest[1], b))
            };
            (year, month, day)
        } else if alpha.is_empty() {
            let p: Vec<u32> = tokens.iter().map(|t| numeric(t)).collect::<Option<_>>()?;
            let year_at = if tokens[0].len() == 4 {
                0
            } else if tokens[2].len() == 4 || tokens[2].len() == 2 {
                2
            } else {
                return None;
            };
            if year_at == 0 {
                (full_year(tokens[0], p[0]), p[1], p[2])
            } else {
                let (month, day) = if p[0] > 12 {
                    (p[1], p[0])
                } else if p[1] > 12 {
                    (p[0], p[1])
                } else if format == "DD/MM/YYYY" {
                    (p[1], p[0])
                } else {
                    (p[0], p[1])
                };
                (full_year(tokens[2], p[2]), month, day)
            }
        } else {
            return None;
        }
    } else {
        return None;
    };
    if !valid_calendar_date(year, month, day) {
        return None;
    }
    Some(match format {
        "MM/DD/YYYY" => format!("{month:02}/{day:02}/{year:04}"),
        "DD/MM/YYYY" => format!("{day:02}/{month:02}/{year:04}"),
        "YYYY-MM-DD" => format!("{year:04}-{month:02}-{day:02}"),
        _ => return None,
    })
}

#[cfg(test)]
mod date_tests {
    use super::normalize_date;

    #[test]
    fn common_date_variants_normalize_to_the_field_format() {
        // Already canonical stays canonical in every supported format.
        assert_eq!(
            normalize_date("1997-11-24", "YYYY-MM-DD").as_deref(),
            Some("1997-11-24")
        );
        // US slash dates into ISO, including one-digit parts and 2-digit years.
        assert_eq!(
            normalize_date("11/24/1997", "YYYY-MM-DD").as_deref(),
            Some("1997-11-24")
        );
        assert_eq!(
            normalize_date("1/5/26", "YYYY-MM-DD").as_deref(),
            Some("2026-01-05")
        );
        assert_eq!(
            normalize_date("1/5/98", "YYYY-MM-DD").as_deref(),
            Some("1998-01-05")
        );
        // Unambiguous day-first input lands correctly in a US field.
        assert_eq!(
            normalize_date("24/11/1997", "MM/DD/YYYY").as_deref(),
            Some("11/24/1997")
        );
        // Ambiguous order follows the target format.
        assert_eq!(
            normalize_date("03/04/1997", "DD/MM/YYYY").as_deref(),
            Some("03/04/1997")
        );
        assert_eq!(
            normalize_date("03/04/1997", "MM/DD/YYYY").as_deref(),
            Some("03/04/1997")
        );
        // Month names, dotted ISO, and compact digits.
        assert_eq!(
            normalize_date("Nov 24, 1997", "YYYY-MM-DD").as_deref(),
            Some("1997-11-24")
        );
        assert_eq!(
            normalize_date("24 November 1997", "MM/DD/YYYY").as_deref(),
            Some("11/24/1997")
        );
        assert_eq!(
            normalize_date("1997.11.24", "MM/DD/YYYY").as_deref(),
            Some("11/24/1997")
        );
        assert_eq!(
            normalize_date("19971124", "YYYY-MM-DD").as_deref(),
            Some("1997-11-24")
        );
        assert_eq!(
            normalize_date("11241997", "YYYY-MM-DD").as_deref(),
            Some("1997-11-24")
        );
        // Leap years are real calendar checks, not pattern matches.
        assert_eq!(
            normalize_date("2/29/2000", "YYYY-MM-DD").as_deref(),
            Some("2000-02-29")
        );
        assert_eq!(normalize_date("2/29/1997", "YYYY-MM-DD"), None);
        assert_eq!(normalize_date("13/40/1997", "YYYY-MM-DD"), None);
        assert_eq!(normalize_date("not a date", "YYYY-MM-DD"), None);
        assert_eq!(normalize_date("", "YYYY-MM-DD"), None);
    }
}
