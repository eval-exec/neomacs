//! A small in-memory ring of recent Lisp CPU captures, so `/diff` can compare
//! two ("did my change help?"). Bounded so long-lived editors don't grow it.

use std::collections::VecDeque;
use std::time::Instant;

pub(crate) struct StoredCapture {
    pub id: u64,
    pub folded: String,
    pub total_samples: u64,
    pub captured_at: Instant,
}

pub(crate) struct CaptureStore {
    next_id: u64,
    ring: VecDeque<StoredCapture>,
    cap: usize,
}

impl CaptureStore {
    pub fn new(cap: usize) -> Self {
        Self {
            next_id: 1,
            ring: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    /// Store folded stacks, returning the assigned id. Evicts the oldest when
    /// over capacity.
    pub fn store(&mut self, folded: String) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let total_samples = folded
            .lines()
            .filter_map(|l| l.trim().rsplit_once(' '))
            .filter_map(|(_, c)| c.trim().parse::<u64>().ok())
            .sum();
        self.ring.push_back(StoredCapture {
            id,
            folded,
            total_samples,
            captured_at: Instant::now(),
        });
        while self.ring.len() > self.cap {
            self.ring.pop_front();
        }
        id
    }

    pub fn folded(&self, id: u64) -> Option<&str> {
        self.ring
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.folded.as_str())
    }

    /// `(id, total_samples, seconds_ago)`, newest last.
    pub fn list(&self) -> Vec<(u64, u64, u64)> {
        let now = Instant::now();
        self.ring
            .iter()
            .map(|c| {
                (
                    c.id,
                    c.total_samples,
                    now.saturating_duration_since(c.captured_at).as_secs(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_assigns_ids_computes_totals_and_evicts() {
        let mut s = CaptureStore::new(2);
        let a = s.store("x;y 3\nx;z 4".to_string()); // total 7
        let b = s.store("p 5".to_string()); // total 5
        assert_eq!(a, 1);
        assert_eq!(b, 2);
        assert_eq!(s.folded(a), Some("x;y 3\nx;z 4"));
        let c = s.store("q 1".to_string()); // evicts id 1
        assert_eq!(c, 3);
        assert_eq!(s.folded(1), None, "oldest should be evicted");
        assert_eq!(s.folded(2), Some("p 5"));
        let list = s.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].0, 2); // id
        assert_eq!(list[1].0, 3);
        assert_eq!(list[1].1, 1); // total_samples of "q 1"
    }
}
