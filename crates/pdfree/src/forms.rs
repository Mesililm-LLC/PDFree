use super::State;
use crate::{AppWindow, FormOption, FormSurface};
use pdf_engine::{EditCommand, Event};
use slint::{ModelRc, VecModel};
use std::collections::BTreeMap;
use viewer_core::{FieldKind, FieldRef, FieldSpec, FormEdit, FormField, FormValue, TextRect};

#[derive(Default)]
pub(crate) struct Forms {
    pub(super) records: BTreeMap<usize, Vec<FormField>>,
    wanted: Vec<usize>,
    selected: Option<FieldRef>,
    pending: Option<(usize, String, Option<i32>)>,
    pending_properties: Option<FieldSpec>,
    draw: Option<(usize, f32, f32)>,
    drag: Option<(FormField, f32, f32, bool)>,
    review_only: bool,
    counter: u64,
    pending_action: Option<String>,
    // (previous, page, pages remaining, review-only)
    navigation: Option<(bool, usize, usize, bool)>,
    blur: i32,
    close_after_commit: bool,
    open_after_commit: Option<std::path::PathBuf>,
    review_pages: std::collections::VecDeque<usize>,
    signature_strokes: Vec<Vec<[f32; 2]>>,
    signature_image: Option<Vec<u8>>,
    signature_pending: bool,
    // Page to auto-detect once its field records arrive after opening Form.
    auto_detect_page: Option<usize>,
    // Last overlay model sent to the UI; replacing an identical model would
    // recreate the focused inline editor and drop keyboard focus.
    last_surfaces: std::cell::RefCell<Vec<FormSurface>>,
    certificate: Option<std::path::PathBuf>,
    signed_target: Option<std::path::PathBuf>,
}
impl State {
    pub(super) fn reset_forms(&mut self, ui: &AppWindow) {
        self.forms = Default::default();
        self.forms.counter = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        ui.set_form_draw(false);
        ui.set_form_draft_visible(false);
        ui.set_form_detecting(false);
        ui.set_signature_visible(false);
        ui.set_digital_visible(false);
        ui.set_digital_signing(false);
        ui.set_digital_password("".into());
        ui.set_document_signed(false);
        ui.set_form_mode(0);
        ui.set_form_selected(false);
        ui.set_form_surfaces(ModelRc::default());
        ui.set_can_form(false);
        ui.set_can_edit_forms(false);
    }
    fn selected_form(&self) -> Option<FormField> {
        self.forms
            .records
            .values()
            .flatten()
            .find(|f| Some(f.id) == self.forms.selected)
            .cloned()
    }
    fn form_error(&self, ui: &AppWindow, message: impl Into<String>) {
        ui.set_form_status(message.into().into());
    }
    fn form_submit(&mut self, ui: &AppWindow, edit: FormEdit) {
        if !ui.get_file_busy() {
            self.submit_edit(ui, EditCommand::Form(edit));
        }
    }
    fn form_commit(&mut self, ui: &AppWindow) -> bool {
        let Some(f) = self.selected_form() else {
            return false;
        };
        if !f.editable || !f.spec.kind.is_text() || f.value.text() == ui.get_form_value().as_str() {
            return false;
        }
        if ui.get_file_busy() {
            return true;
        }
        // Dates typed in a common variant format are corrected to the
        // field's format as the field loses focus.
        if f.spec.kind == FieldKind::Date
            && let Some(normalized) =
                viewer_core::normalize_date(ui.get_form_value().as_str(), &f.spec.date_format)
            && normalized != ui.get_form_value().as_str()
        {
            ui.set_form_value(normalized.into());
        }
        let value = FormValue::Text(ui.get_form_value().to_string());
        if let Err(e) = f.spec.validate_value(&value) {
            self.form_error(ui, e);
            return true;
        }
        self.forms.pending = Some((f.spec.page, f.spec.name.clone(), Some(f.id.widget)));
        self.form_submit(ui, FormEdit::SetValue { id: f.id, value });
        true
    }
    pub(super) fn form_blur(&mut self, ui: &AppWindow) {
        if self.forms.blur != ui.get_form_blur() {
            self.forms.blur = ui.get_form_blur();
            self.form_commit(ui);
        }
    }
    pub(super) fn form_before_close(&mut self, ui: &AppWindow) -> bool {
        if ui.get_file_busy() {
            return true;
        }
        if self.form_commit(ui) {
            if ui.get_file_busy() {
                self.forms.close_after_commit = true;
            }
            true
        } else {
            false
        }
    }
    pub(super) fn form_before_open(&mut self, ui: &AppWindow, path: &std::path::Path) -> bool {
        if ui.get_file_busy() {
            return true;
        }
        if self.form_commit(ui) {
            if ui.get_file_busy() {
                self.forms.open_after_commit = Some(path.to_path_buf());
            }
            true
        } else {
            false
        }
    }
    fn selected_spec(&self, ui: &AppWindow, base: Option<&FieldSpec>) -> Result<FieldSpec, String> {
        let kind = *FieldKind::ALL
            .get(ui.get_form_kind() as usize)
            .ok_or("Invalid field type")?;
        let mut s = base.cloned().unwrap_or(FieldSpec {
            page: 0,
            name: String::new(),
            label: String::new(),
            kind,
            rect: TextRect {
                left: 0.,
                top: 0.,
                right: 100.,
                bottom: 24.,
            },
            required: false,
            read_only: false,
            options: vec![],
            group: String::new(),
            review: false,
            date_format: "YYYY-MM-DD".into(),
            decimals: 2,
        });
        s.kind = kind;
        s.label = ui.get_form_label().to_string();
        s.required = ui.get_form_required();
        s.read_only = ui.get_form_readonly();
        s.options = ui
            .get_form_options_text()
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        s.group = ui.get_form_group().to_string();
        s.date_format = ui.get_form_date().to_string();
        s.decimals = ui
            .get_form_decimals()
            .parse()
            .map_err(|_| "Enter 0–12 decimal places")?;
        Ok(s)
    }
    fn new_form_name(&mut self) -> String {
        loop {
            self.forms.counter += 1;
            let name = format!("PDFree_Field_{}", self.forms.counter);
            if !self
                .forms
                .records
                .values()
                .flatten()
                .any(|f| f.spec.name == name)
            {
                return name;
            }
        }
    }
    pub(super) fn form_action(&mut self, ui: &AppWindow, action: &str) -> bool {
        if action == "form-cancel-detection" {
            self.engine.cancel_detection();
            return true;
        }
        if action == "form-digital-stop" {
            self.engine.cancel_signing();
            return true;
        }
        if !action.starts_with("form-") {
            if ui.get_form_mode() > 0 && self.form_commit(ui) {
                if ui.get_file_busy() {
                    self.forms.pending_action = Some(action.into());
                }
                return true;
            }
            if matches!(action, "annotate-note" | "annotate-highlight") {
                ui.set_form_mode(0);
                ui.set_form_draw(false);
            }
            return false;
        }
        if self.dialog_open {
            return true;
        }
        if ui.get_file_busy() {
            return true;
        }
        match action {
            "form-digital-cancel" => {
                ui.set_digital_visible(false);
                ui.set_digital_password("".into());
                self.forms.certificate = None;
                self.forms.signed_target = None;
            }
            "form-digital-apply" => self.apply_digital_signature(ui),
            "form-sign-type" | "form-sign-draw" => {
                if let Some(f) = self.selected_form()
                    && f.spec.kind == FieldKind::VisualSignature
                    && f.editable
                {
                    self.signature_reset(ui);
                    ui.set_file_message("".into());
                    self.form_error(
                        ui,
                        if action == "form-sign-type" {
                            "Enter the visible signature text."
                        } else {
                            "Draw your visual signature in the preview."
                        },
                    );
                    ui.set_signature_mode(if action == "form-sign-type" { 0 } else { 1 });
                    ui.set_signature_visible(true);
                }
            }
            "form-sign-reset" => self.signature_reset(ui),
            "form-sign-cancel" => {
                ui.set_signature_visible(false);
                self.signature_reset(ui);
            }
            "form-sign-clear" => {
                if let Some(f) = self.selected_form() {
                    self.form_submit(
                        ui,
                        FormEdit::SetVisualSignature {
                            id: f.id,
                            mark: viewer_core::SignatureMark::Empty,
                        },
                    );
                }
            }
            "form-sign-apply" => {
                if let Some(f) = self.selected_form() {
                    let mark = match ui.get_signature_mode() {
                        0 => viewer_core::SignatureMark::Typed(ui.get_signature_text().to_string()),
                        1 => viewer_core::SignatureMark::Strokes(
                            self.forms.signature_strokes.clone(),
                        ),
                        _ => viewer_core::SignatureMark::Image(
                            self.forms.signature_image.clone().unwrap_or_default(),
                        ),
                    };
                    self.forms.signature_pending = true;
                    self.forms.pending = Some((f.spec.page, f.spec.name, Some(f.id.widget)));
                    self.form_submit(ui, FormEdit::SetVisualSignature { id: f.id, mark });
                }
            }
            "form-fill" | "form-prepare" => {
                if action == "form-prepare" && !ui.get_can_edit_forms() {
                    self.form_error(
                        ui,
                        "This document does not permit creating or changing form fields.",
                    );
                    return true;
                }
                if self.form_commit(ui) {
                    if ui.get_file_busy() {
                        self.forms.pending_action = Some(action.into());
                    }
                    return true;
                }
                self.forms.review_only = false;
                self.forms.navigation = None;
                // One form mode: filling and authoring share mode 1. The
                // legacy "form-prepare" entry only arms field drawing.
                let opening = ui.get_form_mode() == 0;
                ui.set_form_mode(1);
                ui.set_form_draw(action == "form-prepare");
                if opening && action == "form-fill" && !self.viewport.sizes.is_empty() {
                    self.forms.auto_detect_page = Some(
                        self.viewport
                            .current_page
                            .min(self.viewport.sizes.len() - 1),
                    );
                }
                self.forms.wanted.clear();
                self.form_error(
                    ui,
                    "Click a field on the page to type in it, or use Field tools to add fields.",
                );
            }
            "form-close" => {
                if self.form_commit(ui) {
                    if ui.get_file_busy() {
                        self.forms.pending_action = Some(action.into());
                    }
                } else {
                    self.forms.auto_detect_page = None;
                    ui.set_form_mode(0);
                    ui.set_form_draw(false);
                    self.forms.last_surfaces.borrow_mut().clear();
                    ui.set_form_surfaces(ModelRc::default());
                }
            }
            "form-new" => {
                if self.form_commit(ui) {
                    if ui.get_file_busy() {
                        self.forms.pending_action = Some(action.into());
                    }
                } else {
                    self.forms.selected = None;
                    ui.set_form_selected(false);
                    ui.set_form_draw(true);
                    ui.set_form_name("".into());
                    self.form_error(
                        ui,
                        "Drag a rectangle inside a page. Options for choices: one per line.",
                    );
                }
            }
            "form-commit" => {
                self.form_commit(ui);
            }
            "form-toggle" => {
                if let Some(f) = self.selected_form()
                    && f.editable
                {
                    let text = if ui.get_form_checked() {
                        f.export_value.clone()
                    } else {
                        "Off".into()
                    };
                    self.forms.pending = Some((f.spec.page, f.spec.name, Some(f.id.widget)));
                    self.form_submit(
                        ui,
                        FormEdit::SetValue {
                            id: f.id,
                            value: FormValue::Text(text),
                        },
                    );
                }
            }
            "form-choice" => {
                if let Some(f) = self.selected_form()
                    && let Some(v) = f.export_values.get(ui.get_form_choice() as usize)
                {
                    self.forms.pending =
                        Some((f.spec.page, f.spec.name.clone(), Some(f.id.widget)));
                    self.form_submit(
                        ui,
                        FormEdit::SetValue {
                            id: f.id,
                            value: FormValue::Text(v.clone()),
                        },
                    );
                }
            }
            "form-next" | "form-previous" | "form-review" => {
                if self.form_commit(ui) {
                    if ui.get_file_busy() {
                        self.forms.pending_action = Some(action.into());
                    }
                    return true;
                }
                if action == "form-review" {
                    self.forms.review_only = true;
                    while let Some(page) = self.forms.review_pages.front().copied() {
                        if let Some(records) = self.forms.records.get(&page) {
                            if records.iter().any(|f| f.spec.review) {
                                break;
                            }
                            self.forms.review_pages.pop_front();
                        } else {
                            self.navigate(ui, page);
                            self.forms.wanted.clear();
                            self.forms.pending_action = Some("form-review".into());
                            return true;
                        }
                    }
                }
                if action == "form-review" {
                    if let Some(page) = self.forms.review_pages.front().copied() {
                        self.forms.navigation =
                            Some((false, page, self.forms.review_pages.len(), true));
                    }
                    self.form_navigate(ui, false, true, true);
                } else {
                    // Ordinary Tab/Previous/Next always walks every field
                    // in reading order; it also exits the review queue.
                    self.forms.review_only = false;
                    self.form_navigate(ui, action == "form-previous", false, false);
                }
            }

            "form-enter-next" | "form-enter-previous" => {
                let review = self.forms.navigation.is_some_and(|(.., r)| r);
                self.form_navigate(ui, action == "form-enter-previous", true, review);
            }

            "form-properties" => {
                if let Some(f) = self.selected_form() {
                    match self.selected_spec(ui, Some(&f.spec)).and_then(|s| {
                        s.validate()?;
                        Ok(s)
                    }) {
                        Ok(spec) => {
                            if self.form_commit(ui) {
                                if ui.get_file_busy() {
                                    self.forms.pending_properties = Some(spec);
                                }
                            } else {
                                self.forms.pending =
                                    Some((f.spec.page, f.spec.name, Some(f.id.widget)));
                                self.form_submit(ui, FormEdit::Update { id: f.id, spec });
                            }
                        }
                        Err(e) => self.form_error(ui, e),
                    }
                }
            }
            "form-delete" => {
                if let Some(f) = self.selected_form() {
                    self.forms.selected = None;
                    self.form_submit(ui, FormEdit::Delete(f.id));
                }
            }
            "form-confirm" => {
                if let Some(f) = self.selected_form() {
                    self.forms.pending_action = Some("form-review".into());
                    self.form_submit(ui, FormEdit::Confirm(f.id));
                }
            }
            "form-duplicate" => {
                if let Some(f) = self.selected_form() {
                    let mut spec = f.spec;
                    spec.name = self.new_form_name();
                    spec.rect.left += 12.;
                    spec.rect.right += 12.;
                    spec.rect.top += 12.;
                    spec.rect.bottom += 12.;
                    self.forms.pending = Some((spec.page, spec.name.clone(), None));
                    self.form_submit(ui, FormEdit::Create(vec![spec]));
                }
            }
            "form-detect" => {
                self.form_error(ui, "Preparing field detection…");
                self.start_form_detection(ui);
            }
            _ => {}
        }
        self.viewport.dirty = true;
        true
    }
    pub fn form_select(&mut self, ui: &AppWindow, index: i32) {
        if ui.get_file_busy() {
            return;
        }
        let Some(f) = self
            .forms
            .records
            .values()
            .flatten()
            .nth(index as usize)
            .cloned()
        else {
            return;
        };
        if self.forms.selected != Some(f.id) && self.form_commit(ui) {
            if ui.get_file_busy() {
                self.forms.pending = Some((f.spec.page, f.spec.name, Some(f.id.widget)));
            }
            return;
        }
        if self.forms.selected != Some(f.id) {
            self.forms.drag = None;
            ui.set_form_draft_visible(false);
        }
        self.forms.selected = Some(f.id);
        ui.set_form_password(f.password);
        ui.set_form_selected(true);
        ui.set_form_draw(false);
        let kind = FieldKind::ALL
            .iter()
            .position(|k| *k == f.spec.kind)
            .unwrap_or(0) as i32;
        ui.set_form_kind(kind);
        ui.set_form_value_kind(kind);
        ui.set_form_label(f.spec.label.clone().into());
        ui.set_form_name(f.spec.name.clone().into());
        ui.set_form_value(f.value.text().into());
        ui.set_form_checked(f.value.text() == f.export_value);
        ui.set_form_required(f.spec.required);
        ui.set_form_readonly(f.spec.read_only);
        ui.set_form_options_text(f.spec.options.join("\n").into());
        ui.set_form_group(f.spec.group.clone().into());
        ui.set_form_date(f.spec.date_format.into());
        ui.set_form_decimals(f.spec.decimals.to_string().into());
        ui.set_form_editable(f.editable);
        ui.set_form_authorable(f.authorable);
        ui.set_form_signable(f.signable);
        ui.set_form_choice_labels(ModelRc::new(VecModel::from(
            f.spec
                .options
                .iter()
                .map(|s| s.as_str().into())
                .collect::<Vec<_>>(),
        )));
        ui.set_form_choice(
            f.export_values
                .iter()
                .position(|s| *s == f.value.text())
                .map_or(-1, |i| i as i32),
        );
        let selected = match &f.value {
            FormValue::Selection(v) => v.clone(),
            FormValue::Text(s) => vec![s.clone()],
        };
        ui.set_form_options(ModelRc::new(VecModel::from(
            f.spec
                .options
                .iter()
                .enumerate()
                .map(|(i, s)| FormOption {
                    label: s.as_str().into(),
                    selected: f.export_values.get(i).is_some_and(|s| selected.contains(s)),
                })
                .collect::<Vec<_>>(),
        )));
        self.form_error(
            ui,
            if !f.reason.is_empty() {
                f.reason
            } else if f.spec.review {
                "Review this detected field: confirm its type and rectangle.".into()
            } else {
                format!("{} · page {}", f.spec.kind.label(), f.spec.page + 1)
            },
        );
        if self.viewport.current_page != f.spec.page {
            self.navigate(ui, f.spec.page);
        }
        self.form_overlays(ui);
        if ui.get_form_mode() == 1 && f.spec.kind.is_text() {
            ui.set_form_focus(ui.get_form_focus().wrapping_add(1));
        }
    }
    fn form_navigate(&mut self, ui: &AppWindow, previous: bool, entering: bool, review: bool) {
        let page = self.forms.navigation.map_or_else(
            || {
                self.forms
                    .selected
                    .map_or(self.viewport.current_page, |id| id.page)
            },
            |(_, p, _, _)| p,
        );
        let mut on_page = self
            .forms
            .records
            .values()
            .flatten()
            .enumerate()
            .filter(|(_, f)| f.spec.page == page && (!review || f.spec.review))
            .map(|(i, f)| (i, f.id, f.spec.rect))
            .collect::<Vec<_>>();
        // Reading order: rows top-to-bottom, fields left-to-right within a
        // row, regardless of widget or review ordering in the document.
        on_page.sort_by(|a, b| {
            a.2.top
                .total_cmp(&b.2.top)
                .then(a.2.left.total_cmp(&b.2.left))
        });
        let mut rows: Vec<Vec<(usize, viewer_core::FieldRef, TextRect)>> = Vec::new();
        for item in on_page {
            let same_row = rows.last().is_some_and(|row: &Vec<_>| {
                let anchor = row[0].2;
                let overlap = anchor.bottom.min(item.2.bottom) - anchor.top.max(item.2.top);
                let shortest = (anchor.bottom - anchor.top).min(item.2.bottom - item.2.top);
                shortest > 0. && overlap > 0.5 * shortest
            });
            if same_row {
                rows.last_mut().unwrap().push(item);
            } else {
                rows.push(vec![item]);
            }
        }
        let eligible = rows
            .into_iter()
            .flat_map(|mut row| {
                row.sort_by(|a, b| {
                    a.2.left
                        .total_cmp(&b.2.left)
                        .then(a.2.top.total_cmp(&b.2.top))
                });
                row.into_iter().map(|(i, id, _)| (i, id))
            })
            .collect::<Vec<_>>();
        let position = if entering {
            None
        } else {
            eligible
                .iter()
                .position(|(_, id)| Some(*id) == self.forms.selected)
        };
        let target = if previous {
            position.and_then(|i| i.checked_sub(1)).or_else(|| {
                position
                    .is_none()
                    .then_some(eligible.len().saturating_sub(1))
            })
        } else {
            Some(position.map_or(0, |i| i + 1))
        };
        if let Some((index, _)) = target.and_then(|i| eligible.get(i)) {
            self.forms.navigation = None;
            self.form_select(ui, *index as i32);
            return;
        }
        let pages = if review {
            self.forms.review_pages.iter().copied().collect::<Vec<_>>()
        } else {
            (0..ui.get_page_count().max(0) as usize).collect()
        };
        let remaining = self.forms.navigation.map_or(pages.len(), |(_, _, n, _)| n);
        if pages.is_empty() || remaining == 0 {
            self.forms.navigation = None;
            self.form_error(
                ui,
                if review {
                    "No more uncertain fields."
                } else {
                    "No form fields found."
                },
            );
            return;
        }
        let index = pages.iter().position(|p| *p == page).unwrap_or(0);
        let next = pages[if previous {
            (index + pages.len() - 1) % pages.len()
        } else {
            (index + 1) % pages.len()
        }];
        self.forms.navigation = Some((previous, next, remaining - 1, review));
        let action = if previous {
            "form-enter-previous"
        } else {
            "form-enter-next"
        };
        if self.forms.records.contains_key(&next) {
            self.form_navigate(ui, previous, true, review);
        } else {
            self.navigate(ui, next);
            self.forms.wanted.clear();
            if ui.get_file_busy() {
                self.forms.pending_action = Some(action.into());
            }
        }
    }

