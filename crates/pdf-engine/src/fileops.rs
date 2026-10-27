//! FileOps subsystem: all file transaction handling (Save, SaveAs, Rename,
//! Extract, Split, RenameAndSave, Sign). Extracted from the coordinator
//! monolith.

use crate::{
    detection, digital_signatures::SourceStamp, editing, files, forms, Event, FileAction, FileCommand,
    FileCommandAction, Shared,
};
use crossbeam_channel::{Receiver, RecvTimeoutError, SendTimeoutError};
use pdfium_render::{
    pdf::{Permission},
    PdfDocument,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use viewer_core::{Annotation, PageSize, PixelTile, RenderRequest, SnapshotHistory, TileKey};

/// A completed render result (re-exported type alias from rendering).
pub(crate) type Completed = crate::rendering::Completed;

/// Emits an event through the bounded channel, respecting shutdown.
fn emit(shared: &Shared, events: &crossbeam_channel::Sender<Event>, mut event: Event) -> bool {
    loop {
        match events.send_timeout(event, Duration::from_millis(crate::constants::CHANNEL_SEND_TIMEOUT_MILLIS)) {
            Ok(()) => return true,
            Err(SendTimeoutError::Timeout(value)) => {
                if shared.stopping.load(Ordering::Acquire) {
                    return false;
                }
                event = value;
            }
            Err(_) => return false,
        }
    }
}

/// Mutable state owned by the coordinator that FileOps needs to read and modify.
pub(crate) struct FileOpsState {
    pub document: Option<PdfDocument>,
    pub journal: Option<SnapshotHistory>,
    pub source_path: PathBuf,
    pub source_stamp: Option<SourceStamp>,
    pub session_password: Option<String>,
    pub can_copy: bool,
    pub content: u64,
    pub detection_done: Option<(u64, usize, Vec<usize>)>,
    pub detection_job: Option<detection::Detection>,
    pub deferred_results: VecDeque<Completed>,
    pub notes_cache: HashMap<usize, (Vec<Annotation>, bool)>,
}

impl FileOpsState {
    pub(crate) fn new(source_path: PathBuf) -> Self {
        Self {
            document: None,
            journal: None,
            source_path,
            source_stamp: None,
            session_password: None,
            can_copy: false,
            content: 0,
            detection_done: None,
            detection_job: None,
            deferred_results: VecDeque::new(),
            notes_cache: HashMap::new(),
        }
    }
}

/// The FileOps subsystem handles all file transaction commands.
pub(crate) struct FileOps {
    shared: Arc<Shared>,
    events: crossbeam_channel::Sender<Event>,
    document_id: u64,
}

impl FileOps {
    pub(crate) fn new(
        shared: Arc<Shared>,
        events: crossbeam_channel::Sender<Event>,
        document_id: u64,
    ) -> Self {
        Self {
            shared,
            events,
            document_id,
        }
    }

    /// Handle a file command. Returns `(should_continue, document_changed)`.
    pub(crate) fn handle(
        &mut self,
        state: &mut FileOpsState,
        results: &Receiver<Completed>,
        inflight_len: usize,
        command: FileCommand,
    ) -> (bool, bool) {
        if let FileCommandAction::Sign { revision, request } = &command.action {
            let result = (|| {
                if command.document != self.document_id
                    || *revision != state.journal.as_ref().map_or(0, |h| h.revision())
                    || state.journal.as_ref().is_some_and(|h| h.dirty())
                {
                    return Err("Save and review your edits before signing.".to_string());
                }
                let stamp = state
                    .source_stamp
                    .as_ref()
                    .ok_or("The original PDF file is unavailable")?;
                crate::digital_signatures::sign(
                    &state.source_path,
                    state.session_password.as_deref(),
                    stamp,
                    request,
                    || {
                        self.shared.current_document(command.document)
                            && !self.shared.sign_cancel.load(Ordering::Acquire)
                    },
                )
            })();
            self.shared.file_busy.store(false, Ordering::Release);
            match result {
                Ok((path, integrity)) => {
                    let _ = emit(
                        &self.shared,
                        &self.events,
                        Event::Signed {
                            document: command.document,
                            operation: command.operation,
                            path,
                            integrity,
                        },
                    );
                }
                Err(message) => {
                    let _ = emit(
                        &self.shared,
                        &self.events,
                        Event::FileError {
                            document: command.document,
                            operation: command.operation,
                            message,
                        },
                    );
                }
            }
            return (true, false);
        }

        if let FileCommandAction::Detect {
            revision,
            generation,
            pages,
        } = &command.action
        {
            let result = (|| {
                let doc = state.document.as_ref().ok_or("No PDF is open")?;
                if command.document != self.document_id
                    || *revision != state.journal.as_ref().map_or(0, |h| h.revision())
                {
                    return Err("Document changed before detection started".to_string());
                }
                forms::ensure_writable(doc)?;
                if !forms::permissions(doc)
                    .contains(Permission::COPY | Permission::MODIFY)
                {
                    return Err(
                        "This PDF does not permit field recognition and creation".into(),
                    );
                }
                let count = doc.pages().len();
                let mut unique = HashSet::new();
                if pages.iter().any(|p| *p >= count || !unique.insert(*p)) {
                    return Err("Invalid detection page range".into());
                }
                crate::detection::Detection::new(
                    command.document,
                    *revision,
                    command.operation,
                    *generation,
                    pages.clone(),
                )
            })();
            match result {
                Ok(job) => {
                    state.detection_job = Some(job);
                    state.detection_done = None;
                }
                Err(message) => {
                    self.shared.file_busy.store(false, Ordering::Release);
                    let _ = emit(
                        &self.shared,
                        &self.events,
                        Event::FileError {
                            document: command.document,
                            operation: command.operation,
                            message,
                        },
                    );
                }
            }
            return (true, false);
        }

        let direct_save = matches!(&command.action, FileCommandAction::Save);
        if let FileCommandAction::Edit {
            revision,
            generation,
            command: edit,
        } = &command.action
        {
            let result = if command.document != self.document_id
                || !self.shared.current_document(command.document)
            {
                Err("The document changed before editing started".into())
            } else if matches!(
                edit,
                crate::EditCommand::Form(viewer_core::FormEdit::CreateDetected(_))
            ) && self.shared.detect_cancel.load(Ordering::Acquire)
            {
                Err("Field detection cancelled; no fields were created.".into())
            } else if let Some(doc) = state.document.as_mut() {
                editing::perform(
                    doc,
                    &mut state.journal,
                    state.session_password.as_deref(),
                    *revision,
                    edit,
                    || {
                        self.shared.current_document(command.document)
                            && (!matches!(
                                edit,
                                crate::EditCommand::Form(
                                    viewer_core::FormEdit::CreateDetected(_)
                                )
                            ) || !self.shared.detect_cancel.load(Ordering::Acquire))
                    },
                )
            } else {
                Err("No PDF is open".into())
            };
            match result {
                Ok((doc, focus, revision)) => {
                    let info = editing::info(&doc).expect("validated edited PDF");
                    state.can_copy = info.can_copy;
                    state.content = state.content.wrapping_add(1);
                    self.shared.content.store(state.content, Ordering::Release);
                    self.shared.generation.store(*generation, Ordering::Release);
                    let h = state.journal.as_ref().unwrap();
                    let detection_summary = state.detection_done.take();
                    let _ = emit(
                        &self.shared,
                        &self.events,
                        Event::Changed {
                            document: self.document_id,
                            operation: command.operation,
                            revision,
                            generation: *generation,
                            info,
                            dirty: h.dirty(),
                            can_undo: h.can_undo(),
                            can_redo: h.can_redo(),
                            focus_page: focus,
                        },
                    );
                    if let Some((operation, count, review_pages)) = detection_summary {
                        let _ = emit(
                            &self.shared,
                            &self.events,
                            Event::DetectionDone {
                                document: self.document_id,
                                operation,
                                count,
                                review_pages,
                            },
                        );
                    }
                }
                Err(message) => {
                    state.detection_done = None;
                    self.shared.file_busy.store(false, Ordering::Release);
                    let _ = emit(
                        &self.shared,
                        &self.events,
                        Event::FileError {
                            document: command.document,
                            operation: command.operation,
                            message,
                        },
                    );
                }
            }
            return (true, true);
        }

        let result = if command.document != self.document_id
            || !self.shared.current_document(command.document)
        {
            Err("The document changed before the file operation started".into())
        } else if let Some(doc) = state.document.as_mut() {
            let action = match command.action {
                FileCommandAction::Save => FileCommandAction::RenameAndSave {
                    name: state
                        .source_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .into(),
                    annotations: vec![],
                },
                FileCommandAction::Rename(name) if state.journal.is_some() => {
                    FileCommandAction::RenameAndSave {
                        name,
                        annotations: vec![],
                    }
                }
                action => action,
            };
            match action {
                FileCommandAction::Save => unreachable!(),
                FileCommandAction::Edit { .. }
                | FileCommandAction::Detect { .. }
                | FileCommandAction::Sign { .. } => unreachable!(),
                FileCommandAction::Extract {
                    revision,
                    target,
                    pages,
                } => {
                    if revision != state.journal.as_ref().map_or(0, |h| h.revision()) {
                        Err("Document revision changed".into())
                    } else {
                        editing::extract(
                            doc,
                            &state.source_path,
                            state.session_password.as_deref(),
                            &target,
                            &pages,
                            || self.shared.current_document(command.document),
                        )
                        .map(|p| (p, FileAction::Extract))
                    }
                }
                FileCommandAction::Split {
                    revision,
                    directory,
                    after,
                } => {
                    if revision != state.journal.as_ref().map_or(0, |h| h.revision()) {
                        Err("Document revision changed".into())
                    } else {
                        editing::split(
                            doc,
                            &state.source_path,
                            state.session_password.as_deref(),
                            &directory,
                            after,
                            || self.shared.current_document(command.document),
                        )
                        .map(|p| (p, FileAction::Split))
                    }
                }
                FileCommandAction::SaveAs {
                    target,
                    annotations,
                } => {
                    let switch = state.journal.is_some() || annotations.is_empty();
                    let baseline = if switch {
                        editing::serialize(doc).and_then(|bytes| {
                            let initial = if state.journal.is_none() {
                                Some(SnapshotHistory::new(bytes.clone())?)
                            } else {
                                None
                            };
                            Ok((
                                Some(editing::restore(
                                    &bytes,
                                    state.session_password.as_deref(),
                                )?),
                                initial,
                            ))
                        })
                    } else {
                        Ok((None, None))
                    };
                    match baseline {
                        Err(message) => Err(message),
                        Ok((baseline, initial)) => {
                            let saved = files::save(
                                doc,
                                &state.source_path,
                                state.session_password.as_deref(),
                                &target,
                                if state.journal.is_some() { &[] } else { &annotations },
                                || self.shared.current_document(command.document),
                            );
                            match saved {
                                Err(message) => Err(message),
                                Ok(path) => {
                                    if switch {
                                        state.source_path = path.clone();
                                        state.document = baseline;
                                        if let Some(h) = initial {
                                            state.journal = Some(h);
                                        }
                                        if let Some(h) = state.journal.as_mut() {
                                            h.mark_saved();
                                        }
                                    }
                                    Ok((path, FileAction::SaveAs))
                                }
                            }
                        }
                    }
                }
                FileCommandAction::RenameAndSave { ref annotations, .. }
                    if state.journal.is_some() && !annotations.is_empty() =>
                {
                    Err(
                        "Use native editing commands before saving; external drafts cannot bypass undo history"
                            .into(),
                    )
                }
                FileCommandAction::RenameAndSave { name, annotations } => {
                    match files::prepare_rename(
                        doc,
                        &state.source_path,
                        state.session_password.as_deref(),
                        &name,
                        &annotations,
                        || self.shared.current_document(command.document),
                    ) {
                        Err(e) => Err(e),
                        Ok(prepared) => {
                            while state.deferred_results.len() < inflight_len
                                && !self.shared.stopping.load(Ordering::Acquire)
                            {
                                match results.recv_timeout(Duration::from_millis(crate::constants::CHANNEL_RECV_TIMEOUT_MILLIS)) {
                                    Ok(result) => state.deferred_results.push_back(result),
                                    Err(RecvTimeoutError::Timeout) => {}
                                    Err(_) => break,
                                }
                            }
                            if !self.shared.current_document(command.document) {
                                Err("Document changed before rename completed".into())
                            } else {
                                drop(state.document.take());
                                let (baseline, result) = prepared.commit(&state.source_path);
                                state.document = Some(baseline);
                                match result {
                                    Ok(path) => {
                                        state.source_path = path.clone();
                                        if let Some(h) = state.journal.as_mut() {
                                            h.mark_saved();
                                        }
                                        Ok((
                                            path,
                                            if direct_save {
                                                FileAction::Save
                                            } else {
                                                FileAction::Rename
                                            },
                                        ))
                                    }
                                    Err(failure) => {
                                        state.source_path = failure.path;
                                        Err(failure.message)
                                    }
                                }
                            }
                        }
                    }
                }
                FileCommandAction::Rename(name) => {
                    match files::renamed_path(&state.source_path, &name) {
                        Err(e) => Err(e),
                        Ok(target) if target == state.source_path => {
                            Ok((state.source_path.clone(), FileAction::Rename))
                        }
                        Ok(target) => {
                            while state.deferred_results.len() < inflight_len
                                && !self.shared.stopping.load(Ordering::Acquire)
                            {
                                match results.recv_timeout(Duration::from_millis(crate::constants::CHANNEL_RECV_TIMEOUT_MILLIS)) {
                                    Ok(result) => state.deferred_results.push_back(result),
                                    Err(RecvTimeoutError::Timeout) => {}
                                    Err(_) => break,
                                }
                            }
                            if !self.shared.current_document(command.document) {
                                Err("Document changed before rename completed".into())
                            } else {
                                state.notes_cache.clear();
                                drop(state.document.take());
                                let moved =
                                    files::move_no_replace(&state.source_path, &target);
                                let reopen_path = if moved.is_ok() {
                                    target.clone()
                                } else {
                                    state.source_path.clone()
                                };
                                let reopened =
                                    files::open(&reopen_path, state.session_password.as_deref());
                                match reopened {
                                    Ok(doc) => {
                                        state.document = Some(doc);
                                        match moved {
                                            Ok(()) => {
                                                state.source_path = target.clone();
                                                Ok((target, FileAction::Rename))
                                            }
                                            Err(e) => Err(format!(
                                                "Could not rename PDF: {e}. Original remains open."
                                            )),
                                        }
                                    }
                                    Err(e) => {
                                        if moved.is_err() {
                                            Err(format!(
                                                "Rename failed and the original PDF could not be reopened: {e}. File remains at {}. Use Open to recover.",
                                                state.source_path.display()
                                            ))
                                        } else {
                                            match files::move_no_replace(
                                                &target,
                                                &state.source_path,
                                            ) {
                                                Ok(()) => match files::open(
                                                    &state.source_path,
                                                    state.session_password.as_deref(),
                                                ) {
                                                    Ok(doc) => {
                                                        state.document = Some(doc);
                                                        Err(format!(
                                                            "Renamed PDF could not be reopened ({e}); rename was rolled back. Original remains open."
                                                        ))
                                                    }
                                                    Err(rollback_error) => Err(format!(
                                                        "Rename was rolled back but reopening failed: {rollback_error}. File remains at {}. Use Open to recover.",
                                                        state.source_path.display()
                                                    )),
                                                },
                                                Err(rollback_error) => {
                                                    state.source_path = target;
                                                    Err(format!(
                                                        "Rename succeeded but reopening failed ({e}) and rollback failed ({rollback_error}). File is at {}. Use Open to recover.",
                                                        state.source_path.display()
                                                    ))
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            Err("No PDF is open".into())
        };
        self.shared.file_busy.store(false, Ordering::Release);
        let event = match result {
            Ok((path, kind)) => {
                if matches!(
                    kind,
                    FileAction::Save | FileAction::SaveAs | FileAction::Rename
                ) {
                    state.source_stamp =
                        crate::digital_signatures::SourceStamp::read(&state.source_path).ok();
                }
                Event::FileDone {
                    document: command.document,
                    operation: command.operation,
                    path,
                    kind,
                }
            }
            Err(message) => Event::FileError {
                document: command.document,
                operation: command.operation,
                message,
            },
        };
        let _ = emit(&self.shared, &self.events, event);
        if state.document.is_none() && command.document == self.document_id {
            let _ = emit(
                &self.shared,
                &self.events,
                Event::Error {
                    document: self.document_id,
                    error: crate::EngineError::InvalidDocument(format!(
                        "The PDF at {} could not be reopened. Use Open to recover.",
                        state.source_path.display()
                    )),
                },
            );
        }
        (false, false)
    }
}
