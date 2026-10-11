//! Allocation guard for the common equal-start deletion case.
//! This dedicated integration-test binary owns its allocator. TLS contains
//! only test counters, never Lisp state; other threads do not affect a sample.
use neovm_core::buffer::{BufferId, EmacsByteRange, OverlayList};
use neovm_core::emacs_core::{Context, value::Value};
use neovm_core::heap_types::OverlayDataInit;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
struct CountedSystem;
fn count() {
    if COUNTING.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
    }
}
unsafe impl GlobalAlloc for CountedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountedSystem = CountedSystem;

#[test]
fn gde_review_equal_start_delete_has_no_ordering_scratch_allocations() {
    let _context = Context::new();
    let mut list = OverlayList::new();
    for _ in 0..2 {
        list.insert_overlay(Value::make_overlay(OverlayDataInit {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(BufferId(1)),
            start: 4,
            end: 100,
            front_advance: false,
            rear_advance: false,
        }));
    }
    // Warm symbol resolution, position handles, and write-barrier bookkeeping.
    for _ in 0..2 {
        list.adjust_for_delete_emacs_byte_range(EmacsByteRange::from_usize(4, 5));
    }
    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|enabled| enabled.set(true));
    list.adjust_for_delete_emacs_byte_range(EmacsByteRange::from_usize(4, 5));
    COUNTING.with(|enabled| enabled.set(false));
    let allocations = ALLOCATIONS.with(Cell::get);
    // The existing effects Vec may allocate once. No root paths, candidate
    // Vecs, or rank-sort scratch may add an allocation to this common case.
    assert!(
        allocations <= 1,
        "equal-start delete allocated {allocations} times"
    );
}
