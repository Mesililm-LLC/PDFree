//! BenchmarkManager: benchmark timing and stage progression.

pub enum BenchmarkAction {
    None,
    Next,
    ZoomIn,
    Previous,
    Quit,
}

pub struct BenchmarkManager {
    pub enabled: bool,
    pub stage: u8,
    pub zoom_started: std::time::Instant,
}

impl BenchmarkManager {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            stage: 0,
            zoom_started: std::time::Instant::now(),
        }
    }

    /// Run benchmark logic during tick. Returns the next action to take.
    pub fn tick(
        &mut self,
        first_tile: bool,
        first_pixels_elapsed: std::time::Duration,
        _open_started_elapsed: std::time::Duration,
        cache_bytes: usize,
        cache_hits: u64,
        cache_misses: u64,
    ) -> (BenchmarkAction, Option<serde_json::Value>) {
        if !self.enabled || !first_tile {
            return (BenchmarkAction::None, None);
        }

        if self.stage == 0 && first_pixels_elapsed > std::time::Duration::from_millis(600) {
            self.stage = 1;
            return (
                BenchmarkAction::Next,
                Some(serde_json::json!({
                    "stage": "first-page",
                    "cache_bytes": cache_bytes,
                    "cache_hits": cache_hits,
                    "cache_misses": cache_misses,
                })),
            );
        }
        if self.stage == 1 && first_pixels_elapsed > std::time::Duration::from_millis(1000) {
            self.stage = 2;
            return (BenchmarkAction::ZoomIn, None);
        }
        if self.stage == 2 && first_pixels_elapsed > std::time::Duration::from_millis(1500) {
            self.stage = 3;
            return (BenchmarkAction::Previous, None);
        }
        if first_pixels_elapsed > std::time::Duration::from_millis(2200) {
            return (
                BenchmarkAction::Quit,
                Some(serde_json::json!({
                    "cache_bytes": cache_bytes,
                    "cache_hits": cache_hits,
                    "cache_misses": cache_misses,
                    "native_store_limit_mib": pdf_engine::NATIVE_STORE_LIMIT_MIB,
                })),
            );
        }
        (BenchmarkAction::None, None)
    }

    /// Handle benchmark empty-window case.
    pub fn benchmark_empty_window(
        &self,
        document: u64,
        open_started_elapsed: std::time::Duration,
    ) -> Option<serde_json::Value> {
        if self.enabled
            && document == 0
            && open_started_elapsed > std::time::Duration::from_millis(600)
        {
            return Some(serde_json::json!({
                "empty_window": true,
                "cache_bytes": 0,
                "cache_hits": 0,
                "cache_misses": 0,
            }));
        }
        None
    }

    pub fn reset_zoom_timer(&mut self) {
        self.zoom_started = std::time::Instant::now();
    }
}
