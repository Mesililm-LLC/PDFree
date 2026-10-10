use viewer_core::{PageText, TextGlyph, TextRect};

#[test]
fn indexed_selection_can_take_page_prefix_suffix_and_empty_ranges() {
    let page = PageText {
        glyphs: "abc"
            .chars()
            .enumerate()
            .map(|(i, ch)| TextGlyph {
                ch,
                line: 0,
                bounds: TextRect {
                    left: i as f32,
                    top: 0.,
                    right: i as f32 + 1.,
                    bottom: 1.,
                },
            })
            .collect(),
        truncated: false,
    };
    assert_eq!(page.select_indices(1, 3).text, "bc");
    assert_eq!(page.select_indices(0, 2).text, "ab");
    assert_eq!(page.select_indices(3, 3).text, "");
    assert_eq!(page.nearest_index((2.5, 0.5)), Some(2));
}
