//! Engine-independent viewport, cache, and request scheduling.
mod cache;
mod editing;
mod geometry;
mod navigation;
mod queue;
mod snapshots;
mod text;
pub use cache::*;
pub use editing::*;
pub use geometry::*;
pub use navigation::*;
pub use queue::*;
pub use snapshots::*;
pub use text::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TileKey {
    pub document: u64,
    pub page: usize,
    pub scale_milli: u32,
    pub rotation: u16,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct RenderRequest {
    pub key: TileKey,
    pub priority: u32,
    pub generation: u64,
}

#[derive(Debug)]
pub struct PixelTile {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn accept_result(
    document: u64,
    generation: u64,
    result_document: u64,
    result_generation: u64,
) -> bool {
    document == result_document && generation == result_generation
}
