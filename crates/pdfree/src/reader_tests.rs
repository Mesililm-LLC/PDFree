use super::*;
use slint::Model;
use slint::platform::{
    Clipboard, Key, Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent,
    software_renderer::{MinimalSoftwareWindow, RepaintBufferType},
};
use std::{cell::RefCell, rc::Rc};

#[test]
fn forms_drawing_filling_and_type_conversion_use_native_history() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(TestPlatform {
        window: window.clone(),
        clipboard: Rc::new(RefCell::new(String::new())),
    }))
    .unwrap();
    let ui = AppWindow::new().unwrap();
    ui.window().set_size(slint::LogicalSize::new(1180., 850.));
    ui.show().unwrap();
    let state = Rc::new(RefCell::new(State::new(
        Trace::new(Instant::now(), None).unwrap(),
        false,
    )));
    let callback = state.clone();
    let weak = ui.as_weak();
    ui.on_action(move |a| {
        callback
            .borrow_mut()
            .action(&weak.upgrade().unwrap(), a.as_str())
    });
    let callback = state.clone();
    let weak = ui.as_weak();
    ui.on_reader_pointer(move |x, y, phase| {
        callback
            .borrow_mut()
            .reader_pointer(&weak.upgrade().unwrap(), x, y, phase)
    });
    let callback = state.clone();
    let weak = ui.as_weak();
    ui.on_form_select(move |i| {
        callback
            .borrow_mut()
            .form_select(&weak.upgrade().unwrap(), i)
    });
    let callback = state.clone();
    let weak = ui.as_weak();
    ui.on_form_drag(move |i, x, y, phase, resize| {
        callback
            .borrow_mut()
            .form_drag(&weak.upgrade().unwrap(), i, x, y, phase, resize)
    });
    let callback = state.clone();
    let weak = ui.as_weak();
    ui.on_signature_pointer(move |x, y, phase| {
        callback
            .borrow_mut()
            .signature_pointer(&weak.upgrade().unwrap(), x, y, phase)
    });
    state.borrow_mut().open(&ui, fixture("blank.pdf"));
    wait(&mut state.borrow_mut(), &ui, |_, u| u.get_ready());
    ui.invoke_action("form-prepare".into());
    state.borrow_mut().tick(&ui);
    // Draw through the real viewport's pointer callbacks, at 150% device scale.
    ui.window()
        .dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.5 });
    ui.window().set_size(slint::LogicalSize::new(1180., 850.));
    state.borrow_mut().tick(&ui);
    let p = state.borrow().layout.pages[0];
    let scale = state.borrow().zoom * 96. / 72.;
    let offset_x = 1180. - ui.get_view_width();
    let offset_y = 850. - ui.get_view_height() - 32.;
    let point = |x: f32, y: f32| {
        slint::LogicalPosition::new(
            offset_x + p.x + x * scale + ui.get_scroll_x(),
            offset_y + p.y + y * scale + ui.get_scroll_y(),
        )
    };
    let mut buffer = vec![slint::Rgb8Pixel::default(); 1770 * 1275];
    window.draw_if_needed(|r| {
        r.render(&mut buffer, 1770);
    });
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: point(40., 40.),
        button: PointerEventButton::Left,
    });
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: point(240., 66.),
    });
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: point(240., 66.),
        button: PointerEventButton::Left,
    });
    wait(&mut state.borrow_mut(), &ui, |s, u| {
        !u.get_file_busy() && s.forms.records.values().flatten().count() == 1
    });
    assert!(ui.get_dirty_document());
    ui.invoke_form_select(0);
    ui.set_form_value("2028-02-29".into());
    ui.invoke_action("form-commit".into());
    wait(&mut state.borrow_mut(), &ui, |s, u| {
        !u.get_file_busy()
            && s.forms
                .records
                .values()
                .flatten()
                .any(|f| f.value.text() == "2028-02-29")
    });
    ui.invoke_form_select(0);
    ui.set_form_kind(2);
    ui.invoke_action("form-properties".into());
    wait(&mut state.borrow_mut(), &ui, |s, u| {
        !u.get_file_busy()
            && s.forms
                .records
                .values()
                .flatten()
                .any(|f| f.spec.kind == viewer_core::FieldKind::Date)
    });
    ui.invoke_action("undo".into());
    wait(&mut state.borrow_mut(), &ui, |s, u| {
        !u.get_file_busy()
            && s.forms
                .records
                .values()
                .flatten()
                .any(|f| f.spec.kind == viewer_core::FieldKind::Text)
    });
    ui.invoke_form_select(0);
    ui.set_form_value("2030-01-01".into());
    ui.set_form_kind(2);
    ui.invoke_action("form-properties".into());
    wait(&mut state.borrow_mut(), &ui, |s, u| {
        !u.get_file_busy()
            && s.forms.records.values().flatten().any(|f| {
                f.spec.kind == viewer_core::FieldKind::Date && f.value.text() == "2030-01-01"
            })
    });
    ui.invoke_action("undo".into());
    wait(&mut state.borrow_mut(), &ui, |s, u| {
        !u.get_file_busy()
            && s.forms
                .records
                .values()
                .flatten()
                .any(|f| f.spec.kind == viewer_core::FieldKind::Text)
    });
    ui.invoke_form_select(0);
    let before = state
        .borrow()
        .forms
        .records
        .values()
        .flatten()
        .next()
        .unwrap()
        .spec
        .rect;
    ui.invoke_form_drag(0, 100., 100., 0, true);
    ui.invoke_form_drag(0, 140., 120., 1, true);
    assert!(ui.get_form_draft_visible());
    ui.invoke_form_drag(0, 140., 120., 2, true);
    wait(&mut state.borrow_mut(), &ui, |s, u| {
        !u.get_file_busy()
            && s.forms
                .records
                .values()
                .flatten()
                .next()
                .unwrap()
                .spec
                .rect
                .right
                > before.right
    });
    // Tab leaves a text draft as one history operation, then selects the next field.
    ui.invoke_action("form-duplicate".into());
    wait(&mut state.borrow_mut(), &ui, |s, u| {
        !u.get_file_busy() && s.forms.records.values().flatten().count() == 2
    });
    ui.invoke_action("form-fill".into());
    ui.invoke_form_select(0);
    ui.set_form_value("Typed through fill mode".into());
    ui.invoke_focus_document();
    press_key(&ui, Key::Tab.into());
    wait(&mut state.borrow_mut(), &ui, |s, u| {
        !u.get_file_busy()
            && s.forms
                .records
                .values()
                .flatten()
                .any(|f| f.value.text() == "Typed through fill mode")
    });
    ui.invoke_action("form-review".into());
    ui.invoke_action("form-fill".into());
    ui.invoke_form_select(0);
    let first = ui.get_form_name();
    ui.invoke_action("form-next".into());
    wait(&mut state.borrow_mut(), &ui, |_, u| {
        u.get_form_name() != first
    });
    // Scrolling may evict all visible pages while a text draft is selected.
    ui.invoke_form_select(0);
    ui.set_form_value("Draft survives scrolling".into());
    state.borrow_mut().form_refresh(&ui, &[]);
    ui.invoke_action("form-commit".into());
    wait(&mut state.borrow_mut(), &ui, |s, u| {
        !u.get_file_busy()
            && s.forms
                .records
                .values()
                .flatten()
                .any(|f| f.value.text() == "Draft survives scrolling")
    });
    crate::capture::snapshot(&ui, "forms-fill").unwrap();
    // Visual capture uses the same callbacks as the signature canvas.
    ui.invoke_action("form-prepare".into());
    ui.set_form_kind(8);
    ui.set_form_label("Signature".into());
    ui.invoke_action("form-new".into());
    let p = state.borrow().layout.pages[0];
    let scale = state.borrow().zoom * 96. / 72.;
    ui.invoke_reader_pointer(p.x + 40. * scale, p.y + 160. * scale, 0);
    ui.invoke_reader_pointer(p.x + 260. * scale, p.y + 205. * scale, 2);
    wait(&mut state.borrow_mut(), &ui, |s, u| {
        !u.get_file_busy() && s.forms.records.values().flatten().count() == 3
    });
    let index = state
        .borrow()
        .forms
        .records
        .values()
        .flatten()
        .position(|f| f.spec.kind == viewer_core::FieldKind::VisualSignature)
        .unwrap();
    ui.invoke_form_select(index as i32);
    ui.invoke_action("form-sign-draw".into());
    assert!(ui.get_signature_visible());
    ui.invoke_signature_pointer(20., 100., 0);
    ui.invoke_signature_pointer(100., 30., 1);
    ui.invoke_signature_pointer(240., 85., 1);
    ui.invoke_signature_pointer(360., 45., 2);
    crate::capture::snapshot(&ui, "forms-signature-capture").unwrap();
    ui.invoke_action("form-sign-apply".into());
    wait(&mut state.borrow_mut(), &ui, |s, u| {
        !u.get_file_busy()
            && s.forms.records.values().flatten().any(|f| {
                f.spec.kind == viewer_core::FieldKind::VisualSignature && !f.value.is_empty()
            })
    });
    // New radio members reselect by their generated export/widget identity.
    for (index, top) in [(0, 260.), (1, 300.)] {
        ui.set_form_kind(5);
        ui.set_form_group("choices".into());
        ui.set_form_label(format!("Choice {}", index + 1).into());
        ui.invoke_action("form-new".into());
        let p = state.borrow().layout.pages[0];
        let scale = state.borrow().zoom * 96. / 72.;
        ui.invoke_reader_pointer(p.x + 40. * scale, p.y + top * scale, 0);
        ui.invoke_reader_pointer(p.x + 54. * scale, p.y + (top + 14.) * scale, 2);
        wait(&mut state.borrow_mut(), &ui, |s, u| {
            !u.get_file_busy() && s.forms.records.values().flatten().count() == 4 + index
        });
        assert!(ui.get_form_selected());
        assert_eq!(ui.get_form_value_kind(), 5);
    }
    let radios = state
        .borrow()
        .forms
        .records
        .values()
        .flatten()
        .enumerate()
        .filter(|(_, f)| f.spec.kind == viewer_core::FieldKind::Radio)
        .map(|(i, f)| (i, f.id))
        .collect::<Vec<_>>();
    ui.invoke_form_select(radios[1].0 as i32);
    ui.set_form_checked(true);
    ui.invoke_action("form-toggle".into());
    wait(&mut state.borrow_mut(), &ui, |_, u| !u.get_file_busy());
    let surfaces = ui.get_form_surfaces();
    assert!(
        surfaces
            .iter()
            .any(|f| f.selected && f.index == radios[1].0 as i32)
    );
    crate::capture::snapshot(&ui, "forms-prepare").unwrap();
    ui.window().set_size(slint::LogicalSize::new(680., 480.));
    state.borrow_mut().tick(&ui);
    crate::capture::snapshot(&ui, "forms-small").unwrap();
}

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
            "Reader flow timed out: {} / {} / {}",
            ui.get_status(),
            ui.get_search_status(),
            ui.get_file_message()
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
    let pointer_state = state.clone();
    let pointer_ui = ui.as_weak();
    ui.on_reader_pointer(move |x, y, phase| {
        pointer_state
            .borrow_mut()
            .reader_pointer(&pointer_ui.upgrade().unwrap(), x, y, phase)
    });
    let click_state = state.clone();
    let click_ui = ui.as_weak();
    ui.on_reader_click(move |x, y| {
        click_state
            .borrow_mut()
            .reader_click(&click_ui.upgrade().unwrap(), x, y)
    });
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
    // Native outlines, internal navigation, mode changes and both selection directions.
    state.borrow_mut().open(&ui, fixture("navigation.pdf"));
    wait(&mut state.borrow_mut(), &ui, |state, ui| {
        ui.get_ready()
            && ui.get_bookmarks().row_count() > 0
            && state.tools.test_text(0).is_some()
            && state.navigation.test_links(0).is_some()
    });
    state.borrow_mut().activate_bookmark(&ui, 1);
    assert_eq!(ui.get_page_entry(), "2");
    state.borrow_mut().navigate(&ui, 0);
    let internal = state
        .borrow()
        .navigation
        .test_links(0)
        .unwrap()
        .into_iter()
        .find(|l| {
            matches!(
                l.destination,
                viewer_core::LinkDestination::Internal { page: 1, .. }
            )
        })
        .unwrap();
    let scale = state.borrow().zoom * 96. / 72.;
    state.borrow_mut().activate_link(
        &ui,
        0,
        (internal.bounds.left + internal.bounds.right) / 2. * scale,
        (internal.bounds.top + internal.bounds.bottom) / 2. * scale,
    );
    assert_eq!(ui.get_page_entry(), "2");
    state.borrow_mut().navigate(&ui, 0);
    state.borrow_mut().action(&ui, "bookmarks");
    crate::capture::snapshot(&ui, "navigation-bookmarks").unwrap();
    state.borrow_mut().activate_bookmark(&ui, 0);
    state.borrow_mut().action(&ui, "view-single");
    state.borrow_mut().navigate(&ui, 2);
    state.borrow_mut().tick(&ui);
    assert_eq!(ui.get_pages().row_count(), 1);
    assert_eq!(ui.get_pages().row_data(0).unwrap().number, 3);
    state.borrow_mut().action(&ui, "view-facing");
    state.borrow_mut().navigate(&ui, 0);
    state.borrow_mut().tick(&ui);
    assert_eq!(ui.get_pages().row_count(), 2);
    crate::capture::snapshot(&ui, "navigation-facing").unwrap();
    state.borrow_mut().action(&ui, "view-continuous");
    state.borrow_mut().navigate(&ui, 0);
    wait(&mut state.borrow_mut(), &ui, |state, _| {
        state.tools.test_text(0).is_some() && state.tools.test_text(1).is_some()
    });
    let first = state.borrow().tools.test_text(0).unwrap();
    let second = state.borrow().tools.test_text(1).unwrap();
    assert!(!first.glyphs.is_empty() && !second.glyphs.is_empty());
    let from = first.glyphs[0].bounds;
    let to = second.glyphs.last().unwrap().bounds;
    let scale = state.borrow().zoom * 96. / 72.;
    state
        .borrow_mut()
        .select_text(&ui, 0, from.left * scale, from.top * scale, 0);
    state
        .borrow_mut()
        .select_text(&ui, 1, to.right * scale, to.bottom * scale, 2);
    let forward = ui.get_selection_text();
    assert!(forward.contains("\n\n"));
    ui.invoke_copy_selection();
    assert_eq!(*clipboard.borrow(), forward.as_str());
    state
        .borrow_mut()
        .select_text(&ui, 1, to.right * scale, to.bottom * scale, 0);
    state
        .borrow_mut()
        .select_text(&ui, 0, from.left * scale, from.top * scale, 2);
    assert_eq!(ui.get_selection_text(), forward);
    assert!(state.borrow().tools.selected_highlight().is_none());
    crate::capture::snapshot(&ui, "navigation-cross-page-selection").unwrap();
    // Dispatch a captured drag across the two facing pages, rather than bypassing TouchArea.
    state.borrow_mut().action(&ui, "view-facing");
    state.borrow_mut().navigate(&ui, 0);
    state.borrow_mut().action(&ui, "fit-page");
    wait(&mut state.borrow_mut(), &ui, |state, _| {
        state.tools.test_text(0).is_some() && state.tools.test_text(1).is_some()
    });
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, 1770);
    });
    let scale = state.borrow().zoom * 96. / 72.;
    let pointer_position = |page: usize, r: viewer_core::TextRect| {
        let rect = state.borrow().layout.pages[page];
        slint::LogicalPosition::new(
            rect.x
                + (r.left + r.right) / 2. * scale
                + ui.get_scroll_x()
                + if ui.get_bookmarks_visible() {
                    230.
                } else if ui.get_sidebar_visible() {
                    180.
                } else {
                    0.
                },
            offset_y + rect.y + (r.top + r.bottom) / 2. * scale + ui.get_scroll_y(),
        )
    };
    let begin = pointer_position(0, from);
    let end = pointer_position(1, to);
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: begin,
        button: PointerEventButton::Left,
    });
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position: end });
    state.borrow_mut().tick(&ui);
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: end,
        button: PointerEventButton::Left,
    });
    assert!(ui.get_selection_text().contains("\n\n"));
    assert!(state.borrow().pointer.is_none());
    let stopped = ui.get_scroll_y();
    state.borrow_mut().tick(&ui);
    assert_eq!(ui.get_scroll_y(), stopped);
    crate::capture::snapshot(&ui, "navigation-facing-selection").unwrap();
    // Hold the captured pointer near the lower edge and verify actual cross-page autoscroll.
    state.borrow_mut().action(&ui, "view-continuous");
    state.borrow_mut().zoom = 1.;
    state.borrow_mut().dirty = true;
    state.borrow_mut().navigate(&ui, 0);
    state.borrow_mut().tick(&ui);
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, 1770);
    });
    let page = state.borrow().layout.pages[0];
    let scale = state.borrow().zoom * 96. / 72.;
    let begin = slint::LogicalPosition::new(
        230. + page.x + (from.left + from.right) / 2. * scale + ui.get_scroll_x(),
        offset_y + page.y + (from.top + from.bottom) / 2. * scale + ui.get_scroll_y(),
    );
    let edge = slint::LogicalPosition::new(begin.x, 850. - 32. - 5.);
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: begin,
        button: PointerEventButton::Left,
    });
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position: edge });
    for _ in 0..45 {
        state.borrow_mut().tick(&ui);
    }
    assert!(
        ui.get_scroll_y() < -500.,
        "autoscroll did not advance: {}",
        ui.get_scroll_y()
    );
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: edge,
        button: PointerEventButton::Left,
    });
    wait(&mut state.borrow_mut(), &ui, |_, ui| {
        ui.get_selection_text().contains("\n\n")
    });
    assert!(state.borrow().pointer.is_none());
    let stopped = ui.get_scroll_y();
    for _ in 0..3 {
        state.borrow_mut().tick(&ui);
    }
    assert_eq!(stopped, ui.get_scroll_y());
    state.borrow_mut().reader_pointer(&ui, 0., 0., 0);
    state
        .borrow_mut()
        .reader_pointer(&ui, page.x + 100., page.y + 200., 1);
    assert!(state.borrow().pointer.is_none());
    assert!(ui.get_selection_text().is_empty());
    state
        .borrow_mut()
        .reader_pointer(&ui, page.x + 100., page.y + 200., 0);
    state.borrow_mut().reader_pointer(&ui, 0., 0., 3);
    assert!(state.borrow().pointer.is_none());
    crate::capture::snapshot(&ui, "navigation-autoscroll").unwrap();
    ui.window().set_size(slint::LogicalSize::new(680., 480.));
    state.borrow_mut().tick(&ui);
    crate::capture::snapshot(&ui, "navigation-small").unwrap();
    ui.window().set_size(slint::LogicalSize::new(1180., 850.));
    let before_restricted = clipboard.borrow().clone();
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
    assert_eq!(*clipboard.borrow(), before_restricted);
    state.borrow_mut().open(&ui, fixture("malformed.pdf"));
    wait(&mut state.borrow_mut(), &ui, |_, ui| !ui.get_loading());
    assert!(!ui.get_ready());
    state.borrow_mut().open(&ui, fixture("text.pdf"));
    wait(&mut state.borrow_mut(), &ui, |_, ui| ui.get_ready());
    editing_flows(&state, &ui, &window);
    ui.hide().unwrap();
}

