//! Sticky collection-read observations, separate from all collector marks.
//!
//! Non-cons owners use their header's observation byte. Cons cells have no
//! header, so a process-wide side bitmap records their exact aligned address,
//! including owned, mapped and foreign cells. No Lisp value is rooted here.
//!
//! Threading: the registry publishes initialized, shared atomic bitmaps under
//! its mutex; observations use Release and queries use Acquire. Bitmap Arcs
//! remain registered so a concurrent observer cannot publish into a detached
//! bitmap. Clearing requires object-lifetime exclusion at sweep/destruction;
//! a stopped mutator cannot still observe a reclaimed object. These shared
//! marks do not change the existing mutator-local journals and certificates:
//! concurrent mutation by another mutator still has no certificate-coherence
//! guarantee. Mark publication precedes the observing mutator's dependency
//! snapshot; its exclusive Context publishes the JIT gate before any store.

use super::cons_block_trailer::{CONS_BLOCK_BYTES, ConsBlockTrailer};
use crate::tagged::header::{ConsCell, GcHeader, StringObj};
use crate::tagged::value::{TAG_CONS, TAG_FLOAT, TAG_MASK, TAG_STRING, TAG_VECLIKE};
use rustc_hash::FxHashMap;
use std::sync::{
    Arc, LazyLock, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

// Mapped/static cells may be only 8-aligned, unlike the 16-byte stride of an
// owned block. Index alignment-sized addresses, not owned-block cell indices.
const CONS_ADDRESS_ALIGN: usize = std::mem::align_of::<ConsCell>();
const CONS_ADDRESSES_PER_GRANULE: usize = CONS_BLOCK_BYTES / CONS_ADDRESS_ALIGN;
const CONS_OBSERVED_WORDS: usize = CONS_ADDRESSES_PER_GRANULE / u64::BITS as usize;
const OWNED_CELL_ADDRESS_STRIDE: usize = std::mem::size_of::<ConsCell>() / CONS_ADDRESS_ALIGN;

const _: () = {
    assert!(CONS_ADDRESS_ALIGN == 8);
    assert!(CONS_OBSERVED_WORDS == 128);
    assert!(OWNED_CELL_ADDRESS_STRIDE == 2);
};

struct ConsObservedBits {
    words: [AtomicU64; CONS_OBSERVED_WORDS],
}

impl ConsObservedBits {
    fn new() -> Self {
        Self {
            words: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
}

static CONS_OBSERVED: LazyLock<Mutex<FxHashMap<usize, Arc<ConsObservedBits>>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

// A monotonic process gate, containing no Lisp identities. Sweep needs one
// Acquire load per block and never initializes/locks the registry before the
// first observed cons. It is set before mark_collection_observed returns.
static HAS_CONS_MARKS: AtomicBool = AtomicBool::new(false);
static HAS_NONCONS_MARKS: AtomicBool = AtomicBool::new(false);

/// Avoid initializing an otherwise unused mutator journal on heap installation.
/// These process flags contain no Lisp identities and never reset. Release
/// publication completes before the marking mutator snapshots a read or
/// publishes its envelope; its subsequent Acquire query cannot miss that mark.
/// Another mutator's true result only causes a conservative local lookup.
#[inline]
pub(crate) fn has_collection_observations() -> bool {
    HAS_CONS_MARKS.load(Ordering::Acquire) || HAS_NONCONS_MARKS.load(Ordering::Acquire)
}

/// Select observation-aware destruction once per arena sweep, preserving its
/// ordinary inner loop until any process mutator has observed a non-cons.
#[inline]
pub(super) fn has_noncons_collection_observations() -> bool {
    HAS_NONCONS_MARKS.load(Ordering::Acquire)
}

#[inline]
fn cons_address_bit(address: usize) -> (usize, usize, u64) {
    let base = address & !(CONS_BLOCK_BYTES - 1);
    let index = (address - base) / CONS_ADDRESS_ALIGN;
    (
        base,
        index / u64::BITS as usize,
        1u64 << (index % u64::BITS as usize),
    )
}

/// Publish an exact sticky mark for a live heap owner, without observing a
/// pointer projection or consulting the active heap. The caller gates this
/// API on observed mode and publishes its own JIT address envelope before
/// any subsequent compiled mutation. Returns whether the shared mark is new.
#[cold]
#[inline(never)]
pub(crate) fn mark_collection_observed(bits: usize) -> bool {
    let address = bits & !TAG_MASK;
    if address == 0 {
        return false;
    }
    match bits & TAG_MASK {
        TAG_CONS => {
            let (base, word, mask) = cons_address_bit(address);
            let bitmap = {
                let mut registry = CONS_OBSERVED
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                Arc::clone(
                    registry
                        .entry(base)
                        .or_insert_with(|| Arc::new(ConsObservedBits::new())),
                )
            };
            let newly_set = bitmap.words[word].fetch_or(mask, Ordering::Release) & mask == 0;
            HAS_CONS_MARKS.store(true, Ordering::Release);
            newly_set
        }
        TAG_STRING => {
            // SAFETY: the caller supplies a live string owner. Complete both
            // sticky publications before the certificate's revision snapshot.
            let owner = unsafe { &*(address as *const StringObj) };
            let newly_set = owner.header.mark_collection_observed();
            // Also repair the owned mirror when this header was marked while
            // its payload was borrowed. Repeated owner observations are safe.
            owner.data.mark_owned_storage_collection_observed();
            HAS_NONCONS_MARKS.store(true, Ordering::Release);
            newly_set
        }
        TAG_FLOAT | TAG_VECLIKE => {
            // SAFETY: a live non-cons heap owner begins with GcHeader. Raw
            // tag decoding avoids recursively observing while STATE is held.
            let newly_set = unsafe { &*(address as *const GcHeader) }.mark_collection_observed();
            HAS_NONCONS_MARKS.store(true, Ordering::Release);
            newly_set
        }
        _ => false,
    }
}

/// Query a live owner's sticky mark. The side bitmap makes no assumption
/// about which Context owns a cons or whether it has an owned-block trailer.
#[inline]
pub(crate) fn collection_observed(bits: usize) -> bool {
    let address = bits & !TAG_MASK;
    if address == 0 {
        return false;
    }
    match bits & TAG_MASK {
        TAG_CONS => {
            if !HAS_CONS_MARKS.load(Ordering::Acquire) {
                return false;
            }
            cons_observed(address)
        }
        TAG_STRING | TAG_FLOAT | TAG_VECLIKE => {
            // SAFETY: the caller supplies a live owner, as for the marking API.
            unsafe { &*(address as *const GcHeader) }.collection_observed()
        }
        _ => false,
    }
}

#[cold]
#[inline(never)]
fn cons_observed(address: usize) -> bool {
    let (base, word, mask) = cons_address_bit(address);
    let bitmap = {
        let registry = CONS_OBSERVED
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        registry.get(&base).map(Arc::clone)
    };
    bitmap.is_some_and(|bitmap| bitmap.words[word].load(Ordering::Acquire) & mask != 0)
}

/// Clear only dead cells before a stopped-world owned-block sweep puts them
/// on the free list. Live observations survive every collection. Generational
/// sweep retains old|marked cells, matching ConsBlock's own liveness test.
#[inline]
pub(super) fn clear_cons_observed_dead(
    base: usize,
    trailer: &ConsBlockTrailer,
    cells: usize,
    generational: bool,
) {
    if HAS_CONS_MARKS.load(Ordering::Acquire) {
        clear_cons_observed_dead_slow(base, trailer, cells, generational);
    }
}

#[cold]
#[inline(never)]
fn clear_cons_observed_dead_slow(
    base: usize,
    trailer: &ConsBlockTrailer,
    cells: usize,
    generational: bool,
) {
    let bitmap = {
        let registry = CONS_OBSERVED
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        registry.get(&base).map(Arc::clone)
    };
    let Some(bitmap) = bitmap else { return };
    let cells_per_side_word = u64::BITS as usize / OWNED_CELL_ADDRESS_STRIDE;
    for (word, observed) in bitmap.words.iter().enumerate() {
        if observed.load(Ordering::Acquire) == 0 {
            continue;
        }
        let first = word * cells_per_side_word;
        let keep = if first >= cells {
            0
        } else {
            let live = trailer.live_word(first / u64::BITS as usize, generational) as u64;
            let live = (live >> (first % u64::BITS as usize)) as u32;
            let remaining = (cells - first).min(cells_per_side_word);
            let valid = u32::MAX >> (u32::BITS as usize - remaining);
            spread_live_bits(live & valid)
        };
        observed.fetch_and(keep, Ordering::Release);
    }
}

// An owned cell's 16-byte stride covers every second 8-byte address bit.
#[inline]
fn spread_live_bits(live: u32) -> u64 {
    let mut bits = u64::from(live);
    bits = (bits | (bits << 16)) & 0x0000_ffff_0000_ffff;
    bits = (bits | (bits << 8)) & 0x00ff_00ff_00ff_00ff;
    bits = (bits | (bits << 4)) & 0x0f0f_0f0f_0f0f_0f0f;
    bits = (bits | (bits << 2)) & 0x3333_3333_3333_3333;
    (bits | (bits << 1)) & 0x5555_5555_5555_5555
}

/// Reset an exclusively owned whole granule before block destruction or
/// first allocation. No live external object may occupy this allocation.
#[inline]
pub(super) fn clear_cons_observed_block(base: usize) {
    if HAS_CONS_MARKS.load(Ordering::Acquire) {
        clear_cons_observed_block_slow(base);
    }
}

#[cold]
#[inline(never)]
fn clear_cons_observed_block_slow(base: usize) {
    let bitmap = {
        let registry = CONS_OBSERVED
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        registry.get(&base).map(Arc::clone)
    };
    if let Some(bitmap) = bitmap {
        for word in &bitmap.words {
            word.store(0, Ordering::Release);
        }
    }
}

#[cfg(test)]
fn with_collection_observed_registry_locked<R>(f: impl FnOnce() -> R) -> R {
    let _registry = CONS_OBSERVED
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    f()
}

#[cfg(test)]
#[path = "tests/collection_observed_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/collection_observed_registry_tests.rs"]
mod registry_tests;
