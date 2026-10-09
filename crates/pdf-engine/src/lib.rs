//! MuPDF ownership and background rendering boundary.
mod coordinator;
mod raster;

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
use viewer_core::{PageSize, PixelTile, RenderRequest, TileKey};

pub const NATIVE_STORE_LIMIT_MIB: usize = 256;

#[derive(Clone, Debug)]
pub struct DocumentInfo {
    pub pages: usize,
    pub first_page: PageSize,
}

#[derive(Clone, Debug)]
pub enum EngineError {
    PasswordRequired,
    InvalidDocument(String),
    Render(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PasswordRequired => f.write_str(
                "This PDF requires a password. Password entry is planned for a later milestone.",
            ),
            Self::InvalidDocument(s) => write!(f, "Could not open PDF: {s}"),
            Self::Render(s) => write!(f, "Could not render page: {s}"),
        }
    }
}
impl std::error::Error for EngineError {}

#[derive(Debug)]
pub enum Event {
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
}

struct Shared {
    desired: Mutex<Desired>,
    document: AtomicU64,
    generation: AtomicU64,
    stopping: AtomicBool,
}

impl Shared {
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
            stopping: AtomicBool::new(false),
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
        self.shared.document.store(document, Ordering::Release);
        self.shared.generation.store(generation, Ordering::Release);
        let mut desired = self.shared.desired.lock().unwrap();
        desired.open = Some(Open {
            document,
            generation,
            path,
        });
        desired.view = None;
        drop(desired);
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