fn annotation_count(state: &State) -> usize {
    state.editor.notes.values().map(Vec::len).sum()
}

fn wait_annotations(state: &Rc<RefCell<State>>, ui: &AppWindow, count: usize) {
    wait(&mut state.borrow_mut(), ui, |state, ui| {
        !ui.get_file_busy() && annotation_count(state) == count
    });
}

fn edit_action(state: &Rc<RefCell<State>>, ui: &AppWindow, action: &str) {
    let revision = state.borrow().editor.revision;
    state.borrow_mut().action(ui, action);
    wait(&mut state.borrow_mut(), ui, |state, ui| {
        !ui.get_file_busy() && state.editor.revision != revision
    });
}

fn press_key(ui: &AppWindow, text: slint::SharedString) {
    ui.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    ui.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
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
    let directory = std::fs::canonicalize(directory).unwrap();
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
    let callback_state = state.clone();
    let weak = ui.as_weak();
    ui.on_annotation_click(move |index| {
        callback_state
            .borrow_mut()
            .annotation_click(&weak.upgrade().unwrap(), index)
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
    let original_width = state.borrow().sizes[0].width;
    let clipped_width = (a.left + b.right) / 2.0;
    state.borrow_mut().sizes[0].width = clipped_width;
    edit_action(state, ui, "annotate-highlight");
    wait_annotations(state, ui, 1);
    assert!(state.borrow().editor.notes.values().flatten().any(|a| matches!(&a.annotation,viewer_core::Annotation::Highlight{rects,..} if rects.iter().all(|r|r.right<=clipped_width&&r.left>=0.0))));
    // Only owned fixture copies are edited; the source remains unchanged until save.
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert!(ui.get_dirty_document());
    edit_action(state, ui, "undo");
    wait_annotations(state, ui, 0);
    assert!(!ui.get_dirty_document());
    edit_action(state, ui, "redo");
    wait_annotations(state, ui, 1);
    assert!(ui.get_dirty_document());
    assert_eq!(state.borrow().sizes[0].width, original_width);

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
    press_key(ui, Key::Escape.into());
    assert!(!ui.get_rename_active());
    assert!(source.exists());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    state.borrow_mut().action(ui, "rename-start");
    ui.set_rename_text("Renamed document".into());
    press_key(ui, Key::Return.into());
    wait(&mut state.borrow_mut(), ui, |_, ui| !ui.get_file_busy());
    let renamed = directory.join("Renamed document.pdf");
    assert!(renamed.exists() && !source.exists());
    assert_eq!(ui.get_document_name(), "Renamed document.pdf");
    assert!(!ui.get_dirty_document(), "renaming saves native edits");
    wait_annotations(state, ui, 1);
    crate::capture::snapshot(ui, "annotation-renamed").unwrap();

    state.borrow_mut().action(ui, "annotate-note");
    state.borrow_mut().page_point(ui, 0, 150.0, 220.0);
    assert!(ui.get_note_visible());
    ui.invoke_note_submit("   ".into());
    assert!(ui.get_note_visible());
    assert!(ui.get_file_message().contains("16,384"));
    let revision = state.borrow().editor.revision;
    ui.invoke_note_submit("A note — café 日本語\nSecond line".into());
    wait(&mut state.borrow_mut(), ui, |state, ui| {
        !ui.get_file_busy() && state.editor.revision != revision
    });
    wait_annotations(state, ui, 2);
    assert!(!ui.get_note_visible());
    let note_index = state
        .borrow()
        .editor
        .notes
        .values()
        .flatten()
        .position(|a| matches!(a.annotation, viewer_core::Annotation::Note { .. }))
        .unwrap() as i32;
    state.borrow_mut().annotation_click(ui, note_index);
    assert!(ui.get_note_visible() && !ui.get_note_read_only());
    let revision = state.borrow().editor.revision;
    ui.invoke_note_submit("Edited note — café 日本語\nSecond line".into());
    wait(&mut state.borrow_mut(), ui, |state, ui| {
        !ui.get_file_busy() && state.editor.revision != revision
    });
    wait_annotations(state, ui, 2);
    edit_action(state, ui, "undo");
    wait_annotations(state, ui, 2);
    assert!(state.borrow().editor.notes.values().flatten().any(|a|matches!(&a.annotation,viewer_core::Annotation::Note{text,..} if text.starts_with("A note"))));
    edit_action(state, ui, "redo");
    wait_annotations(state, ui, 2);
    assert!(state.borrow().editor.notes.values().flatten().any(|a|matches!(&a.annotation,viewer_core::Annotation::Note{text,..} if text.starts_with("Edited note"))));
    state.borrow_mut().action(ui, "rename-start");
    state.borrow_mut().rename(ui, "Renamed document.pdf".into());
    wait(&mut state.borrow_mut(), ui, |_, ui| !ui.get_file_busy());
    assert!(!ui.get_dirty_document());
    let checkpoint = std::fs::read(&renamed).unwrap();
    std::fs::write(directory.join("rename-checkpoint.pdf"), &checkpoint).unwrap();
    let target = directory.join("annotated.pdf");
    state.borrow_mut().save_as(ui, target.clone());
    wait(&mut state.borrow_mut(), ui, |_, ui| !ui.get_file_busy());
    assert!(target.exists());
    assert_eq!(
        state.borrow().path.as_ref(),
        Some(&target),
        "Save As switches the active file"
    );
    assert!(!ui.get_dirty_document());
    assert_eq!(std::fs::read(&renamed).unwrap(), checkpoint);

    // Reopened saved annotations remain editable, with native ordinals independent of UI indices.
    state.borrow_mut().request_open(ui, target.clone());
    wait(&mut state.borrow_mut(), ui, |state, ui| {
        ui.get_ready() && annotation_count(state) == 2 && ui.get_tiles().row_count() > 0
    });
    let note_index = state
        .borrow()
        .editor
        .notes
        .values()
        .flatten()
        .position(|a| matches!(a.annotation, viewer_core::Annotation::Note { .. }))
        .unwrap() as i32;
    ui.invoke_annotation_click(note_index);
    assert!(ui.get_note_visible() && !ui.get_note_read_only() && ui.get_annotation_editable());
    assert!(ui.get_note_text().contains("日本語"));
    crate::capture::snapshot(ui, "annotation-saved-note").unwrap();
    edit_action(state, ui, "delete-annotation");
    wait_annotations(state, ui, 1);
    edit_action(state, ui, "undo");
    wait_annotations(state, ui, 2);
    let highlight_index = state
        .borrow()
        .editor
        .notes
        .values()
        .flatten()
        .position(|a| matches!(a.annotation, viewer_core::Annotation::Highlight { .. }))
        .unwrap() as i32;
    let original_highlight_rgb = state
        .borrow()
        .editor
        .notes
        .values()
        .flatten()
        .nth(highlight_index as usize)
        .unwrap()
        .rgb;
    state.borrow_mut().annotation_click(ui, highlight_index);
    assert!(ui.get_highlight_visible() && ui.get_annotation_editable());
    edit_action(state, ui, "highlight-green");
    wait_annotations(state, ui, 2);
    assert!(
        state
            .borrow()
            .editor
            .notes
            .values()
            .flatten()
            .any(
                |a| matches!(a.annotation, viewer_core::Annotation::Highlight { .. })
                    && (a.rgb[0] - 0.3).abs() < 0.02
                    && (a.rgb[1] - 1.0).abs() < 0.02
            )
    );
    edit_action(state, ui, "undo");
    wait_annotations(state, ui, 2);
    assert!(state.borrow().editor.notes.values().flatten().any(|a| {
        matches!(a.annotation, viewer_core::Annotation::Highlight { .. })
            && a.rgb
                .iter()
                .zip(original_highlight_rgb)
                .all(|(actual, original)| (actual - original).abs() < 0.02)
    }));

    // Real Ctrl+S updates this owned PDF directly rather than requesting Save As.
    let note_index = state
        .borrow()
        .editor
        .notes
        .values()
        .flatten()
        .position(|a| matches!(a.annotation, viewer_core::Annotation::Note { .. }))
        .unwrap() as i32;
    state.borrow_mut().annotation_click(ui, note_index);
    let revision = state.borrow().editor.revision;
    ui.invoke_note_submit("Edited note — café 日本語\nSecond line\nSaved with Ctrl+S".into());
    wait(&mut state.borrow_mut(), ui, |state, ui| {
        !ui.get_file_busy() && state.editor.revision != revision
    });
    wait_annotations(state, ui, 2);
    ui.invoke_focus_document();
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    press_key(ui, "s".into());
    ui.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    wait(&mut state.borrow_mut(), ui, |_, ui| !ui.get_file_busy());
    assert!(!ui.get_dirty_document());
    assert!(state.borrow_mut().take_save_dialog().is_none());
    assert_eq!(state.borrow().path.as_ref(), Some(&target));
    assert_eq!(std::fs::read(&renamed).unwrap(), checkpoint);

    // Pending open/close blocks undo and saves before an explicitly authorized close.
    edit_action(state, ui, "undo");
    assert!(ui.get_dirty_document());
    state.borrow_mut().request_open(ui, fixture("office.pdf"));
    assert!(ui.get_pending_open());
    let revision = state.borrow().editor.revision;
    state.borrow_mut().action(ui, "undo");
    assert_eq!(state.borrow().editor.revision, revision);
    state.borrow_mut().action(ui, "pending-cancel");
    assert!(!ui.get_pending_open());
    state.borrow_mut().action(ui, "page-tools");
    assert!(ui.get_page_tools_visible());
    assert!(!state.borrow_mut().request_close(ui));
    assert!(ui.get_pending_open() && ui.get_pending_close());
    assert!(!ui.get_page_tools_visible());
    state.borrow_mut().action(ui, "pending-save");
    wait(&mut state.borrow_mut(), ui, |_, ui| {
        !ui.get_file_busy() && !ui.get_pending_open()
    });
    assert!(state.borrow_mut().take_close_request());
    assert!(!ui.get_dirty_document());

    // Page operations and exports use a separate owned multi-page fixture copy.
    let pages_source = directory.join("pages.pdf");
    std::fs::copy(fixture("office.pdf"), &pages_source).unwrap();
    let pages_original = std::fs::read(&pages_source).unwrap();
    state.borrow_mut().open(ui, pages_source.clone());
    wait(&mut state.borrow_mut(), ui, |_, ui| {
        ui.get_ready() && ui.get_page_count() == 20
    });
    assert!(ui.get_can_assemble() && ui.get_can_copy());
    state.borrow_mut().action(ui, "page-tools");
    assert!(ui.get_page_tools_visible());
    crate::capture::snapshot(ui, "page-tools").unwrap();
    ui.window().set_size(slint::LogicalSize::new(680.0, 480.0));
    state.borrow_mut().tick(ui);
    crate::capture::snapshot(ui, "page-tools-small").unwrap();
    ui.window().set_size(slint::LogicalSize::new(1180.0, 850.0));
    state.borrow_mut().tick(ui);
    edit_action(state, ui, "blank-page");
    assert_eq!(ui.get_page_count(), 21);
    assert_eq!(ui.get_page_entry(), "2");
    state.borrow_mut().action(ui, "page-tools");
    ui.set_page_range("2".into());
    edit_action(state, ui, "rotate-right");
    wait(&mut state.borrow_mut(), ui, |state, _| {
        state.sizes[1].width > state.sizes[1].height
    });
    state.borrow_mut().action(ui, "page-tools");
    ui.set_move_to("3".into());
    edit_action(state, ui, "move-page");
    assert_eq!(ui.get_page_entry(), "3");
    state.borrow_mut().action(ui, "page-tools");
    ui.set_page_range("3".into());
    edit_action(state, ui, "delete-pages");
    assert_eq!(ui.get_page_count(), 20);
    edit_action(state, ui, "undo");
    assert_eq!(ui.get_page_count(), 21);
    edit_action(state, ui, "undo");
    edit_action(state, ui, "undo");
    edit_action(state, ui, "undo");
    assert_eq!(ui.get_page_count(), 20);
    assert!(!ui.get_dirty_document());
    assert_eq!(std::fs::read(&pages_source).unwrap(), pages_original);

    state.borrow_mut().navigate(ui, 0);
    state.borrow_mut().action(ui, "page-tools");
    ui.set_page_range("1-2,4".into());
    state.borrow_mut().action(ui, "extract-pages");
    let (document, kind, _) = state.borrow_mut().take_tool_dialog().unwrap();
    assert!(matches!(kind, super::ToolDialog::Extract));
    let extracted = directory.join("extracted.pdf");
    state
        .borrow_mut()
        .tool_dialog_result(ui, document, kind, vec![extracted.clone()]);
    wait(&mut state.borrow_mut(), ui, |_, ui| !ui.get_file_busy());
    assert!(extracted.exists());
    assert_eq!(state.borrow().path.as_ref(), Some(&pages_source));
    assert!(!ui.get_dirty_document());
    let split_directory = directory.join("split");
    std::fs::create_dir(&split_directory).unwrap();
    state.borrow_mut().action(ui, "page-tools");
    state.borrow_mut().action(ui, "split-pdf");
    let (document, kind, _) = state.borrow_mut().take_tool_dialog().unwrap();
    assert!(matches!(kind, super::ToolDialog::Split));
    state
        .borrow_mut()
        .tool_dialog_result(ui, document, kind, vec![split_directory.clone()]);
    wait(&mut state.borrow_mut(), ui, |_, ui| !ui.get_file_busy());
    assert!(split_directory.join("pages-part-1.pdf").exists());
    assert!(split_directory.join("pages-part-2.pdf").exists());
    assert_eq!(std::fs::read(&pages_source).unwrap(), pages_original);
    state.borrow_mut().open(ui, extracted);
    wait(&mut state.borrow_mut(), ui, |_, ui| ui.get_ready());
    assert_eq!(ui.get_page_count(), 3);
    state
        .borrow_mut()
        .open(ui, split_directory.join("pages-part-1.pdf"));
    wait(&mut state.borrow_mut(), ui, |_, ui| ui.get_ready());
    assert_eq!(ui.get_page_count(), 1);
    state
        .borrow_mut()
        .open(ui, split_directory.join("pages-part-2.pdf"));
    wait(&mut state.borrow_mut(), ui, |_, ui| ui.get_ready());
    assert_eq!(ui.get_page_count(), 19);
}
