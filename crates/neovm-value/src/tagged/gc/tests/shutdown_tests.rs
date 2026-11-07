use super::*;
use std::sync::Arc;

fn stalled_mark(heap: &mut TaggedHeap) -> std::sync::mpsc::Sender<ConcurrentMarkResult> {
    let (tx, rx) = std::sync::mpsc::channel();
    heap.concurrent_mark_running = true;
    heap.gc_exited = Some(rx);
    tx
}

#[test]
fn explicit_marker_finish_is_idle_and_idempotent() {
    let mut heap = TaggedHeap::new();
    heap.finish_concurrent_mark().unwrap();
    heap.finish_concurrent_mark().unwrap();
    heap.shutdown().unwrap();
}

#[test]
fn explicit_marker_finish_reclaims_pages_at_shutdown() {
    let before = LIVE_FLOAT_PAGES.load(Ordering::Relaxed);
    let mut heap = TaggedHeap::new();
    let root = heap.alloc_float(6.5);
    heap.concurrent_begin();
    heap.seed_root(root);
    heap.launch_concurrent_mark();
    heap.shutdown().unwrap();
    assert_eq!(LIVE_FLOAT_PAGES.load(Ordering::Relaxed), before);
}

unsafe extern "C" fn count_native_finalizer(data: *mut std::ffi::c_void) {
    // SAFETY: the fixtures register a live AtomicUsize for exactly the
    // duration of the heap's explicit shutdown or synchronous collection.
    unsafe { &*data.cast::<AtomicUsize>() }.fetch_add(1, Ordering::Relaxed);
}

fn allocate_counted_native_objects(heap: &mut TaggedHeap, count: &AtomicUsize) {
    let data = std::ptr::from_ref(count).cast_mut().cast();
    heap.alloc_user_ptr(data, Some(count_native_finalizer));
    let function = heap.alloc_module_function(
        0,
        0,
        std::ptr::null(),
        data,
        TaggedValue::NIL,
        TaggedValue::NIL,
    );
    // SAFETY: this test exclusively owns the initialized module function and
    // its data points to count, which outlives all synchronous reclamation.
    unsafe {
        (*(function.as_veclike_ptr().unwrap() as *mut ModuleFunctionObj)).finalizer =
            Some(count_native_finalizer);
    }
}

#[test]
fn idle_heap_drop_skips_foreign_finalizers() {
    let count = AtomicUsize::new(0);
    let mut heap = TaggedHeap::new();
    allocate_counted_native_objects(&mut heap, &count);
    drop(heap);
    assert_eq!(count.load(Ordering::Relaxed), 0);
}

#[test]
fn explicit_shutdown_runs_foreign_finalizers_once() {
    let count = AtomicUsize::new(0);
    let mut heap = TaggedHeap::new();
    allocate_counted_native_objects(&mut heap, &count);
    heap.shutdown().unwrap();
    assert_eq!(count.load(Ordering::Relaxed), 2);
}

#[test]
fn ordinary_sweep_runs_foreign_finalizers_once() {
    let count = AtomicUsize::new(0);
    let mut heap = TaggedHeap::new();
    allocate_counted_native_objects(&mut heap, &count);
    heap.collect_exact(std::iter::empty());
    assert_eq!(count.load(Ordering::Relaxed), 2);
    heap.shutdown().unwrap();
    assert_eq!(count.load(Ordering::Relaxed), 2);
}

#[test]
fn failed_shutdown_skips_foreign_finalizers() {
    let count = AtomicUsize::new(0);
    let mut heap = TaggedHeap::new();
    allocate_counted_native_objects(&mut heap, &count);
    let completion = stalled_mark(&mut heap);
    drop(completion);
    assert!(matches!(
        heap.shutdown(),
        Err(MarkFinishError::Disconnected(_))
    ));
    assert_eq!(count.load(Ordering::Relaxed), 0);
}

#[test]
fn explicit_shutdown_reclaims_all_intrusive_owner_lists_once() {
    let count = AtomicUsize::new(0);
    let mut heap = TaggedHeap::new();
    let data = std::ptr::from_ref(&count).cast_mut().cast();
    for _ in 0..5 {
        heap.alloc_user_ptr(data, Some(count_native_finalizer));
    }
    let mut heads = [std::ptr::null_mut(); 5];
    let mut current = heap.all_objects;
    for head in &mut heads {
        *head = current;
        // SAFETY: these are five disjoint initialized residual Boxes owned by
        // the heap. The fixture transfers each to a different ownership list
        // without reclaiming it or introducing a callback or marker.
        unsafe {
            current = (*current).gc_link();
            (**head).set_gc_link_world_stopped(std::ptr::null_mut());
        }
    }
    assert!(current.is_null());
    heap.all_objects = heads[0];
    heap.tenured_objects = heads[1];
    heap.sweep_noncons_pending = heads[2];
    heap.generational.old_objects = heads[3];
    heap.generational.old_sweep_pending = heads[4];
    heap.shutdown().unwrap();
    assert_eq!(count.load(Ordering::Relaxed), 5);
}

