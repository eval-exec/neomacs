//! Sticky collection-read observations, separate from all collector marks.
//!
//! Non-cons owners use their header's observation byte. Cons cells have no
//! header, so a process-wide side bitmap records their exact aligned address,
//! including owned, mapped and foreign cells. No Lisp value is rooted here.
//!
//! Threading: an append-only radix publishes initialized, permanent atomic
//! bitmaps with Release; queries and repeated marks use Acquire without a
//! lock or reference-count operation. Only registration of an absent granule
//! takes a mutex. Clearing requires object-lifetime exclusion at destruction;
//! a stopped mutator cannot still observe a reclaimed object. These shared
//! marks do not change the existing mutator-local journals and certificates:
//! concurrent mutation by another mutator still has no certificate-coherence
//! guarantee. Mark publication precedes the observing mutator's dependency
//! snapshot; its exclusive Context publishes the JIT gate before any store.

use super::cons_block_trailer::{CONS_BLOCK_BYTES, ConsBlockTrailer};
use crate::tagged::header::{ConsCell, GcHeader, StringObj};
use crate::tagged::value::{TAG_CONS, TAG_FLOAT, TAG_MASK, TAG_STRING, TAG_VECLIKE};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[path = "observation_radix.rs"]
mod observation_radix;

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

// A monotonic process gate, containing no Lisp identities. Sweep needs one
// Acquire load per block and never registers metadata before the first mark.
// Publish it BEFORE its first sticky bit so a repeat observer's Acquire hit
// cannot reinstall a Context while the empty-observation guard still is false.
static HAS_CONS_MARKS: AtomicBool = AtomicBool::new(false);
static HAS_NONCONS_MARKS: AtomicBool = AtomicBool::new(false);

// A scalar, not a Lisp identity cache. Release advancement follows metadata
// clearing and precedes allocation-slot reuse or mutator resumption. Readers
// Acquire it before trusting the existing RECENT cache or narrowing a ledger.
static OBSERVATION_EPOCH: AtomicU64 = AtomicU64::new(0);

#[inline]
pub(crate) fn collection_observation_epoch() -> u64 {
    OBSERVATION_EPOCH.load(Ordering::Acquire)
}

/// Complete a reclamation batch after its sticky bits have been cleared.
#[inline]
pub(crate) fn advance_collection_observation_epoch() {
    OBSERVATION_EPOCH.fetch_add(1, Ordering::Release);
}

#[inline]
fn publish_has_marks(flag: &AtomicBool) {
    if !flag.load(Ordering::Acquire) {
        // A losing first publisher must acquire the winning transition before
        // publishing its own sticky bit, just like the already-true load.
        let _ = flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire);
    }
}

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

/// Obtain a process-stable full-width atomic word for a baked Cons guard.
/// Registration is cold and does not dereference or root the owner. Clearing
/// and reuse change this word's bits, never its address. Serialized AOT code
/// must not retain this process-specific pointer.
#[cold]
#[inline(never)]
pub(crate) fn cons_collection_observed_word(bits: usize) -> (&'static AtomicU64, u64) {
    debug_assert_eq!(bits & TAG_MASK, TAG_CONS);
    let address = bits & !TAG_MASK;
    debug_assert_ne!(address, 0);
    let (_, word, mask) = cons_address_bit(address);
    (
        &observation_radix::lookup_or_register(address).cons[word],
        mask,
    )
}

#[inline]
fn mark_cons_observed(address: usize) -> bool {
    let (_, index, mask) = cons_address_bit(address);
    let word = &observation_radix::lookup_or_register(address).cons[index];
    if word.load(Ordering::Acquire) & mask != 0 {
        return false;
    }
    publish_has_marks(&HAS_CONS_MARKS);
    word.fetch_or(mask, Ordering::Release) & mask == 0
}

