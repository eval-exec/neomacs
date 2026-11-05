//! Bounded first-read revisions with O(1) reuse between captures.
//!
//! Dense entries retain first-read order/revisions. The index is generation
//! stamped: recycling a capture clears its used entry prefix, not a whole
//! MAX_READS hash table. Generation wrap explicitly resets the index.
use super::{LispCollectionRevision, MAX_DEPTH, MAX_READS};
use rustc_hash::FxHasher;
use std::hash::Hasher;

const INDEX_SIZE: usize = MAX_READS * 2;
const _: () = {
    assert!(INDEX_SIZE.is_power_of_two());
    assert!(MAX_READS <= u32::MAX as usize);
};

#[derive(Clone, Copy, Default)]
struct ReadSlot {
    generation: u32,
    entry: u32,
}

pub(super) struct CaptureReads {
    entries: Vec<(usize, LispCollectionRevision)>,
    index: Box<[ReadSlot]>,
    generation: u32,
}

/// One independently leased map for each active scope. The pool lives in the
/// origin World's cold State and travels with its Baton, like the captures.
pub(super) struct CapturePool {
    free: Vec<CaptureReads>,
}

impl Default for CapturePool {
    fn default() -> Self {
        Self {
            free: Vec::with_capacity(MAX_DEPTH),
        }
    }
}

impl CapturePool {
    pub(super) fn acquire(&mut self) -> CaptureReads {
        self.free.pop().unwrap_or_default()
    }

    pub(super) fn recycle(&mut self, mut reads: CaptureReads) {
        reads.clear();
        self.free.push(reads);
    }
}

impl Default for CaptureReads {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: vec![ReadSlot::default(); INDEX_SIZE].into_boxed_slice(),
            generation: 1,
        }
    }
}

impl CaptureReads {
    fn bucket(bits: usize) -> usize {
        let mut hash = FxHasher::default();
        hash.write_usize(bits);
        hash.finish() as usize & (INDEX_SIZE - 1)
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn get(&self, bits: &usize) -> Option<&LispCollectionRevision> {
        let bucket = Self::bucket(*bits);
        for offset in 0..INDEX_SIZE {
            let slot = self.index[(bucket + offset) & (INDEX_SIZE - 1)];
            if slot.generation != self.generation {
                return None;
            }
            if let Some((owner, revision)) = self.entries.get(slot.entry as usize)
                && owner == bits
            {
                return Some(revision);
            }
        }
        None
    }

    pub(super) fn contains_key(&self, bits: &usize) -> bool {
        self.get(bits).is_some()
    }

    /// Keep the first revision, including when a full capture sees a repeat.
    /// False means a new identity would exceed the unchanged read budget.
    pub(super) fn insert_first(&mut self, bits: usize, revision: LispCollectionRevision) -> bool {
        let bucket = Self::bucket(bits);
        for offset in 0..INDEX_SIZE {
            let slot = &mut self.index[(bucket + offset) & (INDEX_SIZE - 1)];
            if slot.generation != self.generation {
                if self.entries.len() == MAX_READS {
                    return false;
                }
                slot.entry = self.entries.len() as u32;
                self.entries.push((bits, revision));
                slot.generation = self.generation;
                return true;
            }
            if self
                .entries
                .get(slot.entry as usize)
                .is_some_and(|entry| entry.0 == bits)
            {
                return true;
            }
        }
        false
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = &usize> {
        self.entries.iter().map(|(bits, _)| bits)
    }

    pub(super) fn rebase(&mut self, revision: LispCollectionRevision) {
        for (_, read_at) in &mut self.entries {
            *read_at = revision;
        }
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.index.fill(ReadSlot::default());
            self.generation = 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_capture_index_keeps_first_revision_and_reuses_generation() {
        let mut reads = CaptureReads::default();
        let first = LispCollectionRevision::from_sequence(7);
        let later = LispCollectionRevision::from_sequence(11);
        for bits in 0..MAX_READS {
            assert!(reads.insert_first(bits, first));
        }
        assert_eq!(reads.len(), MAX_READS);
        assert!(
            reads.insert_first(0, later),
            "a full capture admits existing identities"
        );
        assert_eq!(reads.get(&0), Some(&first));
        assert!(!reads.insert_first(MAX_READS, later));
        assert_eq!(reads.get(&MAX_READS), None);
        reads.clear();
        assert_eq!(reads.get(&0), None);
        assert!(reads.insert_first(MAX_READS, later));
        assert_eq!(reads.get(&MAX_READS), Some(&later));
        reads.rebase(first);
        assert_eq!(reads.get(&MAX_READS), Some(&first));
    }

    #[test]
    fn capture_index_generation_wrap_cannot_revive_old_entries() {
        let mut reads = CaptureReads::default();
        let revision = LispCollectionRevision::from_sequence(7);
        assert!(reads.insert_first(0, revision));
        reads.generation = u32::MAX;
        assert!(reads.insert_first(1, revision));
        reads.clear();
        assert_eq!(reads.get(&0), None);
        assert_eq!(reads.get(&1), None);
        assert!(reads.insert_first(2, revision));
        assert_eq!(reads.get(&2), Some(&revision));
    }
}
