use super::super::super::Context;
use super::super::source_entry::install_inactive_source_module_probe_for_test;
use super::{
    NoEffects, OwnedSourceRegistries, RetainedSourceFailure, SourceEntryRefusal, SourceFrame,
    SourceWithdrawalFailure, SourceWithdrawalStep,
};
use neovm_text::emacs_core::ccl::{
    builtin_register_ccl_program_impl, collect_ccl_registry_gc_roots, current_ccl_registry_handle,
};
use neovm_value::value::Value;

fn source() -> Box<Context> {
    install_inactive_source_module_probe_for_test();
    let mut source = Box::new(Context::new_minimal_vm_harness(
        crate::test_utils::unit_harness(),
    ));
    source.setup_thread_locals();
    source.gc_collect_exact();
    source
}

fn capture(source: Box<Context>) -> Result<OwnedSourceRegistries, SourceWithdrawalFailure> {
    // SAFETY: fixtures own fresh serial Contexts with no collector, running
    // mutator, store callback, or borrowed heap view. Registry-reader controls
    // borrow only out-of-heap registry containers, not heap or store storage.
    unsafe { OwnedSourceRegistries::capture(source) }
}

fn gen1_source() -> Box<Context> {
    let source = source();
    if source.tagged_heap.tagged_heap_generational_enabled() {
        return source;
    }
    match capture(source) {
        Err(SourceWithdrawalFailure::BeforeEffects(failure)) => {
            assert_eq!(failure.reason, SourceEntryRefusal::GenerationalRequired);
            assert!(neovm_value::tagged::gc::tagged_heap_is_current(
                &failure.source.tagged_heap
            ));
            panic!("these rows need a generational heap; NEOVM_GC_GENERATIONAL=0 disables it")
        }
        other => panic!("GEN0 must refuse before any effect: {:?}", other.err()),
    }
}

fn retained(error: SourceWithdrawalFailure, step: SourceWithdrawalStep) -> RetainedSourceFailure {
    match error {
        SourceWithdrawalFailure::Retained(failure) => {
            assert_eq!(failure.step, step);
            failure
        }
        other => panic!("expected owned partial failure: {other:?}"),
    }
}

#[test]
fn pentry_e2b_owned_source_exact_box_and_heap_are_retained() {
    let source = gen1_source();
    let address = std::ptr::from_ref(&*source);
    let heap = source.tagged_heap.heap_identity();
    let captured = capture(source).expect("actual source receipt chain");
    assert_eq!(std::ptr::from_ref(&**captured._frame.source), address);
    assert_eq!(captured._frame.source.tagged_heap.heap_identity(), heap);
    assert!(captured._frame.source.tagged_heap.facade_mark_is_excluded());
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &captured._frame.source.tagged_heap
    ));
}

#[test]
fn pentry_e2b_owned_source_drop_retains_actual_world_and_registries() {
    let source = gen1_source();
    let world_alive = source.world_lifetime_probe_for_test();
    let ccl = source.ccl_registry.downgrade();
    drop(capture(source).expect("selected effects"));
    assert!(world_alive());
    assert!(ccl.upgrade().is_some());
}

#[test]
fn pentry_e2b_owned_source_forget_retains_actual_world() {
    let source = gen1_source();
    let world_alive = source.world_lifetime_probe_for_test();
    std::mem::forget(capture(source).expect("selected effects"));
    assert!(world_alive());
}

#[test]
fn pentry_e2b_owned_source_unwind_after_capture_retains_actual_world() {
    let source = gen1_source();
    let world_alive = source.world_lifetime_probe_for_test();
    let captured = capture(source).expect("selected effects");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _captured = captured;
        panic!("OWNED_SOURCE_AFTER_CAPTURE_UNWIND");
    }));
    assert!(result.is_err());
    assert!(world_alive());
}

