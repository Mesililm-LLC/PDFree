use std::collections::{BTreeSet, VecDeque};

const BUDGET: usize = 128 * 1024 * 1024;
const MAX_STATES: usize = 201;

/// Bounded, revision-based history of complete PDF snapshots.
pub struct SnapshotHistory {
    states: VecDeque<(u64, Vec<u8>)>,
    cursor: usize,
    saved: u64,
    next_revision: u64,
}

impl SnapshotHistory {
    pub fn new(bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.len() > BUDGET {
            return Err("This PDF exceeds the 128 MiB editing history limit.".into());
        }
        Ok(Self {
            states: VecDeque::from([(0, bytes)]),
            cursor: 0,
            saved: 0,
            next_revision: 1,
        })
    }
    pub fn current(&self) -> &[u8] {
        &self.states[self.cursor].1
    }
    pub fn revision(&self) -> u64 {
        self.states[self.cursor].0
    }
    pub fn dirty(&self) -> bool {
        self.revision() != self.saved
    }
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }
    pub fn can_redo(&self) -> bool {
        self.cursor + 1 < self.states.len()
    }
    pub fn previous(&self) -> Option<&[u8]> {
        self.cursor
            .checked_sub(1)
            .map(|i| self.states[i].1.as_slice())
    }
    pub fn next(&self) -> Option<&[u8]> {
        self.states
            .get(self.cursor + 1)
            .map(|state| state.1.as_slice())
    }
    pub fn push(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        if self.current().len().saturating_add(bytes.len()) > BUDGET {
            return Err(
                "This change exceeds the 128 MiB undo history limit. The PDF was not changed."
                    .into(),
            );
        }
        self.states.truncate(self.cursor + 1);
        self.states.push_back((self.next_revision, bytes));
        self.next_revision += 1;
        self.cursor += 1;
        let mut total: usize = self.states.iter().map(|state| state.1.len()).sum();
        while self.states.len() > MAX_STATES || total > BUDGET {
            total -= self
                .states
                .pop_front()
                .expect("at least two states")
                .1
                .len();
            self.cursor -= 1;
        }
        Ok(())
    }
    pub fn undo(&mut self) -> bool {
        if self.can_undo() {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self) -> bool {
        if self.can_redo() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
    pub fn mark_saved(&mut self) {
        self.saved = self.revision();
    }
}

/// Parse one-based page ranges into unique, ordered zero-based page indices.
pub fn parse_pages(input: &str, total: usize) -> Result<Vec<usize>, String> {
    if input.len() > 8192 || total == 0 || total > 100_000 || input.trim().is_empty() {
        return Err("Enter page numbers or ranges, for example 1-3,5.".into());
    }
    let mut pages = BTreeSet::new();
    let number = |s: &str| -> Result<usize, String> {
        let value = s
            .trim()
            .parse::<usize>()
            .map_err(|_| "Invalid page number.".to_string())?;
        if value == 0 || value > total {
            return Err(format!("Page numbers must be between 1 and {total}."));
        }
        Ok(value)
    };
    for item in input.split(',') {
        if let Some((first, last)) = item.split_once('-') {
            let (first, last) = (number(first)?, number(last)?);
            if first > last {
                return Err("Page ranges must run from smaller to larger numbers.".into());
            }
            pages.extend((first..=last).map(|p| p - 1));
        } else {
            pages.insert(number(item)? - 1);
        }
    }
    Ok(pages.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revisions_survive_save_undo_and_branch() {
        let mut h = SnapshotHistory::new(vec![0]).unwrap();
        h.push(vec![1]).unwrap();
        h.mark_saved();
        assert!(!h.dirty());
        assert_eq!(h.previous(), Some([0].as_slice()));
        assert!(h.undo());
        assert!(h.dirty());
        assert!(h.redo());
        assert!(!h.dirty());
        h.undo();
        h.push(vec![2]).unwrap();
        assert!(h.dirty());
        assert!(!h.can_redo());
        assert_eq!(h.revision(), 2);
    }
    #[test]
    fn history_prunes_without_losing_current() {
        let mut h = SnapshotHistory::new(vec![0]).unwrap();
        for i in 1..=250 {
            h.push(vec![i as u8]).unwrap();
        }
        assert_eq!(h.states.len(), 201);
        assert_eq!(h.current(), &[250]);
        for _ in 0..200 {
            assert!(h.undo());
        }
        assert!(!h.undo());
    }
    #[test]
    fn failed_push_preserves_redo_and_checkpoint() {
        let mut h = SnapshotHistory::new(vec![0]).unwrap();
        h.push(vec![1]).unwrap();
        h.mark_saved();
        h.undo();
        assert!(h.push(vec![0; BUDGET]).is_err());
        assert!(h.can_redo());
        h.redo();
        assert!(!h.dirty());
    }
    #[test]
    fn ranges_validate_and_deduplicate() {
        assert_eq!(parse_pages(" 3, 1-3,5 ", 5).unwrap(), vec![0, 1, 2, 4]);
        for s in ["", "0", "6", "3-1", "1-2-3", "1,", "-1", "one"] {
            assert!(parse_pages(s, 5).is_err(), "{s}");
        }
    }
}
