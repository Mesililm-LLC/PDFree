use viewer_core::{PageText, TextGlyph, TextRect};

fn page() -> PageText {
    PageText {
        glyphs: "Aé\u{1f600}B"
            .chars()
            .enumerate()
            .map(|(i, ch)| TextGlyph {
                ch,
                line: i / 2,
                bounds: TextRect {
                    left: (i % 2) as f32 * 10.0,
                    top: (i / 2) as f32 * 20.0,
                    right: (i % 2) as f32 * 10.0 + 10.0,
                    bottom: (i / 2) as f32 * 20.0 + 10.0,
                },
            })
            .collect(),
        truncated: false,
    }
}

#[test]
fn unicode_selection_is_reading_order_and_keeps_line_breaks_in_both_directions() {
    let text = page();
    let forward = text.select((1.0, 5.0), (19.0, 25.0));
    let reverse = text.select((19.0, 25.0), (1.0, 5.0));
    assert_eq!(forward.text, "Aé\n😀B");
    assert_eq!(forward, reverse);
    assert_eq!(forward.rects.len(), 2);
    assert_eq!(forward.rects[0].right, 20.0);
}

#[test]
fn empty_or_nonfinite_selection_never_returns_unrelated_text() {
    assert!(
        PageText::default()
            .select((0.0, 0.0), (1.0, 1.0))
            .text
            .is_empty()
    );
    assert!(page().select((f32::NAN, 0.0), (1.0, 1.0)).text.is_empty());
}

#[test]
fn pointer_outside_glyph_bounds_uses_the_nearest_reading_endpoint() {
    assert_eq!(page().select((-5.0, 5.0), (50.0, 5.0)).text, "Aé");
    assert_eq!(page().select((11.0, 25.0), (19.0, 25.0)).text, "B");
}