#[test]
fn heap_drop_retains_stalled_marker_storage_without_waiting() {
    let before = LIVE_FLOAT_PAGES.load(Ordering::Relaxed);
    let mut heap = TaggedHeap::new();
    let float = heap.alloc_float(7.5);
    let vector = heap.alloc_vector(vec![float]);
    let cons = heap.alloc_cons(vector, TaggedValue::NIL);
    let tx = stalled_mark(&mut heap);
    let wake = Arc::clone(&heap.gc_wake);
    let stop = Arc::clone(&heap.gc_stop);
    // Holding this lock on this thread makes an accidental Drop lock a
    // deterministic deadlock, rather than a timing-dependent assertion.
    let _wake_held = wake.0.lock().unwrap();
    drop(heap);
    assert!(stop.load(Ordering::Acquire));
    assert_eq!(LIVE_FLOAT_PAGES.load(Ordering::Relaxed), before + 1);
    // SAFETY: active-marker abandonment intentionally retained these owned
    // allocations, including the vector's backing and cons block bitmap.
    unsafe {
        assert_eq!((*cons.xcons_ptr()).load_car(), vector);
        assert_eq!((*float.as_float_ptr().unwrap()).value, 7.5);
        let object = &*(vector.as_veclike_ptr().unwrap() as *const VectorObj);
        assert_eq!(object.data.as_slice(), &[float]);
    }
    assert!(tx.send(ConcurrentMarkResult::default()).is_err());
}

#[test]
fn heap_drop_during_unwind_ignores_poisoned_marker_wake() {
    let mut heap = TaggedHeap::new();
    heap.alloc_float(8.5);
    let _tx = stalled_mark(&mut heap);
    let wake = Arc::clone(&heap.gc_wake);
    let _ = std::panic::catch_unwind(|| {
        let _lock = wake.0.lock().unwrap();
        panic!("poison wake latch for destructor test");
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _heap = heap;
        panic!("unwind while heap owns a marker");
    }));
    assert!(result.is_err());
}

#[test]
fn failed_marker_finish_preserves_active_state_and_storage() {
    enum Failure {
        Missing,
        Disconnected,
    }
    for failure in [Failure::Missing, Failure::Disconnected] {
        let before = LIVE_FLOAT_PAGES.load(Ordering::Relaxed);
        let mut heap = TaggedHeap::new();
        heap.alloc_float(9.5);
        let tx = stalled_mark(&mut heap);
        drop(tx);
        if matches!(failure, Failure::Missing) {
            heap.gc_exited = None;
        }
        let error = heap.finish_concurrent_mark().unwrap_err();
        assert!(matches!(
            (failure, error),
            (Failure::Missing, MarkFinishError::MissingCompletion)
                | (Failure::Disconnected, MarkFinishError::Disconnected(_))
        ));
        assert!(heap.concurrent_mark_running());
        assert!(matches!(
            heap.finish_concurrent_mark(),
            Err(MarkFinishError::MissingCompletion)
        ));
        let sweep = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            heap.collect_exact(std::iter::empty());
        }));
        assert!(sweep.is_err(), "failed completion must refuse reclamation");
        drop(heap);
        assert_eq!(LIVE_FLOAT_PAGES.load(Ordering::Relaxed), before + 1);
    }
}

#[test]
fn poisoned_collector_queues_fail_finish_before_reclamation() {
    let before = LIVE_FLOAT_PAGES.load(Ordering::Relaxed);
    let mut heap = TaggedHeap::new();
    heap.alloc_float(11.5);
    let tx = stalled_mark(&mut heap);
    tx.send(ConcurrentMarkResult::default()).unwrap();
    heap.satb_handoff.poison_for_test();
    assert!(heap.satb_handoff.is_poisoned());
    assert!(matches!(
        heap.finish_concurrent_mark(),
        Err(MarkFinishError::PoisonedCollectorState)
    ));
    assert!(heap.concurrent_mark_running());
    let sweep = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        heap.collect_exact(std::iter::empty());
    }));
    assert!(sweep.is_err());
    drop(heap);
    assert_eq!(LIVE_FLOAT_PAGES.load(Ordering::Relaxed), before + 1);
}

