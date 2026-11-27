//! E3 slice 1: the stopped major runs the shared post-mark adapters.
//!
//! The serial stop (`complete_collection`) and incremental termination already
//! call `finish_stopped_mark_adapters`; the stopped-major arm of the parallel
//! era still calls only the weak sweep under a test permit and refuses a heap
//! with a finalizer registry, a weak-table list, or a resource queue. These
//! shells drive one real `AllStopped` job per case, through the ordinary
//! (no-permit) proof admission, and inspect the heap after the job returns.

use crate::emacs_core::eval::Context;
use neovm_eval::test_support::authority::{
    admit_owned_proof_parallel_for_test, baton_collect_parallel_world_roots,
};
use neovm_value::tagged::value::TaggedValue;
use neovm_value::test_support::authority::{
    assert_stopped_adapter_ephemerons, assert_stopped_adapter_resources,
    prepare_stopped_adapter_ephemerons, prepare_stopped_adapter_resources,
    set_generational_enabled_for_test,
};
use std::sync::atomic::{AtomicUsize, Ordering};

unsafe extern "C" fn count_native_finalizer(data: *mut std::ffi::c_void) {
    // SAFETY: each fixture registers a live AtomicUsize for the duration of
    // the stopped collection and its post-stop drain.
    unsafe { &*data.cast::<AtomicUsize>() }.fetch_add(1, Ordering::Relaxed);
}

fn clean_context() -> Context {
    // The canonical core boot installs the eval hooks and builds the Context.
    let mut context = crate::emacs_core::eval_hook_installers::new_context();
    context.set_variable("window-dead-windows-table", TaggedValue::NIL);
    context.gc_collect_exact();
    context
}

/// One ordinary-admission stopped major (`ProofPlan`, no weak permit).
fn run_stopped_major(context: &mut Context) -> Result<(), String> {
    admit_owned_proof_parallel_for_test(context, |projection| {
        let era = projection.era();
        let mut stopped = era.stop().expect("stopped world");
        era.collect_stopped(&mut stopped, baton_collect_parallel_world_roots)
            .expect("stopped major collection");
        stopped.complete().expect("stop completion");
        let (collector, keeper) = projection.finish().expect("era finish");
        (collector, keeper, ())
    })
    .map_err(|error| format!("{error:?}"))
}

/// One ordinary-admission stopped major that observes the deferred-native
/// batch and the caller's counters BETWEEN `collect_stopped` and
/// `stopped.complete()`. A drain moved to era finish (after owners resume)
/// would leave the batch and the finalizer unsettled here.
fn run_stopped_major_observing_batch(
    context: &mut Context,
    mut observe: impl FnMut(Option<usize>),
) -> Result<(), String> {
    admit_owned_proof_parallel_for_test(context, |projection| {
        let era = projection.era();
        let mut stopped = era.stop().expect("stopped world");
        era.collect_stopped(&mut stopped, baton_collect_parallel_world_roots)
            .expect("stopped major collection");
        let batch = era
            .with_stopped_heap_for_test(&stopped, |heap| {
                heap.pending_native_destroy_count_for_test()
            })
            .expect("observe the stopped native batch");
        observe(batch);
        stopped.complete().expect("stop completion");
        let (collector, keeper) = projection.finish().expect("era finish");
        (collector, keeper, ())
    })
    .map_err(|error| format!("{error:?}"))
}

#[test]
fn stopped_major_admits_a_finalizer_and_queues_its_function() {
    let mut context = clean_context();
    let function = TaggedValue::fixnum(7);
    // Allocated and deliberately unrooted: the stop must doom the shell and
    // queue its function without running it.
    let _finalizer = unsafe { context.tagged_heap_mut() }.alloc_finalizer(function);

    let outcome = run_stopped_major(&mut context);
    assert!(
        outcome.is_ok(),
        "a finalizer registry must be admitted after E3 slice 1: {outcome:?}"
    );

    let heap = unsafe { context.tagged_heap_mut() };
    assert_eq!(heap.live_finalizer_count(), 0, "doomed shell stays swept");
    assert!(
        heap.has_pending_doomed_finalizers(),
        "the doomed function is queued, not run"
    );
    assert_eq!(heap.take_doomed_finalizer_functions(), vec![function]);
}

