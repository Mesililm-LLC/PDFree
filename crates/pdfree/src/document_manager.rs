//! DocumentManager: document identity, path, and open timing.

use crate::AppWindow;
use pdf_engine::Engine;

pub struct DocumentManager {
    pub document: u64,
    pub path: Option<std::path::PathBuf>,
    pub open_started: std::time::Instant,
}

impl DocumentManager {
    pub fn new() -> Self {
        Self {
            document: 0,
            path: None,
            open_started: std::time::Instant::now(),
        }
    }

    /// Open a document (without password).
    pub fn open(&mut self, ui: &AppWindow, engine: &Engine, path: std::path::PathBuf) {
        self.open_password(ui, engine, path, None);
    }

    /// Open a document with an optional password.
    /// Returns the new document id.
    pub fn open_password(
        &mut self,
        ui: &AppWindow,
        engine: &Engine,
        path: std::path::PathBuf,
        password: Option<String>,
    ) -> u64 {
        self.path = Some(path.clone());
        self.document += 1;
        let doc = self.document;
        self.open_started = std::time::Instant::now();

        ui.set_status("Opening PDF…".into());
        ui.set_detail("Preparing the first page".into());
        ui.set_document_name(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref()
                .into(),
        );

        engine.open_with_password(doc, 0, path, password);
        doc
    }

    /// Invalidate the current document (e.g. password cancelled).
    pub fn invalidate(&mut self) {
        self.document += 1;
        self.path = None;
    }
}
