use super::{
    SourceEntryRefusal, SourceModuleObservation, module_refusal, validate_serial_source_entry,
};
use crate::emacs_core::eval::{
    Context, SpecBinding, push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use neovm_text::emacs_core::ccl::source_scope::{SourceCclScope, source_ccl_scope};
use neovm_text::emacs_core::ccl::with_ccl_obarray;
use neovm_value::value::Value;

fn source() -> Context {
    let mut source = Context::new_minimal_vm_harness(crate::test_utils::unit_harness());
    source.setup_thread_locals();
    source
}

fn ready_result(source: &Context) -> Result<(), SourceEntryRefusal> {
    if source.tagged_heap.tagged_heap_generational_enabled() {
        Ok(())
    } else {
        Err(SourceEntryRefusal::GenerationalRequired)
    }
}

#[test]
fn pentry_e2b_source_entry_module_refusal_maps_each_observation() {
    assert_eq!(module_refusal(SourceModuleObservation::Inactive), Ok(()));
    assert_eq!(
        module_refusal(SourceModuleObservation::Active),
        Err(SourceEntryRefusal::ModuleScope)
    );
    assert_eq!(
        module_refusal(SourceModuleObservation::Borrowed),
        Err(SourceEntryRefusal::ModuleSlotBorrowed)
    );
}

#[test]
fn pentry_e2b_source_entry_unregistered_module_probe_is_inactive() {
    assert_eq!(
        super::source_module_observation(),
        SourceModuleObservation::Inactive
    );
}

#[test]
fn pentry_e2b_source_entry_gen1_or_typed_gen0_route() {
    let source = source();
    assert_eq!(validate_serial_source_entry(&source), ready_result(&source));
}

#[test]
fn pentry_e2b_source_entry_foreign_heap_tls_is_preserved() {
    let source = source();
    let foreign = self::source();
    assert_eq!(
        validate_serial_source_entry(&source),
        Err(SourceEntryRefusal::HeapTlsNotCurrent)
    );
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &foreign.tagged_heap
    ));
}

#[test]
fn pentry_e2b_source_entry_real_gc_inhibit_guard_refuses() {
    let mut source = source();
    source.with_gc_inhibited(|source| {
        assert_eq!(
            validate_serial_source_entry(source),
            Err(SourceEntryRefusal::GcInhibited)
        );
        assert_ne!(source.gc_inhibit_depth, 0);
    });
    assert_eq!(validate_serial_source_entry(&source), ready_result(&source));
}

#[test]
fn pentry_e2b_source_entry_driver_fault_refuses_without_clearing() {
    let mut source = source();
    source.gc_driver_active = true;
    assert_eq!(
        validate_serial_source_entry(&source),
        Err(SourceEntryRefusal::CollectionDriver)
    );
    assert!(source.gc_driver_active);
    source.gc_driver_active = false;
}

#[test]
fn pentry_e2b_source_entry_live_callback_accounting_is_not_detached() {
    let mut source = source();
    let mut accounting = source.capture_user_test_gc_accounting();
    let pointer = std::ptr::NonNull::from(&mut accounting);
    let previous = source.replace_user_test_gc_accounting(Some(pointer));
    assert_eq!(
        validate_serial_source_entry(&source),
        Err(SourceEntryRefusal::CallbackAccounting)
    );
    assert_eq!(source.user_test_gc_accounting_pointer(), Some(pointer));
    assert_eq!(
        source.replace_user_test_gc_accounting(previous),
        Some(pointer)
    );
}

#[test]
fn pentry_e2b_source_entry_live_scratch_roots_are_not_truncated() {
    let source = source();
    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(Value::fixnum(47));
    assert_eq!(
        validate_serial_source_entry(&source),
        Err(SourceEntryRefusal::ScratchRoots)
    );
    assert_eq!(save_scratch_gc_roots(), saved + 1);
    super::super::super::SCRATCH_GC_ROOTS.with(|roots| {
        assert_eq!(roots.borrow()[saved].value, Value::fixnum(47));
    });
    restore_scratch_gc_roots(saved);
}