/// One ordinary-admission stopped minor (GEN0), same adapter contract.
fn run_stopped_minor(context: &mut Context) -> Result<(), String> {
    admit_owned_proof_parallel_for_test(context, |projection| {
        let era = projection.era();
        let mut stopped = era.stop().expect("stopped world");
        era.collect_stopped_minor(&mut stopped, baton_collect_parallel_world_roots)
            .expect("stopped minor collection");
        stopped.complete().expect("stop completion");
        let (collector, keeper) = projection.finish().expect("era finish");
        (collector, keeper, ())
    })
    .map_err(|error| format!("{error:?}"))
}

#[test]
fn stopped_minor_admits_a_finalizer_and_queues_its_function() {
    let mut context = clean_context();
    // R4 GEN0 evidence: the minor termination reaches the same adapter.
    unsafe { set_generational_enabled_for_test(context.tagged_heap_mut(), true) };
    let function = TaggedValue::fixnum(9);
    let _finalizer = unsafe { context.tagged_heap_mut() }.alloc_finalizer(function);

    let outcome = run_stopped_minor(&mut context);
    assert!(
        outcome.is_ok(),
        "a finalizer registry must be admitted on the minor era path: {outcome:?}"
    );

    let heap = unsafe { context.tagged_heap_mut() };
    assert_eq!(heap.live_finalizer_count(), 0, "the doomed shell is swept");
    assert!(
        heap.has_pending_doomed_finalizers(),
        "the minor termination queues the function, not run it"
    );
}

#[test]
fn stopped_major_sweeps_a_weak_table_without_a_test_permit() {
    let mut context = clean_context();
    context
        .eval_str("(setq e3-weak-table (make-hash-table :test 'eq :weakness 'key))")
        .expect("make weak table");
    context
        .eval_str("(setq e3-live-key (list 1))")
        .expect("live key");
    context
        .eval_str("(puthash e3-live-key 'kept e3-weak-table)")
        .expect("live entry");
    context
        .eval_str("(let ((dead (list 2))) (puthash dead 'dropped e3-weak-table))")
        .expect("dead entry");
    assert_eq!(
        context
            .eval_str("(hash-table-count e3-weak-table)")
            .expect("setup count")
            .as_fixnum(),
        Some(2),
        "the setup must start with both entries"
    );

    let outcome = run_stopped_major(&mut context);
    assert!(
        outcome.is_ok(),
        "an eligible weak table must be admitted without a permit: {outcome:?}"
    );

    // The live key is a symbol-rooted cons, so W keeps its entry; the dead
    // key's entry is removed by the same stopped weak sweep.
    assert_eq!(
        context
            .eval_str("(hash-table-count e3-weak-table)")
            .expect("post-stop count")
            .as_fixnum(),
        Some(1),
        "the stopped weak sweep drops the dead key's entry"
    );
    assert_eq!(
        context
            .eval_str("(if (eq (gethash e3-live-key e3-weak-table) 'kept) 1 0)")
            .expect("live entry value")
            .as_fixnum(),
        Some(1),
        "the reached key's entry survives"
    );
}

#[test]
fn stopped_major_records_dead_handles_and_preserves_pending_drains() {
    let mut context = clean_context();
    // SAFETY: the fresh Context is the sole serial owner and no collection or
    // callback runs before admission freezes the heap.
    let fixture = unsafe { prepare_stopped_adapter_resources(context.tagged_heap_mut()) };
    context.set_variable("e3-live-surface", fixture.live_surface);
    context.set_variable("e3-live-video", fixture.live_video);

    let outcome = run_stopped_major(&mut context);
    assert!(
        outcome.is_ok(),
        "resource handles and pending drains must be admitted: {outcome:?}"
    );
    assert_stopped_adapter_resources(unsafe { context.tagged_heap_mut() }, &fixture);
}

