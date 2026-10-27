use pdf_engine::{Engine, EngineError, Event};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus")
        .join(name)
}
fn wait(engine: &Engine, predicate: impl Fn(&Event) -> bool) -> Event {
    let started = Instant::now();
    loop {
        assert!(started.elapsed() < Duration::from_secs(15));
        let event = engine.events.recv_timeout(Duration::from_secs(15)).unwrap();
        if predicate(&event) {
            return event;
        }
    }
}
fn open(engine: &Engine, id: u64, name: &str) {
    engine.open(id, id, fixture(name));
    wait(
        engine,
        |e| matches!(e,Event::Opened{document,..} if *document==id),
    );
}

#[test]
fn passwords_retry_and_copy_permissions_are_real_native_state() {
    let engine = Engine::spawn(2);
    engine.open_with_password(1, 1, fixture("encrypted.pdf"), Some("wrong".into()));
    assert!(matches!(
        wait(&engine, |e| matches!(e, Event::Error { .. })),
        Event::Error {
            error: EngineError::PasswordRejected,
            ..
        }
    ));
    engine.open_with_password(2, 2, fixture("encrypted.pdf"), Some("pdfree-test".into()));
    assert!(
        matches!(wait(&engine,|e|matches!(e,Event::Opened{..})),Event::Opened{info,..} if info.can_copy)
    );
    engine.open_with_password(
        3,
        3,
        fixture("restricted-copy.pdf"),
        Some("pdfree-test".into()),
    );
    assert!(
        matches!(wait(&engine,|e|matches!(e,Event::Opened{document:3,..})),Event::Opened{info,..} if !info.can_copy)
    );
    engine.text_pages(3, vec![0]);
    assert!(
        matches!(wait(&engine, |e| matches!(e, Event::Text { document: 3, .. })), Event::Text { text, .. } if text.glyphs.is_empty())
    );
}

#[test]
fn text_and_case_insensitive_search_return_owned_page_local_geometry() {
    let engine = Engine::spawn(2);
    open(&engine, 1, "office.pdf");
    engine.text_pages(1, vec![0]);
    let Event::Text { text, .. } = wait(&engine, |e| {
        matches!(
            e,
            Event::Text {
                document: 1,
                page: 0,
                ..
            }
        )
    }) else {
        unreachable!()
    };
    assert!(
        text.glyphs
            .iter()
            .map(|g| g.ch)
            .collect::<String>()
            .contains("PDFree fixture")
    );
    assert!(
        text.glyphs
            .iter()
            .all(|g| g.bounds.left >= 0.0 && g.bounds.top >= 0.0)
    );
    // A coalesced leave/reenter presents the same wanted set to the coordinator.
    // The UI has released its snapshot, so the engine must publish it again.
    engine.text_pages(1, vec![0]);
    let Event::Text { text: again, .. } = wait(&engine, |e| {
        matches!(
            e,
            Event::Text {
                document: 1,
                page: 0,
                ..
            }
        )
    }) else {
        unreachable!()
    };
    assert!(std::sync::Arc::ptr_eq(&text, &again));
    engine.search(1, 10, "PDFREE FIXTURE");
    let mut count = 0;
    loop {
        let Event::SearchPage { hits, done, .. } = wait(&engine, |e| {
            matches!(e, Event::SearchPage { search: 10, .. })
        }) else {
            unreachable!()
        };
        count += hits.len();
        if done {
            break;
        }
    }
    assert_eq!(count, 20);
    engine.search(1, 11, "PDFree");
    engine.search(1, 12, "");
    assert!(
        matches!(wait(&engine,|e|matches!(e,Event::SearchPage{search:12,..})),Event::SearchPage{done:true,hits,..} if hits.is_empty())
    );
}

#[test]
fn thumbnails_are_small_complete_pages_and_survive_viewport_generation_changes() {
    let engine = Engine::spawn(2);
    open(&engine, 1, "office.pdf");
    engine.thumbnails(1, vec![0, 1, 2]);
    engine.view(1, 99, vec![], vec![]);
    let mut thumbnails = std::collections::HashMap::new();
    for _ in 0..3 {
        let Event::Thumbnail { key, pixels, .. } = wait(
            &engine,
            |e| matches!(e,Event::Thumbnail{key,..} if key.document==1),
        ) else {
            unreachable!()
        };
        assert!(key.width <= 140 && key.height <= 160);
        assert_eq!(pixels.rgba.len(), (key.width * key.height * 4) as usize);
        assert!(pixels.rgba.as_chunks::<4>().0.iter().any(|p| p[0] < 240));
        thumbnails.insert(key.page, pixels);
    }
    engine.thumbnails(1, vec![0, 1, 2]);
    for _ in 0..3 {
        let Event::Thumbnail { key, pixels, .. } = wait(
            &engine,
            |e| matches!(e, Event::Thumbnail { key, .. } if key.document == 1),
        ) else {
            unreachable!()
        };
        assert!(std::sync::Arc::ptr_eq(&thumbnails[&key.page], &pixels));
    }
    engine.open(2, 2, fixture("text.pdf"));
    wait(&engine, |e| matches!(e, Event::Opened { document: 2, .. }));
    engine.thumbnails(2, vec![0]);
    wait(
        &engine,
        |e| matches!(e,Event::Thumbnail{key,..} if key.document==2),
    );
}

#[test]
fn a_blank_page_has_no_fabricated_selectable_or_searchable_text() {
    let engine = Engine::spawn(1);
    open(&engine, 1, "blank.pdf");
    engine.text_pages(1, vec![0]);
    assert!(
        matches!(wait(&engine,|e|matches!(e,Event::Text{..})),Event::Text{text,..} if text.glyphs.is_empty())
    );
    engine.search(1, 1, "anything");
    assert!(
        matches!(wait(&engine,|e|matches!(e,Event::SearchPage{..})),Event::SearchPage{done:true,hits,..} if hits.is_empty())
    );
}

#[test]
fn crop_origin_text_and_search_use_the_same_local_coordinates() {
    let engine = Engine::spawn(1);
    open(&engine, 1, "crop-origin.pdf");
    engine.text_pages(1, vec![0]);
    let Event::Text { text, .. } = wait(&engine, |e| matches!(e, Event::Text { .. })) else {
        unreachable!()
    };
    // Original text starts at PDF x=50; the crop origin is x=40.
    assert!((text.glyphs[0].bounds.left - 10.0).abs() < 0.1);
    engine.search(1, 30, "PDFree");
    let Event::SearchPage { hits, .. } = wait(&engine, |e| matches!(e, Event::SearchPage { .. }))
    else {
        unreachable!()
    };
    assert!((hits[0].rects[0].left - text.glyphs[0].bounds.left).abs() < 0.1);
    assert!((hits[0].rects[0].top - text.glyphs[0].bounds.top).abs() < 0.1);
}
