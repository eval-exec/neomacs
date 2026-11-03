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
        assert_eq!(
            allocations,
            (0, 0),
            "warmed escaping captures must not allocate"
        );
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
    RECENT_CLEARED_MASKS.with(|count| count.set(0));
    clear_recent_reads();
    let cleared = RECENT_CLEARED_CELLS.with(Cell::get);
    let masks = RECENT_CLEARED_MASKS.with(Cell::get);
    assert_eq!(cleared, used, "clear only cells used by real projections");
    assert_eq!(masks, 4);
    assert!(RECENT_READS.with(|recent| recent.words.iter().all(|word| word.get() == 0)));
    eprintln!(
        "obsalloc RecentReads::clear used={used} cleared={cleared} fill_bytes={} bitmap_reset_bytes={}",
        cleared * size_of::<usize>(),
        masks * size_of::<u64>()
    );
    assert!(scope.finish().unwrap().unchanged());
}

/// Every depth needs independent first-read revisions. Reusing the innermost
/// map for an outer scope would erase its dependency or move its first read.
#[test]
fn collection_reads_capture_map_reuses_nested_storage() {
    use crate::emacs_core::Value;
    for count in [24, 512] {
        let owners: Vec<_> = (0..count)
            .map(|_| Value::cons(Value::NIL, Value::NIL))
            .collect();
        let mut scopes = Vec::with_capacity(MAX_DEPTH);
        let mut record_nested = || {
            for _ in 0..MAX_DEPTH {
                scopes.push(CollectionReadScope::begin());
            }
            for owner in &owners {
                owner.cons_car();
            }
            // Dropping a scope returns its leased revision map without
            // constructing an escaping certificate (measured separately).
            while let Some(scope) = scopes.pop() {
                drop(scope);
            }
        };
        record_nested();
        let (_, allocations) = measured(|| {
            for _ in 0..32 {
                record_nested();
            }
        });
        assert_eq!(allocations, (0, 0), "nested maps retain their storage");
        eprintln!(
            "obsalloc capture maps owners={count} depth={MAX_DEPTH} captures=256 steady={allocations:?} (calls, requested bytes)"
        );

        // Every outer first-read revision must still precede this write,
        // even after repeated reuse of all the nested map storage.
        for _ in 0..MAX_DEPTH {
            scopes.push(CollectionReadScope::begin());
            owners[0].cons_car();
        }
        owners[0].set_car(Value::T);
        while let Some(scope) = scopes.pop() {
            assert!(scope.finish().is_none());
        }
    }
}

/// Small, heap-free capture fixture for both Miri aliasing models. Recording
/// numeric identities bypasses publication and never dereferences an owner.
#[test]
fn collection_reads_small_capture_storage_lifetime_fixture() {
    let record = |bits| {
        with_state(|state| {
            record_observation(
                &mut state.borrow_mut(),
                bits,
                LispCollectionRevision::current(),
                None,
            );
        });
    };
    let (_, first) = capture(|| record(8));
    let (_, second) = capture(|| record(16));
    drop(first);
    let (_, replacement) = capture(|| record(24));
    let (_, outer) = capture(|| {
        record(32);
        let (_, inner) = capture(|| record(40));
        assert!(inner.unwrap().unchanged());
    });
    with_store(|store| {
        let revision = store.advance();
        store.record(16, revision);
    });
    assert!(!second.unwrap().unchanged());
    assert!(replacement.unwrap().unchanged());
    assert!(outer.unwrap().unchanged());
}

#[test]
fn collection_reads_allocation_steady_phase_counters() {
    for count in [0, 24, 512] {
        let record = || {
            with_state(|state| {
                let mut state = state.borrow_mut();
                for bits in 0..count {
                    record_observation(&mut state, bits, LispCollectionRevision::current(), None);
                }
            });
        };
        let (_, retained) = capture(record);
        // A second lease can retire and warm the free list while the first
        // immutable certificate remains live and unavailable for reuse.
        drop(capture(record));
        let (scope, begin) = measured(CollectionReadScope::begin);
        let (_, recording) = measured(record);
        let (certificate, finish) = measured(|| scope.finish());
        let certificate = certificate.unwrap();
        assert!(retained.unwrap().unchanged());
        let (_, retire) = measured(|| drop(certificate));
        assert_eq!(begin, (0, 0));
        assert_eq!(recording, (0, 0));
        assert_eq!(finish, (0, 0));
        assert_eq!(retire, (0, 0));
        eprintln!(
            "obsalloc steady owners={count} begin={begin:?} record={recording:?} finish={finish:?} retire={retire:?} (calls, requested bytes)"
        );
    }
}

#[test]
fn collection_reads_certificate_retires_into_its_origin_world() {
    let origin = ObservationWorld::current();
    let foreign = ObservationWorld::default();
    let record = || {
        with_state(|state| {
            record_observation(
                &mut state.borrow_mut(),
                8,
                LispCollectionRevision::current(),
                None,
            );
        });
    };
    let (_, reads) = capture(record);
    let reads = reads.unwrap();
    foreign.install(super::super::gc::BarrierWindow::NONE);
    with_store(|store| {
        let revision = store.advance();
        store.record(8, revision);
    });
    assert!(
        reads.unchanged(),
        "a foreign journal does not change this certificate"
    );
    let (_, retire) = measured(|| drop(reads));
    assert_eq!(
        retire,
        (0, 0),
        "origin retirement cannot grow its free list"
    );
    origin.install(super::super::gc::BarrierWindow::NONE);
    let (_, reused) = measured(|| {
        let (_, reads) = capture(record);
        assert!(reads.unwrap().unchanged());
    });
    assert_eq!(reused, (0, 0), "the origin receives its own retired lease");
}
