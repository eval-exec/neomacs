//! Owner-local SATB logs and the marker's batch handoff.
//!
//! Each buffer has exactly one mutator owner, including after a Context moves
//! between OS threads under the runtime's serialized-owner admission. Only
//! completed chunks cross threads. No referent can be reclaimed before the
//! stopped-world termination folds every published and owner-local word.

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::mark_word::MarkWord;
use super::scan_contract::StoppedMutatorsProof;

#[cfg(not(test))]
pub(super) const SATB_CHUNK_WORDS: usize = 256;
#[cfg(test)]
pub(super) const SATB_CHUNK_WORDS: usize = 4;

/// Poison proves an interruption inside correctness-critical queue mutation.
#[derive(Debug, thiserror::Error)]
#[error("SATB handoff was poisoned")]
pub(super) struct SatbPoisoned;

/// An owned batch, created only by an owner-local buffer.
#[derive(Debug)]
#[must_use = "a discarded chunk loses SATB pre-images"]
pub(super) struct SatbChunk(Vec<MarkWord>);

impl SatbChunk {
    pub(super) fn into_words(self) -> Vec<MarkWord> {
        self.0
    }

    pub(super) fn len(&self) -> usize {
        self.0.len()
    }
}

/// An explicitly completed retirement, with no remaining owner-local words.
#[derive(Debug)]
#[must_use = "acknowledge completed retirement before removing the mutator"]
pub(super) struct SatbRetired(());

impl SatbRetired {
    pub(super) fn acknowledge(self) {}
}

/// Allocated word storage. Counts do not imply the queued words are roots.
#[cfg(any(test, feature = "gc-memory-telemetry"))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct SatbStorageUsage {
    pub(super) words: usize,
    pub(super) capacity_words: usize,
}

/// Handoff storage includes the chunk descriptors and each chunk's payload.
#[cfg(any(test, feature = "gc-memory-telemetry"))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct SatbHandoffUsage {
    pub(super) chunks: usize,
    pub(super) capacity_chunks: usize,
    pub(super) payload: SatbStorageUsage,
}

/// Only the owning mutator pushes or publishes this log. Stopped-world access
/// requires a proof minted by the closed collector admission.
#[derive(Debug)]
pub(super) struct SatbBuffer {
    words: Vec<MarkWord>,
    _owner: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(SatbBuffer: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(SatbBuffer: std::fmt::Debug);
static_assertions::assert_impl_all!(SatbChunk: Send, Sync, std::fmt::Debug);
static_assertions::assert_not_impl_any!(SatbChunk: Clone, Copy);
static_assertions::assert_impl_all!(SatbPoisoned: Send, Sync, std::error::Error);

impl Default for SatbBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl SatbBuffer {
    pub(super) fn new() -> Self {
        Self {
            words: Vec::with_capacity(SATB_CHUNK_WORDS),
            _owner: PhantomData,
        }
    }

    #[inline(always)]
    pub(super) fn push(&mut self, word: MarkWord, handoff: &SatbHandoff) {
        if self.words.len() == SATB_CHUNK_WORDS {
            self.publish_full(handoff);
        }
        self.words.push(word);
    }

    #[cold]
    #[inline(never)]
    fn publish_full(&mut self, handoff: &SatbHandoff) {
        self.publish_partial(handoff)
            .expect("cannot continue a mutation after SATB poison");
    }

    pub(super) fn publish_partial(&mut self, handoff: &SatbHandoff) -> Result<(), SatbPoisoned> {
        if self.words.is_empty() {
            return handoff.check_poisoned();
        }
        let words = std::mem::replace(&mut self.words, Vec::with_capacity(SATB_CHUNK_WORDS));
        handoff.publish(SatbChunk(words))
    }

    /// Called by explicit owner unregistration, never by a destructor. On
    /// poison the epoch cannot sweep; the caller must abandon the collector.
    #[must_use = "retirement must succeed before unregistering the mutator"]
    pub(super) fn retire(mut self, handoff: &SatbHandoff) -> Result<SatbRetired, SatbPoisoned> {
        self.publish_partial(handoff)?;
        Ok(SatbRetired(()))
    }

    /// The collector's stopped-world proof is the only production drain key.
    pub(super) fn take_partial(&mut self, _: &StoppedMutatorsProof) -> Option<SatbChunk> {
        if self.words.is_empty() {
            None
        } else {
            Some(SatbChunk(std::mem::replace(
                &mut self.words,
                Vec::with_capacity(SATB_CHUNK_WORDS),
            )))
        }
    }

    /// Abandonment has retained every marker-readable allocation. No lock or
    /// callback is permitted here: TaggedHeap invokes this from its Drop.
    pub(super) fn abandon(&mut self) {
        self.words.clear();
    }

    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    #[cfg(any(test, feature = "gc-memory-telemetry"))]
    pub(super) fn storage_usage(&self) -> SatbStorageUsage {
        SatbStorageUsage {
            words: self.words.len(),
            capacity_words: self.words.capacity(),
        }
    }

    #[cfg(test)]
    pub(super) fn words_for_test(&self) -> &[MarkWord] {
        &self.words
    }
}

// NOTE: this implements the approved note's explicit debug drop-bomb request.
// It intentionally conflicts with the general C-DTOR-FAIL rule, so the lane
// report must record the exception. Production Drop never locks or panics;
// unwinding never introduces a second panic even in a debug build.
impl Drop for SatbBuffer {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if !std::thread::panicking() {
            debug_assert!(self.words.is_empty(), "unretired non-empty SATB buffer");
        }
    }
}

/// Completed batches shared by all mutators of one heap and its marker.
/// Alignment reserves an entire adjacent-line pair for the payload, including
/// when Arc places its refcounts before the separately aligned payload.
#[repr(align(128))]
#[derive(Debug, Default)]
pub(super) struct SatbHandoff {
    pending: AtomicUsize,
    chunks: Mutex<Vec<SatbChunk>>,
}

static_assertions::assert_impl_all!(SatbHandoff: Send, Sync, std::fmt::Debug);
static_assertions::const_assert_eq!(std::mem::align_of::<SatbHandoff>(), 128);
static_assertions::const_assert_eq!(std::mem::size_of::<SatbHandoff>(), 128);

// ArcInner uses a repr(C) strong/weak header followed by the aligned payload.
// Count its requested allocation extent, including the header's padding; the
// marker's temporary Arc clone does not create another allocation.
#[repr(C)]
#[cfg(any(test, feature = "gc-memory-telemetry"))]
struct SatbArcAllocationLayout {
    strong: AtomicUsize,
    weak: AtomicUsize,
    payload: SatbHandoff,
}

#[cfg(any(test, feature = "gc-memory-telemetry"))]
pub(super) const SATB_HANDOFF_ALLOCATION_BYTES: usize =
    std::mem::size_of::<SatbArcAllocationLayout>();
#[cfg(any(test, feature = "gc-memory-telemetry"))]
static_assertions::const_assert_eq!(SATB_HANDOFF_ALLOCATION_BYTES, 256);

impl SatbHandoff {
    pub(super) fn new() -> Self {
        Self::default()
    }

