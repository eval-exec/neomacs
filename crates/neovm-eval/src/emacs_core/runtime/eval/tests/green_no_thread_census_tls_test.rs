//! S3 gate proof: GC census and TLS bookkeeping on the no-thread Green path.
//!
//! When a `ThreadingMode::Green` process never calls `make-thread`, Green's
//! thread/parallel-era machinery must stay completely dormant and match the
//! serial (`Sim`) owner's census, tagged-heap identity/root TLS, and clean
//! post-teardown state:
//!
//! 1. The mutator census (`stopped_mutators_world_stopped` and
//!    `seed_registered_mutator_roots_world_stopped`) sees only the single
//!    serial owner, with zero parked/transport roots and the same root group
//!    names and counts as `Sim`.
//! 2. The tagged-heap identity and installed heap root TLS (`TAGGED_HEAP` /
//!    `TAGGED_HEAP_ID`) belong strictly to the serial owner at startup and
//!    after GC, matching `Sim`'s no-thread TLS lifecycle.
//! 3. Startup, ordinary Lisp, read-only thread queries (`current-thread`,
//!    `all-threads`, `thread-live-p`, `thread-name`), uncontended
//!    mutex/condvar operations, `thread-yield`, and `(garbage-collect)`
//!    allocate no `GreenRuntime`/`OwnerSlab`/`ThreadProjectionTable`/root
//!    table (so no carrier pool is constructed), leave symbols unprojected,
//!    and on drop restore the GC + collection-reads carrier TLS fingerprints
//!    to the untouched fresh-thread state.
//! 4. Teardown (`Drop for Context`) leaves nothing registered — both when no
//!    thread ever started and when Green workers ran (joined or parked live in
//!    `condition-wait` at exit): the `World` drops, the tagged-heap TLS slot
//!    and identity clear, the collector and root table are unregistered, and
//!    GC + collection-reads TLS returns to its untouched fresh-thread state.

use super::*;
use crate::emacs_core::print::print_value;
use neovm_value::intern::intern;
use neovm_value::tagged::gc::{
    carrier_heap_is_acceptable_for_job_start, collector_is_registered,
    current_tagged_heap_identity, tagged_heap_is_current, tagged_heap_is_installed,
};
use neovm_value::tagged::transport::{collect_shared_root_gc_roots, root_table_is_alive_for_test};

const NO_THREAD_WORKLOAD: &str = r#"(progn
  (defvar s3-nt-global 41)
  (let ((s3-nt-global 99)
        (xs (list 1 2 (copy-sequence "s3-no-thread") [a b c])))
    (set-buffer (get-buffer-create "s3-nt-buf"))
    (insert "hello")
    (let ((m (make-mutex "s3-nt-m")))
      (mutex-lock m)
      (make-condition-variable m "s3-nt-c")
      (mutex-unlock m))
    (thread-yield)
    (garbage-collect)
    (list s3-nt-global
          (car xs)
          (buffer-string)
          (length (all-threads))
          (eq (car (all-threads)) (current-thread))
          (thread-live-p (current-thread)))))"#;

const EXPECTED_NO_THREAD_RESULT: &str = "(99 1 \"hello\" 1 t t)";

fn run_on_fresh_os_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .name("s3-no-thread-census-tls".to_string())
        .spawn(f)
        .expect("spawn fresh OS thread for isolated TLS observation")
        .join()
        .expect("isolated TLS observation thread must not panic")
}

fn new_context(mode: ThreadingMode) -> Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    match mode {
        ThreadingMode::Sim => Context::new(crate::test_utils::unit_runtime()),
        ThreadingMode::Green => {
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime())
        }
    }
}

fn eval_printed(ctx: &mut Context, src: &str) -> String {
    print_value(&ctx.eval_str(src).expect("S3 census/TLS fixture eval"))
}

