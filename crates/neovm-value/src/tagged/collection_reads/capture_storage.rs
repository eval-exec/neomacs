//! Bounded first-read revisions with O(1) reuse between captures.
//!
//! Dense entries retain first-read order/revisions. The index is generation
//! stamped: recycling a capture clears its used entry prefix, not a whole
//! MAX_READS hash table. Generation wrap explicitly resets the index.
use super::{LispCollectionRevision, MAX_DEPTH, MAX_READS};
use rustc_hash::FxHasher;
use std::hash::Hasher;

const INDEX_SIZE: usize = MAX_READS * 2;
// Start probing inside a 1 KiB L1-resident prefix (128 x 8-byte slots) and
// double the active mask inside the pre-allocated `index` only when a capture
// actually grows past 50% load. Zero extra heap allocations on growth or reuse.
const INITIAL_ACTIVE_SIZE: usize = 128;
const _: () = {
    assert!(INDEX_SIZE.is_power_of_two());
    assert!(INITIAL_ACTIVE_SIZE.is_power_of_two());
    assert!(INITIAL_ACTIVE_SIZE <= INDEX_SIZE);
    assert!(MAX_READS <= u32::MAX as usize);
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InsertFirstOutcome {
    Existing,
    Inserted,
    Overflow,
}

#[derive(Clone, Copy, Default)]
struct ReadSlot {
    generation: u32,
    entry: u32,
}

pub(super) struct CaptureReads {
    entries: Vec<(usize, LispCollectionRevision)>,
    index: Box<[ReadSlot]>,
    active_size: usize,
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
            active_size: INITIAL_ACTIVE_SIZE,
            generation: 1,
        }
    }
}

impl CaptureReads {
    #[inline(always)]
    fn hash(bits: usize) -> usize {
        let mut hash = FxHasher::default();
        hash.write_usize(bits);
        hash.finish() as usize
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn get(&self, bits: &usize) -> Option<&LispCollectionRevision> {
        let mask = self.active_size - 1;
        let bucket = Self::hash(*bits) & mask;
        for offset in 0..self.active_size {
            let slot = self.index[(bucket + offset) & mask];
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
    /// Reports whether `bits` was already present, newly inserted, or rejected
    /// by the unchanged read budget.
    #[inline]
    pub(super) fn insert_first_outcome(
        &mut self,
        bits: usize,
        revision: LispCollectionRevision,
    ) -> InsertFirstOutcome {
        let mask = self.active_size - 1;
        let hash = Self::hash(bits);
        let bucket = hash & mask;
        for offset in 0..self.active_size {
            let slot_idx = (bucket + offset) & mask;
            let slot = self.index[slot_idx];
            if slot.generation != self.generation {
                if self.entries.len() == MAX_READS {
                    return InsertFirstOutcome::Overflow;
                }
                if (self.entries.len() + 1) * 2 > self.active_size && self.active_size < INDEX_SIZE
                {
                    self.grow_active();
                    let entry_idx = self.entries.len() as u32;
                    self.entries.push((bits, revision));
                    let new_mask = self.active_size - 1;
                    let mut new_slot = hash & new_mask;
                    while self.index[new_slot].generation == self.generation {
                        new_slot = (new_slot + 1) & new_mask;
                    }
                    self.index[new_slot] = ReadSlot {
                        generation: self.generation,
                        entry: entry_idx,
                    };
                    return InsertFirstOutcome::Inserted;
                }
                let entry_idx = self.entries.len() as u32;
                self.entries.push((bits, revision));
                self.index[slot_idx] = ReadSlot {
                    generation: self.generation,
                    entry: entry_idx,
                };
                return InsertFirstOutcome::Inserted;
            }
            if self
                .entries
                .get(slot.entry as usize)
                .is_some_and(|entry| entry.0 == bits)
            {
                return InsertFirstOutcome::Existing;
            }
        }
        InsertFirstOutcome::Overflow
    }

    /// Keep the first revision, including when a full capture sees a repeat.
    /// False means a new identity would exceed the unchanged read budget.
    pub(super) fn insert_first(&mut self, bits: usize, revision: LispCollectionRevision) -> bool {
        !matches!(
            self.insert_first_outcome(bits, revision),
            InsertFirstOutcome::Overflow
        )
    }

    #[cold]
    #[inline(never)]
    fn grow_active(&mut self) {
        self.active_size = (self.active_size * 2).min(INDEX_SIZE);
        self.bump_generation();
        let mask = self.active_size - 1;
        for (entry_idx, &(bits, _)) in self.entries.iter().enumerate() {
            let mut slot_idx = Self::hash(bits) & mask;
            while self.index[slot_idx].generation == self.generation {
                slot_idx = (slot_idx + 1) & mask;
            }
            self.index[slot_idx] = ReadSlot {
                generation: self.generation,
                entry: entry_idx as u32,
            };
        }
    }

    #[inline]
    fn bump_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.index.fill(ReadSlot::default());
            self.generation = 1;
        }
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
        self.active_size = INITIAL_ACTIVE_SIZE;
        self.bump_generation();
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

    #[test]
    fn adaptive_active_size_grows_on_load_and_resets_cleanly_on_clear() {
        let mut reads = CaptureReads::default();
        let first = LispCollectionRevision::from_sequence(3);
        let later = LispCollectionRevision::from_sequence(9);
        assert_eq!(reads.active_size, INITIAL_ACTIVE_SIZE);

        // Stay inside the L1-resident initial prefix for small captures.
        for bits in 0..(INITIAL_ACTIVE_SIZE / 2) {
            assert_eq!(
                reads.insert_first_outcome(bits, first),
                InsertFirstOutcome::Inserted
            );
        }
        assert_eq!(reads.active_size, INITIAL_ACTIVE_SIZE);
        assert_eq!(
            reads.insert_first_outcome(0, later),
            InsertFirstOutcome::Existing
        );
        assert_eq!(reads.get(&0), Some(&first));

        // Crossing 50% load doubles active_size without losing prior entries.
        for bits in (INITIAL_ACTIVE_SIZE / 2)..(INITIAL_ACTIVE_SIZE * 4) {
            assert_eq!(
                reads.insert_first_outcome(bits, first),
                InsertFirstOutcome::Inserted
            );
        }
        assert!(reads.active_size > INITIAL_ACTIVE_SIZE);
        for bits in 0..(INITIAL_ACTIVE_SIZE * 4) {
            assert_eq!(reads.get(&bits), Some(&first));
        }

        // Clearing returns to INITIAL_ACTIVE_SIZE and invalidates the full table.
        reads.clear();
        assert_eq!(reads.active_size, INITIAL_ACTIVE_SIZE);
        for bits in 0..(INITIAL_ACTIVE_SIZE * 4) {
            assert_eq!(reads.get(&bits), None);
        }

        // Re-inserting an old key after clear() must be treated as a fresh
        // insertion and store the new revision cleanly in the reset table.
        assert_eq!(
            reads.insert_first_outcome(0, later),
            InsertFirstOutcome::Inserted
        );
        assert_eq!(reads.get(&0), Some(&later));
    }
}