#[inline]
fn mark_noncons_observed(address: usize, header: &GcHeader) -> bool {
    if header.collection_observed() {
        return false;
    }
    let (_, index, mask) = cons_address_bit(address);
    let word = &observation_radix::lookup_or_register(address).noncons[index];
    publish_has_marks(&HAS_NONCONS_MARKS);
    // A repeat observer may return from the header's Acquire check without
    // consulting metadata: publish the plane before the header's Release bit.
    if word.load(Ordering::Acquire) & mask == 0 {
        word.fetch_or(mask, Ordering::Release);
    }
    header.mark_collection_observed()
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
        TAG_CONS => mark_cons_observed(address),
        TAG_STRING => {
            // SAFETY: the caller supplies a live string owner. Complete both
            // sticky publications before the certificate's revision snapshot.
            let owner = unsafe { &*(address as *const StringObj) };
            let newly_set = mark_noncons_observed(address, &owner.header);
            // Also repair the owned mirror when this header was marked while
            // its payload was borrowed. Repeated owner observations are safe.
            owner.data.mark_owned_storage_collection_observed();
            newly_set
        }
        TAG_FLOAT | TAG_VECLIKE => {
            // SAFETY: a live non-cons heap owner begins with GcHeader. Raw
            // tag decoding avoids recursively observing while STATE is held.
            mark_noncons_observed(address, unsafe { &*(address as *const GcHeader) })
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
    let (_, word, mask) = cons_address_bit(address);
    observation_radix::lookup(address)
        .is_some_and(|bitmap| bitmap.cons[word].load(Ordering::Acquire) & mask != 0)
}

/// Query exact metadata even after the original owner has been reclaimed.
/// Neither plane lookup dereferences Lisp storage. A reused address that was
/// observed again can conservatively survive another mutator's old ledger;
/// certificates still require the caller to retain their original owners.
#[inline]
pub(crate) fn collection_observed_metadata(bits: usize) -> bool {
    let address = bits & !TAG_MASK;
    if address == 0 {
        return false;
    }
    let (_, word, mask) = cons_address_bit(address);
    match bits & TAG_MASK {
        TAG_CONS => {
            HAS_CONS_MARKS.load(Ordering::Acquire)
                && observation_radix::lookup(address)
                    .is_some_and(|bitmap| bitmap.cons[word].load(Ordering::Acquire) & mask != 0)
        }
        TAG_STRING | TAG_FLOAT | TAG_VECLIKE => {
            HAS_NONCONS_MARKS.load(Ordering::Acquire)
                && observation_radix::lookup(address)
                    .is_some_and(|bitmap| bitmap.noncons[word].load(Ordering::Acquire) & mask != 0)
        }
        _ => false,
    }
}

/// Retire an exact non-cons identity before clearing its header/freeing it.
/// No missing entry is registered. The caller completes header clearing and
/// then advances the epoch before reuse/resumption. Different live owners in
/// this granule can continue publishing their independent atomic bits.
#[inline]
pub(crate) fn clear_noncons_collection_observed_metadata(address: usize) -> bool {
    if !HAS_NONCONS_MARKS.load(Ordering::Acquire) {
        return false;
    }
    let Some(bitmap) = observation_radix::lookup(address) else {
        return false;
    };
    let (_, index, mask) = cons_address_bit(address);
    let word = &bitmap.noncons[index];
    if word.load(Ordering::Acquire) & mask == 0 {
        return false;
    }
    word.fetch_and(!mask, Ordering::Release) & mask != 0
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
    let Some(bitmap) = observation_radix::lookup(base) else {
        return;
    };
    let mut changed = false;
    let cells_per_side_word = u64::BITS as usize / OWNED_CELL_ADDRESS_STRIDE;
    for (word, observed) in bitmap.cons.iter().enumerate() {
        let current = observed.load(Ordering::Acquire);
        if current == 0 {
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
        if current & !keep != 0 {
            changed |= observed.fetch_and(keep, Ordering::Release) & !keep != 0;
        }
    }
    if changed {
        advance_collection_observation_epoch();
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
    if let Some(bitmap) = observation_radix::lookup(base) {
        let mut changed = false;
        for word in &bitmap.cons {
            if word.load(Ordering::Acquire) != 0 {
                // Whole-granule ownership excludes concurrent new marks.
                word.store(0, Ordering::Release);
                changed = true;
            }
        }
        if changed {
            advance_collection_observation_epoch();
        }
    }
}

#[cfg(test)]
fn with_collection_observed_registry_locked<R>(f: impl FnOnce() -> R) -> R {
    observation_radix::with_insertion_locked(f)
}

#[cfg(test)]
#[path = "tests/collection_observed_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/collection_observed_registry_tests.rs"]
mod registry_tests;
