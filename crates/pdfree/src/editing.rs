use super::State;
use crate::{AnnotationSurface, AppWindow};
use pdf_engine::{Event, FileAction};
use slint::{ModelRc, VecModel};
use std::{collections::BTreeMap, path::PathBuf};
use viewer_core::{Annotation, EditHistory, validate_pdf_name};

#[derive(Default)]
pub(super) struct Editor {
    pub(super) history: EditHistory,
    operation: u64,
    pending_revision: Option<u64>,
    wanted: Vec<usize>,
    notes: BTreeMap<usize, Vec<Annotation>>,
    note_target: Option<(usize, f32, f32, Option<usize>)>,
    pending_open: Option<PathBuf>,
    pending_close: bool,
    continue_after_save: bool,
    save_dialog: bool,
    close: bool,
}
impl Editor {
    #[cfg(test)]
    pub(super) fn native_notes_count(&self) -> usize {
        self.notes.values().map(Vec::len).sum()
    }
}
impl State {
    pub(super) fn reset_editor(&mut self, ui: &AppWindow) {
        self.editor = Editor::default();
        ui.set_annotations(ModelRc::default());
        ui.set_can_annotate(false);
        ui.set_file_busy(false);
        ui.set_rename_active(false);
        ui.set_file_message("".into());
        ui.set_note_visible(false);
        ui.set_note_placement(false);
        ui.set_pending_open(false);
        ui.set_pending_close(false);
        self.edit_flags(ui);
    }
    fn edit_flags(&self, ui: &AppWindow) {
        ui.set_dirty_document(self.editor.history.dirty());
        ui.set_can_undo(self.editor.history.can_undo());
        ui.set_can_redo(self.editor.history.can_redo());
    }
    fn edit_blocked(&self, ui: &AppWindow) -> bool {
        if ui.get_file_busy() || self.dialog_open {
            ui.set_file_message("Finish the current file operation first.".into());
            true
        } else if ui.get_note_visible() && !ui.get_note_read_only() {
            ui.set_file_message("Save or cancel the open note first.".into());
            true
        } else {
            false
        }
    }
    pub fn request_open(&mut self, ui: &AppWindow, path: PathBuf) {
        if self.edit_blocked(ui) {
            return;
        }
        if self.editor.history.dirty() {
            self.editor.pending_open = Some(path);
            self.editor.pending_close = false;
            ui.set_pending_close(false);
            ui.set_pending_open(true);
        } else {
            self.open(ui, path);
        }
    }
    pub fn request_close(&mut self, ui: &AppWindow) -> bool {
        if self.edit_blocked(ui) {
            return false;
        }
        if !self.editor.history.dirty() {
            return true;
        }
        self.editor.pending_open = None;
        self.editor.pending_close = true;
        ui.set_pending_open(true);
        ui.set_pending_close(true);
        false
    }
    pub fn take_close_request(&mut self) -> bool {
        std::mem::take(&mut self.editor.close)
    }
    pub fn take_save_dialog(&mut self) -> Option<(u64, PathBuf)> {
        if !std::mem::take(&mut self.editor.save_dialog) {
            return None;
        }
        self.path.as_ref().map(|path| {
            (
                self.document,
                path.with_file_name(format!(
                    "{} annotated.pdf",
                    path.file_stem().unwrap_or_default().to_string_lossy()
                )),
            )
        })
    }
    pub fn save_dialog_result(&mut self, ui: &AppWindow, document: u64, path: Option<PathBuf>) {
        if document != self.document {
            return;
        }
        if let Some(path) = path {
            self.save_as(ui, path);
        } else {
            self.editor.continue_after_save = false;
        }
    }
    pub fn save_as(&mut self, ui: &AppWindow, path: PathBuf) {
        if !ui.get_ready() || self.edit_blocked(ui) {
            return;
        }
        self.editor.operation += 1;
        if self.engine.save_as(
            self.document,
            self.editor.operation,
            path,
            self.editor.history.annotations().to_vec(),
        ) {
            self.editor.pending_revision = Some(self.editor.history.revision());
            ui.set_file_busy(true);
            ui.set_file_message("Saving annotated copy…".into());
        } else {
            ui.set_file_message("A file operation is already running. Try again.".into());
        }
    }
    pub fn rename(&mut self, ui: &AppWindow, name: String) {
        if !ui.get_ready() || self.edit_blocked(ui) || !ui.get_rename_active() {
            return;
        }
        let name = match validate_pdf_name(&name) {
            Ok(name) => name,
            Err(message) => {
                ui.set_file_message(message.into());
                return;
            }
        };
        self.editor.operation += 1;
        if self
            .engine
            .rename(self.document, self.editor.operation, name)
        {
            ui.set_file_busy(true);
            ui.set_file_message("Renaming PDF…".into());
        } else {
            ui.set_file_message("A file operation is already running. Try again.".into());
        }
    }
    fn finish_pending(&mut self, ui: &AppWindow) {
        ui.set_pending_open(false);
        ui.set_pending_close(false);
        if self.editor.pending_close {
            self.editor.close = true;
        } else if let Some(path) = self.editor.pending_open.take() {
            self.open(ui, path);
        }
    }
    pub(super) fn edit_action(&mut self, ui: &AppWindow, action: &str) -> bool {
        if !matches!(
            action,
            "annotate-highlight"
                | "annotate-note"
                | "undo"
                | "redo"
                | "save-as"
                | "rename-start"
                | "rename-cancel"
                | "cancel-note"
                | "pending-save"
                | "pending-discard"
                | "pending-cancel"
        ) {
            return false;
        }
        if ui.get_file_busy() || self.dialog_open {
            return true;
        }
        match action {
            "cancel-note" => {
                ui.set_note_visible(false);
                ui.set_note_placement(false);
                self.editor.note_target = None;
                ui.invoke_focus_document();
            }
            "pending-cancel" => {
                self.editor.pending_open = None;
                self.editor.pending_close = false;
                self.editor.continue_after_save = false;
                ui.set_pending_open(false);
                ui.set_pending_close(false);
            }
            "pending-discard" => self.finish_pending(ui),
            "pending-save" => {
                self.editor.continue_after_save = true;
                self.editor.save_dialog = true;
            }
            "rename-cancel" => {
                ui.set_rename_active(false);
                ui.set_file_message("".into());
                ui.invoke_focus_document();
            }
            _ if self.edit_blocked(ui) => {}
            "rename-start" if ui.get_ready() && !ui.get_pending_open() => {
                ui.set_rename_text(ui.get_document_name());
                ui.set_rename_active(true);
                ui.set_file_message("Enter to rename • Escape to cancel".into());
                ui.invoke_focus_rename();
            }
            "save-as" if ui.get_ready() => self.editor.save_dialog = true,
            "undo" => {
                self.editor.history.undo();
            }
            "redo" => {
                self.editor.history.redo();
            }
            "annotate-highlight" if ui.get_can_annotate() => {
                if let Some(Annotation::Highlight { page, mut rects }) =
                    self.tools.selected_highlight()
                {
                    let Some(size) = self.sizes.get(page) else {
                        return true;
                    };
                    for r in &mut rects {
                        r.left = r.left.max(0.0);
                        r.top = r.top.max(0.0);
                        r.right = r.right.min(size.width);
                        r.bottom = r.bottom.min(size.height);
                    }
                    rects.retain(|r| r.right > r.left && r.bottom > r.top);
                    if rects.is_empty() {
                        ui.set_file_message(
                            "The selected text is outside the visible page.".into(),
                        );
                        return true;
                    }
                    let annotation = Annotation::Highlight { page, rects };
                    let mut draft = self.editor.history.annotations().to_vec();
                    draft.push(annotation);
                    match self.editor.history.push(draft) {
                        Ok(()) => self.tools.clear_selection(ui),
                        Err(message) => ui.set_file_message(message.into()),
                    }
                    self.tools.overlays(ui, &self.layout, self.zoom);
                } else {
                    ui.set_file_message("Select text on a page, then choose Highlight.".into());
                }
            }
            "annotate-note" if ui.get_can_annotate() => {
                ui.set_note_placement(true);
                ui.set_file_message("Click a page to place a note.".into());
            }
            _ => {}
        }
        self.edit_flags(ui);
        self.edit_overlays(ui);
        true
    }
    pub fn page_point(&mut self, ui: &AppWindow, page: usize, x: f32, y: f32) {
        if ui.get_pending_open() {
            return;
        }
        if !ui.get_note_placement() || !ui.get_can_annotate() || self.edit_blocked(ui) {
            return;
        }
        let scale = self.zoom * 96.0 / 72.0;
        if let Some(size) = self.sizes.get(page) {
            let (x, y) = (x / scale, y / scale);
            if !x.is_finite()
                || !y.is_finite()
                || x < 0.0
                || y < 0.0
                || x >= size.width
                || y >= size.height
            {
                return;
            }
            self.editor.note_target = Some((page, x, y, None));
            ui.set_note_placement(false);
            ui.set_note_read_only(false);
            ui.set_note_text("".into());
            ui.set_note_visible(true);
            ui.invoke_focus_note();
        }
    }
    pub fn annotation_click(&mut self, ui: &AppWindow, index: i32) {
        if ui.get_pending_open() {
            return;
        }
        if self.edit_blocked(ui) {
            return;
        }
        let annotation = if index >= 0 {
            self.editor
                .history
                .annotations()
                .get(index as usize)
                .cloned()
        } else {
            self.editor
                .notes
                .values()
                .flatten()
                .nth((-i64::from(index) - 1) as usize)
                .cloned()
        };
        if let Some(Annotation::Note { page, x, y, text }) = annotation {
            self.editor.note_target = Some((page, x, y, (index >= 0).then_some(index as usize)));
            ui.set_note_text(text.into());
            ui.set_note_read_only(index < 0);
            ui.set_note_visible(true);
            ui.invoke_focus_note();
        }
    }
    pub fn note_submit(&mut self, ui: &AppWindow, text: String) {
        if ui.get_note_read_only() || ui.get_file_busy() || !ui.get_can_annotate() {
            return;
        }
        if let Some((page, x, y, index)) = self.editor.note_target {
            let annotation = Annotation::Note { page, x, y, text };
            let mut draft = self.editor.history.annotations().to_vec();
            if let Some(index) = index {
                if let Some(old) = draft.get_mut(index) {
                    *old = annotation;
                } else {
                    return;
                }
            } else {
                draft.push(annotation);
            }
            match self.editor.history.push(draft) {
                Ok(()) => {
                    ui.set_note_visible(false);
                    self.editor.note_target = None;
                    ui.set_file_message("".into());
                    self.edit_flags(ui);
                    self.edit_overlays(ui);
                    ui.invoke_focus_document();
                }
                Err(message) => ui.set_file_message(message.into()),
            }
        }
    }
    pub(super) fn edit_event(&mut self, ui: &AppWindow, event: &Event) -> bool {
        match event {
            Event::Notes {
                document,
                page,
                notes,
                truncated,
            } if *document == self.document && self.editor.wanted.contains(page) => {
                self.editor.notes.insert(*page, notes.clone());
                if *truncated {
                    ui.set_file_message(
                        "Some existing notes exceed the page preview limit.".into(),
                    );
                }
                true
            }
            Event::FileDone {
                document,
                operation,
                path,
                kind,
            } if *document == self.document && *operation == self.editor.operation => {
                ui.set_file_busy(false);
                if *kind == FileAction::Rename {
                    self.path = Some(path.clone());
                    ui.set_document_name(
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .as_ref()
                            .into(),
                    );
                    ui.set_rename_active(false);
                    ui.set_file_message("PDF renamed.".into());
                    ui.invoke_focus_document();
                } else {
                    if self.editor.pending_revision == Some(self.editor.history.revision()) {
                        self.editor.history.mark_saved();
                    }
                    self.editor.pending_revision = None;
                    ui.set_file_message(
                        format!(
                            "Saved copy as {}. Original remains open.",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        )
                        .into(),
                    );
                    if std::mem::take(&mut self.editor.continue_after_save) {
                        self.finish_pending(ui);
                    }
                }
                self.edit_flags(ui);
                true
            }
            Event::FileError {
                document,
                operation,
                message,
            } if *document == self.document && *operation == self.editor.operation => {
                ui.set_file_busy(false);
                self.editor.pending_revision = None;
                self.editor.continue_after_save = false;
                ui.set_file_message(message.as_str().into());
                true
            }
            _ => false,
        }
    }
    pub(super) fn edit_refresh(&mut self, ui: &AppWindow, resident: &[usize]) {
        let wanted: Vec<_> = resident.iter().copied().take(16).collect();
        if self.editor.wanted != wanted {
            self.editor.notes.retain(|page, _| wanted.contains(page));
            self.editor.wanted = wanted.clone();
            self.engine.note_pages(self.document, wanted);
        }
        self.edit_overlays(ui);
    }
    fn edit_overlays(&self, ui: &AppWindow) {
        let scale = self.zoom * 96.0 / 72.0;
        let mut surfaces = Vec::new();
        let mut append = |annotation: &Annotation, index: i32, native: bool| {
            let page_index = annotation.page();
            if !self.editor.wanted.contains(&page_index) {
                return;
            }
            let Some(page) = self.layout.pages.get(page_index) else {
                return;
            };
            match annotation {
                Annotation::Highlight { rects, .. } => {
                    for r in rects {
                        surfaces.push(AnnotationSurface {
                            x: page.x + r.left * scale,
                            y: page.y + r.top * scale,
                            w: (r.right - r.left) * scale,
                            h: (r.bottom - r.top) * scale,
                            kind: 0,
                            label: "".into(),
                            index,
                        });
                    }
                }
                Annotation::Note { x, y, .. } => surfaces.push(AnnotationSurface {
                    x: page.x + x * scale,
                    y: page.y + y * scale,
                    w: 20.0 * scale,
                    h: 20.0 * scale,
                    kind: if native { 2 } else { 1 },
                    label: "Note".into(),
                    index,
                }),
            }
        };
        for (i, annotation) in self.editor.history.annotations().iter().enumerate() {
            append(annotation, i as i32, false);
        }
        for (i, annotation) in self.editor.notes.values().flatten().enumerate() {
            append(annotation, -(i as i32) - 1, true);
        }
        ui.set_annotations(ModelRc::new(VecModel::from(surfaces)));
    }
}
