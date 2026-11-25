//! NDP/1 display wire contracts (DESIGN §6.6-§6.11; MILESTONES M2).

#[path = "wire/assets_test.rs"]
mod assets_test;
#[path = "wire/codec_test.rs"]
mod codec_test;
#[path = "wire/corpus.rs"]
mod corpus;
#[path = "wire/framesync_test.rs"]
mod framesync_test;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Records the largest single allocation while armed, so decoding can be
/// shown to allocate in proportion to its input (`decode_is_total`).
struct TrackingAllocator;

static ARMED: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every method forwards to `System` with the caller's arguments
// unchanged; the only addition is a relaxed atomic update, which neither
// allocates nor touches the returned memory.
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        // SAFETY: forwarded unchanged (see the impl's safety comment).
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        // SAFETY: forwarded unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            LARGEST.fetch_max(new_size, Ordering::Relaxed);
        }
        // SAFETY: forwarded unchanged.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

/// Run `f` and report the largest single allocation it made.
pub fn largest_allocation<R>(f: impl FnOnce() -> R) -> (R, usize) {
    LARGEST.store(0, Ordering::SeqCst);
    ARMED.store(true, Ordering::SeqCst);
    let result = f();
    ARMED.store(false, Ordering::SeqCst);
    (result, LARGEST.load(Ordering::SeqCst))
}