#[cfg(debug_assertions)]
fn gc_tls_fingerprint() -> u64 {
    let mut h = neovm_value::tls_fingerprint::SEED;
    neovm_value::tagged::gc::carrier_tls_fingerprint(&mut h);
    neovm_value::tagged::collection_reads::carrier_tls_fingerprint(&mut h);
    h
}

/// Assert every Green / parallel-era state slot is dormant on `ctx`.
fn assert_green_machinery_dormant(ctx: &mut Context, label: &str) {
    let heap_id = ctx.tagged_heap.heap_identity();
    assert!(
        !ctx.threads_admitted(),
        "{label}: threads_admitted must remain false without make-thread"
    );
    assert!(
        !ctx.tagged_heap.threads_admitted(),
        "{label}: tagged_heap.threads_admitted must remain false without make-thread"
    );
    assert!(
        ctx.owner.slab().is_none(),
        "{label}: owner slab must stay unallocated without make-thread"
    );
    assert_eq!(
        ctx.owner_slab_count_for_test(),
        0,
        "{label}: World owner_slabs registry must stay empty"
    );
    assert!(
        ctx.green_seat().is_none(),
        "{label}: main must hold no GreenSeat without make-thread"
    );
    assert!(
        !ctx.has_green_runtime_for_test(),
        "{label}: World must allocate no GreenRuntime without make-thread"
    );
    assert!(
        ctx.baton_holder_for_test().is_none(),
        "{label}: World must record no Baton holder without make-thread"
    );
    assert_eq!(
        ctx.thread_projection_count_for_test(),
        0,
        "{label}: World must issue zero thread projections without make-thread"
    );
    assert!(
        !ctx.tagged_heap.facade_mark_is_excluded(),
        "{label}: concurrent mark facade must not be excluded by a Baton epoch"
    );
    assert!(
        ctx.condition_snapshots.is_dormant(),
        "{label}: condition_snapshots must stay dormant"
    );
    assert_eq!(
        ctx.tagged_heap.payload_publisher_count_for_test(),
        0,
        "{label}: payload publisher count must be 0"
    );
    assert!(
        !root_table_is_alive_for_test(heap_id),
        "{label}: transport root table must not be allocated without make-thread"
    );
    let mut shared_roots = Vec::new();
    collect_shared_root_gc_roots(&ctx.tagged_heap, &mut shared_roots);
    assert!(
        shared_roots.is_empty(),
        "{label}: shared/parked transport roots must be empty"
    );
}

