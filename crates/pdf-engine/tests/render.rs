use pdf_engine::{Engine, Event};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use viewer_core::{Layout, Viewport};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus")
        .join(name)
}

#[test]
fn real_pdf_renders_clipped_pixels_on_workers() {
    let engine = Engine::spawn(2);
    engine.open(1, 1, fixture("transparency.pdf"));
    let info = loop {
        match engine.events.recv_timeout(Duration::from_secs(20)).unwrap() {
            Event::Opened { info, .. } => break info,
            Event::Error { error, .. } => panic!("{error}"),
            _ => {}
        }
    };
    assert_eq!(info.pages, 1);
    let view = Viewport {
        width: 900.0,
        height: 900.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom: 1.0,
        dpi: 1.0,
    };
    let layout = Layout::new(&[info.first_page], 1.0, 900.0);
    let requests = layout.requests(1, 2, view, 0);
    engine.view(1, 2, requests, vec![0]);
    let started = Instant::now();
    let mut tiles = 0;
    let mut colored = false;
    while tiles < 4 {
        assert!(started.elapsed() < Duration::from_secs(20));
        match engine.events.recv_timeout(Duration::from_secs(20)).unwrap() {
            Event::Tile {
                key,
                generation,
                pixels,
                ..
            } => {
                assert_eq!((key.document, generation), (1, 2));
                assert!(pixels.width <= 512 && pixels.height <= 512);
                assert_eq!(
                    pixels.rgba.len(),
                    (pixels.width * pixels.height * 4) as usize
                );
                colored |= pixels.rgba.as_chunks::<4>().0.iter().any(|p| p[0] != p[1]);
                tiles += 1;
            }
            Event::Error { error, .. } => panic!("{error}"),
            _ => {}
        }
    }
    assert!(
        colored,
        "Transparency graphics must survive clipped rasterization"
    );
}

#[test]
fn malformed_documents_report_error_and_next_open_recovers() {
    let engine = Engine::spawn(1);
    engine.open(1, 1, fixture("malformed.pdf"));
    assert!(matches!(
        engine.events.recv_timeout(Duration::from_secs(20)).unwrap(),
        Event::Error { .. }
    ));
    engine.open(2, 2, fixture("text.pdf"));
    assert!(matches!(
        engine.events.recv_timeout(Duration::from_secs(20)).unwrap(),
        Event::Opened { document: 2, .. }
    ));
}

#[test]
fn thousand_page_open_is_lazy_and_repeated_open_releases_document() {
    let engine = Engine::spawn(2);
    for document in 1..=5 {
        engine.open(document, document, fixture("thousand.pdf"));
        loop {
            if let Event::Opened {
                document: id, info, ..
            } = engine.events.recv_timeout(Duration::from_secs(20)).unwrap()
                && id == document
            {
                assert_eq!(info.pages, 1000);
                break;
            }
        }
    }
}

#[test]
fn encrypted_pdf_reports_password_required_without_attempting_pages() {
    let engine = Engine::spawn(1);
    engine.open(1, 1, fixture("encrypted.pdf"));
    assert!(matches!(
        engine.events.recv_timeout(Duration::from_secs(20)).unwrap(),
        Event::Error {
            error: pdf_engine::EngineError::PasswordRequired,
            ..
        }
    ));
}

#[test]
fn independently_produced_embedded_font_pdf_opens() {
    let engine = Engine::spawn(1);
    engine.open(1, 1, fixture("w3c-dummy.pdf"));
    assert!(
        matches!(engine.events.recv_timeout(Duration::from_secs(20)).unwrap(), Event::Opened { info, .. } if info.pages == 1)
    );
}

#[test]
fn rapid_open_replacement_only_returns_pixels_for_the_latest_document() {
    let engine = Engine::spawn(4);
    for id in 1..=20 {
        engine.open(
            id,
            id,
            fixture(if id == 20 { "text.pdf" } else { "thousand.pdf" }),
        );
    }
    loop {
        match engine.events.recv_timeout(Duration::from_secs(20)).unwrap() {
            Event::Opened {
                document: 20, info, ..
            } => {
                assert_eq!(info.pages, 1);
                break;
            }
            Event::Error {
                document: 20,
                error,
            } => panic!("{error}"),
            _ => {}
        }
    }
    let view = Viewport {
        width: 800.0,
        height: 600.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom: 1.0,
        dpi: 1.0,
    };
    let layout = Layout::new(&[viewer_core::PageSize::new(612.0, 792.0)], 1.0, 800.0);
    engine.view(20, 21, layout.requests(20, 21, view, 0), vec![0]);
    loop {
        if let Event::Tile {
            key, generation, ..
        } = engine.events.recv_timeout(Duration::from_secs(20)).unwrap()
        {
            assert_eq!((key.document, generation), (20, 21));
            break;
        }
    }
}

#[test]
fn corpus_first_pages_render_without_background_ui_work() {
    let engine = Engine::spawn(4);
    let cases = [
        ("text.pdf", 1),
        ("office.pdf", 20),
        ("mixed.pdf", 100),
        ("scan.pdf", 200),
        ("vector.pdf", 1),
        ("transparency.pdf", 1),
        ("tracemonkey.pdf", 14),
    ];
    for (index, (name, pages)) in cases.iter().enumerate() {
        let document = index as u64 + 1;
        let generation = document * 2;
        engine.open(document, generation, fixture(name));
        let info = loop {
            match engine.events.recv_timeout(Duration::from_secs(20)).unwrap() {
                Event::Opened {
                    document: id, info, ..
                } if id == document => break info,
                Event::Error {
                    document: id,
                    error,
                } if id == document => panic!("{name}: {error}"),
                _ => {}
            }
        };
        assert_eq!(info.pages, *pages);
        let view = Viewport {
            width: 800.0,
            height: 600.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom: 1.0,
            dpi: 1.0,
        };
        let layout = Layout::new(&[info.first_page], 1.0, 800.0);
        let requests = layout.requests(document, generation + 1, view, 0);
        let tile_count = requests.len();
        engine.view(document, generation + 1, requests, vec![0]);
        let mut completed = 0;
        let mut has_ink = false;
        loop {
            match engine.events.recv_timeout(Duration::from_secs(20)).unwrap() {
                Event::Tile { key, pixels, .. } if key.document == document => {
                    assert!(!pixels.rgba.is_empty());
                    has_ink |= pixels
                        .rgba
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|pixel| pixel[0] < 240);
                    completed += 1;
                    if completed == tile_count {
                        break;
                    }
                }
                Event::Error {
                    document: id,
                    error,
                } if id == document => panic!("{name}: {error}"),
                _ => {}
            }
        }
        assert!(has_ink, "{name}: visible region must contain content");
    }
}
