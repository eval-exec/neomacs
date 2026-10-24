//! Count requested allocations on this test thread only. The disabled allocator
//! delegates directly to System; no instrumentation is present in product builds.
use super::*;
use std::alloc::{GlobalAlloc, Layout, System};

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

fn measured<T>(f: impl FnOnce() -> T) -> (T, (usize, usize)) {
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

/// Independent phase attribution: map recording never publishes fake owners,
/// and the envelope test handles numeric identities without dereferencing them.
#[test]
fn collection_reads_allocation_phase_counters() {
    for count in [24, 512] {
        // Warm initialization and nesting-vector storage independently of the
        // measured capture. A live certificate cannot donate its storage.
        let (_, retained) = capture(|| ());
        let (scope, begin) = measured(CollectionReadScope::begin);
        let (_, recording) = measured(|| {
            with_state(|state| {
                let mut state = state.borrow_mut();
                for index in 1..=count {
                    record_observation(
                        &mut state,
                        (index << 8) | super::super::value::TAG_CONS,
                        LispCollectionRevision::current(),
                        None,
                    );
                }
            });
        });
        let (certificate, finish) = measured(|| scope.finish());
        let certificate = certificate.expect("numeric dependency capture is coherent");
        let mut found = 0;
        assert!(certificate.all_dependency_bits(|_| {
            found += 1;
            true
        }));
        assert_eq!(found, count);
        assert!(retained.unwrap().unchanged());

        let mut envelope = observed_envelope::ObservedEnvelope::new();
        let (_, insertion) = measured(|| {
            for index in 1..=count {
                envelope.insert(
                    (index << 8) | super::super::value::TAG_CONS,
                    NativeWindowCollectionOwner::Cons,
                );
            }
        });
        let (_, repeat_insertion) = measured(|| {
            for index in 1..=count {
                assert!(!envelope.insert(
                    (index << 8) | super::super::value::TAG_CONS,
                    NativeWindowCollectionOwner::Cons,
                ));
            }
        });
        assert_eq!(repeat_insertion, (0, 0));
        eprintln!(
            "obsalloc owners={count} begin={begin:?} record={recording:?} finish={finish:?} envelope_first={insertion:?} envelope_repeat={repeat_insertion:?} (calls, requested bytes)"
        );
    }
}

/// Real pointer projections, with all Lisp allocation outside the sample.
/// Retain several certificates across replacement to exercise escaping storage.
#[test]
fn collection_reads_allocation_steady_projection_counters() {
    use crate::emacs_core::Value;
    for count in [24, 512] {
        let owners: Vec<_> = (0..count)
            .map(|_| Value::cons(Value::NIL, Value::NIL))
            .collect();
        let read = || {
            for owner in &owners {
                std::hint::black_box(owner.cons_car());
            }
        };
        let mut retained: [Option<CollectionReads>; 4] = std::array::from_fn(|_| None);
        for index in 0..16 {
            retained[index % 4] = capture(read).1;
        }
        let (_, allocations) = measured(|| {
            for index in 0..32 {
                let certificate = capture(read).1.expect("read-only projections");
                assert!(certificate.unchanged());
                retained[index % 4] = Some(certificate);
            }
        });
        assert!(
            retained
                .iter()
                .all(|reads| reads.as_ref().unwrap().unchanged())
        );
        eprintln!(
            "obsalloc real owners={count} captures=32 steady={allocations:?} (calls, requested bytes)"
        );
    }
}

#[test]
fn collection_reads_recent_clear_fill_counter() {
    use crate::emacs_core::Value;
    let owners: Vec<_> = (0..7)
        .map(|_| Value::cons(Value::NIL, Value::NIL))
        .collect();
    let scope = CollectionReadScope::begin();
    for owner in &owners {
        owner.cons_car();
    }
    let used =
        RECENT_READS.with(|recent| recent.words.iter().filter(|word| word.get() != 0).count());
    RECENT_CLEARED_CELLS.with(|count| count.set(0));
    clear_recent_reads();
    let cleared = RECENT_CLEARED_CELLS.with(Cell::get);
    assert!(RECENT_READS.with(|recent| recent.words.iter().all(|word| word.get() == 0)));
    eprintln!(
        "obsalloc RecentReads::clear used={used} cleared={cleared} fill_bytes={}",
        cleared * size_of::<usize>()
    );
    assert!(scope.finish().unwrap().unchanged());
}