#[test]
fn abandoned_marker_retains_retired_vector_original() {
    let mut heap = TaggedHeap::new();
    let original = vec![TaggedValue::fixnum(21), TaggedValue::fixnum(22)];
    let vector = heap.alloc_vector(original.clone());
    let tx = stalled_mark(&mut heap);
    // SAFETY: this test exclusively owns the initialized vector in heap.
    let object = unsafe { &mut *(vector.as_veclike_ptr().unwrap() as *mut VectorObj) };
    let before = object.data.as_slice().as_ptr();
    let retired = object.data.clone_owned_backing().expect("owned fixture");
    object.data.store_atomic(0, TaggedValue::fixnum(23));
    heap.retired_vector_buffers.push(retired);
    drop(heap);
    // SAFETY: active-marker abandonment retained the original backing as
    // well as the current vector; this reader runs after owner destruction.
    let retained = unsafe { std::slice::from_raw_parts(before, original.len()) };
    assert_eq!(retained, original);
    assert!(tx.send(ConcurrentMarkResult::default()).is_err());
}

#[test]
fn failed_generational_marker_retains_promotion_storage() {
    let before = LIVE_FLOAT_PAGES.load(Ordering::Relaxed);
    let mut heap = TaggedHeap::new();
    // SAFETY: serial fixture heap; the caller holds the only heap view and no
    // collector or compiled leaf runs.
    unsafe { heap.set_generational_enabled(true) };
    heap.generational.major_in_progress = true;
    let float = heap.alloc_float(12.5);
    let tx = stalled_mark(&mut heap);
    drop(tx);
    assert!(matches!(
        heap.finish_concurrent_mark(),
        Err(MarkFinishError::Disconnected(_))
    ));
    assert!(heap.concurrent_mark_running());
    drop(heap);
    assert_eq!(LIVE_FLOAT_PAGES.load(Ordering::Relaxed), before + 1);
    // SAFETY: failed generational completion preserves all potential promotion
    // targets and their owning pages instead of treating lost logs as empty.
    assert_eq!(unsafe { (*float.as_float_ptr().unwrap()).value }, 12.5);
}

#[test]
fn heap_drop_requests_real_marker_stop_and_retains_reachable_storage() {
    let before = LIVE_FLOAT_PAGES.load(Ordering::Relaxed);
    let mut heap = TaggedHeap::new();
    let float = heap.alloc_float(10.5);
    let vector = heap.alloc_vector(vec![float]);
    let mut root = vector;
    for _ in 0..10_000 {
        root = heap.alloc_cons(vector, root);
    }
    let mut obarray = crate::symbol::Obarray::new();
    obarray.set_symbol_value("abandoned-marker-root", root);
    // SAFETY: this fixture owns heap and obarray without TLS writer aliases;
    // start, capture and publication run without callbacks or safepoints.
    let permit = unsafe { heap.permit_concurrent_mark() }.unwrap();
    let mut capture = permit.begin();
    let snapshot = {
        // SAFETY: capture runs on that sole admitted heap/obarray writer
        // before the worker can inspect any of the snapshots.
        let world = unsafe { scan_contract::SingleMutatorWorld::from_heap(capture.heap_mut()) };
        obarray.scan_snapshot(&world)
    };
    capture.heap_mut().set_pending_obarray_scan(snapshot);
    capture.heap_mut().seed_root(root);
    capture.launch().unwrap();
    let completion = heap.gc_exited.take().unwrap();
    drop(heap);
    drop(obarray);
    // The explicit test wait is outside Drop. Completion proves the real
    // marker safely finished its snapshots after both owners were abandoned.
    completion
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    assert_eq!(LIVE_FLOAT_PAGES.load(Ordering::Relaxed), before + 1);
    // SAFETY: fallback retained the reachable allocations; the completion
    // proves there is no marker left, and no writer remains.
    unsafe {
        assert_eq!((*float.as_float_ptr().unwrap()).value, 10.5);
        assert_eq!((*root.xcons_ptr()).load_car(), vector);
    }
}

