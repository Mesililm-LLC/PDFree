use crate::RenderRequest;
use std::collections::{HashSet, VecDeque};

pub struct RenderQueue {
    pending: VecDeque<RenderRequest>,
    capacity: usize,
}

impl RenderQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            pending: VecDeque::new(),
            capacity,
        }
    }
    pub fn replace(&mut self, mut requests: Vec<RenderRequest>) {
        requests.sort_by_key(|r| r.priority);
        let mut seen = HashSet::new();
        self.pending = requests
            .into_iter()
            .filter(|r| seen.insert(r.key))
            .take(self.capacity)
            .collect();
    }
    pub fn pop(&mut self) -> Option<RenderRequest> {
        self.pending.pop_front()
    }
    pub fn len(&self) -> usize {
        self.pending.len()
    }
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}
