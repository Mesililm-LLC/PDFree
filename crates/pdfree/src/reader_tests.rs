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
    ui.hide().unwrap();
}