#[test]
fn obarray_snapshot_leases_survive_owner_drop() {
    let mut heap = TaggedHeap::new();
    let mut obarray = crate::symbol::Obarray::new();
    let child =
        TaggedValue::from_sym_id(crate::intern::intern_uninterned("snapshot-retained-child"));
    obarray.set_symbol_value("snapshot-retained-owner", child);
    // SAFETY: this test is the sole writer; capture and read have no callbacks
    // and no other mutator can modify this owner after it is dropped.
    let world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
    let snapshot = obarray.scan_snapshot(&world);
    assert_eq!(snapshot.heap_identity(), heap.tagged_heap_identity());
    drop(obarray);
    let mut children = Vec::new();
    // SAFETY: the snapshot's leases made owner Drop retain its chunk/side
    // allocations. No writer remains, and every captured slot is initialized.
    unsafe { snapshot.scan_for_major(|value| children.push(value)) };
    assert!(children.contains(&child));
}

#[test]
fn snapshot_storage_lease_releases_after_final_read() {
    let mut heap = TaggedHeap::new();
    let mut storage = scan_contract::ScanStorageOwner::new();
    assert!(!storage.has_leases());
    // SAFETY: this sole test owner admits a callback-free capture.
    let world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
    let first = scan_contract::ScanStorageLease::capture(&storage, &world);
    let last = scan_contract::ScanStorageLease::capture(&storage, &world);
    assert!(storage.has_leases());
    drop(first);
    assert!(storage.has_leases());

    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        ready_tx.send(()).unwrap();
        release_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        drop(last);
    });
    ready_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert!(storage.has_leases());
    release_tx.send(()).unwrap();
    reader.join().unwrap();
    assert!(!storage.has_leases());
}

// UNCOMPILED scratch fixtures: append inside gc/tests/shutdown_tests.rs.
// Reuses its existing stalled_mark/allocate_counted_native_objects helpers.
// Never dereference any object pointer after consuming shutdown or heap Drop.

#[derive(Clone, Copy, Debug)]
enum YoungLoanRetentionOwner {
    Current,
    Registered,
}

#[derive(Clone, Copy, Debug)]
enum YoungLoanRetentionGeneration {
    Gen0,
    Gen1,
}

fn young_loan_retention_heap(generation: YoungLoanRetentionGeneration) -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    // SAFETY: serial fixture heap; the caller holds the only heap view and no
    // collector or compiled leaf runs.
    unsafe {
        heap.set_generational_enabled(matches!(generation, YoungLoanRetentionGeneration::Gen1))
    };
    heap
}

fn take_retention_test_loan(
    heap: &mut TaggedHeap,
    owner: YoungLoanRetentionOwner,
) -> young_registry::YoungLogLoan {
    // SAFETY: fixture owns every writer/registration with no marker, callback,
    // Lisp execution or safepoint while the young-only payload is loaned out.
    let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(heap) };
    let id = match owner {
        YoungLoanRetentionOwner::Current => world.current_young_registration_id(),
        YoungLoanRetentionOwner::Registered => {
            let physical = world.register_mutator_gc_for_test();
            world.registered_young_id_for_test(physical).unwrap()
        }
    };
    world.take_young_log_loan(id).unwrap()
}

fn young_retention_live_pages() -> (usize, usize) {
    (
        LIVE_FLOAT_PAGES.load(Ordering::Relaxed),
        LIVE_VECTOR_PAGES.load(Ordering::Relaxed),
    )
}

fn allocate_young_retention_graph(heap: &mut TaggedHeap, finalizers: &AtomicUsize) {
    let float = heap.alloc_float(641.5);
    let vector = heap.alloc_vector(vec![float]);
    let _root = heap.alloc_cons(vector, TaggedValue::NIL);
    allocate_counted_native_objects(heap, finalizers);
}

#[test]
fn shutdown_rejects_current_and_registered_young_loans_before_teardown() {
    for generation in [
        YoungLoanRetentionGeneration::Gen0,
        YoungLoanRetentionGeneration::Gen1,
    ] {
        for owner in [
            YoungLoanRetentionOwner::Current,
            YoungLoanRetentionOwner::Registered,
        ] {
            let finalizers = AtomicUsize::new(0);
            let mut heap = young_loan_retention_heap(generation);
            allocate_young_retention_graph(&mut heap, &finalizers);
            let identity = heap.heap_identity();
            let loan = take_retention_test_loan(&mut heap, owner);
            assert!(!heap.concurrent_mark_running());
            assert!(!heap.facade_mark_is_excluded());
            let pages = young_retention_live_pages();
            let wake = Arc::clone(&heap.gc_wake);
            // A hidden finish/wait in shutdown/Drop would deadlock this thread.
            // No worker or job exists in this fixture.
            let _wake_held = wake.0.lock().unwrap();
            assert!(
                matches!(
                    heap.shutdown(),
                    Err(MarkFinishError::MutatorsNotStopped(actual)) if actual == identity
                ),
                "{generation:?}, {owner:?}"
            );
            assert_eq!(finalizers.load(Ordering::Relaxed), 0);
            assert_eq!(young_retention_live_pages(), pages);
            // Dropping the uniquely owned payload only drops its Vec; this does
            // not dereference any block address or grant reclamation authority.
            drop(loan);
            assert_eq!(young_retention_live_pages(), pages);
        }
    }
}