#[test]
fn stopped_major_reclaims_unreached_ephemeron_registries() {
    let mut context = clean_context();
    // SAFETY: same sole-serial-owner contract as the resource fixture.
    let fixture = unsafe { prepare_stopped_adapter_ephemerons(context.tagged_heap_mut()) };
    context.set_variable("e3-live-buffer", fixture.live_buffer);
    context.set_variable("e3-live-process", fixture.live_process);
    context.set_variable("e3-live-frame", fixture.live_frame);

    let outcome = run_stopped_major(&mut context);
    assert!(
        outcome.is_ok(),
        "explicit-ephemeron registries must be admitted: {outcome:?}"
    );
    assert_stopped_adapter_ephemerons(unsafe { context.tagged_heap_mut() }, &fixture);
}

#[test]
fn stopped_major_admits_callback_payloads() {
    let mut context = clean_context();
    let heap = unsafe { context.tagged_heap_mut() };
    let _user_ptr = unsafe { heap.alloc_user_ptr(std::ptr::null_mut(), None) };
    let _module_function = unsafe {
        heap.alloc_module_function(
            0,
            0,
            std::ptr::null(),
            std::ptr::null_mut(),
            TaggedValue::NIL,
            TaggedValue::NIL,
        )
    };

    let outcome = run_stopped_major(&mut context);
    assert!(
        outcome.is_ok(),
        "a stopped sweep defers these payloads instead of refusing them: {outcome:?}"
    );
    assert_eq!(
        unsafe { context.tagged_heap_mut() }.pending_native_destroy_count_for_test(),
        None,
        "the post-stop drain consumes the batch"
    );
}

/// E3.2 real collect_stopped row: a counting user-ptr finalizer runs exactly
/// once, on the owner side of the stopped job (the batch is empty on return).
#[test]
fn stopped_major_defers_a_user_ptr_finalizer_and_drains_before_resume() {
    let count = AtomicUsize::new(0);
    let mut context = clean_context();
    let heap = unsafe { context.tagged_heap_mut() };
    let data = std::ptr::from_ref(&count).cast_mut().cast();
    let _user_ptr = unsafe { heap.alloc_user_ptr(data, Some(count_native_finalizer)) };

    let outcome = run_stopped_major_observing_batch(&mut context, |batch| {
        assert_eq!(
            count.load(Ordering::Relaxed),
            1,
            "the deferred finalizer must already have run before stopped.complete()"
        );
        assert_eq!(
            batch, None,
            "the stopped completion must have consumed the batch before owners resume"
        );
    });
    assert!(
        outcome.is_ok(),
        "user-ptr payload must be admitted: {outcome:?}"
    );
    assert_eq!(
        count.load(Ordering::Relaxed),
        1,
        "the deferred finalizer ran once before the stopped job returned"
    );
    assert_eq!(
        unsafe { context.tagged_heap_mut() }.pending_native_destroy_count_for_test(),
        None,
        "no finalizer is still owed"
    );

    Box::new(context).shutdown().unwrap();
    assert_eq!(
        count.load(Ordering::Relaxed),
        1,
        "shutdown does not call the drained finalizer again"
    );
}

#[test]
fn stopped_major_admits_a_module_function_and_drains_its_finalizer() {
    let count = AtomicUsize::new(0);
    let mut context = clean_context();
    let heap = unsafe { context.tagged_heap_mut() };
    let data = std::ptr::from_ref(&count).cast_mut().cast();
    let function = unsafe {
        heap.alloc_module_function(
            0,
            0,
            std::ptr::null(),
            data,
            TaggedValue::NIL,
            TaggedValue::NIL,
        )
    };
    // SAFETY: this fixture exclusively owns the initialized module function;
    // its data points to count, which outlives the stopped collection.
    unsafe {
        (*(function.as_veclike_ptr().unwrap()
            as *mut neovm_value::tagged::header::ModuleFunctionObj))
            .finalizer = Some(count_native_finalizer);
    }

    let outcome = run_stopped_major(&mut context);
    assert!(
        outcome.is_ok(),
        "module-function payload must be admitted: {outcome:?}"
    );
    assert_eq!(
        count.load(Ordering::Relaxed),
        1,
        "the deferred module finalizer ran once before resume"
    );
    assert_eq!(
        unsafe { context.tagged_heap_mut() }.pending_native_destroy_count_for_test(),
        None
    );
}

