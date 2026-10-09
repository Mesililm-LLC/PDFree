use super::*;
use slint::Model;
use slint::platform::{
    Clipboard, Key, Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent,
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
};
use std::{cell::RefCell, rc::Rc};

struct TestPlatform {
    window: Rc<MinimalSoftwareWindow>,
    clipboard: Rc<RefCell<String>>,
}
impl Platform for TestPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
    fn set_clipboard_text(&self, text: &str, clipboard: Clipboard) {
        if clipboard == Clipboard::DefaultClipboard {
            *self.clipboard.borrow_mut() = text.into();
        }
    }
    fn clipboard_text(&self, _: Clipboard) -> Option<String> {
        Some(self.clipboard.borrow().clone())
    }
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus")
        .join(name)
}
fn wait(state: &mut State, ui: &AppWindow, condition: impl Fn(&State, &AppWindow) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        slint::platform::update_timers_and_animations();
        state.tick(ui);
        if condition(state, ui) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Reader flow timed out: {} / {}",
            ui.get_status(),
            ui.get_search_status()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn reader_flows_use_real_native_text_pointer_events_and_a_mock_clipboard() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    let clipboard = Rc::new(RefCell::new("untouched".into()));
    slint::platform::set_platform(Box::new(TestPlatform {
        window: window.clone(),
        clipboard: clipboard.clone(),
    }))
    .unwrap();
    let ui = AppWindow::new().unwrap();
    ui.window().set_size(slint::LogicalSize::new(1180.0, 850.0));
    ui.show().unwrap();
    let state = Rc::new(RefCell::new(State::new(
        Trace::new(Instant::now(), None).unwrap(),
        false,
    )));
    let callback_state = state.clone();
    let weak = ui.as_weak();
    ui.on_select_text(move |page, x, y, phase| {
        callback_state.borrow_mut().select_text(
            &weak.upgrade().unwrap(),
            page as usize,
            x,
            y,
            phase,
        );
    });
    let thumbnail_state = state.clone();
    let thumbnail_ui = ui.as_weak();
    ui.on_thumbnail_click(move |page| {
        thumbnail_state
            .borrow_mut()
            .navigate(&thumbnail_ui.upgrade().unwrap(), page as usize);
    });
    let search_state = state.clone();
    let search_ui = ui.as_weak();
    ui.on_search(move |query| {
        search_state
            .borrow_mut()
            .search(&search_ui.upgrade().unwrap(), query.to_string());
    });
    state.borrow_mut().open(&ui, fixture("encrypted.pdf"));
    wait(&mut state.borrow_mut(), &ui, |_, ui| {
        ui.get_password_required()
    });
    state.borrow_mut().action(&ui, "password-cancel");
    assert!(!ui.get_password_required() && !ui.get_loading() && !ui.get_ready());
    state.borrow_mut().open(&ui, fixture("encrypted.pdf"));
    wait(&mut state.borrow_mut(), &ui, |_, ui| {
        ui.get_password_required()
    });
    ui.set_password("wrong".into());
    crate::capture::snapshot(&ui, "reader-password").unwrap();
    state.borrow_mut().password(&ui, "wrong".into());
    assert!(ui.get_password().is_empty());
    wait(&mut state.borrow_mut(), &ui, |_, ui| {
        ui.get_password_required()
    });
    assert!(!ui.get_ready());
    state.borrow_mut().password(&ui, "pdfree-test".into());
    wait(&mut state.borrow_mut(), &ui, |state, ui| {
        ui.get_ready() && state.tools.test_text(0).is_some()
    });
    assert!(ui.get_can_copy());

    state.borrow_mut().open(&ui, fixture("office.pdf"));
    wait(&mut state.borrow_mut(), &ui, |state, ui| {
        ui.get_ready() && state.tools.test_text(0).is_some()
    });
    ui.set_search_visible(true);
    ui.invoke_focus_search();
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: "PDFREE FIXTURE".into(),
    });
    ui.window().dispatch_event(WindowEvent::KeyReleased {
        text: "PDFREE FIXTURE".into(),
    });
    assert_eq!(ui.get_search_query(), "PDFREE FIXTURE");
    wait(&mut state.borrow_mut(), &ui, |_, ui| {
        ui.get_search_status().contains("20/20")
    });
    assert!(ui.get_search_status().starts_with("20 matches"));
    crate::capture::snapshot(&ui, "reader-search").unwrap();
    state.borrow_mut().action(&ui, "search-next");
    ui.set_search_visible(false);
    ui.set_scroll_y(0.0);
    state.borrow_mut().action(&ui, "zoom-in");
    wait(&mut state.borrow_mut(), &ui, |state, _| {
        state.tools.test_text(0).is_some()
    });
    ui.window()
        .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.5 });
    ui.window().set_size(slint::LogicalSize::new(1180.0, 850.0));
    state.borrow_mut().tick(&ui);
    let text = state.borrow().tools.test_text(0).unwrap();
    let page = state.borrow().layout.pages[0];
    let scale = state.borrow().zoom * 96.0 / 72.0;
    let glyph0 = text.glyphs[0].bounds;
    let glyph5 = text.glyphs[5].bounds;
    let offset_y = 850.0 - ui.get_view_height() - 32.0;
    let position = |rect: viewer_core::TextRect| {
        slint::LogicalPosition::new(
            page.x + (rect.left + rect.right) / 2.0 * scale + ui.get_scroll_x(),
            offset_y + page.y + (rect.top + rect.bottom) / 2.0 * scale + ui.get_scroll_y(),
        )
    };
    let mut buffer = vec![slint::Rgb8Pixel::default(); 1770 * 1275];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, 1770);
    });
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: position(glyph0),
        button: PointerEventButton::Left,
    });
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: position(glyph5),
    });
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: position(glyph5),
        button: PointerEventButton::Left,
    });
    assert_eq!(ui.get_selection_text(), "PDFree");
    assert_eq!(*clipboard.borrow(), "untouched");
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    ui.window()
        .dispatch_event(WindowEvent::KeyPressed { text: "c".into() });
    ui.window()
        .dispatch_event(WindowEvent::KeyReleased { text: "c".into() });
    ui.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    assert_eq!(*clipboard.borrow(), "PDFree");
    crate::capture::snapshot(&ui, "reader-selection").unwrap();

    ui.set_sidebar_visible(true);
    wait(&mut state.borrow_mut(), &ui, |_, ui| {
        let thumbnails = ui.get_thumbnails();
        thumbnails.row_count() > 0 && thumbnails.row_data(0).unwrap().pixels.size().width > 0
    });
    assert!(ui.get_thumbnails().row_count() <= 32);
    crate::capture::snapshot(&ui, "reader-thumbnails").unwrap();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, 1770);
    });
    let thumbnail_position = slint::LogicalPosition::new(70.0, offset_y + 196.0 + 60.0);
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: thumbnail_position,
        button: PointerEventButton::Left,
    });
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: thumbnail_position,
        button: PointerEventButton::Left,
    });
    assert_eq!(ui.get_page_entry(), "2");
    state.borrow_mut().open(&ui, fixture("restricted-copy.pdf"));
    wait(&mut state.borrow_mut(), &ui, |_, ui| {
        ui.get_password_required()
    });
    state.borrow_mut().password(&ui, "pdfree-test".into());
    wait(&mut state.borrow_mut(), &ui, |_, ui| ui.get_ready());
    assert!(!ui.get_can_copy());
    assert!(ui.get_selection_text().is_empty());
    ui.set_selection_text("must not copy".into());
    ui.invoke_copy_selection();
    assert_eq!(*clipboard.borrow(), "PDFree");
    state.borrow_mut().open(&ui, fixture("malformed.pdf"));
    wait(&mut state.borrow_mut(), &ui, |_, ui| !ui.get_loading());
    assert!(!ui.get_ready());
    state.borrow_mut().open(&ui, fixture("text.pdf"));
    wait(&mut state.borrow_mut(), &ui, |_, ui| ui.get_ready());
    editing_flows(&state, &ui, &window);
    ui.hide().unwrap();
}

