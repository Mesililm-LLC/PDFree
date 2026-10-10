use viewer_core::{Layout, PageSize, ReadingMode, Viewport};

fn view() -> Viewport {
    Viewport {
        width: 600.0,
        height: 600.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom: 1.0,
        dpi: 2.0,
    }
}

#[test]
fn single_page_keeps_indexes_and_does_not_prefetch_hidden_pages() {
    let layout = Layout::with_mode(
        &[PageSize::new(300.0, 300.0); 5],
        1.0,
        600.0,
        ReadingMode::Single,
        3,
    );
    assert_eq!(layout.pages.len(), 5);
    assert_eq!(layout.page_at(0.0), 3);
    assert_eq!(layout.page_at(10000.0), 3);
    assert_eq!(layout.visible_pages(view()), 3..=3);
    assert_eq!(layout.anchor(224.0), (3, 0.5));
    assert_eq!(layout.resolve_anchor((3, 0.5)), 224.0);
    assert!(
        layout
            .pages
            .iter()
            .enumerate()
            .all(|(i, p)| i == 3 || (p.width == 0.0 && p.height == 0.0))
    );
    let requests = layout.requests(7, 9, view(), 2);
    assert_eq!(requests.len(), 4);
    assert!(
        requests
            .iter()
            .all(|r| r.key.page == 3 && r.key.scale_milli == 2667)
    );
    let edge = requests
        .iter()
        .find(|r| r.key.x == 512 && r.key.y == 512)
        .unwrap();
    assert_eq!((edge.key.width, edge.key.height), (289, 289));
}

#[test]
fn facing_mixed_sizes_share_a_centered_top_aligned_row() {
    let sizes = [
        PageSize::new(72.0, 144.0),
        PageSize::new(144.0, 72.0),
        PageSize::new(300.0, 400.0),
    ];
    let layout = Layout::with_mode(&sizes, 1.0, 600.0, ReadingMode::Facing, 1);
    assert_eq!(layout.pages[0].x, 148.0);
    assert_eq!(layout.pages[1].x, 260.0);
    assert_eq!(layout.pages[0].y, layout.pages[1].y);
    assert_eq!(layout.height, 240.0);
    assert_eq!(layout.visible_pages(view()), 0..=1);
    assert_eq!(layout.pages[2].width, 0.0);
    let requests = layout.requests(1, 2, view(), 2);
    assert!(requests.iter().any(|r| r.key.page == 0));
    assert!(requests.iter().any(|r| r.key.page == 1));
    assert!(requests.iter().all(|r| r.key.page < 2));
}

#[test]
fn facing_odd_end_and_out_of_bounds_current_page_center_the_last_page() {
    let sizes = [PageSize::new(72.0, 144.0); 3];
    let layout = Layout::with_mode(&sizes, 1.0, 600.0, ReadingMode::Facing, usize::MAX);
    assert_eq!(layout.pages[2].x, 252.0);
    assert_eq!(layout.visible_pages(view()), 2..=2);
    assert_eq!(layout.page_at(10000.0), 2);
    assert!(
        layout
            .requests(1, 2, view(), 2)
            .iter()
            .all(|r| r.key.page == 2)
    );
}

#[test]
fn row_fit_zoom_has_equal_outer_margins_and_clipped_horizontal_tiles() {
    let sizes = [PageSize::new(300.0, 300.0); 2];
    // Fit the 816-point logical spread into 1000 pixels, reserving 48 for margins.
    let zoom = (1000.0 - 48.0 - 16.0) / (600.0 * 96.0 / 72.0);
    let layout = Layout::with_mode(&sizes, zoom, 1000.0, ReadingMode::Facing, 0);
    assert!((layout.pages[0].x - 24.0).abs() < 0.001);
    assert!((layout.width - layout.pages[1].x - layout.pages[1].width - 24.0).abs() < 0.001);
    let mut viewport = view();
    viewport.zoom = zoom;
    viewport.width = 30.0;
    viewport.scroll_x = layout.pages[1].x;
    let requests = layout.requests(1, 1, viewport, 2);
    assert!(!requests.is_empty());
    assert!(requests.iter().all(|r| r.key.page == 1 && r.key.x == 0));
}

#[test]
fn continuous_constructor_and_empty_modes_remain_compatible() {
    let sizes = [PageSize::new(300.0, 300.0); 2];
    let old = Layout::new(&sizes, 1.0, 600.0);
    let new = Layout::with_mode(&sizes, 1.0, 600.0, ReadingMode::Continuous, 1);
    assert_eq!(old.pages, new.pages);
    assert_eq!(old.height, new.height);
    for mode in [
        ReadingMode::Continuous,
        ReadingMode::Single,
        ReadingMode::Facing,
    ] {
        let empty = Layout::with_mode(&[], 1.0, 600.0, mode, 99);
        assert!(empty.requests(1, 1, view(), 2).is_empty());
        assert_eq!(empty.anchor(0.0), (0, 0.0));
    }
}