#[test]
fn idle_drop_retains_backing_for_current_and_registered_young_loans_without_waiting() {
    for generation in [
        YoungLoanRetentionGeneration::Gen0,
        YoungLoanRetentionGeneration::Gen1,
    ] {
        for owner in [
            YoungLoanRetentionOwner::Current,
            YoungLoanRetentionOwner::Registered,
        ] {
            let finalizers = AtomicUsize::new(0);
            let mut heap = young_loan_retention_heap(generation);
            allocate_young_retention_graph(&mut heap, &finalizers);
            let loan = take_retention_test_loan(&mut heap, owner);
            assert!(!heap.concurrent_mark_running());
            assert!(!heap.facade_mark_is_excluded());
            let pages = young_retention_live_pages();
            let wake = Arc::clone(&heap.gc_wake);
            let stop = Arc::clone(&heap.gc_stop);
            let _wake_held = wake.0.lock().unwrap();
            drop(heap);
            assert!(stop.load(Ordering::Acquire));
            assert_eq!(finalizers.load(Ordering::Relaxed), 0);
            assert_eq!(
                young_retention_live_pages(),
                pages,
                "{generation:?}, {owner:?}"
            );
            drop(loan);
            assert_eq!(young_retention_live_pages(), pages);
        }
    }
}

#[test]
fn young_loan_shutdown_preflight_precedes_disconnected_marker_finish() {
    for generation in [
        YoungLoanRetentionGeneration::Gen0,
        YoungLoanRetentionGeneration::Gen1,
    ] {
        for owner in [
            YoungLoanRetentionOwner::Current,
            YoungLoanRetentionOwner::Registered,
        ] {
            let finalizers = AtomicUsize::new(0);
            let mut heap = young_loan_retention_heap(generation);
            allocate_young_retention_graph(&mut heap, &finalizers);
            let identity = heap.heap_identity();
            let loan = take_retention_test_loan(&mut heap, owner);
            // A disconnected synthetic marker would return Disconnected if
            // finish were entered. Keeping no sender avoids a hanging receive.
            let completion = stalled_mark(&mut heap);
            drop(completion);
            let pages = young_retention_live_pages();
            assert!(
                matches!(
                    heap.shutdown(),
                    Err(MarkFinishError::MutatorsNotStopped(actual)) if actual == identity
                ),
                "loan refusal must precede finish: {generation:?}, {owner:?}"
            );
            assert_eq!(finalizers.load(Ordering::Relaxed), 0);
            assert_eq!(young_retention_live_pages(), pages);
            drop(loan);
        }
    }
}

#[test]
fn shutdown_after_returned_young_loan_reclaims_normally() {
    for generation in [
        YoungLoanRetentionGeneration::Gen0,
        YoungLoanRetentionGeneration::Gen1,
    ] {
        for owner in [
            YoungLoanRetentionOwner::Current,
            YoungLoanRetentionOwner::Registered,
        ] {
            let baseline = young_retention_live_pages();
            let finalizers = AtomicUsize::new(0);
            let mut heap = young_loan_retention_heap(generation);
            allocate_young_retention_graph(&mut heap, &finalizers);
            let loan = take_retention_test_loan(&mut heap, owner);
            let id = loan.id();
            {
                // SAFETY: no worker/callback exists; the fixture returns its
                // sole owned payload before granting collection/reclamation.
                let mut world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
                world.return_young_log_loan(loan).unwrap();
                if matches!(owner, YoungLoanRetentionOwner::Registered) {
                    world
                        .retire_young_registration_for_test(id)
                        .unwrap()
                        .acknowledge();
                }
                world.ensure_young_logs_stopped().unwrap();
            }
            heap.shutdown().unwrap();
            assert_eq!(
                finalizers.load(Ordering::Relaxed),
                2,
                "{generation:?}, {owner:?}"
            );
            assert_eq!(young_retention_live_pages(), baseline);
        }
    }
}
