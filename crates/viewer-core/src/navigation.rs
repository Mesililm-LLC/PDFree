use crate::TextRect;

/// Destinations are data only: callers must never execute unsupported actions.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkDestination {
    Internal { page: usize, x: f32, y: f32 },
    External(String),
    Unsupported(String),
}
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineEntry {
    pub title: String,
    pub depth: usize,
    pub destination: LinkDestination,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PageLink {
    pub bounds: TextRect,
    pub destination: LinkDestination,
}
