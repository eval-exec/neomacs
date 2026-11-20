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
fn stopped_major_still_refuses_callback_payloads() {
    let mut context = clean_context();
    let heap = unsafe { context.tagged_heap_mut() };
    let _user_ptr = heap.alloc_user_ptr(std::ptr::null_mut(), None);
    let _module_function = heap.alloc_module_function(
        0,
        0,
        std::ptr::null(),
        std::ptr::null_mut(),
        TaggedValue::NIL,
        TaggedValue::NIL,
    );

    let outcome = run_stopped_major(&mut context);
    let error = outcome.expect_err("a callback payload must stay refused");
    assert!(
        error.contains("Publication"),
        "admission refuses a native callback payload: {error}"
    );
}

/// E3 s4': each callback-bearing payload refuses on its own heap. The
/// combined test above allocates UserPtr and ModuleFunction together, so the
/// catalog walk could reach either first and mask the other; Sqlite has no
/// other stopped-major cover at all. One fresh context per payload makes
/// each refusal independent of walk order.
#[test]
fn stopped_major_still_refuses_a_user_ptr_payload_alone() {
    let mut context = clean_context();
    let heap = unsafe { context.tagged_heap_mut() };
    let _user_ptr = heap.alloc_user_ptr(std::ptr::null_mut(), None);

    let outcome = run_stopped_major(&mut context);
    let error = outcome.expect_err("a user_ptr payload must stay refused");
    assert!(
        error.contains("Publication"),
        "admission refuses a user_ptr payload: {error}"
    );
}

#[test]
fn stopped_major_still_refuses_a_module_function_payload_alone() {
    let mut context = clean_context();
    let heap = unsafe { context.tagged_heap_mut() };
    let _module_function = heap.alloc_module_function(
        0,
        0,
        std::ptr::null(),
        std::ptr::null_mut(),
        TaggedValue::NIL,
        TaggedValue::NIL,
    );

    let outcome = run_stopped_major(&mut context);
    let error = outcome.expect_err("a module_function payload must stay refused");
    assert!(
        error.contains("Publication"),
        "admission refuses a module_function payload: {error}"
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
