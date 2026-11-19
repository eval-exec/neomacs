use super::super::super::super::Baton;
use super::super::super::factory::{AdmissionFailure, AdmissionPlan, ProductionPlan};
use super::super::super::{OwnedParallelError, is_empty_weak_value_eql};
use super::super::PreparedAdmission;
use super::super::SerialRecovery;
use super::{
    ClosedPreparationFailure, ClosedSource, PreparationPolicy, PreparationResources, RecoverSerial,
};
use crate::emacs_core::eval::Context;
use neovm_value::tagged::value::TaggedValue;
use std::mem::ManuallyDrop;
use std::sync::Arc;

fn source() -> Context {
    let mut source = Context::new_minimal_vm_harness(crate::test_utils::unit_runtime());
    source.setup_thread_locals();
    source.set_variable("window-dead-windows-table", TaggedValue::NIL);
    source.gc_collect_exact();
    source
}

#[test]
fn pentry_e2b_closed_preparation_preserves_weak_table_without_reclamation_gc() {
    let mut source = source();
    source.install_empty_dead_windows_table();
    source.gc_collect_exact();
    let table = source
        .builtin_var_value_by_name("window-dead-windows-table")
        .unwrap();
    let collections = source.tagged_heap.gc_collections();
    let (reason, recovery) = {
        let PreparedAdmission {
            collector,
            keeper,
            frozen,
            recovery,
        } = source
            .prepare_closed_source()
            .unwrap_or_else(|_| panic!("closed preparation"));
        assert!(matches!(recovery, SerialRecovery::Unchanged));
        let failure = match ProductionPlan.admit(collector, keeper, frozen) {
            Ok(_) => panic!("unsupported weak table must refuse production admission"),
            Err(failure) => failure,
        };
        let AdmissionFailure {
            reason,
            collector,
            keeper,
        } = failure;
        drop(collector);
        drop(keeper);
        (reason, recovery)
    };
    source.recover_serial_context(recovery);
    if source.tagged_heap.tagged_heap_generational_enabled() {
        assert_eq!(reason, OwnedParallelError::Publication);
    } else {
        assert_eq!(reason, OwnedParallelError::GenerationalRequired);
    }
    assert_eq!(
        source.builtin_var_value_by_name("window-dead-windows-table"),
        Some(table)
    );
    assert_eq!(source.tagged_heap.gc_collections(), collections);
}

#[test]
fn pentry_e2b_closed_preparation_empty_context_success_ends_before_recovery() {
    let mut source = source();
    let identity = source.tagged_heap.heap_identity();
    let PreparedAdmission {
        collector,
        keeper,
        frozen,
        recovery,
    } = source
        .prepare_closed_source()
        .unwrap_or_else(|_| panic!("closed preparation"));
    assert_eq!(collector.heap_identity(), identity);
    assert_eq!(frozen.heap_identity(), identity);
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    drop(frozen);
    drop(collector);
    drop(keeper);
    source.recover_serial_context(recovery);
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &source.tagged_heap
    ));
}

#[test]
fn pentry_e2b_closed_preparation_actual_census_failure_retains_keeper() {
    let mut source = source();
    let world = Arc::clone(&source.obarray.lease.0);
    let before = Arc::strong_count(&world);
    let loan = world.green.borrow_mut();
    let mut failure = match source.prepare_closed_source() {
        Ok(_) => panic!("actual Green slot loan must refuse census"),
        Err(failure) => failure,
    };
    assert_eq!(failure.reason, OwnedParallelError::Busy);
    assert!(
        matches!(&*failure._resources, PreparationResources::Keeper(keeper) if keeper.held_here())
    );
    assert_eq!(Arc::strong_count(&world), before + 1);
    // SAFETY: test-only cleanup. This fixture has never parked, exported,
    // detached, or transferred anything. Production has no extraction method.
    let resources = unsafe { ManuallyDrop::take(&mut failure._resources) };
    drop(failure);
    drop(resources);
    drop(loan);
    assert_eq!(Arc::strong_count(&world), before);
}

#[test]
fn pentry_e2b_closed_preparation_failure_before_keeper_never_installs_tls() {
    let mut source = source();
    neovm_value::tagged::gc::clear_tagged_heap_if_installed(&source.tagged_heap);
    let failure = ClosedSource::fail(
        &mut source,
        SerialRecovery::Unchanged,
        PreparationResources::<Baton>::BeforeKeeper,
        OwnedParallelError::Poisoned,
    );
    assert_eq!(failure.reason, OwnedParallelError::Poisoned);
    drop(failure);
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    source.setup_thread_locals();
}

#[test]
fn pentry_e2b_recovering_preparation_failure_still_installs_tls() {
    let mut source = source();
    neovm_value::tagged::gc::clear_tagged_heap_if_installed(&source.tagged_heap);
    let reason = RecoverSerial::fail(
        &mut source,
        SerialRecovery::Unchanged,
        PreparationResources::<Baton>::BeforeKeeper,
        OwnedParallelError::Poisoned,
    );
    assert_eq!(reason, OwnedParallelError::Poisoned);
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &source.tagged_heap
    ));
}

#[test]
fn pentry_e2b_recovering_preparation_still_reclaims_empty_weak_table() {
    let mut source = source();
    source.install_empty_dead_windows_table();
    let before = source.tagged_heap.gc_collections();
    let PreparedAdmission {
        collector,
        keeper,
        frozen,
        recovery,
    } = source
        .prepare_owned_admission()
        .expect("ordinary serial preparation");
    assert!(matches!(recovery, SerialRecovery::RecreateDeadWindows));
    drop(frozen);
    drop(collector);
    drop(keeper);
    source.recover_serial_context(recovery);
    assert!(source.tagged_heap.gc_collections() > before);
    assert!(is_empty_weak_value_eql(
        source
            .builtin_var_value_by_name("window-dead-windows-table")
            .unwrap()
    ));
}

fn abandoned_failure(action: impl FnOnce(ClosedPreparationFailure<'_>)) {
    let mut source = source();
    let world = Arc::clone(&source.obarray.lease.0);
    let keeper = source.issue_green_baton().expect("actual keeper");
    let frozen = source
        .capture_parallel_legacy_roots(&keeper)
        .expect("actual census");
    let count = Arc::strong_count(&world);
    neovm_value::tagged::gc::clear_tagged_heap_if_installed(&source.tagged_heap);
    let failure = ClosedSource::fail(
        &mut source,
        SerialRecovery::Unchanged,
        PreparationResources::Census { frozen, keeper },
        OwnedParallelError::Start,
    );
    assert!(matches!(
        &*failure._resources,
        PreparationResources::Census { .. }
    ));
    action(failure);
    assert_eq!(
        Arc::strong_count(&world),
        count,
        "failed resources must remain retained"
    );
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_closed_preparation_drop_retains_resources_without_recovery() {
    abandoned_failure(|failure| drop(failure));
}

#[test]
fn pentry_e2b_closed_preparation_forget_retains_resources_without_recovery() {
    abandoned_failure(|failure| std::mem::forget(failure));
}

#[test]
fn pentry_e2b_closed_preparation_unwind_retains_resources_without_recovery() {
    abandoned_failure(|failure| {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _failure = failure;
            panic!("CLOSED_PREPARATION_FAILURE_UNWIND");
        }));
        assert!(result.is_err());
    });
}
