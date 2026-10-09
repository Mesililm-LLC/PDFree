//! Engine-neutral annotation drafts, bounded history, and Windows filename rules.
use crate::TextRect;
use std::{collections::VecDeque, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub enum Annotation {
    Highlight {
        page: usize,
        rects: Vec<TextRect>,
    },
    Note {
        page: usize,
        x: f32,
        y: f32,
        text: String,
    },
}

impl Annotation {
    pub fn page(&self) -> usize {
        match self {
            Self::Highlight { page, .. } | Self::Note { page, .. } => *page,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Highlight { rects, .. } => {
                if rects.is_empty()
                    || rects.len() > 1_000
                    || rects.iter().any(|r| {
                        ![r.left, r.top, r.right, r.bottom]
                            .iter()
                            .all(|v| v.is_finite() && *v >= 0.0 && *v <= 100_000.0)
                            || r.right <= r.left
                            || r.bottom <= r.top
                    })
                {
                    return Err("Highlight geometry is invalid or exceeds 1,000 rectangles.".into());
                }
            }
            Self::Note { x, y, text, .. } => {
                if !x.is_finite()
                    || !y.is_finite()
                    || *x < 0.0
                    || *y < 0.0
                    || *x > 100_000.0
                    || *y > 100_000.0
                    || text.trim().is_empty()
                    || text.len() > 16_384
                    || text.contains('\0')
                {
                    return Err(
                        "Notes need a valid page position and 1–16,384 UTF-8 bytes of text.".into(),
                    );
                }
            }
        }
        Ok(())
    }
    fn bytes(&self) -> usize {
        128 + match self {
            Self::Highlight { rects, .. } => rects.len() * std::mem::size_of::<TextRect>(),
            Self::Note { text, .. } => text.len(),
        }
    }
}

struct Revision {
    id: u64,
    annotations: Arc<Vec<Annotation>>,
    bytes: usize,
}
pub struct EditHistory {
    revisions: VecDeque<Revision>,
    cursor: usize,
    next: u64,
    saved: u64,
}
impl Default for EditHistory {
    fn default() -> Self {
        Self {
            revisions: VecDeque::from([Revision {
                id: 0,
                annotations: Arc::new(Vec::new()),
                bytes: 0,
            }]),
            cursor: 0,
            next: 1,
            saved: 0,
        }
    }
}
impl EditHistory {
    pub fn annotations(&self) -> &[Annotation] {
        &self.revisions[self.cursor].annotations
    }
    pub fn revision(&self) -> u64 {
        self.revisions[self.cursor].id
    }
    pub fn dirty(&self) -> bool {
        self.revision() != self.saved
    }
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }
    pub fn can_redo(&self) -> bool {
        self.cursor + 1 < self.revisions.len()
    }
    pub fn mark_saved(&mut self) {
        self.saved = self.revision();
    }
    pub fn undo(&mut self) -> bool {
        if !self.can_undo() {
            return false;
        }
        self.cursor -= 1;
        true
    }
    pub fn redo(&mut self) -> bool {
        if !self.can_redo() {
            return false;
        }
        self.cursor += 1;
        true
    }
    pub fn push(&mut self, annotations: Vec<Annotation>) -> Result<(), String> {
        if annotations.len() > 2_000 {
            return Err("This session is limited to 2,000 annotations.".into());
        }
        for annotation in &annotations {
            annotation.validate()?;
        }
        if annotations == self.annotations() {
            return Ok(());
        }
        let bytes = annotations.iter().map(Annotation::bytes).sum();
        const BUDGET: usize = 32 * 1024 * 1024;
        if bytes > BUDGET {
            return Err("Annotation drafts exceed the 32 MiB history budget.".into());
        }
        self.revisions.truncate(self.cursor + 1);
        self.revisions.push_back(Revision {
            id: self.next,
            annotations: Arc::new(annotations),
            bytes,
        });
        self.next += 1;
        self.cursor += 1;
        while self.revisions.len() > 201
            || self.revisions.iter().map(|r| r.bytes).sum::<usize>() > BUDGET
        {
            self.revisions.pop_front();
            self.cursor -= 1;
        }
        Ok(())
    }
}

pub fn validate_pdf_name(name: &str) -> Result<String, String> {
    if name.trim().is_empty()
        || name == "."
        || name == ".."
        || name.ends_with([' ', '.'])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
    {
        return Err(
            "Use a filename without separators, reserved characters, or a trailing space/dot."
                .into(),
        );
    }
    let base = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_uppercase();
    if matches!(
        base.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        base.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    }) {
        return Err("That filename is reserved by Windows.".into());
    }
    let name = if name.to_ascii_lowercase().ends_with(".pdf") {
        name.to_owned()
    } else {
        format!("{name}.pdf")
    };
    if name.encode_utf16().count() > 255 {
        return Err("Filename exceeds 255 Windows characters.".into());
    }
    Ok(name)
}
