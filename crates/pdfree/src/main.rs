#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod capture;
mod features;
mod trace;

use slint::ComponentHandle;
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};
slint::include_modules!();

enum DialogResult {
    Open(Option<PathBuf>),
    Save {
        document: u64,
        path: Option<PathBuf>,
    },
}

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
    let timer_dialog_send = dialog_send.clone();
    let action_state = state.clone();
    let action_ui = ui.as_weak();
    ui.on_action(move |action| {
        let Some(ui) = action_ui.upgrade() else {
            return;
        };
        if action == "open" {
            if action_state.borrow().dialog_open
                || ui.get_file_busy()
                || ui.get_rename_active()
                || ui.get_note_visible()
                || ui.get_pending_open()
            {
                return;
            }
            action_state.borrow_mut().dialog_open = true;
            let send = dialog_send.clone();
            std::thread::spawn(move || {
                let path = rfd::FileDialog::new()
                    .set_title("Open PDF")
                    .add_filter("PDF documents", &["pdf"])
                    .pick_file();
                let _ = send.send(DialogResult::Open(path));
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
    let rename_state = state.clone();
    let rename_ui = ui.as_weak();
    ui.on_rename_submit(move |name| {
        if let Some(ui) = rename_ui.upgrade() {
            rename_state.borrow_mut().rename(&ui, name.to_string());
        }
    });
    let note_state = state.clone();
    let note_ui = ui.as_weak();
    ui.on_note_submit(move |text| {
        if let Some(ui) = note_ui.upgrade() {
            note_state.borrow_mut().note_submit(&ui, text.to_string());
        }
    });
    let point_state = state.clone();
    let point_ui = ui.as_weak();
    ui.on_page_point(move |page, x, y| {
        if let Some(ui) = point_ui.upgrade()
            && page >= 0
        {
            point_state
                .borrow_mut()
                .page_point(&ui, page as usize, x, y);
        }
    });
    let annotation_state = state.clone();
    let annotation_ui = ui.as_weak();
    ui.on_annotation_click(move |index| {
        if let Some(ui) = annotation_ui.upgrade() {
            annotation_state.borrow_mut().annotation_click(&ui, index);
        }
    });
    let close_state = state.clone();
    let close_ui = ui.as_weak();
    ui.window().on_close_requested(move || {
        if let Some(ui) = close_ui.upgrade()
            && !close_state.borrow_mut().request_close(&ui)
        {
            slint::CloseRequestResponse::KeepWindowShown
        } else {
            slint::CloseRequestResponse::HideWindow
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
            if let Ok(result) = dialog_receive.try_recv() {
                state.dialog_open = false;
                match result {
                    DialogResult::Open(Some(path)) => state.request_open(&ui, path),
                    DialogResult::Open(None) => {}
                    DialogResult::Save { document, path } => {
                        state.save_dialog_result(&ui, document, path);
                    }
                }
            }
            state.tick(&ui);
            if !state.dialog_open
                && let Some((document, suggested_path)) = state.take_save_dialog()
            {
                state.dialog_open = true;
                let send = timer_dialog_send.clone();
                std::thread::spawn(move || {
                    let mut dialog = rfd::FileDialog::new()
                        .set_title("Save annotated PDF as")
                        .add_filter("PDF documents", &["pdf"]);
                    if let Some(name) = suggested_path.file_name() {
                        dialog = dialog.set_file_name(name.to_string_lossy());
                    }
                    if let Some(directory) = suggested_path.parent() {
                        dialog = dialog.set_directory(directory);
                    }
                    let path = dialog.save_file();
                    let _ = send.send(DialogResult::Save { document, path });
                });
            }
            if state.take_close_request() {
                let _ = ui.hide();
                let _ = slint::quit_event_loop();
            }
        },
    );
    ui.show()?;
    trace.event("window_show_requested", serde_json::json!({}));
    if let Some(path) = path {
        state.borrow_mut().request_open(&ui, path);
    }
    slint::run_event_loop()?;
    timer.stop();
    trace.event("event_loop_finished", serde_json::json!({}));
    Ok(())
}