    fn publish(&self, chunk: SatbChunk) -> Result<(), SatbPoisoned> {
        let mut chunks = self.chunks.lock().map_err(|_| SatbPoisoned)?;
        chunks.push(chunk);
        // Publication and reset both occur UNDER this mutex. Moving this
        // store past unlock permits a consumer reset to suppress a new batch.
        self.pending.store(chunks.len(), Ordering::Release);
        Ok(())
    }

    /// An idle marker reads the pending line without writing the mutex line.
    pub(super) fn take_all(&self) -> Result<Vec<SatbChunk>, SatbPoisoned> {
        self.check_poisoned()?;
        if self.pending.load(Ordering::Acquire) == 0 {
            return Ok(Vec::new());
        }
        let mut chunks = self.chunks.lock().map_err(|_| SatbPoisoned)?;
        let taken = std::mem::take(&mut *chunks);
        self.pending.store(0, Ordering::Release);
        Ok(taken)
    }

    pub(super) fn check_poisoned(&self) -> Result<(), SatbPoisoned> {
        if self.is_poisoned() {
            Err(SatbPoisoned)
        } else {
            Ok(())
        }
    }

    pub(super) fn is_poisoned(&self) -> bool {
        self.chunks.is_poisoned()
    }

    pub(super) fn is_empty(&self) -> Result<bool, SatbPoisoned> {
        self.check_poisoned()?;
        Ok(self.pending.load(Ordering::Acquire) == 0)
    }

    #[cfg(any(test, feature = "gc-memory-telemetry"))]
    pub(super) fn storage_usage(&self) -> Result<SatbHandoffUsage, SatbPoisoned> {
        let chunks = self.chunks.lock().map_err(|_| SatbPoisoned)?;
        let mut usage = SatbHandoffUsage {
            chunks: chunks.len(),
            capacity_chunks: chunks.capacity(),
            payload: SatbStorageUsage::default(),
        };
        for chunk in &*chunks {
            usage.payload.words += chunk.0.len();
            usage.payload.capacity_words += chunk.0.capacity();
        }
        Ok(usage)
    }

    #[cfg(test)]
    pub(super) fn poison_for_test(&self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = self.chunks.lock().unwrap();
            panic!("poison a correctness-critical SATB handoff");
        }));
    }

    #[cfg(test)]
    pub(super) fn snapshot_words_for_test(&self) -> Result<Vec<MarkWord>, SatbPoisoned> {
        let chunks = self.chunks.lock().map_err(|_| SatbPoisoned)?;
        Ok(chunks
            .iter()
            .flat_map(|chunk| chunk.0.iter().copied())
            .collect())
    }
}

#[cfg(test)]
#[path = "tests/satb_buffer_test.rs"]
mod tests;
