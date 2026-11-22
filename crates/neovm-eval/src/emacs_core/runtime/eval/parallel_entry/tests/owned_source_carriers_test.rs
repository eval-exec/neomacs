use super::super::source_entry::install_inactive_source_module_probe_for_test;
use super::{
    OwnedSourceObservationClosed, OwnedSourceRegistries, SourceEntryRefusal,
    SourceWithdrawalFailure, SourceWithdrawalStep,
};

fn ready() -> Option<Box<super::super::super::Context>> {
    install_inactive_source_module_probe_for_test();
    let mut source = Box::new(super::super::super::Context::new_minimal_vm_harness(
        crate::test_utils::unit_harness(),
    ));
    source.setup_thread_locals();
    source.gc_collect_exact();
    if source.tagged_heap.tagged_heap_generational_enabled() {
        return Some(source);
    }
    match capture(source) {
        Err(SourceWithdrawalFailure::BeforeEffects(failure)) => {
            assert_eq!(failure.reason, SourceEntryRefusal::GenerationalRequired);
            None
        }
        other => panic!("GEN0 must refuse before any effect: {:?}", other.err()),
    }
}

fn capture(
    source: Box<super::super::super::Context>,
) -> Result<OwnedSourceRegistries, SourceWithdrawalFailure> {
    // SAFETY: fixtures own fresh serial Contexts with no collector, running
    // mutator, store callback, or borrowed heap view.
    unsafe { OwnedSourceRegistries::capture(source) }
}

fn observation_closed() -> Option<(impl Fn() -> bool, OwnedSourceObservationClosed)> {
    let source = ready()?;
    let alive = source.world_lifetime_probe_for_test();
    Some((
        alive,
        capture(source).unwrap().withdraw_observations().unwrap(),
    ))
}

#[test]
fn pentry_e2b_source_carriers_owned_source_preserves_exact_observation_placeholder() {
    let Some((alive, source)) = observation_closed() else {
        return;
    };
    assert!(
        source
            ._frame
            .effects
            ._receipt
            .1
            .placeholder_is_current_for_test()
    );
    let closed = source.withdraw_carriers().unwrap();
    assert!(
        closed
            ._frame
            .effects
            ._previous
            ._receipt
            .1
            .placeholder_is_current_for_test()
    );
    assert!(
        !closed
            ._frame
            .effects
            ._previous
            ._receipt
            .1
            .original_is_current_for_test()
    );
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    assert!(alive());
}

#[test]
fn pentry_e2b_source_carriers_owned_source_drop_never_restores_native_state() {
    let Some((alive, source)) = observation_closed() else {
        return;
    };
    drop(source.withdraw_carriers().unwrap());
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_source_carriers_owned_source_forget_never_restores_native_state() {
    let Some((alive, source)) = observation_closed() else {
        return;
    };
    std::mem::forget(source.withdraw_carriers().unwrap());
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_source_carriers_owned_source_catch_unwind_retains_native_state() {
    let Some((alive, source)) = observation_closed() else {
        return;
    };
    let closed = source.withdraw_carriers().unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _closed = closed;
        panic!("OWNED_SOURCE_WORLD_CARRIERS_UNWIND");
    }));
    assert!(result.is_err());
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_source_carriers_foreign_heap_is_preserved_with_owned_refusal() {
    let Some((alive, source)) = observation_closed() else {
        return;
    };
    let Some(foreign) = ready() else {
        return;
    };
    let error = source
        .withdraw_carriers()
        .err()
        .expect("new ordinary heap is installed");
    assert!(format!("{error:?}").contains("HeapInstalled"));
    match &error {
        SourceWithdrawalFailure::Retained(failure) => {
            assert_eq!(failure.step, SourceWithdrawalStep::WorldCarriers);
        }
        SourceWithdrawalFailure::BeforeEffects(_) => panic!("source already withdrawn"),
    }
    drop(error);
    assert!(alive());
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &foreign.tagged_heap
    ));
}
