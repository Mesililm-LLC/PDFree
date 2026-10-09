#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod capture;
mod features;
mod trace;

use slint::ComponentHandle;
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};
slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let started = std::time::Instant::now();
    let mut args = std::env::args_os().skip(1);
    let mut path = None;
    let mut trace_path = None;
    let mut benchmark = false;
    while let Some(arg) = args.next() {
        if arg == "--trace" {
            trace_path = args.next().map(PathBuf::from);
        } else if arg == "--benchmark-ui" {
            benchmark = true;
        } else if arg == "--help" {
            println!("PDFree [document.pdf] [--trace events.jsonl] [--benchmark-ui]");
            return Ok(());
        } else {
            path = Some(PathBuf::from(arg));
        }
    }
    let trace = trace::Trace::new(started, trace_path)?;
    trace.event(
        "process_start",
        serde_json::json!({"version":env!("CARGO_PKG_VERSION")}),
    );
    // Winit falls back to software if FemtoVG initialization is unavailable.
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .select()?;
    let ui = AppWindow::new()?;
    trace.event("window_created", serde_json::json!({}));
    let notifier_trace = trace.clone();
    let notifier = ui.window().set_rendering_notifier(move |state, _| {
        if matches!(state, slint::RenderingState::AfterRendering) {
            notifier_trace.event(
                "frame_rendered",
                serde_json::json!({"presentation_verified":false}),
            );
        }
    });
    trace.event(
        "render_notifier",
        serde_json::json!({"available":notifier.is_ok()}),
    );
    let state = Rc::new(RefCell::new(app::State::new(trace.clone(), benchmark)));
    let (dialog_send, dialog_receive) = std::sync::mpsc::sync_channel(1);
    let action_state = state.clone();
    let action_ui = ui.as_weak();
    ui.on_action(move |action| {
        let Some(ui) = action_ui.upgrade() else {
            return;
        };
        if action == "open" {
            if action_state.borrow().dialog_open {
                return;
            }
            action_state.borrow_mut().dialog_open = true;
            let send = dialog_send.clone();
            std::thread::spawn(move || {
                let path = rfd::FileDialog::new()
                    .set_title("Open PDF")
                    .add_filter("PDF documents", &["pdf"])
                    .pick_file();
                let _ = send.send(path);
            });
        } else {
            action_state.borrow_mut().action(&ui, action.as_str());
        }
    });
    let navigate_state = state.clone();
    let navigate_ui = ui.as_weak();
    ui.on_navigate(move |page| {
        if let Some(ui) = navigate_ui.upgrade()
            && let Ok(page) = page.trim().parse::<usize>()
        {
            navigate_state
                .borrow_mut()
                .navigate(&ui, page.saturating_sub(1));
        }
    });
    let password_state = state.clone();
    let password_ui = ui.as_weak();
    ui.on_password_submit(move |password| {
        if let Some(ui) = password_ui.upgrade() {
            password_state
                .borrow_mut()
                .password(&ui, password.to_string());
        }
    });
    let search_state = state.clone();
    let search_ui = ui.as_weak();
    ui.on_search(move |query| {
        if let Some(ui) = search_ui.upgrade() {
            search_state.borrow_mut().search(&ui, query.to_string());
        }
    });
    let select_state = state.clone();
    let select_ui = ui.as_weak();
    ui.on_select_text(move |page, x, y, phase| {
        if let Some(ui) = select_ui.upgrade()
            && page >= 0
        {
            select_state
                .borrow_mut()
                .select_text(&ui, page as usize, x, y, phase);
        }
    });
    let thumbnail_state = state.clone();
    let thumbnail_ui = ui.as_weak();
    ui.on_thumbnail_click(move |page| {
        if let Some(ui) = thumbnail_ui.upgrade()
            && page >= 0
        {
            thumbnail_state.borrow_mut().navigate(&ui, page as usize);
        }
    });
    let timer = slint::Timer::default();
    let timer_ui = ui.as_weak();
    let timer_state = state.clone();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            let Some(ui) = timer_ui.upgrade() else {
                return;
            };
            let mut state = timer_state.borrow_mut();
            if let Ok(path) = dialog_receive.try_recv() {
                state.dialog_open = false;
                if let Some(path) = path {
                    state.open(&ui, path);
                }
            }
            state.tick(&ui);
        },
    );
    ui.show()?;
    trace.event("window_show_requested", serde_json::json!({}));
    if let Some(path) = path {
        state.borrow_mut().open(&ui, path);
    }
    slint::run_event_loop()?;
    timer.stop();
    trace.event("event_loop_finished", serde_json::json!({}));
    Ok(())
}
