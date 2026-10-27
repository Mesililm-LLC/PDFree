//! Persisted UI preferences: sidebar visibility/widths and colour scheme.
//! Stored as JSON under `%LOCALAPPDATA%\PDFree\ui-settings.json` (falling back
//! to the current directory when LOCALAPPDATA is unset). Failures to read or
//! write are ignored so a read-only profile never blocks the viewer.

use crate::AppWindow;
use std::path::PathBuf;

#[derive(Clone, PartialEq)]
pub struct UiSettings {
    pub sidebar_visible: bool,
    pub bookmarks_visible: bool,
    pub sidebar_width: f32,
    pub bookmarks_width: f32,
    pub form_width: f32,
    pub dark_mode: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            sidebar_visible: false,
            bookmarks_visible: false,
            sidebar_width: 180.0,
            bookmarks_width: 230.0,
            form_width: 300.0,
            dark_mode: false,
        }
    }
}

fn settings_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("PDFree").join("ui-settings.json")
}

pub fn load() -> UiSettings {
    let defaults = UiSettings::default();
    let Ok(text) = std::fs::read_to_string(settings_path()) else {
        return defaults;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return defaults;
    };
    let number = |key: &str, fallback: f32| {
        value
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .map(|v| v as f32)
            .unwrap_or(fallback)
    };
    let flag = |key: &str, fallback: bool| {
        value
            .get(key)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(fallback)
    };
    UiSettings {
        sidebar_visible: flag("sidebar_visible", defaults.sidebar_visible),
        bookmarks_visible: flag("bookmarks_visible", defaults.bookmarks_visible),
        sidebar_width: number("sidebar_width", defaults.sidebar_width).clamp(140.0, 360.0),
        bookmarks_width: number("bookmarks_width", defaults.bookmarks_width).clamp(160.0, 420.0),
        form_width: number("form_width", defaults.form_width).clamp(240.0, 480.0),
        dark_mode: flag("dark_mode", defaults.dark_mode),
    }
}

pub fn save(settings: &UiSettings) {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let value = serde_json::json!({
        "sidebar_visible": settings.sidebar_visible,
        "bookmarks_visible": settings.bookmarks_visible,
        "sidebar_width": settings.sidebar_width,
        "bookmarks_width": settings.bookmarks_width,
        "form_width": settings.form_width,
        "dark_mode": settings.dark_mode,
    });
    let _ = std::fs::write(&path, value.to_string());
}

pub fn apply(ui: &AppWindow, settings: &UiSettings) {
    ui.set_sidebar_visible(settings.sidebar_visible);
    ui.set_bookmarks_visible(settings.bookmarks_visible);
    ui.set_sidebar_width(settings.sidebar_width);
    ui.set_bookmarks_width(settings.bookmarks_width);
    ui.set_form_width(settings.form_width);
    ui.set_dark_mode(settings.dark_mode);
}

pub fn snapshot(ui: &AppWindow) -> UiSettings {
    UiSettings {
        sidebar_visible: ui.get_sidebar_visible(),
        bookmarks_visible: ui.get_bookmarks_visible(),
        sidebar_width: ui.get_sidebar_width(),
        bookmarks_width: ui.get_bookmarks_width(),
        form_width: ui.get_form_width(),
        dark_mode: ui.get_dark_mode(),
    }
}
