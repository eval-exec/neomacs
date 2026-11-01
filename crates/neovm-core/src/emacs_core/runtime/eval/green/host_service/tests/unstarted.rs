//! Actual actor/admission proofs dispose only owner steps that never ran.
use super::super::{
    RecordState,
    owner_steps::{
        OwnerStepError, OwnerStepState,
        tests::{FixtureStepStage, step_fixture},
    },
};
use super::*;
use crate::emacs_core::print::print_value;
use std::sync::Arc;

#[derive(Clone, Copy)]
enum TerminalOrigin {
    CancelBeforeDispatch,
    CloseBeforeDispatch,
    NativeComplete,
}

fn unused_step_finishes_only_after_actual_terminal(
    stage: FixtureStepStage,
    origin: TerminalOrigin,
) {
    let (mut owner, baton, step) = step_fixture(stage);
    let key = step.parent();
    let control = owner.with_green_runtime(|runtime| Arc::clone(&runtime.control));
    let _receiver = control.register_main();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let slice = control
            .reserve_local_service(roots.hosts().registration(key).unwrap())
            .unwrap();
        roots
            .services
            .publish(key, OwnedNativeData::None, slice)
            .ok()
            .unwrap();
        assert!(
            roots.hosts().settle(key).is_err(),
            "an unused frame alone cannot settle its parent"
        );
        match origin {
            TerminalOrigin::CancelBeforeDispatch => roots
                .services
                .cancel(key, CancelReason::TargetQuit)
                .unwrap(),
            TerminalOrigin::CloseBeforeDispatch => roots.services.close().unwrap(),
            TerminalOrigin::NativeComplete => {}
        }
        let settled_and_slice = match roots.services.take_turn(key).unwrap() {
            NativeTurn::AlreadySettled { slice, unstarted } => {
                assert!(matches!(
                    origin,
                    TerminalOrigin::CancelBeforeDispatch | TerminalOrigin::CloseBeforeDispatch
                ));
                let settled = settle_unstarted_host_call(&mut roots, &baton, unstarted);
                (settled, slice)
            }
            NativeTurn::Dispatch {
                slice,
                ticket,
                native,
            } => {
                assert!(matches!(origin, TerminalOrigin::NativeComplete));
                assert_eq!(native, OwnedNativeData::None);
                let settled = roots
                    .services
                    .complete(ticket, NativeOutcome::HostClosed)
                    .unwrap();
                roots
                    .hosts()
                    .finish_native_owner_step(&baton, settled)
                    .unwrap();
                let settled = roots.hosts().settle(key).unwrap();
                (settled, slice)
            }
        };
        let (settled, slice) = settled_and_slice;
        control
            .cancel_current_service(slice, settled, &baton)
            .ok()
            .unwrap();
        let record = &roots.hosts().records[&key];
        assert!(matches!(
            record.steps.state_for_test(),
            OwnerStepState::Idle
        ));
        assert!(
            matches!(record.state, RecordState::TerminalAwaitingOwner),
            "terminal settlement is not owner acknowledgement"
        );
        let terminal = roots.services.take_terminal(key).unwrap();
        match origin {
            TerminalOrigin::CancelBeforeDispatch => assert!(matches!(
                terminal.settlement,
                NativeSettlement::Completed(NativeOutcome::Interrupted(CancelReason::TargetQuit))
            )),
            TerminalOrigin::CloseBeforeDispatch | TerminalOrigin::NativeComplete => {
                assert!(matches!(
                    terminal.settlement,
                    NativeSettlement::Completed(NativeOutcome::HostClosed)
                ))
            }
        }
    });
    owner.gc_collect_exact();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let record = &roots.hosts().records[&key];
        assert!(matches!(record.state, RecordState::TerminalAwaitingOwner));
        let mut retained = Vec::new();
        for lease in &record.roots {
            let reader = lease.reader(&owner.tagged_heap).unwrap();
            reader.append_roots(&mut retained);
            reader.finish().unwrap();
        }
        assert!(
            retained
                .iter()
                .any(|value| print_value(value) == "(\"step-root-only\" 73)")
        );
    });
}

#[test]
fn cancelled_undispatched_initial_steps_keep_roots_until_restored_owner_acknowledges() {
    for stage in [FixtureStepStage::Issued, FixtureStepStage::Prepared] {
        unused_step_finishes_only_after_actual_terminal(
            stage,
            TerminalOrigin::CancelBeforeDispatch,
        );
    }
}

#[test]
fn closed_undispatched_initial_steps_keep_roots_until_restored_owner_acknowledges() {
    for stage in [FixtureStepStage::Issued, FixtureStepStage::Prepared] {
        unused_step_finishes_only_after_actual_terminal(stage, TerminalOrigin::CloseBeforeDispatch);
    }
}

#[test]
fn actual_native_completion_can_dispose_an_initial_step_it_never_requested() {
    for stage in [FixtureStepStage::Issued, FixtureStepStage::Prepared] {
        unused_step_finishes_only_after_actual_terminal(stage, TerminalOrigin::NativeComplete);
    }
}

#[test]
fn unstarted_terminal_cannot_dispose_published_owner_authority() {
    let (owner, baton, step) = step_fixture(FixtureStepStage::Published);
    let key = step.parent();
    let control = owner.with_green_runtime(|runtime| Arc::clone(&runtime.control));
    let _receiver = control.register_main();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let slice = control
            .reserve_local_service(roots.hosts().registration(key).unwrap())
            .unwrap();
        roots
            .services
            .publish(key, OwnedNativeData::None, slice)
            .ok()
            .unwrap();
        roots
            .services
            .cancel(key, CancelReason::TargetQuit)
            .unwrap();
        let NativeTurn::AlreadySettled {
            unstarted,
            slice: _retained,
        } = roots.services.take_turn(key).unwrap()
        else {
            panic!("cancelled actor publication must not dispatch native code")
        };
        assert!(matches!(
            roots.hosts().finish_unstarted_owner_step(&baton, unstarted),
            Err(OwnerStepError::WrongStage)
        ));
        let record = &roots.hosts().records[&key];
        assert!(matches!(
            record.steps.state_for_test(),
            OwnerStepState::AwaitingOwner(_)
        ));
        assert!(matches!(record.state, RecordState::Published(_)));
        assert_eq!(roots.hosts().awaiting_owner_step(key).unwrap(), step);
        assert!(roots.hosts().settle(key).is_err());
    });
}