#[test]
fn pentry_e2b_owned_source_unwind_during_first_adapter_retains_box() {
    let source = gen1_source();
    let world_alive = source.world_lifetime_probe_for_test();
    let frame = SourceFrame {
        source: std::mem::ManuallyDrop::new(source),
        effects: NoEffects,
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _ = frame.advance::<(), ()>(SourceWithdrawalStep::Keeper, |_| {
            panic!("OWNED_SOURCE_FIRST_ADAPTER_UNWIND")
        });
    }));
    assert!(result.is_err());
    assert!(world_alive());
}

#[test]
fn pentry_e2b_owned_source_unwind_after_real_receipt_retains_prior_effects() {
    let source = gen1_source();
    let world_alive = source.world_lifetime_probe_for_test();
    let ccl = source.ccl_registry.downgrade();
    let frame = SourceFrame {
        source: std::mem::ManuallyDrop::new(source),
        effects: NoEffects,
    };
    let frame = frame
        .advance(SourceWithdrawalStep::Keeper, Context::issue_green_baton)
        .unwrap();
    let frame = frame
        .advance(SourceWithdrawalStep::Ccl, |source| {
            neovm_text::emacs_core::ccl::detach_ccl_registry(&source.ccl_registry)
        })
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _ = frame.advance::<(), ()>(SourceWithdrawalStep::Charset, |_| {
            panic!("OWNED_SOURCE_AFTER_RECEIPT_UNWIND")
        });
    }));
    assert!(result.is_err());
    assert!(world_alive());
    assert!(ccl.upgrade().is_some());
}

#[test]
fn pentry_e2b_owned_source_first_registry_refusal_retains_actual_inputs() {
    let source = gen1_source();
    let world_alive = source.world_lifetime_probe_for_test();
    let handle = source.ccl_registry.clone();
    let reader = handle.borrow();
    let error = capture(source).err().expect("real registry loan refuses");
    assert!(format!("{error:?}").contains("RegistryBorrowed"));
    let failure = retained(error, SourceWithdrawalStep::Ccl);
    drop(reader);
    drop(failure);
    assert!(world_alive());
}

#[test]
fn pentry_e2b_owned_source_later_registry_refusal_preserves_prior_receipts() {
    let source = gen1_source();
    let world_alive = source.world_lifetime_probe_for_test();
    let ccl = source.ccl_registry.downgrade();
    let handle = source.hash_table_test_registry.clone();
    let reader = handle.borrow();
    let error = capture(source).err().expect("later registry loan refuses");
    assert!(format!("{error:?}").contains("RegistryBorrowed"));
    let failure = retained(error, SourceWithdrawalStep::HashTest);
    drop(reader);
    drop(failure);
    assert!(world_alive());
    assert!(ccl.upgrade().is_some());
}

#[test]
fn pentry_e2b_owned_source_last_slot_refusal_retains_all_prior_effects() {
    let source = gen1_source();
    let world_alive = source.world_lifetime_probe_for_test();
    let failure = neovm_value::quit::with_quit_requested_slot_borrowed_for_test(|| {
        let error = capture(source).err().expect("quit slot loan refuses");
        assert!(format!("{error:?}").contains("SlotBorrowed"));
        retained(error, SourceWithdrawalStep::Quit)
    });
    drop(failure);
    assert!(world_alive());
}

#[test]
fn pentry_e2b_owned_source_pre_effect_refusal_returns_same_serial_context() {
    let mut source = source();
    let address = std::ptr::from_ref(&*source);
    source.gc_inhibit_depth = 1;
    let mut source = match capture(source) {
        Err(SourceWithdrawalFailure::BeforeEffects(failure)) => {
            assert_eq!(failure.reason, SourceEntryRefusal::GcInhibited);
            failure.source
        }
        other => panic!("pre-effect refusal: {:?}", other.err()),
    };
    assert_eq!(std::ptr::from_ref(&*source), address);
    assert!(!source.tagged_heap.facade_mark_is_excluded());
    source.gc_inhibit_depth = 0;
    source.set_variable("pentry-owned-source-return", Value::fixnum(73));
    assert_eq!(
        source.builtin_var_value_by_name("pentry-owned-source-return"),
        Some(Value::fixnum(73))
    );
}