#[test]
fn pentry_e2b_source_entry_empty_scratch_reader_refuses() {
    let source = source();
    super::super::super::SCRATCH_GC_ROOTS.with(|slot| {
        let view = slot.borrow();
        assert!(view.is_empty());
        assert_eq!(
            validate_serial_source_entry(&source),
            Err(SourceEntryRefusal::ScratchSlotBorrowed)
        );
        assert!(view.is_empty());
    });
}

#[test]
fn pentry_e2b_source_entry_empty_scratch_writer_refuses() {
    let source = source();
    super::super::super::SCRATCH_GC_ROOTS.with(|slot| {
        let view = slot.borrow_mut();
        assert!(view.is_empty());
        assert_eq!(
            validate_serial_source_entry(&source),
            Err(SourceEntryRefusal::ScratchSlotBorrowed)
        );
        assert!(view.is_empty());
    });
}

#[test]
fn pentry_e2b_source_entry_backtrace_uses_existing_quiescence_policy() {
    let mut source = source();
    source.specpdl.push(SpecBinding::Backtrace2 {
        function: Value::NIL,
        arg0: Value::NIL,
        arg1: Value::NIL,
    });
    assert_eq!(
        validate_serial_source_entry(&source),
        Err(SourceEntryRefusal::FunctionActivation)
    );
    assert!(matches!(
        source.specpdl.pop(),
        Some(SpecBinding::Backtrace2 { .. })
    ));
    assert_eq!(validate_serial_source_entry(&source), ready_result(&source));
}

#[test]
fn pentry_e2b_source_entry_native_root_phase_fault_uses_existing_policy() {
    let mut source = source();
    source.jit_root_stack_top = 1;
    assert_eq!(
        validate_serial_source_entry(&source),
        Err(SourceEntryRefusal::FunctionActivation)
    );
    assert_eq!(source.jit_root_stack_top, 1);
    source.jit_root_stack_top = 0;
}

#[test]
fn pentry_e2b_source_entry_result_is_not_a_durable_scratch_permission() {
    let source = source();
    assert_eq!(validate_serial_source_entry(&source), ready_result(&source));
    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(Value::fixnum(48));
    assert_eq!(
        validate_serial_source_entry(&source),
        Err(SourceEntryRefusal::ScratchRoots)
    );
    restore_scratch_gc_roots(saved);
}

#[test]
fn pentry_e2b_source_entry_observation_preserves_heap_and_quit_state() {
    let source = source();
    let heap = source.tagged_heap.heap_identity();
    let allocations = source.tagged_heap.total_allocated_bytes();
    let collections = source.tagged_heap.gc_collections();
    source.quit_requested.request();
    assert_eq!(validate_serial_source_entry(&source), ready_result(&source));
    assert_eq!(source.tagged_heap.heap_identity(), heap);
    assert_eq!(source.tagged_heap.total_allocated_bytes(), allocations);
    assert_eq!(source.tagged_heap.gc_collections(), collections);
    assert!(source.quit_requested.take());
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &source.tagged_heap
    ));
}

#[test]
fn pentry_e2b_source_entry_ccl_actual_raw_view_is_preserved() {
    let source = source();
    assert_eq!(source_ccl_scope(), SourceCclScope::Inactive);
    with_ccl_obarray(source.variables(), || {
        assert_eq!(source_ccl_scope(), SourceCclScope::Active);
        assert_eq!(
            validate_serial_source_entry(&source),
            Err(SourceEntryRefusal::CclVariableScope)
        );
        assert_eq!(source_ccl_scope(), SourceCclScope::Active);
    });
    assert_eq!(source_ccl_scope(), SourceCclScope::Inactive);
}

#[test]
fn pentry_e2b_source_entry_ccl_unwind_ends_actual_raw_scope() {
    let source = source();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_ccl_obarray(source.variables(), || {
            assert_eq!(source_ccl_scope(), SourceCclScope::Active);
            panic!("SOURCE_ENTRY_CCL_UNWIND_FAULT");
        });
    }));
    assert!(result.is_err());
    assert_eq!(source_ccl_scope(), SourceCclScope::Inactive);
}
