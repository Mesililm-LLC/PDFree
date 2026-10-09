use std::sync::Arc;
use viewer_core::*;

fn key(x: u32) -> TileKey {
    TileKey {
        document: 1,
        page: 0,
        scale_milli: 1000,
        rotation: 0,
        x,
        y: 0,
        width: 10,
        height: 10,
    }
}

#[test]
fn mixed_pages_have_one_shared_coordinate_scale() {
    let layout = Layout::new(
        &[PageSize::new(72.0, 144.0), PageSize::new(144.0, 72.0)],
        1.0,
        400.0,
    );
    assert_eq!(
        layout.pages[0],
        PageRect {
            x: 152.0,
            y: 24.0,
            width: 96.0,
            height: 192.0
        }
    );
    assert_eq!(layout.pages[1].y, 232.0);
    assert_eq!(layout.height, 352.0);
    assert_eq!(layout.page_at(240.0), 1);
}

#[test]
fn hidpi_tiles_clip_to_page_edges_and_keep_logical_coordinates() {
    let layout = Layout::new(&[PageSize::new(300.0, 300.0)], 1.0, 600.0);
    let view = Viewport {
        width: 600.0,
        height: 600.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom: 1.0,
        dpi: 2.0,
    };
    let requests = layout.requests(7, 9, view, 0);
    assert_eq!(requests.len(), 4);
    assert!(requests.iter().all(|r| r.key.scale_milli == 2667));
    let edge = requests
        .iter()
        .find(|r| r.key.x == 512 && r.key.y == 512)
        .unwrap();
    assert_eq!((edge.key.width, edge.key.height), (289, 289));
    assert_eq!(edge.generation, 9);
    assert_eq!(edge.key.document, 7);
}

#[test]
fn eviction_counts_shared_buffers_and_refreshes_recency() {
    let mut cache = TileCache::new(800);
    let tile = || {
        Arc::new(PixelTile {
            width: 10,
            height: 10,
            rgba: vec![255; 400],
        })
    };
    cache.insert(key(0), tile());
    cache.insert(key(10), tile());
    assert!(cache.get(&key(0)).is_some());
    cache.insert(key(20), tile());
    assert!(cache.get(&key(10)).is_none());
    assert!(cache.get(&key(0)).is_some());
    assert_eq!(cache.bytes(), 800);
    cache.insert(
        key(30),
        Arc::new(PixelTile {
            width: 30,
            height: 30,
            rgba: vec![0; 3600],
        }),
    );
    assert_eq!(cache.bytes(), 800);
}

#[test]
fn queue_prioritizes_visible_deduplicates_and_discards_old_generations() {
    let mut queue = RenderQueue::new(2);
    let req = |x, priority, generation| RenderRequest {
        key: key(x),
        priority,
        generation,
    };
    queue.replace(vec![
        req(0, 10, 1),
        req(10, 0, 1),
        req(10, 0, 1),
        req(20, 1, 1),
    ]);
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.pop().unwrap().key.x, 10);
    queue.replace(vec![req(30, 0, 2)]);
    assert_eq!(queue.pop().unwrap().generation, 2);
    assert!(queue.pop().is_none());
}

#[test]
fn stale_results_cannot_cross_document_or_zoom_changes() {
    assert!(accept_result(4, 8, 4, 8));
    assert!(!accept_result(4, 8, 3, 8));
    assert!(!accept_result(4, 8, 4, 7));
}

#[test]
fn invalid_geometry_and_zoom_do_not_create_unbounded_allocations() {
    let layout = Layout::new(&[PageSize::new(f32::NAN, -1.0)], f32::INFINITY, 0.0);
    assert!(layout.height.is_finite());
    assert!(layout.pages[0].width > 0.0);
    assert_eq!(clamp_zoom(20.0), 4.0);
    assert_eq!(clamp_zoom(0.01), 0.25);
}

#[test]
fn refining_earlier_page_sizes_preserves_the_reading_anchor() {
    let old = Layout::new(
        &[PageSize::new(72.0, 72.0), PageSize::new(72.0, 144.0)],
        1.0,
        400.0,
    );
    let anchor = old.anchor(184.0); // Second page starts at 136; 48 logical pixels into it.
    let new = Layout::new(
        &[PageSize::new(72.0, 144.0), PageSize::new(72.0, 144.0)],
        1.0,
        400.0,
    );
    assert_eq!(new.resolve_anchor(anchor), 280.0);
}

#[test]
fn prefetch_cannot_consume_the_request_budget_before_visible_tiles() {
    let layout = Layout::new(&[PageSize::new(720.0, 3000.0); 5], 1.0, 4000.0);
    let view = Viewport {
        width: 4000.0,
        height: 4000.0,
        scroll_x: 0.0,
        scroll_y: layout.pages[2].y,
        zoom: 1.0,
        dpi: 8.0,
    };
    let requests = layout.requests(1, 1, view, 2);
    assert_eq!(requests[0].key.page, 2);
    assert_eq!(requests[0].priority, 0);
}