#[test]
fn stopped_major_still_refuses_a_sqlite_payload_alone() {
    let mut context = clean_context();
    let heap = unsafe { context.tagged_heap_mut() };
    let _sqlite = heap.alloc_sqlite(false, 0);

    let outcome = run_stopped_major(&mut context);
    let error = outcome.expect_err("a sqlite payload must stay refused");
    assert!(
        error.contains("Publication"),
        "admission refuses a sqlite payload: {error}"
    );
}

/// Guard that keeps synthetic pdump-mapped `HashTableObj` shells alive strictly
/// longer than the `Context` whose `TaggedHeap` registered them via
/// `register_mapped_veclike_object`.
struct MappedTableGuard {
    _boxes: Vec<Box<neovm_value::tagged::header::HashTableObj>>,
}

impl MappedTableGuard {
    fn new() -> Self {
        Self { _boxes: Vec::new() }
    }

    fn register(
        &mut self,
        heap: &mut neovm_value::tagged::gc::TaggedHeap,
        weakness: Option<neovm_value::value::HashTableWeakness>,
    ) -> TaggedValue {
        use neovm_value::tagged::header::{HashTableObj, VecLikeHeader};
        use neovm_value::value::{HashTableTest, LispHashTable};

        let table = Box::new(HashTableObj::new(LispHashTable::new_with_options(
            HashTableTest::Eq,
            4,
            weakness,
            1.5,
            0.8,
        )));
        let ptr = std::ptr::from_ref(&*table).cast_mut();
        // SAFETY: `table` is stored in `self._boxes`, which is declared BEFORE
        // `context` in every test so it outlives `heap` and remains mapped and
        // writable for the entire heap lifetime.
        unsafe {
            heap.register_mapped_veclike_object(
                ptr.cast::<VecLikeHeader>(),
                std::mem::size_of::<HashTableObj>(),
            );
        }
        let value = unsafe { TaggedValue::from_veclike_ptr(ptr.cast::<VecLikeHeader>()) };
        self._boxes.push(table);
        value
    }
}

/// #16441 Bug 2 (pre-partition): a hydrated mapped weak hash table present
/// before the first partition cycle (`dump_blackened == false`) is admitted by
/// `supported(heap)` and swept by a stopped major collection (dropping the
/// weakly-only-reachable entry while keeping the strongly-rooted entry and the
/// mapped shell intact).
#[test]
fn stopped_major_admits_and_sweeps_mapped_weak_table_pre_partition() {
    let mut mapped_guard = MappedTableGuard::new();
    let mut context = crate::emacs_core::eval_hook_installers::new_context();
    let mapped_table = mapped_guard.register(
        unsafe { context.tagged_heap_mut() },
        Some(neovm_value::value::HashTableWeakness::Key),
    );
    context.set_variable("b6-pre-mapped-weak", mapped_table);
    context
        .eval_str("(setq b6-pre-live-key (list 'live))")
        .expect("live key");
    context
        .eval_str("(puthash b6-pre-live-key 'kept b6-pre-mapped-weak)")
        .expect("insert live");
    context
        .eval_str("(let ((dead (list 'dead))) (puthash dead 'dropped b6-pre-mapped-weak))")
        .expect("insert dead");
    assert_eq!(
        context
            .eval_str("(hash-table-count b6-pre-mapped-weak)")
            .expect("count before")
            .as_fixnum(),
        Some(2)
    );

    let outcome = run_stopped_major(&mut context);
    assert!(
        outcome.is_ok(),
        "pre-partition hydrated mapped weak table must be admitted: {outcome:?}"
    );
    assert_eq!(
        context
            .eval_str("(hash-table-count b6-pre-mapped-weak)")
            .expect("count after")
            .as_fixnum(),
        Some(1),
        "stopped major must drop the dead weak entry from a pre-partition mapped weak table"
    );
    assert_eq!(
        context
            .eval_str("(if (eq (gethash b6-pre-live-key b6-pre-mapped-weak) 'kept) 1 0)")
            .expect("live check")
            .as_fixnum(),
        Some(1),
        "strongly-rooted entry and mapped shell must remain intact"
    );
    drop(context);
    drop(mapped_guard);
}

