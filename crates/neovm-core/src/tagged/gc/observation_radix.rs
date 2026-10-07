//! Permanent address metadata for collection observations.
//!
//! Threading: radix links are published once, from null to a fully initialized
//! allocation, with Release stores. Readers use Acquire loads and never lock,
//! clone reference counts, or dereference Lisp storage. Nodes and bitmaps are
//! permanent so native code may retain an exact AtomicU64 word address. The
//! insertion mutex protects only absent-node allocation; it never protects a
//! query, existing-word mark, or clear. No address stored here roots an object.

use super::CONS_OBSERVED_WORDS;
use std::alloc::{Layout, alloc_zeroed, handle_alloc_error};
use std::sync::{
    Mutex,
    atomic::{AtomicPtr, AtomicU64, Ordering},
};

const RADIX_BITS: usize = 16;
const RADIX_SIZE: usize = 1 << RADIX_BITS;
const RADIX_MASK: usize = RADIX_SIZE - 1;
// One ObservationBits covers one cons block (64 KiB on every target).
const GRANULE_SHIFT: u32 = super::super::cons_block_trailer::CONS_BLOCK_BYTES.trailing_zeros();
const MID_SHIFT: u32 = GRANULE_SHIFT + RADIX_BITS as u32;
const TOP_SHIFT: u32 = GRANULE_SHIFT + 2 * RADIX_BITS as u32;

/// A level index for `address`. Shifts at or beyond the pointer width fold
/// to slot 0: on the 64-bit host the three levels cover bits 16..63 exactly
/// as before, while a 32-bit host needs only the granule level (its whole
/// address space is bits 0..31, so `MID_SHIFT`/`TOP_SHIFT` saturate).
#[inline]
const fn level_index(address: usize, shift: u32) -> usize {
    if shift >= usize::BITS {
        0
    } else {
        (address >> shift) & RADIX_MASK
    }
}

struct Middle {
    leaves: [AtomicPtr<Leaf>; RADIX_SIZE],
}

struct Leaf {
    granules: [AtomicPtr<ObservationBits>; RADIX_SIZE],
}

pub(super) struct ObservationBits {
    pub(super) cons: [AtomicU64; CONS_OBSERVED_WORDS],
    pub(super) noncons: [AtomicU64; CONS_OBSERVED_WORDS],
}

static ROOT: [AtomicPtr<Middle>; RADIX_SIZE] =
    [const { AtomicPtr::new(std::ptr::null_mut()) }; RADIX_SIZE];
static INSERTION_LOCK: Mutex<()> = Mutex::new(());

/// A fixed-depth query covering every 64-bit address, including foreign cells.
#[inline]
pub(super) fn lookup(address: usize) -> Option<&'static ObservationBits> {
    let middle = ROOT[level_index(address, TOP_SHIFT)].load(Ordering::Acquire);
    // SAFETY: each non-null link points to an initialized permanent allocation.
    let middle = unsafe { middle.as_ref() }?;
    let leaf = middle.leaves[level_index(address, MID_SHIFT)].load(Ordering::Acquire);
    let leaf = unsafe { leaf.as_ref() }?;
    let bits = leaf.granules[level_index(address, GRANULE_SHIFT)].load(Ordering::Acquire);
    unsafe { bits.as_ref() }
}

#[inline]
pub(super) fn lookup_or_register(address: usize) -> &'static ObservationBits {
    lookup(address).unwrap_or_else(|| register(address))
}

/// Allocate large nodes directly, avoiding a 512 KiB temporary on the stack.
///
/// # Safety
/// `T` must consist only of atomic null pointers or zero-valued atomic words,
/// for which an all-zero allocation is a valid initialized representation.
unsafe fn permanent_zeroed<T>() -> *mut T {
    let layout = Layout::new::<T>();
    let allocation = unsafe { alloc_zeroed(layout) }.cast::<T>();
    if allocation.is_null() {
        handle_alloc_error(layout);
    }
    allocation
}

#[cold]
#[inline(never)]
fn register(address: usize) -> &'static ObservationBits {
    let _insertion = INSERTION_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(bits) = lookup(address) {
        return bits;
    }

    let root = &ROOT[level_index(address, TOP_SHIFT)];
    let mut middle = root.load(Ordering::Acquire);
    if middle.is_null() {
        // SAFETY: Middle consists solely of atomic null pointers.
        middle = unsafe { permanent_zeroed::<Middle>() };
        root.store(middle, Ordering::Release);
    }
    // SAFETY: all nodes are initialized before publication and never freed;
    // only this insertion lock can replace a null child pointer.
    let middle = unsafe { &*middle };
    let leaves = &middle.leaves[level_index(address, MID_SHIFT)];
    let mut leaf = leaves.load(Ordering::Acquire);
    if leaf.is_null() {
        // SAFETY: Leaf consists solely of atomic null pointers.
        leaf = unsafe { permanent_zeroed::<Leaf>() };
        leaves.store(leaf, Ordering::Release);
    }
    let leaf = unsafe { &*leaf };
    let slot = &leaf.granules[level_index(address, GRANULE_SHIFT)];
    // SAFETY: ObservationBits consists solely of zero-valued atomic words.
    let bits = unsafe { permanent_zeroed::<ObservationBits>() };
    slot.store(bits, Ordering::Release);
    unsafe { &*bits }
}

#[cfg(test)]
pub(super) fn with_insertion_locked<R>(f: impl FnOnce() -> R) -> R {
    let _insertion = INSERTION_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    f()
}
