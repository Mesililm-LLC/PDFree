//! Engine-only replacement/cleanup evidence; not UI presentation timing.
use pdf_engine::{Engine, Event};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use viewer_core::{Layout, Viewport};

fn main() {
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut trials = Vec::new();
    for trial in 1..=20u64 {
        let engine = Engine::spawn(4);
        let old = trial * 2;
        engine.open(old, old, corpus.join("vector.pdf"));
        let info = loop {
            match engine.events.recv_timeout(Duration::from_secs(20)).unwrap() {
                Event::Opened { document, info, .. } if document == old => break info,
                Event::Error { error, .. } => panic!("{error}"),
                _ => {}
            }
        };
        let view = Viewport {
            width: 1400.0,
            height: 1000.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom: 4.0,
            dpi: 1.25,
        };
        let layout = Layout::new(&[info.first_page], view.zoom, view.width);
        engine.view(old, old, layout.requests(old, old, view, 0), vec![0]);
        loop {
            match engine.events.recv_timeout(Duration::from_secs(20)).unwrap() {
                Event::DisplayListReady { .. } => break,
                Event::Error { error, .. } => panic!("{error}"),
                _ => {}
            }
        }
        // Replacement follows display-list preparation; worker start is not observed.
        let started = Instant::now();
        let next = old + 1;
        engine.open(next, next, corpus.join("office.pdf"));
        loop {
            match engine.events.recv_timeout(Duration::from_secs(20)).unwrap() {
                Event::Opened { document, .. } if document == next => break,
                Event::Error { error, .. } => panic!("{error}"),
                _ => {}
            }
        }
        let replacement_ms = started.elapsed().as_secs_f64() * 1000.0;
        let cleanup = Instant::now();
        drop(engine);
        trials.push(serde_json::json!({"trial":trial,"replacement_ack_ms":replacement_ms,"shutdown_join_ms":cleanup.elapsed().as_secs_f64()*1000.0}));
    }
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "profile":if cfg!(debug_assertions) {"debug"} else {"release"},
        "workers":4,"native_store_threshold_mib":256,
        "meaning":"Replacement acknowledgement after vector display-list preparation; worker start is not observed. Active native calls finish. Shutdown joins workers. Not hard native cancellation or UI presentation timing.",
        "trials":trials
    })).unwrap());
}
