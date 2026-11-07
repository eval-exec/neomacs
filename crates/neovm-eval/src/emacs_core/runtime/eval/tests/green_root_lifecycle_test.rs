//! Parked publications end only after their original owners have resumed.

use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;
use neovm_value::tagged::transport::{collect_shared_root_gc_roots, root_table_is_alive_for_test};
use neovm_value::value::Value;

const OWNERS: usize = 4;

#[test]
fn a_synchronous_collector_extent_cannot_yield_its_green_owner() {
    let mut owner = green();
    eval(
        &mut owner,
        "(progn
           (setq p3b-driver-peer-ran nil)
           (setq p3b-driver-peer
             (make-thread (lambda () (setq p3b-driver-peer-ran t)))))",
    );
    assert_eq!(owner.gc_inhibit_depth, 0);
    owner.gc_driver_active = true;
    let refused = owner.green_thread_yield(&[]);
    owner.gc_driver_active = false;
    assert!(refused.is_err());
    assert!(
        owner
            .visible_variable_value_or_nil("p3b-driver-peer-ran")
            .is_nil()
    );
    assert!(owner.with_green_runtime(|runtime| runtime.parked.borrow().is_empty()));
    assert_eq!(eval(&mut owner, "(thread-join p3b-driver-peer)"), "t");
    assert!(
        owner
            .visible_variable_value_or_nil("p3b-driver-peer-ran")
            .is_truthy()
    );
}

#[test]
fn restored_owner_retirement_leaves_cooperative_sweep_pending() {
    let mut owner = green();
    let baton = owner.start_green_runtime().expect("start Green era");
    let root = Value::string("restored owner during deferred sweep");
    owner.push_vm_root_frame();
    owner.push_vm_frame_root(root);
    let prepared = baton.prepare_parked_batch(&owner.tagged_heap, 1).unwrap();
    // SAFETY: the sole admitted owner retains this same-heap value in its VM
    // frame; filling and publication cannot collect or hand the Baton off.
    let publication = unsafe { prepared.finalize(&[root]) }
        .unwrap()
        .publish()
        .unwrap();
    if owner.tagged_heap.tagged_heap_generational_enabled() {
        assert!(!owner.tagged_heap.value_is_old_for_test(root));
        owner.tagged_heap.begin_minor_collection();
    } else {
        owner.tagged_heap.begin_stw_collection();
    }
    let heap: *mut neovm_value::tagged::gc::TaggedHeap = owner.tagged_heap.heap_root().as_ptr();
    // SAFETY: this fixture owns the Baton without any other owner, callback,
    // collector or handoff. Enumerate the complete current Context, symbol
    // cells and publication table before finishing this stopped mark extent.
    unsafe { owner.seed_registered_mutator_roots_world_stopped(heap) };
    owner.tagged_heap.incremental_drain_all();
    let bytes = owner.tagged_heap.live_bytes();
    owner
        .tagged_heap
        .incremental_finish(bytes, std::time::Instant::now());
    assert!(!owner.tagged_heap.mark_in_progress());
    assert!(owner.tagged_heap.sweep_in_progress());

    // Even an otherwise safe pending sweep cannot authorize retirement from
    // inside a synchronous driver extent. Refusal leaves publication intact.
    owner.gc_driver_active = true;
    let refused = owner.prepare_parked_root_retirement();
    owner.gc_driver_active = false;
    assert!(refused.is_err());
    let mut roots = Vec::new();
    collect_shared_root_gc_roots(&owner.tagged_heap, &mut roots);
    assert_eq!(roots, vec![root]);
    assert!(owner.tagged_heap.sweep_in_progress());

    owner.prepare_parked_root_retirement().unwrap();
    let prepared = publication.try_recycle().unwrap();
    assert!(owner.tagged_heap.sweep_in_progress());
    roots.clear();
    collect_shared_root_gc_roots(&owner.tagged_heap, &mut roots);
    assert!(roots.is_empty());
    owner.tagged_heap.finish_incremental_sweep_now();
    assert_eq!(
        print_value(&root),
        "\"restored owner during deferred sweep\""
    );
    eval(&mut owner, "(garbage-collect)");
    assert_eq!(
        print_value(&root),
        "\"restored owner during deferred sweep\""
    );
    drop(prepared);
    owner.pop_vm_root_frame();
    drop(baton);
    assert!(!owner.tagged_heap.facade_mark_is_excluded());
}

