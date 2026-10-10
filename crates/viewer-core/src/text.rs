//! Owned text geometry in page-local PDF points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl TextRect {
    pub fn union(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }

    fn distance(self, (x, y): (f32, f32)) -> f32 {
        let dx = (self.left - x).max(0.0).max(x - self.right);
        let dy = (self.top - y).max(0.0).max(y - self.bottom);
        let center = (x - (self.left + self.right) * 0.5).powi(2)
            + (y - (self.top + self.bottom) * 0.5).powi(2);
        dx * dx + dy * dy * 16.0 + center * 0.0001
    }
}

#[derive(Clone, Debug)]
pub struct TextGlyph {
    pub ch: char,
    pub line: usize,
    pub bounds: TextRect,
}

#[derive(Clone, Debug, Default)]
pub struct PageText {
    pub glyphs: Vec<TextGlyph>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextSelection {
    pub text: String,
    pub rects: Vec<TextRect>,
}

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub page: usize,
    pub rects: Vec<TextRect>,
}

impl PageText {
    pub fn nearest_index(&self, point: (f32, f32)) -> Option<usize> {
        if !point.0.is_finite() || !point.1.is_finite() {
            return None;
        }
        self.glyphs
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.bounds
                    .distance(point)
                    .total_cmp(&b.bounds.distance(point))
            })
            .map(|(i, _)| i)
    }

    pub fn select_indices(&self, start: usize, end: usize) -> TextSelection {
        let mut result = TextSelection::default();
        let mut line = None;
        for glyph in
            &self.glyphs[start.min(self.glyphs.len())..end.max(start).min(self.glyphs.len())]
        {
            if line != Some(glyph.line) {
                if line.is_some() {
                    result.text.push('\n');
                }
                result.rects.push(glyph.bounds);
                line = Some(glyph.line);
            } else if let Some(bounds) = result.rects.last_mut() {
                *bounds = bounds.union(glyph.bounds);
            }
            result.text.push(glyph.ch);
        }
        result
    }
    pub fn select(&self, start: (f32, f32), end: (f32, f32)) -> TextSelection {
        if ![start.0, start.1, end.0, end.1]
            .iter()
            .all(|v| v.is_finite())
        {
            return TextSelection::default();
        }
        let (Some(a), Some(b)) = (self.nearest_index(start), self.nearest_index(end)) else {
            return TextSelection::default();
        };
        self.select_indices(a.min(b), a.max(b) + 1)
    }
}
