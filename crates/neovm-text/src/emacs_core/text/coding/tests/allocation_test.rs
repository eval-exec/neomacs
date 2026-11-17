//! Count requested allocations on this test thread only. The allocator
//! delegates directly to System; no instrumentation is present in product builds.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static REQUESTS: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

struct CountingAllocator;

fn record_request(bytes: usize) {
    if COUNTING.try_with(Cell::get).unwrap_or(false) {
        let _ = REQUESTS.try_with(|requests| {
            let (calls, total) = requests.get();
            requests.set((calls + 1, total + bytes));
        });
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_request(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_request(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_request(size);
        unsafe { System.realloc(pointer, layout, size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Run F and return its value with the (calls, bytes) it asked the allocator for.
pub(super) fn measured<T>(f: impl FnOnce() -> T) -> (T, (usize, usize)) {
    struct StopCounting;
    impl Drop for StopCounting {
        fn drop(&mut self) {
            COUNTING.with(|enabled| enabled.set(false));
        }
    }
    REQUESTS.with(|requests| requests.set((0, 0)));
    COUNTING.with(|enabled| assert!(!enabled.replace(true)));
    let stop = StopCounting;
    let value = f();
    drop(stop);
    (value, REQUESTS.with(Cell::get))
}