#[test]
fn a_busy_recycle_failure_is_explicitly_finished_without_abandoning_its_lease() {
    use neovm_value::tagged::transport::RootBatchError;
    use neovm_value::test_support::authority::with_root_table_lock_for_test;
    use std::sync::{Arc, Condvar, Mutex};

    let mut owner = green();
    let baton = owner.start_green_runtime().expect("start Green era");
    let prepared = baton.prepare_parked_batch(&owner.tagged_heap, 0).unwrap();
    // SAFETY: this sole admitted owner publishes an empty same-heap census;
    // no allocation, collection or handoff can occur during publication.
    let publication = unsafe { prepared.finalize(&[]) }
        .unwrap()
        .publish()
        .unwrap();
    let heap = owner.tagged_heap.heap_identity();
    let handshake = Arc::new((Mutex::new((false, false)), Condvar::new()));
    let peer_handshake = Arc::clone(&handshake);
    let host = std::thread::spawn(move || {
        with_root_table_lock_for_test(heap, || {
            let (state, ready) = &*peer_handshake;
            let mut state = state.lock().unwrap();
            state.0 = true;
            ready.notify_one();
            while !state.1 {
                state = ready.wait(state).unwrap();
            }
        });
    });
    let (state, ready) = &*handshake;
    let mut state = state.lock().unwrap();
    while !state.0 {
        state = ready.wait(state).unwrap();
    }
    drop(state);
    let failure = publication.try_recycle().unwrap_err();
    assert!(matches!(failure.error(), RootBatchError::Busy));
    let (publication, _) = failure.into_parts();
    assert!(owner.tagged_heap.facade_mark_is_excluded());
    handshake.0.lock().unwrap().1 = true;
    ready.notify_one();
    // The owner never handed off and the collector remains quiescent. This
    // is the same explicit finish used when Green cannot reuse its storage.
    super::root_registry::finish(publication).unwrap();
    host.join().unwrap();
    assert!(!root_table_is_alive_for_test(heap));
    drop(baton);
    assert!(!owner.tagged_heap.facade_mark_is_excluded());
}

#[test]
fn a_final_unseated_baton_releases_its_epoch_before_destroying_the_world() {
    let mut owner = green();
    let baton = owner.start_green_runtime().expect("start Green era");
    let world_is_alive = owner.world_lifetime_probe_for_test();
    drop(owner);
    assert!(world_is_alive(), "the unseated Baton keeps its World alive");
    // World's destructor checks that every Green exclusion has ended before
    // TaggedHeap::drop, whose unfinished-epoch fallback retains backing pages.
    drop(baton);
    assert!(!world_is_alive());
}

fn eval(owner: &mut Context, source: &str) -> String {
    print_value(&owner.eval_str(source).expect("Green lifecycle fixture"))
}

fn green() -> Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    Context::new_with_threading(ThreadingMode::Green)
}

