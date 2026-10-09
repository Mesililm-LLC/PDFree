//! MuPDF ownership and background rendering boundary.
mod coordinator;
mod files;
mod raster;
mod reader;

use crossbeam_channel::{Receiver, Sender, bounded};
use std::{
    fmt,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use viewer_core::{Annotation, PageSize, PageText, PixelTile, RenderRequest, SearchHit, TileKey};

pub const NATIVE_STORE_LIMIT_MIB: usize = 256;

#[derive(Clone, Debug)]
pub struct DocumentInfo {
    pub pages: usize,
    pub first_page: PageSize,
    pub can_copy: bool,
    pub can_annotate: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileAction {
    SaveAs,
    Rename,
}
struct FileCommand {
    document: u64,
    operation: u64,
    action: FileCommandAction,
}
enum FileCommandAction {
    SaveAs {
        target: PathBuf,
        annotations: Vec<Annotation>,
    },
    Rename(String),
    RenameAndSave {
        name: String,
        annotations: Vec<Annotation>,
    },
}

#[derive(Clone, Debug)]
pub enum EngineError {
    PasswordRequired,
    PasswordRejected,
    InvalidDocument(String),
    Render(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PasswordRequired => f.write_str("This PDF requires a password."),
            Self::PasswordRejected => f.write_str("The password was not accepted. Try again."),
            Self::InvalidDocument(s) => write!(f, "Could not open PDF: {s}"),
            Self::Render(s) => write!(f, "Could not render page: {s}"),
        }
    }
}
impl std::error::Error for EngineError {}

#[derive(Debug)]
pub enum Event {
    FileDone {
        document: u64,
        operation: u64,
        path: PathBuf,
        kind: FileAction,
    },
    FileError {
        document: u64,
        operation: u64,
        message: String,
    },
    Notes {
        document: u64,
        page: usize,
        notes: Vec<Annotation>,
        truncated: bool,
    },
    Text {
        document: u64,
        page: usize,
        text: Arc<PageText>,
    },
    SearchPage {
        document: u64,
        search: u64,
        page: usize,
        hits: Vec<SearchHit>,
        scanned: usize,
        total: usize,
        done: bool,
        truncated: bool,
        errors: usize,
    },
    Thumbnail {
        key: TileKey,
        pixels: Arc<PixelTile>,
        elapsed: Duration,
    },
    ReaderError {
        document: u64,
        operation: &'static str,
        page: Option<usize>,
        message: String,
    },
    Opened {
        document: u64,
        info: DocumentInfo,
        elapsed: Duration,
    },
    Geometry {
        document: u64,
        pages: Vec<(usize, PageSize)>,
    },
    DisplayListReady {
        document: u64,
        page: usize,
        elapsed: Duration,
    },
    Tile {
        key: TileKey,
        generation: u64,
        pixels: Arc<PixelTile>,
        elapsed: Duration,
    },
    Error {
        document: u64,
        error: EngineError,
    },
}

struct Open {
    document: u64,
    generation: u64,
    path: PathBuf,
    password: Option<String>,
}
struct View {
    document: u64,
    generation: u64,
    requests: Vec<RenderRequest>,
    resident: Vec<usize>,
}

#[derive(Default)]
struct Desired {
    open: Option<Open>,
    view: Option<View>,
    text: Option<(u64, Vec<usize>)>,
    thumbnails: Option<(u64, Vec<usize>)>,
    search: Option<(u64, u64, String)>,
    notes: Option<(u64, Vec<usize>)>,
    file: Option<FileCommand>,
}

struct Shared {
    desired: Mutex<Desired>,
    document: AtomicU64,
    generation: AtomicU64,
    search: AtomicU64,
    stopping: AtomicBool,
    file_busy: AtomicBool,
}

impl Shared {
    fn current_document(&self, document: u64) -> bool {
        !self.stopping.load(Ordering::Acquire) && self.document.load(Ordering::Acquire) == document
    }
    fn current(&self, document: u64, generation: u64) -> bool {
        !self.stopping.load(Ordering::Acquire)
            && self.document.load(Ordering::Acquire) == document
            && self.generation.load(Ordering::Acquire) == generation
    }
}

pub struct Engine {
    shared: Arc<Shared>,
    wake: Sender<()>,
    pub events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    pub fn spawn(workers: usize) -> Self {
        let shared = Arc::new(Shared {
            desired: Mutex::new(Desired::default()),
            document: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            search: AtomicU64::new(0),
            stopping: AtomicBool::new(false),
            file_busy: AtomicBool::new(false),
        });
        let (wake, receive) = bounded(1);
        let (send, events) = bounded(64);
        let owner = shared.clone();
        let thread = thread::Builder::new()
            .name("pdf-coordinator".into())
            .spawn(move || coordinator::run(owner, receive, send, workers.clamp(1, 4)))
            .expect("create PDF coordinator");
        Self {
            shared,
            wake,
            events,
            thread: Some(thread),
        }
    }

    pub fn open(&self, document: u64, generation: u64, path: PathBuf) {
        self.open_with_password(document, generation, path, None);
    }

    pub fn open_with_password(
        &self,
        document: u64,
        generation: u64,
        path: PathBuf,
        password: Option<String>,
    ) {
        self.shared.document.store(document, Ordering::Release);
        self.shared.generation.store(generation, Ordering::Release);
        let mut desired = self.shared.desired.lock().unwrap();
        desired.open = Some(Open {
            document,
            generation,
            path,
            password,
        });
        desired.view = None;
        desired.text = None;
        desired.thumbnails = None;
        desired.search = None;
        desired.notes = None;
        drop(desired);
        let _ = self.wake.try_send(());
    }

    pub fn text_pages(&self, document: u64, pages: Vec<usize>) {
        self.shared.desired.lock().unwrap().text =
            Some((document, pages.into_iter().take(16).collect()));
        let _ = self.wake.try_send(());
    }

    pub fn note_pages(&self, document: u64, pages: Vec<usize>) {
        self.shared.desired.lock().unwrap().notes =
            Some((document, pages.into_iter().take(16).collect()));
        let _ = self.wake.try_send(());
    }
    fn file(&self, command: FileCommand) -> bool {
        if self
            .shared
            .file_busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        self.shared.desired.lock().unwrap().file = Some(command);
        let _ = self.wake.try_send(());
        true
    }
    pub fn save_as(
        &self,
        document: u64,
        operation: u64,
        target: PathBuf,
        annotations: Vec<Annotation>,
    ) -> bool {
        self.file(FileCommand {
            document,
            operation,
            action: FileCommandAction::SaveAs {
                target,
                annotations,
            },
        })
    }
    pub fn rename(&self, document: u64, operation: u64, name: String) -> bool {
        self.file(FileCommand {
            document,
            operation,
            action: FileCommandAction::Rename(name),
        })
    }
    pub fn rename_and_save(
        &self,
        document: u64,
        operation: u64,
        name: String,
        annotations: Vec<Annotation>,
    ) -> bool {
        self.file(FileCommand {
            document,
            operation,
            action: FileCommandAction::RenameAndSave { name, annotations },
        })
    }

    pub fn thumbnails(&self, document: u64, pages: Vec<usize>) {
        self.shared.desired.lock().unwrap().thumbnails =
            Some((document, pages.into_iter().take(32).collect()));
        let _ = self.wake.try_send(());
    }

    pub fn search(&self, document: u64, search: u64, query: impl Into<String>) {
        let mut query = query.into();
        if query.len() > 512 || query.contains('\0') {
            query.clear();
        }
        self.shared.search.store(search, Ordering::Release);
        self.shared.desired.lock().unwrap().search = Some((document, search, query));
        let _ = self.wake.try_send(());
    }

    pub fn view(
        &self,
        document: u64,
        generation: u64,
        requests: Vec<RenderRequest>,
        resident: Vec<usize>,
    ) {
        self.shared.generation.store(generation, Ordering::Release);
        self.shared.desired.lock().unwrap().view = Some(View {
            document,
            generation,
            requests,
            resident,
        });
        let _ = self.wake.try_send(());
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::Release);
        let _ = self.wake.try_send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod request_tests {
    use super::*;

    #[test]
    fn reader_requests_are_bounded_and_replace_pending_work() {
        // Leave the coordinator absent so the latest-value slots can be examined deterministically.
        let (wake, _) = bounded(1);
        let (_, events) = bounded(1);
        let engine = Engine {
            shared: Arc::new(Shared {
                desired: Mutex::new(Desired::default()),
                document: AtomicU64::new(1),
                generation: AtomicU64::new(1),
                search: AtomicU64::new(0),
                stopping: AtomicBool::new(false),
                file_busy: AtomicBool::new(false),
            }),
            wake,
            events,
            thread: None,
        };
        engine.text_pages(1, (0..10_000).collect());
        engine.thumbnails(1, (0..10_000).collect());
        engine.note_pages(1, (0..10_000).collect());
        {
            let desired = engine.shared.desired.lock().unwrap();
            assert_eq!(desired.text.as_ref().unwrap().1.len(), 16);
            assert_eq!(desired.thumbnails.as_ref().unwrap().1.len(), 32);
            assert_eq!(desired.notes.as_ref().unwrap().1.len(), 16);
        }
        engine.text_pages(1, vec![99]);
        engine.thumbnails(1, vec![98]);
        engine.search(1, 10, "x".repeat(100_000));
        assert!(engine.rename(1, 1, "first.pdf".into()));
        assert!(!engine.rename(1, 2, "second.pdf".into()));
        assert!(!engine.save_as(1, 3, PathBuf::from("third.pdf"), vec![]));
        let desired = engine.shared.desired.lock().unwrap();
        assert_eq!(desired.text.as_ref().unwrap().1, vec![99]);
        assert_eq!(desired.thumbnails.as_ref().unwrap().1, vec![98]);
        assert!(desired.search.as_ref().unwrap().2.is_empty());
    }
}