fn editing_flows(state: &Rc<RefCell<State>>, ui: &AppWindow, window: &Rc<MinimalSoftwareWindow>) {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/runs/annotations-tests");
    let directory = root.join(format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let source = directory.join("source.pdf");
    std::fs::copy(fixture("text.pdf"), &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    state.borrow_mut().open(ui, source.clone());
    wait(&mut state.borrow_mut(), ui, |state, ui| {
        ui.get_ready() && state.tools.test_text(0).is_some()
    });
    let callback_state = state.clone();
    let weak = ui.as_weak();
    ui.on_action(move |action| {
        callback_state
            .borrow_mut()
            .action(&weak.upgrade().unwrap(), action.as_str())
    });
    let callback_state = state.clone();
    let weak = ui.as_weak();
    ui.on_rename_submit(move |name| {
        callback_state
            .borrow_mut()
            .rename(&weak.upgrade().unwrap(), name.to_string())
    });
    let callback_state = state.clone();
    let weak = ui.as_weak();
    ui.on_note_submit(move |text| {
        callback_state
            .borrow_mut()
            .note_submit(&weak.upgrade().unwrap(), text.to_string())
    });
    let callback_state = state.clone();
    let weak = ui.as_weak();
    ui.on_page_point(move |page, x, y| {
        callback_state
            .borrow_mut()
            .page_point(&weak.upgrade().unwrap(), page as usize, x, y)
    });
    let text = state.borrow().tools.test_text(0).unwrap();
    let a = text.glyphs[0].bounds;
    let b = text.glyphs[5].bounds;
    state.borrow_mut().select_text(
        ui,
        0,
        a.left * 96.0 / 72.0,
        (a.top + a.bottom) / 2.0 * 96.0 / 72.0,
        0,
    );
    state.borrow_mut().select_text(
        ui,
        0,
        b.right * 96.0 / 72.0,
        (b.top + b.bottom) / 2.0 * 96.0 / 72.0,
        2,
    );
    // Exercise text that crosses a page edge: only visible rectangles become drafts.
    let original_width = state.borrow().sizes[0].width;
    let clipped_width = (a.left + b.right) / 2.0;
    state.borrow_mut().sizes[0].width = clipped_width;
    state.borrow_mut().action(ui, "annotate-highlight");
    if let viewer_core::Annotation::Highlight { rects, .. } =
        &state.borrow().editor.history.annotations()[0]
    {
        assert!(
            rects
                .iter()
                .all(|r| r.right <= clipped_width && r.left >= 0.0)
        );
    } else {
        panic!("expected clipped highlight");
    }
    state.borrow_mut().sizes[0].width = original_width;
    assert!(ui.get_dirty_document());
    assert_eq!(state.borrow().editor.history.annotations().len(), 1);
    state.borrow_mut().action(ui, "undo");
    assert!(!ui.get_dirty_document());
    state.borrow_mut().action(ui, "redo");
    assert!(ui.get_dirty_document());

    let mut buffer = vec![slint::Rgb8Pixel::default(); 1770 * 1275];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, 1770);
    });
    let click = |position| {
        ui.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        ui.window().dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    };
    click(slint::LogicalPosition::new(200.0, 32.0));
    click(slint::LogicalPosition::new(200.0, 32.0));
    assert!(
        ui.get_rename_active(),
        "document-name double click should enter inline rename"
    );
    crate::capture::snapshot(ui, "annotation-inline-rename").unwrap();
    ui.set_rename_text("cancelled".into());
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Escape.into(),
    });
    ui.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Escape.into(),
    });
    assert!(!ui.get_rename_active());
    assert!(source.exists());
    state.borrow_mut().action(ui, "rename-start");
    ui.set_rename_text("Renamed document".into());
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Return.into(),
    });
    ui.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Return.into(),
    });
    wait(&mut state.borrow_mut(), ui, |_, ui| !ui.get_file_busy());
    let renamed = directory.join("Renamed document.pdf");
    assert!(renamed.exists() && !source.exists());
    assert_eq!(ui.get_document_name(), "Renamed document.pdf");
    assert!(ui.get_dirty_document());
    assert_eq!(state.borrow().editor.history.annotations().len(), 1);
    crate::capture::snapshot(ui, "annotation-renamed").unwrap();

    state.borrow_mut().action(ui, "annotate-note");
    state.borrow_mut().page_point(ui, 0, 150.0, 220.0);
    assert!(ui.get_note_visible());
    ui.invoke_note_submit("   ".into());
    assert!(ui.get_note_visible());
    assert!(ui.get_file_message().contains("16,384"));
    ui.invoke_note_submit("A note — café 日本語\nSecond line".into());
    assert!(!ui.get_note_visible());
    assert_eq!(state.borrow().editor.history.annotations().len(), 2);
    state.borrow_mut().annotation_click(ui, 1);
    assert!(ui.get_note_visible() && !ui.get_note_read_only());
    ui.invoke_note_submit("Edited note — café 日本語\nSecond line".into());
    state.borrow_mut().action(ui, "undo");
    assert!(
        matches!(&state.borrow().editor.history.annotations()[1], viewer_core::Annotation::Note{text,..} if text.starts_with("A note"))
    );
    state.borrow_mut().action(ui, "redo");
    assert!(
        matches!(&state.borrow().editor.history.annotations()[1], viewer_core::Annotation::Note{text,..} if text.starts_with("Edited note"))
    );
    let target = directory.join("annotated.pdf");
    state.borrow_mut().save_as(ui, target.clone());
    wait(&mut state.borrow_mut(), ui, |_, ui| !ui.get_file_busy());
    assert!(target.exists());
    assert!(!ui.get_dirty_document());
    assert_eq!(std::fs::read(&renamed).unwrap(), original);
    state.borrow_mut().action(ui, "undo");
    assert!(ui.get_dirty_document());
    state.borrow_mut().request_open(ui, fixture("office.pdf"));
    assert!(ui.get_pending_open());
    state.borrow_mut().action(ui, "pending-cancel");
    assert!(!ui.get_pending_open());
    assert!(ui.get_dirty_document());
    assert!(!state.borrow_mut().request_close(ui));
    assert!(ui.get_pending_open() && ui.get_pending_close());
    let revision = state.borrow().editor.history.revision();
    state.borrow_mut().action(ui, "undo");
    assert_eq!(state.borrow().editor.history.revision(), revision);
    state.borrow_mut().action(ui, "pending-save");
    let (document, _) = state.borrow_mut().take_save_dialog().unwrap();
    state
        .borrow_mut()
        .save_dialog_result(ui, document, Some(renamed.clone()));
    wait(&mut state.borrow_mut(), ui, |_, ui| !ui.get_file_busy());
    assert!(ui.get_pending_open() && ui.get_dirty_document());
    assert!(!ui.get_file_message().is_empty());
    assert_eq!(std::fs::read(&renamed).unwrap(), original);
    state.borrow_mut().action(ui, "pending-cancel");
    assert!(!state.borrow_mut().take_close_request());
    state.borrow_mut().action(ui, "redo");
    assert!(!ui.get_dirty_document());
    state.borrow_mut().request_open(ui, target);
    wait(&mut state.borrow_mut(), ui, |state, ui| {
        ui.get_ready() && state.editor.native_notes_count() > 0 && ui.get_tiles().row_count() > 0
    });
    state.borrow_mut().annotation_click(ui, -1);
    assert!(ui.get_note_visible() && ui.get_note_read_only());
    assert!(ui.get_note_text().contains("日本語"));
    crate::capture::snapshot(ui, "annotation-saved-note").unwrap();
    state.borrow_mut().action(ui, "cancel-note");
}
