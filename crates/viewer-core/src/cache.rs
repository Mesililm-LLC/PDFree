use crate::{PixelTile, TileKey};
use std::{collections::HashMap, sync::Arc};

pub struct TileCache {
    entries: HashMap<TileKey, (Arc<PixelTile>, u64)>,
    budget: usize,
    bytes: usize,
    clock: u64,
    pub hits: u64,
    pub misses: u64,
}

impl TileCache {
    pub fn new(budget: usize) -> Self {
        Self {
            entries: HashMap::new(),
            budget,
            bytes: 0,
            clock: 0,
            hits: 0,
            misses: 0,
        }
    }
    pub fn get(&mut self, key: &TileKey) -> Option<Arc<PixelTile>> {
        self.clock += 1;
        if let Some((tile, age)) = self.entries.get_mut(key) {
            *age = self.clock;
            self.hits += 1;
            Some(tile.clone())
        } else {
            self.misses += 1;
            None
        }
    }
    pub fn insert(&mut self, key: TileKey, tile: Arc<PixelTile>) {
        let size = tile.rgba.len();
        if size > self.budget {
            return;
        }
        if let Some((old, _)) = self.entries.remove(&key) {
            self.bytes -= old.rgba.len();
        }
        while self.bytes + size > self.budget {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, (_, age))| age)
                .map(|(key, _)| *key);
            if let Some(oldest) = oldest {
                self.bytes -= self.entries.remove(&oldest).unwrap().0.rgba.len();
            } else {
                break;
            }
        }
        self.clock += 1;
        self.bytes += size;
        self.entries.insert(key, (tile, self.clock));
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}
