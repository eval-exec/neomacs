use super::super::source_entry::install_inactive_source_module_probe_for_test;
use super::{
    HasKeeper, OwnedSourceObservationClosed, OwnedSourceRegistries, RetainedSourceFailure,
    SourceEntryRefusal, SourceWithdrawalFailure, SourceWithdrawalStep,
};
use neovm_value::value::Value;

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

fn expect_observation_failure(
    outcome: Result<OwnedSourceObservationClosed, SourceWithdrawalFailure>,
    expected: &str,
) -> RetainedSourceFailure {
    let error = outcome
        .err()
        .expect("actual observation effect must refuse");
    assert!(format!("{error:?}").contains(expected), "{error:?}");
    match error {
        SourceWithdrawalFailure::Retained(failure) => {
            assert_eq!(failure.step, SourceWithdrawalStep::Observations);
            failure
        }
        SourceWithdrawalFailure::BeforeEffects(_) => {
            panic!("partial source cannot return ordinary access")
        }
    }
}

#[test]
fn pentry_e2b_source_observations_actual_source_census_and_rc_withdrawal() {
    let Some(mut source) = ready() else {
        return;
    };
    let private = Value::vector(vec![Value::fixnum(79)]);
    source.lexenv = private;
    let heap = source.tagged_heap.heap_identity();
    let alive = source.world_lifetime_probe_for_test();
    let closed = capture(source).unwrap().withdraw_observations().unwrap();
    assert_eq!(closed._frame.effects._receipt.0.heap_identity(), heap);
    assert!(closed._frame.effects._receipt.0.contains_for_test(private));
    assert!(
        !closed
            ._frame
            .effects
            ._receipt
            .1
            .original_is_current_for_test()
    );
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    assert!(closed._frame.effects.keeper().held_here());
    assert!(alive());
}

#[test]
fn pentry_e2b_source_observations_live_certificate_owns_failure_and_census() {
    let Some(source) = ready() else {
        return;
    };
    let alive = source.world_lifetime_probe_for_test();
    let registry = source.ccl_registry.downgrade();
    let captured = capture(source).unwrap();
    let (_, certificate) = neovm_value::tagged::collection_reads::capture(|| ());
    let certificate = certificate.expect("actual canonical observation certificate");
    let failure = expect_observation_failure(captured.withdraw_observations(), "LiveCertificates");
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    drop(certificate);
    drop(failure);
    assert!(alive() && registry.upgrade().is_some());
    assert!(
        !neovm_value::tagged::gc::tagged_heap_is_installed(),
        "no drop-triggered recovery"
    );
}

#[test]
fn pentry_e2b_source_observations_forgotten_certificate_never_grants_overlap() {
    let Some(source) = ready() else {
        return;
    };
    let alive = source.world_lifetime_probe_for_test();
    let captured = capture(source).unwrap();
    let (_, certificate) = neovm_value::tagged::collection_reads::capture(|| ());
    std::mem::forget(certificate.expect("actual completed certificate"));
    let failure = expect_observation_failure(captured.withdraw_observations(), "LiveCertificates");
    std::mem::forget(failure);
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_source_observations_active_capture_refuses_actual_owner() {
    let Some(source) = ready() else {
        return;
    };
    let alive = source.world_lifetime_probe_for_test();
    let captured = capture(source).unwrap();
    let (failure, certificate) = neovm_value::tagged::collection_reads::capture(|| {
        expect_observation_failure(captured.withdraw_observations(), "ActiveCaptures")
    });
    drop(certificate);
    drop(failure);
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

fn closed() -> Option<(impl Fn() -> bool, OwnedSourceObservationClosed)> {
    let source = ready()?;
    let alive = source.world_lifetime_probe_for_test();
    Some((
        alive,
        capture(source).unwrap().withdraw_observations().unwrap(),
    ))
}

#[test]
fn pentry_e2b_source_observations_drop_retains_source_without_restoration() {
    let Some((alive, closed)) = closed() else {
        return;
    };
    drop(closed);
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_source_observations_forget_retains_source_without_restoration() {
    let Some((alive, closed)) = closed() else {
        return;
    };
    std::mem::forget(closed);
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_source_observations_catch_unwind_retains_source_without_restoration() {
    let Some((alive, closed)) = closed() else {
        return;
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _closed = closed;
        panic!("OWNED_SOURCE_OBSERVATIONS_UNWIND");
    }));
    assert!(result.is_err());
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_source_observations_foreign_heap_refuses_before_withdrawal() {
    let Some(source) = ready() else {
        return;
    };
    let alive = source.world_lifetime_probe_for_test();
    let captured = capture(source).unwrap();
    let Some(foreign) = ready() else {
        return;
    };
    let error = captured
        .withdraw_observations()
        .err()
        .expect("fresh source check");
    assert!(format!("{error:?}").contains("HeapTlsNotCurrent"));
    match &error {
        SourceWithdrawalFailure::Retained(failure) => {
            assert_eq!(failure.step, SourceWithdrawalStep::TextEntry);
        }
        SourceWithdrawalFailure::BeforeEffects(_) => panic!("source already armed"),
    }
    drop(error);
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &foreign.tagged_heap
    ));
    assert!(alive());
}