    pub fn form_option(&mut self, ui: &AppWindow, index: i32, selected: bool) {
        if ui.get_file_busy() {
            return;
        }
        if let Some(f) = self.selected_form()
            && f.editable
            && let Some(v) = f.export_values.get(index as usize)
        {
            let mut values = match f.value {
                FormValue::Selection(v) => v,
                FormValue::Text(s) => {
                    if s.is_empty() {
                        vec![]
                    } else {
                        vec![s]
                    }
                }
            };
            if !f.multi_select {
                values.clear();
            }
            values.retain(|s| s != v);
            if selected {
                values.push(v.clone());
            }
            self.forms.pending = Some((f.spec.page, f.spec.name, Some(f.id.widget)));
            self.form_submit(
                ui,
                FormEdit::SetValue {
                    id: f.id,
                    value: FormValue::Selection(values),
                },
            );
        }
    }
    fn signature_reset(&mut self, ui: &AppWindow) {
        self.forms.signature_strokes.clear();
        self.forms.signature_image = None;
        ui.set_signature_text("".into());
        ui.set_signature_preview(slint::Image::default());
    }
    pub fn signature_can_import(&self) -> bool {
        self.selected_form()
            .is_some_and(|f| f.editable && f.spec.kind == FieldKind::VisualSignature)
    }
    pub fn certificate_can_import(&self, ui: &AppWindow) -> bool {
        if ui.get_dirty_document() {
            self.form_error(
                ui,
                "Save and review your edits before signing with a certificate.",
            );
            return false;
        }
        self.selected_form().is_some_and(|f| f.signable) && ui.get_can_form() && !ui.get_file_busy()
    }
    pub fn certificate_dialog_result(
        &mut self,
        ui: &AppWindow,
        document: u64,
        certificate: Option<std::path::PathBuf>,
        target: Option<std::path::PathBuf>,
    ) {
        if document != self.document.document || !self.certificate_can_import(ui) {
            return;
        }
        if let (Some(certificate), Some(target)) = (certificate, target) {
            ui.set_digital_target(format!("Output: {}", target.display()).into());
            self.forms.certificate = Some(certificate);
            self.forms.signed_target = Some(target);
            ui.set_digital_password("".into());
            ui.set_digital_visible(true);
        }
    }
    fn apply_digital_signature(&mut self, ui: &AppWindow) {
        if !self.certificate_can_import(ui) {
            return;
        }
        let (Some(f), Some(certificate), Some(target)) = (
            self.selected_form(),
            self.forms.certificate.take(),
            self.forms.signed_target.take(),
        ) else {
            return;
        };
        let password = pdf_engine::SecretPassword::new(ui.get_digital_password().to_string());
        ui.set_digital_password("".into());
        ui.set_digital_visible(false);
        self.editor.operation += 1;
        let ok = self.engine.sign(
            self.document.document,
            self.editor.revision,
            self.editor.operation,
            pdf_engine::SigningRequest {
                field: f.id,
                certificate,
                target,
                password,
            },
        );
        if ok {
            ui.set_file_busy(true);
            ui.set_digital_signing(true);
            self.form_error(
                ui,
                "Signing the saved PDF locally and verifying the new output…",
            );
        }
    }
    pub fn signature_image_result(
        &mut self,
        ui: &AppWindow,
        document: u64,
        path: Option<std::path::PathBuf>,
    ) {
        if document != self.document.document || !self.signature_can_import() {
            return;
        }
        let Some(path) = path else {
            return;
        };
        let result = (|| -> Result<Vec<u8>, String> {
            use std::io::Read;
            let mut bytes = vec![];
            std::fs::File::open(&path)
                .map_err(|e| e.to_string())?
                .take(4 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err("Choose an image of at most 4 MiB.".into());
            }
            viewer_core::signature_image_size(&bytes)?;
            Ok(bytes)
        })();
        match result {
            Ok(bytes) => match slint::Image::load_from_path(&path) {
                Ok(preview) => {
                    self.signature_reset(ui);
                    self.forms.signature_image = Some(bytes);
                    ui.set_signature_preview(preview);
                    ui.set_signature_mode(2);
                    ui.set_signature_visible(true);
                }
                Err(e) => self.form_error(ui, e.to_string()),
            },
            Err(e) => self.form_error(ui, e),
        }
    }
    pub fn signature_pointer(&mut self, ui: &AppWindow, x: f32, y: f32, phase: i32) {
        if !ui.get_signature_visible() || ui.get_signature_mode() != 1 {
            return;
        }
        let p = [x.clamp(0., 1.), y.clamp(0., 1.)];
        if phase == 0 && self.forms.signature_strokes.len() < 128 {
            self.forms.signature_strokes.push(vec![p]);
        }
        if phase == 1
            && self
                .forms
                .signature_strokes
                .iter()
                .map(Vec::len)
                .sum::<usize>()
                < 16_384
            && let Some(line) = self.forms.signature_strokes.last_mut()
            && line
                .last()
                .is_none_or(|last| (last[0] - p[0]).abs() + (last[1] - p[1]).abs() > 0.001)
        {
            line.push(p);
        }
        let mut pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(420, 140);
        for pixel in pixels.make_mut_slice() {
            *pixel = slint::Rgba8Pixel {
                r: 251,
                g: 252,
                b: 254,
                a: 255,
            };
        }
        for line in &self.forms.signature_strokes {
            for pair in line.windows(2) {
                let (x1, y1) = (pair[0][0] * 419., pair[0][1] * 139.);
                let (x2, y2) = (pair[1][0] * 419., pair[1][1] * 139.);
                let steps = ((x2 - x1).abs().max((y2 - y1).abs()).ceil() as usize).max(1);
                for step in 0..=steps {
                    let t = step as f32 / steps as f32;
                    let x = (x1 + (x2 - x1) * t) as i32;
                    let y = (y1 + (y2 - y1) * t) as i32;
                    for dx in -1..=1 {
                        for dy in -1..=1 {
                            if (0..420).contains(&(x + dx)) && (0..140).contains(&(y + dy)) {
                                pixels.make_mut_slice()[((y + dy) * 420 + x + dx) as usize] =
                                    slint::Rgba8Pixel {
                                        r: 12,
                                        g: 36,
                                        b: 80,
                                        a: 255,
                                    };
                            }
                        }
                    }
                }
            }
        }
        ui.set_signature_preview(slint::Image::from_rgba8(pixels));
    }
    pub(super) fn form_pointer(&mut self, ui: &AppWindow, x: f32, y: f32, phase: i32) {
        if ui.get_form_mode() == 0 || !ui.get_form_draw() || ui.get_file_busy() {
            return;
        }
        if phase == 3 {
            self.forms.draw = None;
            return;
        }
        let scale = self.viewport.zoom * 96. / 72.;
        if phase == 0 {
            self.forms.draw = self
                .viewport
                .layout
                .pages
                .iter()
                .enumerate()
                .find(|(_, p)| x >= p.x && y >= p.y && x <= p.x + p.width && y <= p.y + p.height)
                .map(|(i, p)| (i, (x - p.x) / scale, (y - p.y) / scale));
        }
        if matches!(phase, 0 | 1)
            && let Some((page, sx, sy)) = self.forms.draw
        {
            let p = self.viewport.layout.pages[page];
            let ex = ((x - p.x) / scale).clamp(0., p.width / scale);
            let ey = ((y - p.y) / scale).clamp(0., p.height / scale);
            ui.set_form_draft_x(p.x + sx.min(ex) * scale);
            ui.set_form_draft_y(p.y + sy.min(ey) * scale);
            ui.set_form_draft_w((ex - sx).abs() * scale);
            ui.set_form_draft_h((ey - sy).abs() * scale);
            ui.set_form_draft_visible(true);
        }
        if matches!(phase, 2 | 3) {
            ui.set_form_draft_visible(false);
        }
        if phase == 2
            && let Some((page, sx, sy)) = self.forms.draw.take()
        {
            let p = self.viewport.layout.pages[page];
            let ex = ((x - p.x) / scale).clamp(0., p.width / scale);
            let ey = ((y - p.y) / scale).clamp(0., p.height / scale);
            match self.selected_spec(ui, None) {
                Ok(mut spec) => {
                    spec.page = page;
                    spec.name = self.new_form_name();
                    spec.rect = TextRect {
                        left: sx.min(ex),
                        top: sy.min(ey),
                        right: sx.max(ex),
                        bottom: sy.max(ey),
                    };
                    if let Err(e) = spec.validate() {
                        self.form_error(ui, e);
                        return;
                    }
                    self.forms.pending = Some((page, spec.name.clone(), None));
                    self.form_submit(ui, FormEdit::Create(vec![spec]));
                }
                Err(e) => self.form_error(ui, e),
            }
        }
    }
    pub fn form_drag(
        &mut self,
        ui: &AppWindow,
        index: i32,
        x: f32,
        y: f32,
        phase: i32,
        resize: bool,
    ) {
        if ui.get_form_mode() == 0 || ui.get_file_busy() {
            if phase == 0 && ui.get_form_mode() > 0 {
                self.form_select(ui, index);
            }
            return;
        }
        if phase == 0 {
            // Resolve the clicked field first so a deferred selection
            // (previous draft still committing) can never toggle it.
            let target = self
                .forms
                .records
                .values()
                .flatten()
                .nth(index as usize)
                .cloned();
            // A click that selects a field must never also drag it: only a
            // field that was already selected can be moved by this press.
            let was_selected = target
                .as_ref()
                .is_some_and(|f| self.forms.selected == Some(f.id));
            self.form_select(ui, index);
            // Clicking a checkbox or radio on the page toggles it directly.
            if !ui.get_file_busy()
                && let Some(f) = &target
                && self.forms.selected == Some(f.id)
                && f.editable
                && matches!(f.spec.kind, FieldKind::Checkbox | FieldKind::Radio)
            {
                let checked = f.value.text() == f.export_value;
                let text = if f.spec.kind == FieldKind::Checkbox && checked {
                    "Off".to_string()
                } else {
                    f.export_value.clone()
                };
                ui.set_form_checked(text != "Off");
                self.forms.pending = Some((f.spec.page, f.spec.name.clone(), Some(f.id.widget)));
                self.form_submit(
                    ui,
                    FormEdit::SetValue {
                        id: f.id,
                        value: FormValue::Text(text),
                    },
                );
                return;
            }
            if was_selected
                && let Some(f) = self.selected_form()
                && f.authorable
                && target.as_ref().is_some_and(|t| t.id == f.id)
            {
                self.forms.drag = Some((f, x, y, resize));
            }
        }
        // A drag belongs to one field; events naming another field (or a
        // cancellation) end it without moving anything.
        if matches!(phase, 1..=3)
            && let Some((f, ..)) = &self.forms.drag
            && self
                .forms
                .records
                .values()
                .flatten()
                .nth(index as usize)
                .is_none_or(|t| t.id != f.id)
        {
            self.forms.drag = None;
            ui.set_form_draft_visible(false);
            return;
        }
        if phase == 3 {
            self.forms.drag = None;
        }
        if phase == 1
            && let Some((f, sx, sy, resize)) = &self.forms.drag
        {
            let p = self.viewport.layout.pages[f.spec.page];
            let scale = self.viewport.zoom * 96. / 72.;
            let (dx, dy) = ((x - *sx) / scale, (y - *sy) / scale);
            let mut r = f.spec.rect;
            if *resize {
                r.right += dx;
                r.bottom += dy;
            } else {
                r.left += dx;
                r.right += dx;
                r.top += dy;
                r.bottom += dy;
            }
            ui.set_form_draft_x(p.x + r.left * scale);
            ui.set_form_draft_y(p.y + r.top * scale);
            ui.set_form_draft_w((r.right - r.left).max(0.) * scale);
            ui.set_form_draft_h((r.bottom - r.top).max(0.) * scale);
            ui.set_form_draft_visible(true);
        }
        if matches!(phase, 2 | 3) {
            ui.set_form_draft_visible(false);
        }
        if phase == 2
            && let Some((f, sx, sy, resize)) = self.forms.drag.take()
        {
            let scale = self.viewport.zoom * 96. / 72.;
            let (dx, dy) = ((x - sx) / scale, (y - sy) / scale);
            if dx.abs() + dy.abs() < 1. {
                return;
            }
            let mut spec = f.spec;
            let r = &mut spec.rect;
            if resize {
                r.right += dx;
                r.bottom += dy;
            } else {
                r.left += dx;
                r.right += dx;
                r.top += dy;
                r.bottom += dy;
            }
            self.forms.pending = Some((spec.page, spec.name.clone(), None));
            self.form_submit(ui, FormEdit::Update { id: f.id, spec });
        }
    }
    pub(super) fn form_event(&mut self, ui: &AppWindow, event: &Event) -> bool {
        match event {
            Event::Signed {
                document,
                operation,
                path,
                integrity,
            } if *document == self.document.document && *operation == self.editor.operation => {
                ui.set_digital_signing(false);
                ui.set_file_busy(false);
                self.open(ui, path.clone());
                ui.set_file_message(format!("Digital signature integrity verified · {} · certificate trust not evaluated",integrity.signer).into());
                true
            }
            Event::DetectionProgress {
                document,
                operation,
                processed,
                total,
            } if *document == self.document.document && *operation == self.editor.operation => {
                self.form_error(ui,format!("Recognizing fields: {processed}/{total} pages. Uncertain fields receive a second local pass."));
                false
            }
            Event::DetectionDone {
                document,
                operation,
                count,
                review_pages,
            } if *document == self.document.document && *operation == self.editor.operation => {
                ui.set_file_busy(false);
                ui.set_form_detecting(false);
                self.forms.review_pages = review_pages.iter().copied().collect();
                self.form_error(
                    ui,
                    if review_pages.is_empty() && *count > 0 {
                        format!("Created {count} fields. Click a field on the page to type in it.")
                    } else {
                        format!(
                            "Created {count} fields. {} page(s) need review.",
                            review_pages.len()
                        )
                    },
                );
                if let Some(page) = review_pages.first() {
                    self.forms.review_only = true;
                    self.navigate(ui, *page);
                    self.forms.wanted.clear();
                    self.forms.pending_action = Some("form-review".into());
                }
                true
            }
            Event::Changed { document, .. } if *document == self.document.document => {
                if self.forms.signature_pending {
                    self.forms.signature_pending = false;
                    ui.set_signature_visible(false);
                    self.signature_reset(ui);
                }
                if self.forms.pending.is_none() {
                    self.forms.pending = self
                        .selected_form()
                        .map(|f| (f.spec.page, f.spec.name, Some(f.id.widget)));
                }
                self.forms.records.clear();
                self.forms.wanted.clear();
                self.forms.selected = None;
                ui.set_form_selected(false);
                self.forms.last_surfaces.borrow_mut().clear();
                ui.set_form_surfaces(ModelRc::default());
                true
            }
            Event::Forms {
                document,
                revision,
                page,
                fields,
            } if *document == self.document.document
                && *revision == self.editor.revision
                && self.forms.wanted.contains(page) =>
            {
                self.forms.records.insert(*page, fields.clone());
                // Opening Form on a page with no fields starts detection for
                // that page once its (empty) field list is known.
                if self.forms.auto_detect_page == Some(*page) {
                    self.forms.auto_detect_page = None;
                    if fields.is_empty()
                        && ui.get_form_mode() > 0
                        && !ui.get_file_busy()
                        && ui.get_can_copy()
                        && ui.get_can_edit_forms()
                    {
                        self.start_detection_pages(ui, vec![*page]);
                    }
                }
                if let Some((p, name, widget)) = &self.forms.pending
                    && let Some(index) = self.forms.records.values().flatten().position(|f| {
                        f.spec.page == *p
                            && widget.map_or_else(
                                || {
                                    f.spec.name == *name
                                        || (f.spec.kind == FieldKind::Radio
                                            && f.export_value == *name)
                                },
                                |id| f.id.widget == id,
                            )
                    })
                {
                    self.forms.pending = None;
                    self.form_select(ui, index as i32);
                }
                self.form_overlays(ui);
                if self.forms.pending.is_none()
                    && let Some(spec) = self.forms.pending_properties.take()
                    && let Some(f) = self.selected_form()
                {
                    self.forms.pending = Some((f.spec.page, f.spec.name, Some(f.id.widget)));
                    self.form_submit(ui, FormEdit::Update { id: f.id, spec });
                    return true;
                }
                if self.forms.pending.is_none() && self.forms.close_after_commit {
                    self.forms.close_after_commit = false;
                    self.request_close(ui);
                }
                if self.forms.pending.is_none()
                    && let Some(path) = self.forms.open_after_commit.take()
                {
                    self.request_open(ui, path);
                }
                let navigation_ready = self.forms.navigation.is_none_or(|(_, p, _, _)| p == *page);
                let review_ready = self.forms.pending_action.as_deref() != Some("form-review")
                    || self.forms.review_pages.front().is_none_or(|p| *p == *page);
                if self.forms.pending.is_none()
                    && review_ready
                    && navigation_ready
                    && let Some(action) = self.forms.pending_action.take()
                {
                    self.action(ui, &action);
                }
                true
            }
            Event::FileError {
                document, message, ..
            } if *document == self.document.document => {
                ui.set_form_detecting(false);
                ui.set_digital_signing(false);
                self.forms.signature_pending = false;
                self.forms.pending_action = None;
                self.forms.pending = None;
                self.forms.pending_properties = None;
                self.forms.close_after_commit = false;
                self.forms.open_after_commit = None;
                self.forms.navigation = None;
                // Restore controls after a rejected toggle or choice operation.
                if let Some(index) = self
                    .forms
                    .records
                    .values()
                    .flatten()
                    .position(|f| Some(f.id) == self.forms.selected && !f.spec.kind.is_text())
                {
                    self.form_select(ui, index as i32);
                }
                self.form_error(ui, message.clone());
                false
            }
            _ => false,
        }
    }
    pub(super) fn form_refresh(&mut self, ui: &AppWindow, pages: &[usize]) {
        if ui.get_form_mode() == 0 {
            return;
        }
        let wanted = pages.iter().copied().take(16).collect::<Vec<_>>();
        if wanted != self.forms.wanted {
            let selected_page = self.forms.selected.map(|id| id.page);
            self.forms
                .records
                .retain(|p, _| wanted.contains(p) || Some(*p) == selected_page);
            self.forms.wanted = wanted.clone();
            self.engine
                .form_pages(self.document.document, self.editor.revision, wanted);
        }
        self.form_overlays(ui);
    }
    fn form_overlays(&self, ui: &AppWindow) {
        let scale = self.viewport.zoom * 96. / 72.;
        let surfaces = self
            .forms
            .records
            .values()
            .flatten()
            .enumerate()
            .filter_map(|(i, f)| {
                let p = self.viewport.layout.pages.get(f.spec.page)?;
                let r = f.spec.rect;
                Some(FormSurface {
                    index: i as i32,
                    x: p.x + r.left * scale,
                    y: p.y + r.top * scale,
                    w: (r.right - r.left) * scale,
                    h: (r.bottom - r.top) * scale,
                    label: f.spec.label.as_str().into(),
                    selected: Some(f.id) == self.forms.selected,
                    review: f.spec.review,
                    kind: FieldKind::ALL
                        .iter()
                        .position(|k| *k == f.spec.kind)
                        .unwrap_or(0) as i32,
                    value: f.value.text().into(),
                    checked: f.value.text() == f.export_value,
                    editable: f.editable,
                    password: f.password,
                })
            })
            .collect::<Vec<_>>();
        if *self.forms.last_surfaces.borrow() == surfaces {
            return;
        }
        *self.forms.last_surfaces.borrow_mut() = surfaces.clone();
        ui.set_form_surfaces(ModelRc::new(VecModel::from(surfaces)));
    }
    fn start_form_detection(&mut self, ui: &AppWindow) {
        let count = self.viewport.sizes.len();
        let pages = match ui.get_form_scan_scope() {
            0 => Ok(vec![
                self.viewport.current_page.min(count.saturating_sub(1)),
            ]),
            1 => viewer_core::parse_pages(ui.get_form_scan_pages().as_str(), count),
            _ => Ok((0..count).collect()),
        };
        let pages = match pages {
            Ok(p) if p.len() <= 100 => p,
            Ok(_) => {
                self.form_error(ui, "Choose a range of at most 100 pages per detection run.");
                return;
            }
            Err(e) => {
                self.form_error(ui, e);
                return;
            }
        };
        self.start_detection_pages(ui, pages);
    }
    fn start_detection_pages(&mut self, ui: &AppWindow, pages: Vec<usize>) {
        if pages.is_empty() || pages.len() > 100 {
            return;
        }
        if !ui.get_can_copy() || !ui.get_can_edit_forms() {
            self.form_error(
                ui,
                "This PDF does not permit recognition and creation of fields.",
            );
            return;
        }
        if self.form_commit(ui) {
            self.forms.pending_action = Some("form-detect".into());
            return;
        }
        self.editor.operation += 1;
        let accepted = self.engine.detect_fields(
            self.document.document,
            self.editor.revision,
            self.editor.operation,
            self.viewport.generation + 1,
            pages,
        );
        if accepted {
            ui.set_file_busy(true);
            ui.set_form_detecting(true);
            self.form_error(ui,"Recognizing fields locally… Cancel before installation to leave the PDF unchanged.");
        }
    }
}