/// #16441 Bug 2 (post-partition): after `promote_and_blacken` promotes a
/// loader-stamped boxed weak hash table to `generation.permanent()` and
/// blackens a mapped weak hash table (registering both in
/// `always_swept_weak_tables`), a stopped major collection admits BOTH, sweeps
/// their weakly-only-reachable entries, and preserves their strongly-rooted
/// entries and permanent/mapped shells intact.
#[test]
fn stopped_major_admits_and_sweeps_permanent_and_mapped_weak_tables_post_partition() {
    let mut mapped_guard = MappedTableGuard::new();
    let mut context = crate::emacs_core::eval_hook_installers::new_context();
    let mapped_table = mapped_guard.register(
        unsafe { context.tagged_heap_mut() },
        Some(neovm_value::value::HashTableWeakness::Key),
    );
    context.set_variable("b6-post-mapped-weak", mapped_table);
    let perm_table = context
        .eval_str("(setq b6-post-perm-weak (make-hash-table :test 'eq :weakness 'key))")
        .expect("make perm weak table");
    let _stamp = unsafe { context.tagged_heap_mut() }.note_image_object(perm_table);

    // First full collection runs `promote_and_blacken`: `perm_table` becomes
    // `generation.permanent()`, `mapped_table` becomes blackened, and both are
    // registered in `always_swept_weak_tables`.
    context.gc_collect_exact();
    let perm_header_is_permanent = unsafe {
        let ptr = perm_table
            .as_veclike_ptr()
            .expect("perm table veclike")
            .cast::<neovm_value::tagged::header::GcHeader>();
        (*ptr).generation.permanent()
    };
    assert!(
        perm_header_is_permanent,
        "setup must promote image-resident weak table to permanent generation"
    );

    context
        .eval_str(
            "(progn
               (setq b6-post-perm-live (list 'perm-live))
               (setq b6-post-mapped-live (list 'mapped-live))
               (puthash b6-post-perm-live 'perm-kept b6-post-perm-weak)
               (let ((dead (list 'perm-dead)))
                 (puthash dead 'perm-dropped b6-post-perm-weak))
               (puthash b6-post-mapped-live 'mapped-kept b6-post-mapped-weak)
               (let ((dead (list 'mapped-dead)))
                 (puthash dead 'mapped-dropped b6-post-mapped-weak)))",
        )
        .expect("populate post-partition tables");

    let outcome = run_stopped_major(&mut context);
    assert!(
        outcome.is_ok(),
        "post-partition permanent and mapped weak tables must be admitted: {outcome:?}"
    );
    assert_eq!(
        context
            .eval_str(
                "(list (hash-table-count b6-post-perm-weak)
                       (if (eq (gethash b6-post-perm-live b6-post-perm-weak) 'perm-kept) 1 0)
                       (hash-table-count b6-post-mapped-weak)
                       (if (eq (gethash b6-post-mapped-live b6-post-mapped-weak) 'mapped-kept) 1 0))",
            )
            .map(|v| crate::emacs_core::print::print_value(&v))
            .expect("post-major check"),
        "(1 1 1 1)",
        "stopped major must drop dead entries and keep live entries in both permanent and mapped weak tables"
    );
    drop(context);
    drop(mapped_guard);
}

/// #16441 Bug 2 (Criteria 1 & 4 — `TableMut::edit` barrier + stopped minor):
/// when a parallel owner inserts young keys and unrooted-except-by-table young
/// values via `TableMut::edit` into (a) a permanent boxed weak table, (b) a
/// mapped weak table, and (c) a mapped non-weak table, `TableMut::edit` must
/// record `r_mapped_seen` + `remset` so a subsequent `collect_stopped_minor`:
/// 1. drops the dead young keys from both weak tables (`count == 1`),
/// 2. marks and promotes the surviving young values (only reachable through
///    the permanent/mapped tables), and
/// 3. preserves persistent `mapped_remembered` across a subsequent serial major.
#[test]
fn stopped_minor_after_parallel_table_mut_edit_on_permanent_and_mapped_tables() {
    use neovm_value::tagged::gc::mutator_registration::TableMut;
    use neovm_value::value::HashKey;

    let mut mapped_guard = MappedTableGuard::new();
    let mut context = crate::emacs_core::eval_hook_installers::new_context();
    context.set_variable("window-dead-windows-table", TaggedValue::NIL);
    unsafe { set_generational_enabled_for_test(context.tagged_heap_mut(), true) };

    let mapped_weak = mapped_guard.register(
        unsafe { context.tagged_heap_mut() },
        Some(neovm_value::value::HashTableWeakness::Key),
    );
    let mapped_strong = mapped_guard.register(unsafe { context.tagged_heap_mut() }, None);
    context.set_variable("b6-min-mapped-weak", mapped_weak);
    context.set_variable("b6-min-mapped-strong", mapped_strong);

    let perm_weak = context
        .eval_str("(setq b6-min-perm-weak (make-hash-table :test 'eq :weakness 'key))")
        .expect("make perm weak table");
    let _stamp = unsafe { context.tagged_heap_mut() }.note_image_object(perm_weak);

    // Promote `perm_weak` to permanent + blacken `mapped_weak` / `mapped_strong`.
    // `window-dead-windows-table` is already NIL so `admit_owned_proof_parallel_for_test`
    // runs zero GC during admission, keeping the conses below genuinely young.
    context.gc_collect_exact();

    // Allocate fresh YOUNG keys and young values AFTER partition with ZERO
    // `SharedRoot`s (no deferred root retention):
    // - `perm_live_key` / `mapped_live_key` are rooted in Lisp vars;
    // - `perm_live_val`, `mapped_live_val`, `mapped_strong_val` are ONLY
    //   referenced through the permanent/mapped tables after `TableMut::edit`;
    // - `perm_dead_key`, `mapped_dead_key` have ZERO strong roots anywhere.
    let perm_live_key = context
        .eval_str("(setq b6-min-perm-live-key (list 'plk))")
        .expect("perm live key");
    let mapped_live_key = context
        .eval_str("(setq b6-min-mapped-live-key (list 'mlk))")
        .expect("mapped live key");
    let (perm_live_val, perm_dead_key, mapped_live_val, mapped_dead_key, mapped_strong_val) = {
        let heap = unsafe { context.tagged_heap_mut() };
        let plv = heap.alloc_cons(TaggedValue::fixnum(111), TaggedValue::NIL);
        let pdk = heap.alloc_cons(TaggedValue::fixnum(199), TaggedValue::NIL);
        let mlv = heap.alloc_cons(TaggedValue::fixnum(222), TaggedValue::NIL);
        let mdk = heap.alloc_cons(TaggedValue::fixnum(299), TaggedValue::NIL);
        let msv = heap.alloc_cons(TaggedValue::fixnum(333), TaggedValue::NIL);
        (plv, pdk, mlv, mdk, msv)
    };
    let perm_weak_bits = perm_weak.bits();
    let mapped_weak_bits = mapped_weak.bits();
    let mapped_strong_bits = mapped_strong.bits();

    admit_owned_proof_parallel_for_test(&mut context, |projection| {
        let era = projection.era();
        let mut owner = era
            .register(1, 8)
            .expect("register parallel owner")
            .activate(era.heap_identity(), 1)
            .expect("activate parallel owner");
        {
            let mut run = owner.run_access().expect("run access");
            // Edit permanent boxed weak table via parallel `TableMut::edit`.
            let mut lease =
                TableMut::enter(&mut run, unsafe { TaggedValue::from_bits(perm_weak_bits) })
                    .expect("enter perm weak");
            lease.edit(|ht| {
                ht.insert(
                    HashKey::Ptr(perm_live_key.bits()),
                    perm_live_key,
                    perm_live_val,
                );
                ht.insert(
                    HashKey::Ptr(perm_dead_key.bits()),
                    perm_dead_key,
                    TaggedValue::fixnum(901),
                );
            });
            drop(lease);

            // Edit mapped weak table via parallel `TableMut::edit`.
            let mut lease = TableMut::enter(&mut run, unsafe {
                TaggedValue::from_bits(mapped_weak_bits)
            })
            .expect("enter mapped weak");
            lease.edit(|ht| {
                ht.insert(
                    HashKey::Ptr(mapped_live_key.bits()),
                    mapped_live_key,
                    mapped_live_val,
                );
                ht.insert(
                    HashKey::Ptr(mapped_dead_key.bits()),
                    mapped_dead_key,
                    TaggedValue::fixnum(902),
                );
            });
            drop(lease);

            // Edit mapped non-weak table via parallel `TableMut::edit`.
            let mut lease = TableMut::enter(&mut run, unsafe {
                TaggedValue::from_bits(mapped_strong_bits)
            })
            .expect("enter mapped strong");
            lease.edit(|ht| {
                ht.insert(HashKey::Int(7), TaggedValue::fixnum(7), mapped_strong_val);
            });
            drop(lease);
        }

        owner.enter_native(&[]).expect("enter native before stop");
        let mut stopped = era.stop().expect("stop world for minor");
        let report = era
            .collect_stopped_minor(&mut stopped, baton_collect_parallel_world_roots)
            .expect("stopped minor collection");
        assert!(!report.major, "must execute a stopped minor cycle");
        stopped.complete().expect("complete minor stop");
        owner.leave_native().expect("leave native");
        owner.retire_rooted(&[]).expect("retire owner");
        let (collector, keeper) = projection.finish().expect("era finish");
        (collector, keeper, ())
    })
    .expect("parallel era with permanent/mapped tables must succeed");

    // Verify dead young keys dropped AND surviving young values promoted intact.
    assert_eq!(
        context
            .eval_str(
                "(list (hash-table-count b6-min-perm-weak)
                       (car (gethash b6-min-perm-live-key b6-min-perm-weak))
                       (hash-table-count b6-min-mapped-weak)
                       (car (gethash b6-min-mapped-live-key b6-min-mapped-weak))
                       (car (gethash 7 b6-min-mapped-strong)))",
            )
            .map(|v| crate::emacs_core::print::print_value(&v))
            .expect("post-minor check"),
        "(1 111 1 222 333)",
        "stopped minor must sweep dead young keys and retain live young values written via TableMut::edit"
    );

    // Run a subsequent serial major GC to prove `r_mapped_seen` persisted into
    // `heap.mapped_remembered` across cycles (the mapped strong table's value
    // and both permanent/mapped weak tables stay intact).
    context.gc_collect_exact();
    assert_eq!(
        context
            .eval_str(
                "(list (hash-table-count b6-min-perm-weak)
                       (car (gethash b6-min-perm-live-key b6-min-perm-weak))
                       (hash-table-count b6-min-mapped-weak)
                       (car (gethash b6-min-mapped-live-key b6-min-mapped-weak))
                       (car (gethash 7 b6-min-mapped-strong)))",
            )
            .map(|v| crate::emacs_core::print::print_value(&v))
            .expect("post-subsequent-major check"),
        "(1 111 1 222 333)",
        "subsequent major GC must still see r_mapped_seen-remembered permanent/mapped tables"
    );
    drop(context);
    drop(mapped_guard);
}