#[test]
fn pentry_e2b_owned_source_foreign_heap_refusal_preserves_current_owner() {
    let source = source();
    let parked_source = (*source).park_for_test();
    let foreign = self::source();
    let foreign_root = foreign.tagged_heap.heap_root();
    let parked_foreign = (*foreign).park_for_test();
    let source = Box::new(parked_source.resume_for_test().expect("resume source"));
    // SAFETY: `parked_foreign` owns `foreign_root` and outlives `foreign_heap`;
    // `source` owns the displaced root and outlives `drop(foreign_heap)`.
    let foreign_heap =
        unsafe { neovm_value::tagged::gc::NestedTaggedHeapInstall::install_bare(foreign_root) };
    let result = capture(source);
    drop(foreign_heap);
    match result {
        Err(SourceWithdrawalFailure::BeforeEffects(failure)) => {
            assert_eq!(failure.reason, SourceEntryRefusal::HeapTlsNotCurrent);
            let failure = (*failure.source).park_for_test();
            let foreign = parked_foreign.resume_for_test().expect("resume foreign");
            assert!(neovm_value::tagged::gc::tagged_heap_is_current(
                &foreign.tagged_heap
            ));
            drop(failure);
            assert!(neovm_value::tagged::gc::tagged_heap_is_current(
                &foreign.tagged_heap
            ));
        }
        other => panic!("foreign refusal: {:?}", other.err()),
    }
}

#[test]
fn pentry_e2b_owned_source_chain_order_includes_hook_routed_slots() {
    let source = gen1_source();
    let _ = super::super::super::lifecycle::take_source_shell_trace();
    capture(source).expect("owner parked before the host slots, including both display hooks");
    assert_eq!(
        super::super::super::lifecycle::take_source_shell_trace(),
        [
            "keeper",
            "owner-bindings",
            "ccl",
            "charset",
            "font",
            "font-hook",
            "terminal",
            "terminal-hook",
            "hash-test",
            "file-notify",
            "window-configuration",
            "dynamic-module",
            "in-flight",
            "integer-width",
            "quit",
        ]
    );
}

#[test]
fn pentry_e2b_owned_source_unregistered_font_hook_refuses() {
    let source = source();
    let _ = super::super::super::lifecycle::take_source_shell_trace();
    let error =
        super::super::super::lifecycle::invoke_font_registry_detach(None, &source.font_registry);
    assert!(matches!(
        error,
        Err(super::super::super::lifecycle::SourceShellHookRefusal::Unregistered)
    ));
    assert!(super::super::super::lifecycle::take_source_shell_trace().is_empty());
    assert!(!source.tagged_heap.facade_mark_is_excluded());
}

#[test]
fn pentry_e2b_owned_source_unregistered_terminal_hook_refuses() {
    let source = source();
    let _ = super::super::super::lifecycle::take_source_shell_trace();
    let error = super::super::super::lifecycle::invoke_terminal_registry_detach(
        None,
        &source.terminal_registry,
    );
    assert!(matches!(
        error,
        Err(super::super::super::lifecycle::SourceShellHookRefusal::Unregistered)
    ));
    assert!(super::super::super::lifecycle::take_source_shell_trace().is_empty());
    assert!(!source.tagged_heap.facade_mark_is_excluded());
}

#[test]
fn pentry_e2b_owned_source_original_registry_data_survives_withdrawal() {
    let source = gen1_source();
    let program = Value::vector(vec![Value::fixnum(0); 3]);
    let name = Value::symbol("pentry-e2b-owned-source-program");
    builtin_register_ccl_program_impl(vec![name, program]).expect("actual original registry entry");
    let original = source.ccl_registry.clone();
    let _captured = capture(source).expect("selected effects");
    let mut original_roots = Vec::new();
    collect_ccl_registry_gc_roots(&original, &mut original_roots);
    assert!(original_roots.contains(&program));
    let mut current_roots = Vec::new();
    collect_ccl_registry_gc_roots(&current_ccl_registry_handle(), &mut current_roots);
    assert!(!current_roots.contains(&program));
}