#[test]
fn green_no_thread_mutator_census_sees_only_the_serial_owner() {
    let observe_census = |mode: ThreadingMode| {
        run_on_fresh_os_thread(move || {
            let mut ctx = new_context(mode);
            assert_eq!(
                eval_printed(&mut ctx, NO_THREAD_WORKLOAD),
                EXPECTED_NO_THREAD_RESULT
            );
            assert_green_machinery_dormant(&mut ctx, &format!("{mode:?} after workload"));

            // 1. Mutator census yields exactly the single serial owner.
            let expected_ptr = std::ptr::from_ref(&ctx) as usize;
            let mutator_ptrs: Vec<usize> = ctx
                .stopped_mutators_world_stopped()
                .map(|m| std::ptr::from_ref(m) as usize)
                .collect();
            assert_eq!(
                mutator_ptrs,
                vec![expected_ptr],
                "{mode:?}: mutator census must list only the single serial owner"
            );

            // 2. Stopped-world root seeding matches exact root-group set with
            //    zero parked/parallel roots.
            let heap_ptr = ctx.tagged_heap.heap_root().as_ptr();
            // SAFETY: sole stopped serial owner; no concurrent marker is running
            // right after a synchronous `(garbage-collect)` in this isolated thread.
            let breakdown = unsafe {
                let _quiescent = (*heap_ptr)
                    .drain_to_quiescent(|heap| {
                        ctx.seed_registered_mutator_roots_world_stopped(heap);
                    })
                    .expect("drain collector before stopped census");
                ctx.seed_registered_mutator_roots_world_stopped(heap_ptr)
            };
            let group_census: Vec<(&'static str, usize)> = breakdown
                .groups
                .iter()
                .map(|&(name, _, count)| (name, count))
                .collect();
            assert!(
                !group_census.is_empty(),
                "{mode:?}: root census must be non-empty"
            );
            // Newly interned symbol on the no-thread path is NOT born projected.
            let fresh_sym = intern("s3-no-thread-census-fresh-sym");
            ctx.set_variable("s3-no-thread-census-fresh-sym", Value::fixnum(7));
            assert!(
                ctx.obarray.thread_projection_of(fresh_sym).is_none(),
                "{mode:?}: symbols must not be projected on the no-thread path"
            );
            group_census
        })
    };

    let sim_groups = observe_census(ThreadingMode::Sim);
    let green_groups = observe_census(ThreadingMode::Green);
    assert_eq!(
        green_groups, sim_groups,
        "no-thread Green root census groups and root counts must match Sim identically"
    );

    // Non-vacuity contrast: once `make-thread` DOES run in Green, admission,
    // GreenRuntime, GreenSeat, OwnerSlab, Baton holder, facade exclusion, and
    // thread projections all become active.
    run_on_fresh_os_thread(|| {
        let mut active = new_context(ThreadingMode::Green);
        assert_eq!(
            eval_printed(&mut active, "(thread-join (make-thread (lambda () 'ran)))"),
            "ran"
        );
        assert!(active.threads_admitted());
        assert!(active.tagged_heap.threads_admitted());
        assert!(active.owner.slab().is_some());
        assert!(active.owner_slab_count_for_test() >= 1);
        assert!(active.green_seat().is_some());
        assert!(active.has_green_runtime_for_test());
        assert_eq!(
            active.baton_holder_for_test(),
            Some(std::thread::current().id())
        );
        assert!(active.tagged_heap.facade_mark_is_excluded());
        assert!(active.thread_projection_count_for_test() > 0);
    });
}

#[test]
fn green_no_thread_tagged_heap_identity_and_root_tls_match_serial_owner() {
    let observe_tls = |mode: ThreadingMode| {
        run_on_fresh_os_thread(move || {
            assert!(
                !tagged_heap_is_installed(),
                "fresh OS thread must start with no tagged heap installed"
            );
            assert_eq!(
                current_tagged_heap_identity(),
                None,
                "fresh OS thread must start with no tagged heap identity"
            );

            let mut ctx = new_context(mode);
            let owner_id = ctx.tagged_heap.tagged_heap_identity();

            // Immediately after construction: installed heap and identity are
            // strictly this serial owner's.
            assert!(tagged_heap_is_installed());
            assert!(tagged_heap_is_current(&ctx.tagged_heap));
            assert_eq!(current_tagged_heap_identity(), Some(owner_id));
            assert_green_machinery_dormant(&mut ctx, &format!("{mode:?} at startup"));

            // After ordinary Lisp, read-only thread queries, and GC: heap root
            // and identity TLS still point strictly to this serial owner.
            assert_eq!(
                eval_printed(&mut ctx, NO_THREAD_WORKLOAD),
                EXPECTED_NO_THREAD_RESULT
            );
            assert!(ctx.tagged_heap.gc_collections() > 0);
            assert!(tagged_heap_is_installed());
            assert!(tagged_heap_is_current(&ctx.tagged_heap));
            assert_eq!(current_tagged_heap_identity(), Some(owner_id));
            assert_green_machinery_dormant(&mut ctx, &format!("{mode:?} after GC"));
        })
    };

    observe_tls(ThreadingMode::Sim);
    observe_tls(ThreadingMode::Green);
}

#[test]
fn green_no_thread_startup_and_queries_leak_no_parallel_era_or_carrier_tls() {
    run_on_fresh_os_thread(|| {
        #[cfg(debug_assertions)]
        let fresh_gc_tls = gc_tls_fingerprint();

        assert!(!tagged_heap_is_installed());
        assert_eq!(current_tagged_heap_identity(), None);
        assert!(carrier_heap_is_acceptable_for_job_start());

        // Compare against a no-thread Green Context: creating it, running
        // Lisp, querying thread primitives, locking an uncontended mutex,
        // yielding, and collecting must never touch GreenRuntime, admission,
        // projection scopes, or transport root tables.
        let mut ctx = new_context(ThreadingMode::Green);
        assert_green_machinery_dormant(&mut ctx, "green startup");

        assert_eq!(
            eval_printed(&mut ctx, NO_THREAD_WORKLOAD),
            EXPECTED_NO_THREAD_RESULT
        );
        assert_green_machinery_dormant(&mut ctx, "green after no-thread queries + GC");

        // Verify the serial owner's obarray did not enter a projection scope:
        // creating and binding a new variable leaves its cell unprojected.
        assert_eq!(
            eval_printed(
                &mut ctx,
                "(progn (defvar s3-nt-late-sym 123) s3-nt-late-sym)"
            ),
            "123"
        );
        assert!(
            ctx.obarray
                .thread_projection_of(intern("s3-nt-late-sym"))
                .is_none(),
            "late-created variable must stay unprojected when Green is dormant"
        );
        assert_eq!(ctx.thread_projection_count_for_test(), 0);

        let heap_id = ctx.tagged_heap.heap_identity();
        let world_alive = ctx.world_lifetime_probe_for_test();
        #[cfg(debug_assertions)]
        assert_ne!(
            gc_tls_fingerprint(),
            fresh_gc_tls,
            "live Context with installed TaggedHeap must change GC TLS fingerprint (guards against a constant probe)"
        );
        drop(ctx);

        assert!(!world_alive(), "no-thread Green World must drop cleanly");
        assert!(!tagged_heap_is_installed());
        assert_eq!(current_tagged_heap_identity(), None);
        assert!(!collector_is_registered(heap_id));
        assert!(!root_table_is_alive_for_test(heap_id));
        assert!(carrier_heap_is_acceptable_for_job_start());
        #[cfg(debug_assertions)]
        assert_eq!(
            gc_tls_fingerprint(),
            fresh_gc_tls,
            "post-drop GC TLS mirrors must return to untouched fresh-thread state"
        );
    });
}

#[test]
fn green_no_thread_and_threaded_teardown_leave_nothing_registered() {
    #[derive(Clone, Copy)]
    enum Arm {
        SimNoThread,
        GreenNoThread,
        GreenJoinedWorkers,
        GreenParkedWorkerAtDrop,
    }

    let run_arm = |arm: Arm| {
        run_on_fresh_os_thread(move || {
            #[cfg(debug_assertions)]
            let fresh_gc_tls = gc_tls_fingerprint();

            let (heap_id, world_alive) = {
                let mut ctx = match arm {
                    Arm::SimNoThread => new_context(ThreadingMode::Sim),
                    Arm::GreenNoThread | Arm::GreenJoinedWorkers | Arm::GreenParkedWorkerAtDrop => {
                        new_context(ThreadingMode::Green)
                    }
                };
                match arm {
                    Arm::SimNoThread | Arm::GreenNoThread => {
                        assert_eq!(
                            eval_printed(&mut ctx, NO_THREAD_WORKLOAD),
                            EXPECTED_NO_THREAD_RESULT
                        );
                        assert_green_machinery_dormant(&mut ctx, "before no-thread drop");
                    }
                    Arm::GreenJoinedWorkers => {
                        assert_eq!(
                            eval_printed(
                                &mut ctx,
                                "(progn
                                   (let ((t1 (make-thread (lambda () (+ 20 22)) \"s3-td-1\"))
                                         (t2 (make-thread (lambda () (copy-sequence \"done\")) \"s3-td-2\")))
                                     (list (thread-join t1) (thread-join t2))))"
                            ),
                            "(42 \"done\")"
                        );
                        assert_eq!(eval_printed(&mut ctx, "(progn (garbage-collect) t)"), "t");
                    }
                    Arm::GreenParkedWorkerAtDrop => {
                        // Leave a real worker parked in `condition-wait` when
                        // `Context::drop` (`green_teardown`) runs.
                        assert_eq!(
                            eval_printed(
                                &mut ctx,
                                "(progn
                                   (setq s3-td-m (make-mutex)
                                         s3-td-ready-c (make-condition-variable s3-td-m)
                                         s3-td-park-c (make-condition-variable s3-td-m)
                                         s3-td-ready nil)
                                   (setq s3-td-worker
                                     (make-thread
                                       (lambda ()
                                         (mutex-lock s3-td-m)
                                         (setq s3-td-ready t)
                                         (condition-notify s3-td-ready-c)
                                         (condition-wait s3-td-park-c)
                                         (mutex-unlock s3-td-m))
                                       \"s3-td-parked\"))
                                   (mutex-lock s3-td-m)
                                   (while (null s3-td-ready) (condition-wait s3-td-ready-c))
                                   (mutex-unlock s3-td-m)
                                   (list (thread-live-p s3-td-worker)
                                         (thread-last-error s3-td-worker)))"
                            ),
                            "(t nil)"
                        );
                        assert!(
                            root_table_is_alive_for_test(ctx.tagged_heap.heap_identity()),
                            "parked worker must have published roots before teardown"
                        );
                        let mut parked_roots = Vec::new();
                        collect_shared_root_gc_roots(&ctx.tagged_heap, &mut parked_roots);
                        assert!(
                            !parked_roots.is_empty(),
                            "parked worker must hold non-empty published roots at drop"
                        );
                    }
                }
                #[cfg(debug_assertions)]
                assert_ne!(
                    gc_tls_fingerprint(),
                    fresh_gc_tls,
                    "live Context must differ from untouched fresh-thread GC TLS fingerprint before drop"
                );
                let heap_id = ctx.tagged_heap.heap_identity();
                let world_alive = ctx.world_lifetime_probe_for_test();
                (heap_id, world_alive)
            };

            assert!(
                !world_alive(),
                "World allocation must be completely freed at teardown"
            );
            assert!(
                !tagged_heap_is_installed(),
                "TAGGED_HEAP TLS must be null after teardown"
            );
            assert_eq!(
                current_tagged_heap_identity(),
                None,
                "TAGGED_HEAP_ID TLS must be None after teardown"
            );
            assert!(
                !collector_is_registered(heap_id),
                "collector thread must be unregistered after teardown"
            );
            assert!(
                !root_table_is_alive_for_test(heap_id),
                "transport root table must be unregistered after teardown"
            );
            assert!(
                carrier_heap_is_acceptable_for_job_start(),
                "thread TLS must be clean for subsequent heap installation"
            );
            #[cfg(debug_assertions)]
            assert_eq!(
                gc_tls_fingerprint(),
                fresh_gc_tls,
                "post-teardown GC TLS fingerprint must equal untouched fresh-thread TLS"
            );
            #[cfg(debug_assertions)]
            {
                gc_tls_fingerprint()
            }
            #[cfg(not(debug_assertions))]
            {
                0u64
            }
        })
    };

    let sim_fp = run_arm(Arm::SimNoThread);
    let green_no_thread_fp = run_arm(Arm::GreenNoThread);
    let green_joined_fp = run_arm(Arm::GreenJoinedWorkers);
    let green_parked_fp = run_arm(Arm::GreenParkedWorkerAtDrop);
    assert_eq!(green_no_thread_fp, sim_fp);
    assert_eq!(green_joined_fp, sim_fp);
    assert_eq!(green_parked_fp, sim_fp);
}