/// Each worker reports readiness while holding the mutex, then atomically
/// releases it to wait. Main takes that same mutex before accepting readiness,
/// so returning proves every worker is parked, without scheduler timing.
fn park_owners(owner: &mut Context) {
    assert_eq!(
        eval(
            owner,
            r#"(progn
              (setq p3b-mutex (make-mutex)
                    p3b-ready-condition (make-condition-variable p3b-mutex)
                    p3b-release-condition (make-condition-variable p3b-mutex)
                    p3b-ready 0 p3b-release nil p3b-cleanups nil)
              (setq p3b-workers
                (mapcar
                  (lambda (_)
                    (make-thread
                      (lambda ()
                        (let ((kept (make-list 257 (copy-sequence "parked payload"))))
                          (unwind-protect
                              (progn
                                (mutex-lock p3b-mutex)
                                (setq p3b-ready (1+ p3b-ready))
                                (condition-notify p3b-ready-condition)
                                (while (null p3b-release)
                                  (condition-wait p3b-release-condition))
                                (mutex-unlock p3b-mutex)
                                (list (length kept) (car kept) (nth 256 kept)))
                            (setq p3b-cleanups (cons 'resumed p3b-cleanups)))))))
                  '(1 2 3 4)))
              (mutex-lock p3b-mutex)
              (while (< p3b-ready 4) (condition-wait p3b-ready-condition))
              (mutex-unlock p3b-mutex)
              (mapcar (lambda (worker)
                        (eq (thread--blocker worker) p3b-release-condition))
                      p3b-workers))"#,
        ),
        "(t t t t)"
    );
    assert_eq!(
        owner.with_green_runtime(|runtime| runtime.parked.borrow().len()),
        OWNERS
    );
    let mut roots = Vec::new();
    collect_shared_root_gc_roots(&owner.tagged_heap, &mut roots);
    assert!(!roots.is_empty(), "parked publications are registered");
}

fn assert_finished(owner: &mut Context) {
    assert!(owner.with_green_runtime(|runtime| runtime.parked.borrow().is_empty()));
    let mut roots = Vec::new();
    collect_shared_root_gc_roots(&owner.tagged_heap, &mut roots);
    assert!(roots.is_empty(), "no parked roots remain in the heap table");
    assert!(
        owner
            .green_seat()
            .is_none_or(|seat| seat.parked_root_batch.is_none()),
        "teardown cancels the running owner's cached reservation"
    );
}

#[test]
fn collection_with_many_parked_owners_preserves_each_continuation() {
    let mut owner = green();
    park_owners(&mut owner);
    let collections = owner.tagged_heap.gc_collections();
    eval(&mut owner, "(garbage-collect)");
    assert!(owner.tagged_heap.gc_collections() > collections);
    assert_eq!(
        owner.with_green_runtime(|runtime| runtime.parked.borrow().len()),
        OWNERS
    );
    assert_eq!(
        eval(
            &mut owner,
            r#"(progn
              (mutex-lock p3b-mutex)
              (setq p3b-release t)
              (condition-notify p3b-release-condition t)
              (mutex-unlock p3b-mutex)
              (mapcar #'thread-join p3b-workers))"#,
        ),
        "((257 \"parked payload\" \"parked payload\") (257 \"parked payload\" \"parked payload\") (257 \"parked payload\" \"parked payload\") (257 \"parked payload\" \"parked payload\"))"
    );
    owner.green_teardown();
    assert_finished(&mut owner);
}

#[test]
fn explicit_teardown_restores_parked_owners_and_finishes_their_batches() {
    let mut owner = green();
    park_owners(&mut owner);
    owner.green_teardown();
    assert_finished(&mut owner);
    assert!(owner.visible_variable_value_or_nil("p3b-cleanups").is_nil());
    // Context::drop repeats teardown; an explicit finish is idempotent.
    owner.green_teardown();
    assert_finished(&mut owner);
}

#[test]
fn worker_shutdown_with_parked_owners_finishes_every_batch() {
    let mut owner = green();
    park_owners(&mut owner);
    assert!(
        owner
            .eval_str("(thread-join (make-thread (lambda () (kill-emacs 23))))")
            .is_err()
    );
    assert_eq!(
        owner.shutdown_request().map(|request| request.exit_code),
        Some(23)
    );
    assert_eq!(
        owner.with_green_runtime(|runtime| runtime.parked.borrow().len()),
        OWNERS
    );
    owner.green_teardown();
    assert_finished(&mut owner);
    assert!(owner.visible_variable_value_or_nil("p3b-cleanups").is_nil());
}

#[test]
fn dropping_a_world_with_parked_owners_releases_the_world_allocation() {
    // Multiple Worlds on one OS thread catch cleanup accidentally deferred to
    // thread exit, and the probe catches a World -> seat -> Baton -> World cycle.
    for _ in 0..3 {
        let mut owner = green();
        park_owners(&mut owner);
        let heap = owner.tagged_heap.heap_identity();
        let world_is_alive = owner.world_lifetime_probe_for_test();
        assert!(world_is_alive());
        assert!(root_table_is_alive_for_test(heap));
        drop(owner);
        assert!(
            !world_is_alive(),
            "Context drop restores and joins every owner, then releases the World"
        );
        assert!(
            !root_table_is_alive_for_test(heap),
            "Context drop finishes every root-table and retained epoch obligation"
        );
    }
}

#[test]
fn final_world_drop_explicitly_finishes_residual_registry_entries() {
    let mut owner = green();
    let root = Value::string("residual terminated owner root");
    owner.push_vm_root_frame();
    owner.push_vm_frame_root(root);
    // Start the runtime without seating or spawning anyone. The synthetic key
    // represents an owner that has terminated with a retained publication, so
    // no real continuation can race the final World's cleanup.
    let baton = owner
        .start_green_runtime()
        .expect("start the Green runtime");
    let prepared = baton
        .prepare_parked_batch(&owner.tagged_heap, 1)
        .expect("reserve the terminated owner's batch");
    // SAFETY: this is the original, sole admitted mutator and root belongs to
    // its heap. Its VM frame roots root throughout filling and publication;
    // neither operation can collect or hand ownership to another mutator.
    let publication = unsafe { prepared.finalize(&[root]) }
        .expect("finalize the terminated owner's roots")
        .publish()
        .expect("publish the terminated owner's roots");
    owner.with_green_runtime(|runtime| {
        assert!(
            runtime
                .parked
                .borrow_mut()
                .insert(super::ThreadKey::from_id(99), publication)
                .is_none()
        );
    });
    owner.pop_vm_root_frame();
    drop(baton);
    let heap = owner.tagged_heap.heap_identity();
    let world_is_alive = owner.world_lifetime_probe_for_test();
    assert!(root_table_is_alive_for_test(heap));
    drop(owner);
    assert!(!world_is_alive(), "the final World allocation is released");
    assert!(
        !root_table_is_alive_for_test(heap),
        "explicit batch finish releases the root table and its retained epoch"
    );
}
