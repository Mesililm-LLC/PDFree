use super::State;
use crate::{AnnotationItem, AnnotationSurface, AppWindow};
use pdf_engine::{EditCommand, Event, FileAction, NativeAnnotation};
use slint::{ModelRc, VecModel};
use std::{collections::BTreeMap, path::PathBuf};
use viewer_core::{Annotation, Layout, parse_pages, validate_pdf_name};
#[derive(Clone, Copy)]
pub enum ToolDialog {
    Insert,
    Merge,
    Extract,
    Split,
}
#[derive(Default)]
pub(crate) struct Editor {
    pub(crate) revision: u64,
    dirty: bool,
    pub(super) operation: u64,
    wanted: Vec<usize>,
    pub(super) notes: BTreeMap<usize, Vec<NativeAnnotation>>,
    note_target: Option<(usize, f32, f32, Option<usize>)>,
    selected: Option<(usize, usize)>,
    pending_open: Option<PathBuf>,
    pending_close: bool,
    continue_after_save: bool,
    save_dialog: bool,
    tool_dialog: Option<ToolDialog>,
    dialog_pages: Vec<usize>,
    dialog_page: usize,
    close: bool,
    // Print copies actually handed to the OS printer (recorded under tests).
    #[cfg(test)]
    pub(super) printed: Vec<PathBuf>,
}
impl State {
    pub(super) fn reset_editor(&mut self, ui: &AppWindow) {
        self.editor = Editor::default();
        ui.set_annotations(ModelRc::default());
        ui.set_annotation_items(ModelRc::default());
        ui.set_can_annotate(false);
        ui.set_can_assemble(false);
        ui.set_file_busy(false);
        ui.set_rename_active(false);
        ui.set_file_message("".into());
        ui.set_note_visible(false);
        ui.set_note_placement(false);
        ui.set_pending_open(false);
        ui.set_pending_close(false);
        ui.set_page_tools_visible(false);
        ui.set_annotation_list_visible(false);
        ui.set_highlight_visible(false);
        ui.set_dirty_document(false);
        ui.set_can_undo(false);
        ui.set_can_redo(false);
    }
    fn edit_blocked(&self, ui: &AppWindow) -> bool {
        if ui.get_file_busy() || self.dialog_open {
            true
        } else if ui.get_note_visible() && !ui.get_note_read_only() {
            ui.set_file_message("Save or cancel the open note first.".into());
            true
        } else {
            false
        }
    }
    pub fn request_open(&mut self, ui: &AppWindow, path: PathBuf) {
        if self.form_before_open(ui, &path) {
            return;
        }
        if self.edit_blocked(ui) {
            return;
        }
        if self.editor.dirty {
            self.prepare_pending(ui);
            self.editor.pending_open = Some(path);
            self.editor.pending_close = false;
            ui.set_pending_close(false);
            ui.set_pending_open(true);
        } else {
            self.open(ui, path);
        }
    }
    pub fn request_close(&mut self, ui: &AppWindow) -> bool {
        if self.form_before_close(ui) {
            return false;
        }
        if self.edit_blocked(ui) {
            return false;
        }
        if !self.editor.dirty {
            return true;
        }
        self.editor.pending_open = None;
        self.editor.pending_close = true;
        self.prepare_pending(ui);
        ui.set_pending_open(true);
        ui.set_pending_close(true);
        false
    }
    fn prepare_pending(&self, ui: &AppWindow) {
        ui.set_page_tools_visible(false);
        ui.set_annotation_list_visible(false);
        ui.set_highlight_visible(false);
        ui.set_note_visible(false);
        ui.set_note_placement(false);
        ui.set_rename_active(false);
        ui.invoke_focus_modal();
    }
    pub fn take_close_request(&mut self) -> bool {
        std::mem::take(&mut self.editor.close)
    }
    pub fn take_save_dialog(&mut self) -> Option<(u64, PathBuf)> {
        if !std::mem::take(&mut self.editor.save_dialog) {
            return None;
        }
        self.path.as_ref().map(|p| {
            (
                self.document,
                p.with_file_name(format!(
                    "{} copy.pdf",
                    p.file_stem().unwrap_or_default().to_string_lossy()
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
    pub fn take_tool_dialog(&mut self) -> Option<(u64, ToolDialog, PathBuf)> {
        self.editor
            .tool_dialog
            .take()
            .map(|kind| (self.document, kind, self.path.clone().unwrap_or_default()))
    }
    pub fn tool_dialog_result(
        &mut self,
        ui: &AppWindow,
        document: u64,
        kind: ToolDialog,
        paths: Vec<PathBuf>,
    ) {
        if document != self.document || paths.is_empty() {
            return;
        }
        match kind {
            ToolDialog::Insert | ToolDialog::Merge => {
                let at = if matches!(kind, ToolDialog::Merge) {
                    self.sizes.len()
                } else {
                    self.editor.dialog_page + 1
                };
                self.submit_edit(ui, EditCommand::Insert { paths, at });
            }
            ToolDialog::Extract | ToolDialog::Split => {
                self.editor.operation += 1;
                let path = paths.into_iter().next().unwrap();
                let ok = match kind {
                    ToolDialog::Extract => self.engine.extract(
                        self.document,
                        self.editor.revision,
                        self.editor.operation,
                        path,
                        self.editor.dialog_pages.clone(),
                    ),
                    _ => self.engine.split(
                        self.document,
                        self.editor.revision,
                        self.editor.operation,
                        path,
                        self.editor.dialog_page,
                    ),
                };
                self.file_started(ui, ok, "Exporting pages…");
            }
        }
    }
    fn file_started(&self, ui: &AppWindow, ok: bool, message: &str) {
        if ok {
            ui.set_file_busy(true);
            ui.set_file_message(message.into());
        } else {
            ui.set_file_message("A file operation is already running. Try again.".into());
        }
    }
    pub fn save_as(&mut self, ui: &AppWindow, path: PathBuf) {
        if !ui.get_ready() || self.edit_blocked(ui) {
            return;
        }
        self.editor.operation += 1;
        self.file_started(
            ui,
            self.engine
                .save_as(self.document, self.editor.operation, path, vec![]),
            "Saving PDF copy…",
        );
    }
    fn save(&mut self, ui: &AppWindow) {
        if !ui.get_ready() || self.edit_blocked(ui) {
            return;
        }
        self.editor.operation += 1;
        self.file_started(
            ui,
            self.engine.save(self.document, self.editor.operation),
            "Saving PDF…",
        );
    }
    fn print(&mut self, ui: &AppWindow) {
        if !ui.get_ready() || self.edit_blocked(ui) {
            return;
        }
        self.editor.operation += 1;
        let stem = self
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".into());
        let directory = std::env::temp_dir().join("pdfree-print");
        if std::fs::create_dir_all(&directory).is_err() {
            ui.set_file_message("Could not prepare a print copy in the temp folder.".into());
            return;
        }
        // The operation counter resets when a document is reopened, and print
        // copies stay on disk for the asynchronous spooler, so probe for a
        // name that does not collide with an earlier session's copy.
        let mut target = directory.join(format!("{stem}-print-{}.pdf", self.editor.operation));
        let mut unique = 1;
        while std::fs::symlink_metadata(&target).is_ok() {
            target = directory.join(format!(
                "{stem}-print-{}-{}.pdf",
                self.editor.operation, unique
            ));
            unique += 1;
        }
        self.file_started(
            ui,
            self.engine.print_copy(
                self.document,
                self.editor.revision,
                self.editor.operation,
                target,
            ),
            "Preparing to print…",
        );
    }
    fn invoke_print(&mut self, path: &std::path::Path) -> Result<(), String> {
        #[cfg(test)]
        {
            self.editor.printed.push(path.to_path_buf());
            Ok(())
        }
        #[cfg(not(test))]
        {
            print_file(path)
        }
    }
    pub fn rename(&mut self, ui: &AppWindow, name: String) {
        if !ui.get_ready() || self.edit_blocked(ui) || !ui.get_rename_active() {
            return;
        }
        let name = match validate_pdf_name(&name) {
            Ok(n) => n,
            Err(m) => {
                ui.set_file_message(m.into());
                return;
            }
        };
        self.editor.operation += 1;
        self.file_started(
            ui,
            self.engine
                .rename(self.document, self.editor.operation, name),
            "Saving edits and renaming PDF…",
        );
    }
    pub(super) fn submit_edit(&mut self, ui: &AppWindow, command: EditCommand) {
        self.editor.operation += 1;
        let ok = self.engine.edit(
            self.document,
            self.editor.revision,
            self.editor.operation,
            self.generation + 1,
            command,
        );
        self.file_started(ui, ok, "Applying change…");
        if ok {
            ui.set_page_tools_visible(false);
            ui.set_highlight_visible(false);
            ui.set_annotation_list_visible(false);
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
    fn page_now(&self) -> usize {
        if self.sizes.is_empty() {
            0
        } else {
            self.current_page.min(self.sizes.len() - 1)
        }
    }
    pub(super) fn edit_action(&mut self, ui: &AppWindow, action: &str) -> bool {
        if !matches!(
            action,
            "annotate-highlight"
                | "annotate-note"
                | "undo"
                | "redo"
                | "save"
                | "save-as"
                | "print"
                | "rename-start"
                | "rename-cancel"
                | "cancel-note"
                | "pending-save"
                | "pending-discard"
                | "pending-cancel"
                | "page-tools"
                | "close-page-tools"
                | "annotation-list"
                | "close-annotations"
                | "cancel-highlight"
                | "delete-annotation"
                | "highlight-yellow"
                | "highlight-green"
                | "highlight-blue"
                | "highlight-pink"
                | "rotate-left"
                | "rotate-right"
                | "move-page"
                | "delete-pages"
                | "insert-pdf"
                | "merge-pdf"
                | "blank-page"
                | "extract-pages"
                | "split-pdf"
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
            "close-page-tools" => {
                ui.set_page_tools_visible(false);
                ui.invoke_focus_document();
            }
            "close-annotations" => {
                ui.set_annotation_list_visible(false);
                ui.invoke_focus_document();
            }
            "cancel-highlight" => {
                ui.set_highlight_visible(false);
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
                self.save(ui);
            }
            "rename-cancel" => {
                ui.set_rename_active(false);
                ui.set_file_message("".into());
                ui.invoke_focus_document();
            }
            "delete-annotation" if ui.get_annotation_editable() => {
                if let Some((page, ordinal)) = self.editor.selected {
                    self.submit_edit(ui, EditCommand::DeleteAnnotation { page, ordinal });
                    ui.set_note_visible(false);
                }
            }
            a if a.starts_with("highlight-") && ui.get_annotation_editable() => {
                if let Some((page, ordinal)) = self.editor.selected {
                    let rgb = match a {
                        "highlight-green" => [0.3, 1.0, 0.3],
                        "highlight-blue" => [0.3, 0.6, 1.0],
                        "highlight-pink" => [1.0, 0.4, 0.7],
                        _ => [1.0, 1.0, 0.0],
                    };
                    self.submit_edit(ui, EditCommand::Color { page, ordinal, rgb });
                }
            }
            _ if self.edit_blocked(ui) => {}
            "rename-start" if ui.get_ready() => {
                ui.set_rename_text(ui.get_document_name());
                ui.set_rename_active(true);
                ui.set_file_message("Enter to save and rename • Escape to cancel".into());
                ui.invoke_focus_rename();
            }
            "save" => self.save(ui),
            "save-as" if ui.get_ready() => self.editor.save_dialog = true,
            "print" if ui.get_ready() => self.print(ui),
            "undo" if ui.get_can_undo() => self.submit_edit(ui, EditCommand::Undo),
            "redo" if ui.get_can_redo() => self.submit_edit(ui, EditCommand::Redo),
            "page-tools" if ui.get_ready() => {
                ui.set_page_range((self.page_now() + 1).to_string().into());
                ui.set_move_to((self.page_now() + 1).to_string().into());
                ui.set_page_tools_visible(true);
                ui.invoke_focus_page_tools();
            }
            "annotation-list" if ui.get_ready() => {
                ui.set_annotation_list_visible(true);
                self.edit_overlays(ui);
                ui.invoke_focus_modal();
            }
            "annotate-note" if ui.get_can_annotate() => {
                ui.set_note_placement(true);
                ui.set_file_message("Click a page to place a note.".into());
            }
            "annotate-highlight" if ui.get_can_annotate() => {
                if let Some(Annotation::Highlight { page, mut rects }) =
                    self.tools.selected_highlight()
                {
                    if let Some(size) = self.sizes.get(page) {
                        for r in &mut rects {
                            r.left = r.left.max(0.0);
                            r.top = r.top.max(0.0);
                            r.right = r.right.min(size.width);
                            r.bottom = r.bottom.min(size.height);
                        }
                        rects.retain(|r| r.right > r.left && r.bottom > r.top);
                        if !rects.is_empty() {
                            self.submit_edit(
                                ui,
                                EditCommand::Add(Annotation::Highlight { page, rects }),
                            );
                            self.tools.clear_selection(ui);
                        }
                    }
                } else {
                    ui.set_file_message("Select text on a page, then choose Highlight.".into());
                }
            }
            "rotate-left" | "rotate-right" | "delete-pages" | "extract-pages" => {
                match parse_pages(ui.get_page_range().as_str(), self.sizes.len()) {
                    Err(m) => ui.set_file_message(m.into()),
                    Ok(pages) => match action {
                        "extract-pages" if ui.get_can_copy() && ui.get_can_assemble() => {
                            self.editor.dialog_pages = pages;
                            self.editor.tool_dialog = Some(ToolDialog::Extract);
                        }
                        "delete-pages" if ui.get_can_assemble() => {
                            self.submit_edit(ui, EditCommand::DeletePages { pages })
                        }
                        "rotate-left" | "rotate-right" if ui.get_can_assemble() => self
                            .submit_edit(
                                ui,
                                EditCommand::Rotate {
                                    pages,
                                    degrees: if action == "rotate-left" { -90 } else { 90 },
                                },
                            ),
                        _ => ui.set_file_message("This PDF does not allow that operation.".into()),
                    },
                }
            }
            "move-page" if ui.get_can_assemble() => {
                match ui.get_move_to().trim().parse::<usize>() {
                    Ok(to) if to > 0 && to <= self.sizes.len() => self.submit_edit(
                        ui,
                        EditCommand::Move {
                            page: self.page_now(),
                            to: to - 1,
                        },
                    ),
                    _ => ui.set_file_message("Enter a valid destination page number.".into()),
                }
            }
            "blank-page" if ui.get_can_assemble() => self.submit_edit(
                ui,
                EditCommand::Blank {
                    at: self.page_now() + 1,
                },
            ),
            "insert-pdf" | "merge-pdf" if ui.get_can_assemble() => {
                self.editor.dialog_page = self.page_now();
                self.editor.tool_dialog = Some(if action == "insert-pdf" {
                    ToolDialog::Insert
                } else {
                    ToolDialog::Merge
                });
            }
            "split-pdf" if ui.get_can_copy() && ui.get_can_assemble() => {
                let page = self.page_now();
                if page + 1 >= self.sizes.len() {
                    ui.set_file_message("Navigate to a page before the last page to split.".into());
                } else {
                    self.editor.dialog_page = page;
                    self.editor.tool_dialog = Some(ToolDialog::Split);
                }
            }
            _ => {}
        }
        true
    }
    pub fn page_point(&mut self, ui: &AppWindow, page: usize, x: f32, y: f32) {
        if ui.get_pending_open()
            || !ui.get_note_placement()
            || !ui.get_can_annotate()
            || self.edit_blocked(ui)
        {
            return;
        }
        let scale = self.zoom * 96.0 / 72.0;
        let (x, y) = (x / scale, y / scale);
        if let Some(size) = self.sizes.get(page)
            && x.is_finite()
            && y.is_finite()
            && x >= 0.0
            && y >= 0.0
            && x < size.width
            && y < size.height
        {
            self.editor.note_target = Some((page, x, y, None));
            self.editor.selected = None;
            ui.set_note_placement(false);
            ui.set_note_read_only(false);
            ui.set_annotation_editable(false);
            ui.set_note_text("".into());
            ui.set_note_visible(true);
            ui.invoke_focus_note();
        }
    }
    pub fn annotation_click(&mut self, ui: &AppWindow, index: i32) {
        if ui.get_pending_open() || self.edit_blocked(ui) || index < 0 {
            return;
        }
        let Some(a) = self
            .editor
            .notes
            .values()
            .flatten()
            .nth(index as usize)
            .cloned()
        else {
            return;
        };
        self.editor.selected = Some((a.annotation.page(), a.ordinal));
        let editable = a.editable && ui.get_can_annotate();
        ui.set_annotation_editable(editable);
        ui.set_annotation_list_visible(false);
        match a.annotation {
            Annotation::Note { page, x, y, text } => {
                self.editor.note_target = Some((page, x, y, Some(a.ordinal)));
                ui.set_note_text(text.into());
                ui.set_note_read_only(!editable);
                ui.set_note_visible(true);
                ui.invoke_focus_note();
            }
            Annotation::Highlight { .. } => {
                ui.set_highlight_visible(true);
                ui.invoke_focus_modal();
            }
        }
    }
    pub fn note_submit(&mut self, ui: &AppWindow, text: String) {
        if ui.get_note_read_only() || ui.get_file_busy() || !ui.get_can_annotate() {
            return;
        }
        if text.trim().is_empty() || text.len() > 16384 || text.contains('\0') {
            ui.set_file_message(
                "Notes require nonempty text, at most 16,384 UTF-8 bytes, and no null characters."
                    .into(),
            );
            return;
        }
        if let Some((page, x, y, ordinal)) = self.editor.note_target {
            let command = if let Some(ordinal) = ordinal {
                EditCommand::UpdateNote {
                    page,
                    ordinal,
                    text,
                }
            } else {
                EditCommand::Add(Annotation::Note { page, x, y, text })
            };
            self.submit_edit(ui, command);
        }
    }
    pub(super) fn edit_event(&mut self, ui: &AppWindow, event: &Event) -> bool {
        match event {
            Event::Annotations {
                document,
                revision,
                page,
                annotations,
                truncated,
            } if *document == self.document
                && *revision == self.editor.revision
                && self.editor.wanted.contains(page) =>
            {
                self.editor.notes.insert(*page, annotations.clone());
                if *truncated {
                    ui.set_file_message("Some annotations exceed the page preview limit.".into());
                }
                true
            }
            Event::Changed {
                document,
                operation,
                revision,
                generation,
                info,
                dirty,
                can_undo,
                can_redo,
                focus_page,
            } if *document == self.document && *operation == self.editor.operation => {
                self.editor.revision = *revision;
                self.editor.dirty = *dirty;
                self.editor.notes.clear();
                self.editor.wanted.clear();
                self.editor.note_target = None;
                self.editor.selected = None;
                ui.set_file_busy(false);
                ui.set_dirty_document(*dirty);
                ui.set_can_undo(*can_undo);
                ui.set_can_redo(*can_redo);
                ui.set_note_visible(false);
                ui.set_highlight_visible(false);
                ui.set_file_message("Change applied. Ctrl+S to save.".into());
                self.generation = self.generation.max(*generation);
                self.sizes = vec![info.first_page; info.pages];
                self.tools.reset(ui);
                ui.set_can_copy(info.can_copy);
                ui.set_can_form(info.can_form);
                ui.set_document_signed(info.signed);
                ui.set_can_edit_forms(info.can_edit);
                ui.set_can_annotate(info.can_annotate);
                ui.set_can_assemble(info.can_assemble);
                ui.set_page_count(info.pages as i32);
                ui.set_page_numbers(ModelRc::new(VecModel::from(
                    (1..=info.pages as i32).collect::<Vec<i32>>(),
                )));
                self.cache.clear();
                self.images.clear();
                self.previews.clear();
                ui.set_tiles(ModelRc::default());
                ui.set_previews(ModelRc::default());
                ui.set_annotations(ModelRc::default());
                self.layout = Layout::new(
                    &self.sizes,
                    self.zoom,
                    (ui.get_view_width() - 14.0).max(1.0),
                );
                self.navigate(ui, (*focus_page).min(info.pages - 1));
                self.previous_view = None;
                self.dirty = true;
                self.render_error = None;
                ui.invoke_focus_document();
                true
            }
            Event::FileDone {
                document,
                operation,
                path,
                kind,
            } if *document == self.document && *operation == self.editor.operation => {
                ui.set_file_busy(false);
                if *kind == FileAction::Print {
                    ui.set_file_message(match self.invoke_print(path) {
                        Ok(()) => "Sent to the default printer.".into(),
                        Err(message) => message.into(),
                    });
                } else if matches!(
                    kind,
                    FileAction::Save | FileAction::SaveAs | FileAction::Rename
                ) {
                    self.path = Some(path.clone());
                    ui.set_document_name(
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .as_ref()
                            .into(),
                    );
                    self.editor.dirty = false;
                    ui.set_dirty_document(false);
                    ui.set_rename_active(false);
                    ui.set_file_message(
                        if *kind == FileAction::Rename {
                            "PDF saved and renamed."
                        } else {
                            "PDF saved."
                        }
                        .into(),
                    );
                    if std::mem::take(&mut self.editor.continue_after_save) {
                        self.finish_pending(ui);
                    }
                } else {
                    ui.set_file_message(format!("Pages exported to {}", path.display()).into());
                    ui.set_page_tools_visible(false);
                }
                ui.invoke_focus_document();
                true
            }
            Event::FileError {
                document,
                operation,
                message,
            } if *document == self.document && *operation == self.editor.operation => {
                ui.set_file_busy(false);
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
            self.editor.notes.retain(|p, _| wanted.contains(p));
            self.editor.wanted = wanted.clone();
            self.engine
                .annotation_pages(self.document, self.editor.revision, wanted);
        }
        self.edit_overlays(ui);
    }
    fn edit_overlays(&self, ui: &AppWindow) {
        let scale = self.zoom * 96.0 / 72.0;
        let mut surfaces = Vec::new();
        let mut items = Vec::new();
        for (index, a) in self.editor.notes.values().flatten().enumerate() {
            let p = a.annotation.page();
            let Some(page) = self.layout.pages.get(p) else {
                continue;
            };
            let label = match &a.annotation {
                Annotation::Highlight { .. } => "Highlight".to_string(),
                Annotation::Note { x, y, text, .. } => {
                    surfaces.push(AnnotationSurface {
                        x: page.x + x * scale,
                        y: page.y + y * scale,
                        w: 20.0 * scale,
                        h: 20.0 * scale,
                        kind: 2,
                        label: "Note".into(),
                        index: index as i32,
                    });
                    format!("Note: {}", text.chars().take(70).collect::<String>())
                }
            };
            if p == self.page_now() {
                items.push(AnnotationItem {
                    index: index as i32,
                    label: label.into(),
                    editable: a.editable && ui.get_can_annotate(),
                });
            }
        }
        ui.set_annotations(ModelRc::new(VecModel::from(surfaces)));
        ui.set_annotation_items(ModelRc::new(VecModel::from(items)));
    }
}

/// Ask Windows to print the copy with the default PDF handler. The verb is
/// asynchronous, so the temp file is left for the spooler to read.
#[cfg(all(windows, not(test)))]
fn print_file(path: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    unsafe extern "system" {
        fn ShellExecuteW(
            hwnd: *mut std::ffi::c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    let operation: Vec<u16> = "print\0".encode_utf16().collect();
    let file: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
        )
    };
    if result > 32 {
        Ok(())
    } else {
        Err(format!(
            "Windows could not start printing (error {result}). The print copy is at {}",
            path.display()
        ))
    }
}

#[cfg(not(any(windows, test)))]
fn print_file(path: &std::path::Path) -> Result<(), String> {
    Err(format!(
        "Printing is only supported on Windows. The print copy is at {}",
        path.display()
    ))
}
