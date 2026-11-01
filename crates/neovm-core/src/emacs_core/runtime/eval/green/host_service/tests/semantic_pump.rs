//! Driver-boundary controls; full owner/root histories live in runtime tests.
use super::*;
use crate::host_effect::owner_step::{OwnerInvocationId, OwnerStepKey, OwnerStepSequence};
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicUsize, Ordering};

fn route() -> NativeCompletionRoute {
    let wake = ControlWake::paired().0;
    NativeCompletionRoute::new(wake.handle(), wake.handle())
}

#[test]
fn physical_completion_resumes_semantics_before_owner_step_or_terminal() {
    let key = HostRequestKey::for_test(NonZeroU64::MIN);
    let step = OwnerStepKey::new(
        key,
        ThreadKey::Main,
        OwnerInvocationId::from_owner_id(NonZeroU64::MIN),
        OwnerStepSequence::new().issue().unwrap(),
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let mut backend = NativeBackend::Semantic(Box::new(move |event, _route| match event {
        FixtureSemanticEvent::Native {
            key: actual,
            outcome: NativeOutcome::HostClosed,
        } => {
            assert_eq!(actual, key);
            observed.fetch_add(1, Ordering::SeqCst);
            NativeAdapterStep::RequiresOwner(step)
        }
        FixtureSemanticEvent::Start { .. }
        | FixtureSemanticEvent::Owner { .. }
        | FixtureSemanticEvent::Native { .. } => panic!("wrong semantic continuation event"),
    }));
    let result = backend.native_complete(
        key,
        NativeOutcome::HostClosed,
        SemanticContinuation::Semantic,
        route(),
    );
    assert!(matches!(result, NativeAdapterStep::RequiresOwner(actual) if actual == step));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "physical completion must resume exactly one saved driver"
    );
}

#[test]
fn close_retains_semantic_backend_until_its_owned_cleanup_can_run() {
    struct Backing(Arc<AtomicUsize>);
    impl Drop for Backing {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let backing = Backing(Arc::clone(&drops));
    let mut pump = MainNativePump::default();
    pump.install_semantic_fixture_driver(Box::new(move |_event, _route| {
        let _retained = &backing;
        NativeAdapterStep::Complete(NativeOutcome::HostClosed)
    }));
    pump.close();
    assert_eq!(
        drops.load(Ordering::SeqCst),
        0,
        "close cannot destroy an affine cleanup backend"
    );
    drop(pump);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[derive(Clone, Copy)]
enum InterruptionOrigin {
    NativePumpClose,
    ActorClose,
    ActorCancellation,
    ActorCancellationThenPumpClose,
}

impl InterruptionOrigin {
    fn reason(self) -> super::super::transport::CancelReason {
        use super::super::transport::CancelReason;
        match self {
            Self::NativePumpClose | Self::ActorClose => CancelReason::SessionClosing,
            Self::ActorCancellation | Self::ActorCancellationThenPumpClose => {
                CancelReason::TargetSignal
            }
        }
    }
}

fn interruption_keeps_claimed_owner_roots(origin: InterruptionOrigin) {
    use super::super::{HostRootError, RecordState, RestoredHostOwner, SessionState};
    use crate::emacs_core::eval::{Context, ThreadingMode};
    use crate::emacs_core::print::print_value;
    use crate::emacs_core::value::Value;
    use crate::host_effect::contracts::{
        CallbackStepArguments, CallbackStepResult, OwnerExecutionFrameRecord,
        OwnerRestoreFrameRecord,
    };
    use crate::host_effect::owner_step::{OwnerStepResult, PendingOwnerStep};

    let mut owner = Context::new_with_threading(ThreadingMode::Green);
    let baton = owner.start_green_runtime().unwrap();
    let (key, claim, access) = owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let prepared = baton.prepare_parked_batch(&owner.tagged_heap, 0).unwrap();
        // SAFETY: the empty original-owner root slice is published while its
        // heap's Baton is held, without a collection or ownership handoff.
        let parked = unsafe { prepared.finalize(&[]) }
            .unwrap()
            .publish()
            .unwrap();
        roots.insert(ThreadKey::Main, parked);
        let registry = roots.hosts();
        let input = Value::list(vec![Value::string("closing-owner-root")]);
        // SAFETY: all values remain live in this original-owner extent under
        // the held Baton; registration invokes neither Lisp nor collection.
        let (call, arguments, execution, restore) = unsafe {
            let call = registry
                .prepare(&baton, &owner.tagged_heap, ThreadKey::Main, &[input])
                .unwrap();
            let arguments = registry
                .register::<CallbackStepArguments>(
                    &baton,
                    &owner.tagged_heap,
                    ThreadKey::Main,
                    call.key(),
                    &[input],
                )
                .unwrap();
            let execution = registry
                .register::<OwnerExecutionFrameRecord>(
                    &baton,
                    &owner.tagged_heap,
                    ThreadKey::Main,
                    call.key(),
                    &[input],
                )
                .unwrap();
            let restore = registry
                .register::<OwnerRestoreFrameRecord>(
                    &baton,
                    &owner.tagged_heap,
                    ThreadKey::Main,
                    call.key(),
                    &[Value::NIL],
                )
                .unwrap();
            (call, arguments, execution, restore)
        };
        let key = registry
            .issue_owner_step(
                &baton,
                &owner.tagged_heap,
                call.key(),
                ThreadKey::Main,
                OwnerInvocationId::from_owner_id(NonZeroU64::MIN),
            )
            .unwrap();
        registry
            .prepare_owner_step(
                &baton,
                &owner.tagged_heap,
                PendingOwnerStep::callback(key, arguments, execution, restore).unwrap(),
            )
            .unwrap();
        roots.publish_host(call).unwrap();
        let registry = roots.hosts();
        registry.publish_owner_step(&baton, key).unwrap();
        let restored = RestoredHostOwner {
            context: &owner,
            baton: &baton,
            owner: ThreadKey::Main,
        };
        let (claim, access) = registry.claim_owner_step(&restored, key).unwrap();
        roots
            .services
            .install_semantic_fixture_driver(Box::new(|_, _| {
                NativeAdapterStep::Complete(NativeOutcome::HostClosed)
            }));
        (key, claim, access)
    });
    let (ticket, waiter) = owner.with_green_runtime(|runtime| {
        runtime
            .parked
            .borrow_mut()
            .services
            .native_dispatch_for_test(key.parent())
    });
    let mut pump = MainNativePump::default();
    match origin {
        InterruptionOrigin::NativePumpClose => {
            pump.close();
            pump.close_admission(&baton);
            pump.observe_controls(&baton);
        }
        InterruptionOrigin::ActorClose => {
            owner.with_green_runtime(|runtime| {
                runtime.parked.borrow_mut().services.close().unwrap()
            });
            pump.observe_controls(&baton);
        }
        InterruptionOrigin::ActorCancellationThenPumpClose => {
            owner.with_green_runtime(|runtime| {
                runtime
                    .parked
                    .borrow_mut()
                    .services
                    .cancel(
                        key.parent(),
                        super::super::transport::CancelReason::TargetSignal,
                    )
                    .unwrap();
            });
            pump.close();
            pump.close_admission(&baton);
            pump.observe_controls(&baton);
        }
        InterruptionOrigin::ActorCancellation => {
            use super::super::transport::CancelReason;
            owner.with_green_runtime(|runtime| {
                let mut roots = runtime.parked.borrow_mut();
                roots
                    .services
                    .cancel(key.parent(), CancelReason::TargetSignal)
                    .unwrap();
                roots
                    .services
                    .cancel(key.parent(), CancelReason::TargetQuit)
                    .unwrap();
            });
            pump.observe_controls(&baton);
        }
    }
    owner.gc_collect_exact();
    let ready = owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        match origin {
            InterruptionOrigin::NativePumpClose
            | InterruptionOrigin::ActorClose
            | InterruptionOrigin::ActorCancellationThenPumpClose => {
                assert!(matches!(registry.session, SessionState::Closing));
            }
            InterruptionOrigin::ActorCancellation => {
                assert!(matches!(registry.session, SessionState::Open(_)));
            }
        }
        assert_eq!(
            registry.owner_interruption(&baton, key).unwrap(),
            OwnerInterruption::Requested(origin.reason())
        );
        assert!(matches!(
            registry.records[&key.parent()].state,
            RecordState::Published(_)
        ));
        if let InterruptionOrigin::NativePumpClose
        | InterruptionOrigin::ActorClose
        | InterruptionOrigin::ActorCancellationThenPumpClose = origin
        {
            // SAFETY: empty root slices remain valid throughout these no-callback
            // calls. Closing must refuse ordinary admission and registration.
            unsafe {
                assert!(matches!(
                    registry.prepare(&baton, &owner.tagged_heap, ThreadKey::Main, &[]),
                    Err(HostRootError::Closed)
                ));
                assert!(matches!(
                    registry.register::<CallbackStepResult>(
                        &baton,
                        &owner.tagged_heap,
                        ThreadKey::Main,
                        key.parent(),
                        &[]
                    ),
                    Err(HostRootError::Closed)
                ));
            }
            assert!(
                registry
                    .issue_owner_step(
                        &baton,
                        &owner.tagged_heap,
                        key.parent(),
                        ThreadKey::Main,
                        key.invocation()
                    )
                    .is_err()
            );
        }
        let mut retained = Vec::new();
        for lease in &registry.records[&key.parent()].roots {
            let reader = lease.reader(&owner.tagged_heap).unwrap();
            reader.append_roots(&mut retained);
            reader.finish().unwrap();
        }
        assert!(
            retained
                .iter()
                .any(|value| print_value(value) == "(\"closing-owner-root\")"),
            "closing must preserve the rooted graph through collection"
        );
        // SAFETY: this admitted original-owner claim remains live under the
        // held Baton; registering its result performs no callback or collection.
        let result = unsafe {
            registry.register_continuation::<CallbackStepResult>(
                &baton,
                &owner.tagged_heap,
                &access,
                &[Value::T],
            )
        }
        .unwrap();
        registry
            .complete_owner_step(
                &baton,
                &owner.tagged_heap,
                claim.complete(OwnerStepResult::Callback(result)).unwrap(),
            )
            .unwrap();
        let ready = registry.take_owner_completion(&baton, key).unwrap();
        assert!(
            matches!(
                registry.records[&key.parent()].state,
                RecordState::Published(_)
            ),
            "interruption never forges a terminal acknowledgement"
        );
        ready
    });
    // Resume the actual semantic boundary from the claimed completion. No
    // cancellation hint is treated as a ready native result or a Lisp signal.
    let expected = origin.reason();
    let mut backend = NativeBackend::Semantic(Box::new(move |event, _route| match event {
        FixtureSemanticEvent::Owner {
            key: parent,
            completion,
            next,
            interruption,
        } => {
            assert_eq!(parent, key.parent());
            assert_eq!(completion.key(), key);
            assert_eq!(next, OwnerNextStep::Absent);
            assert_eq!(interruption, OwnerInterruption::Requested(expected));
            NativeAdapterStep::Complete(NativeOutcome::HostClosed)
        }
        FixtureSemanticEvent::Start { .. } | FixtureSemanticEvent::Native { .. } => {
            panic!("wrong semantic resume")
        }
    }));
    let NativeAdapterStep::Complete(outcome) = backend.owner_complete(key.parent(), ready, route())
    else {
        panic!("fixture semantic cleanup did not finish");
    };
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let settled = roots.services.complete_native_dispatch_for_test(ticket, outcome);
        roots.hosts().finish_native_owner_step(&baton, settled).unwrap();
        assert!(matches!(roots.hosts().records[&key.parent()].state, RecordState::Published(_)));
        let terminal = roots.services.take_terminal(key.parent()).unwrap();
        assert!(matches!(terminal.settlement, super::super::transport::NativeSettlement::CompletedAfterCancellation { outcome: NativeOutcome::HostClosed, cancellation } if cancellation == expected));
        assert!(matches!(roots.hosts().records[&key.parent()].state, RecordState::Published(_)), "native settlement still cannot acknowledge the requester");
    });
    waiter.finish();
}

#[test]
fn pump_close_stops_admission_but_keeps_claimed_owner_roots_and_completion() {
    interruption_keeps_claimed_owner_roots(InterruptionOrigin::NativePumpClose);
}

#[test]
fn actor_close_stops_admission_but_keeps_claimed_owner_roots_and_completion() {
    interruption_keeps_claimed_owner_roots(InterruptionOrigin::ActorClose);
}

#[test]
fn actor_cancellation_survives_claimed_owner_resume_until_native_cleanup() {
    interruption_keeps_claimed_owner_roots(InterruptionOrigin::ActorCancellation);
}

#[test]
fn pump_close_cannot_overtake_an_earlier_actor_cancellation_intent() {
    interruption_keeps_claimed_owner_roots(InterruptionOrigin::ActorCancellationThenPumpClose);
}

#[test]
fn late_actor_observation_cannot_reopen_a_native_settled_parent() {
    use super::super::{
        RecordState,
        transport::{CancelReason, NativeSettlement},
    };
    use crate::emacs_core::eval::{Context, ThreadingMode};
    let mut owner = Context::new_with_threading(ThreadingMode::Green);
    let baton = owner.start_green_runtime().unwrap();
    let key = owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let prepared = baton.prepare_parked_batch(&owner.tagged_heap, 0).unwrap();
        // SAFETY: the empty original-owner root slice is finalized/published
        // under its held heap Baton, without collection or handoff.
        let parked = unsafe { prepared.finalize(&[]) }
            .unwrap()
            .publish()
            .unwrap();
        roots.insert(ThreadKey::Main, parked);
        // SAFETY: the empty host payload is live for the whole no-callback
        // publication extent and uses this same owner/heap's held Baton.
        let call = unsafe {
            roots
                .hosts()
                .prepare(&baton, &owner.tagged_heap, ThreadKey::Main, &[])
        }
        .unwrap();
        roots.publish_host(call).unwrap()
    });
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let (ticket, waiter) = roots.services.native_dispatch_for_test(key);
        roots
            .services
            .cancel(key, CancelReason::TargetQuit)
            .unwrap();
        // Completing drains the FIFO command, but leaves its observation for
        // main's next pump turn. The result remains actual native settlement.
        let settled = roots
            .services
            .complete_native_dispatch_for_test(ticket, NativeOutcome::HostClosed);
        roots
            .hosts()
            .finish_native_owner_step(&baton, settled)
            .unwrap();
        let settled_parent = roots.hosts().settle(key).unwrap();
        assert_eq!(settled_parent.key(), key);
        waiter.finish();
    });
    let mut pump = MainNativePump::default();
    pump.observe_controls(&baton);
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        assert!(matches!(
            roots.hosts().records[&key].state,
            RecordState::TerminalAwaitingOwner
        ));
        let terminal = roots.services.take_terminal(key).unwrap();
        assert!(matches!(
            terminal.settlement,
            NativeSettlement::CompletedAfterCancellation {
                outcome: NativeOutcome::HostClosed,
                cancellation: CancelReason::TargetQuit
            }
        ));
        assert!(
            matches!(
                roots.hosts().records[&key].state,
                RecordState::TerminalAwaitingOwner
            ),
            "a late observation cannot acknowledge the requester"
        );
    });
}

#[derive(Clone, Copy)]
enum CleanupHistory {
    Immediate,
    PendingAcrossClose,
}

fn ended_owner_native_cleanup(history: CleanupHistory) {
    use super::super::{RecordState, owner_steps::OwnerStepState};
    use crate::emacs_core::print::print_value;
    use crate::host_effect::native_pending::{self, NativePublication};
    use std::sync::Mutex;

    let (mut owner, baton, step) = super::super::owner_steps::tests::pending_fixture();
    let key = step.parent();
    let control = owner.with_green_runtime(|runtime| Arc::clone(&runtime.control));
    let _main_receiver = control.register_main();
    let (slice, ticket, waiter) = owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let slice = control
            .reserve_local_service(roots.hosts().registration(key).unwrap())
            .unwrap();
        let (ticket, waiter) = roots.services.native_dispatch_for_test(key);
        (slice, ticket, waiter)
    });
    // This fixture publishes an actual explicit exit; it does not derive
    // cleanup authority from an absent or live owner's wait record.
    let ended = Control::ended_service_owner_for_test(key, ThreadKey::Main, &baton);
    drop(ended);
    owner.with_green_runtime(|runtime| {
        runtime
            .parked
            .borrow_mut()
            .services
            .save_owner_continuation(OwnerContinuation {
                step,
                slice,
                ticket,
            })
            .ok()
            .unwrap();
    });
    let native_calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&native_calls);
    let completion = Arc::new(Mutex::new(None));
    let producer = Arc::clone(&completion);
    let host_os = std::thread::current().id();
    let mut pump = MainNativePump::default();
    pump.install_cleanup_fixture_driver(
        Box::new(|_, _| panic!("dead-owner cleanup must never reenter requester semantics")),
        Box::new(move |event, route| {
            assert_eq!(std::thread::current().id(), host_os);
            observed.fetch_add(1, Ordering::SeqCst);
            match event {
                FixtureCleanupEvent::OwnerEnded {
                    key: actual,
                    step: actual_step,
                } => {
                    assert_eq!(actual, key);
                    assert_eq!(actual_step, step);
                    match history {
                        CleanupHistory::Immediate => {
                            NativeAdapterStep::Complete(NativeOutcome::HostClosed)
                        }
                        CleanupHistory::PendingAcrossClose => {
                            let (operation, publisher) =
                                native_pending::pair(key, ReadDeadline::Unbounded, route);
                            assert!(producer.lock().unwrap().replace(publisher).is_none());
                            NativeAdapterStep::Pending(operation)
                        }
                    }
                }
                FixtureCleanupEvent::Native {
                    key: actual,
                    outcome,
                } => {
                    assert!(matches!(history, CleanupHistory::PendingAcrossClose));
                    assert_eq!(actual, key);
                    assert_eq!(outcome, NativeOutcome::HostClosed);
                    NativeAdapterStep::Complete(outcome)
                }
            }
        }),
    );
    pump.owner_attempts
        .insert(key, SemanticContinuation::Semantic);
    let (saved, ended) = pump.take_ended_owner_step(&baton, &control).unwrap();
    assert!(
        pump.take_ended_owner_step(&baton, &control).is_none(),
        "a linear continuation is taken exactly once"
    );
    owner.gc_collect_exact();
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        assert!(matches!(
            registry.records[&key].state,
            RecordState::Published(_)
        ));
        assert!(matches!(
            registry.records[&key].steps.state_for_test(),
            OwnerStepState::Revoked(_)
        ));
        assert!(matches!(
            registry.awaiting_owner_step(key),
            Err(OwnerStepError::Revoked)
        ));
        let mut retained = Vec::new();
        for batch in &registry.records[&key].roots {
            let reader = batch.reader(&owner.tagged_heap).unwrap();
            reader.append_roots(&mut retained);
            reader.finish().unwrap();
        }
        assert!(
            retained
                .iter()
                .any(|value| print_value(value) == "(\"step-root-only\" 73)")
        );
    });
    let cleanup_route = route();
    let cleanup = pump.backend.owner_ended(step, cleanup_route);
    let mut baton = pump.advance_ended(baton, saved.slice, saved.ticket, ended, cleanup, &control);
    match history {
        CleanupHistory::Immediate => assert_eq!(native_calls.load(Ordering::SeqCst), 1),
        CleanupHistory::PendingAcrossClose => {
            assert_eq!(pump.drain_progress(), MainNativeDrain::Pending);
            assert!(matches!(
                waiter.observe(),
                super::super::transport::WaitObservation::Pending
            ));
            pump.close();
            pump.close_admission(&baton);
            pump.observe_controls(&baton);
            owner.gc_collect_exact();
            owner.with_green_runtime(|runtime| {
                let mut roots = runtime.parked.borrow_mut();
                assert!(matches!(
                    roots.hosts().records[&key].state,
                    RecordState::Published(_)
                ));
                assert!(matches!(
                    roots.hosts().records[&key].steps.state_for_test(),
                    OwnerStepState::Revoked(_)
                ));
            });
            assert!(matches!(
                completion
                    .lock()
                    .unwrap()
                    .take()
                    .unwrap()
                    .complete(NativeOutcome::HostClosed),
                NativePublication::Committed(_)
            ));
            let mut ready = pump.pending.observe_ready();
            assert_eq!(ready.len(), 1);
            let (actual, outcome, saved) = ready.pop().unwrap().into_parts();
            assert_eq!(actual, key);
            let PendingContinuation::Ended(ended) = saved.continuation else {
                panic!("cleanup may not become a requester return")
            };
            let NativeReadyOutcome::Completed(outcome) = outcome else {
                panic!("the real producer must settle")
            };
            let cleanup = pump.backend.cleanup_complete(key, outcome, route());
            baton = pump.advance_ended(baton, saved.slice, saved.ticket, ended, cleanup, &control);
            assert_eq!(native_calls.load(Ordering::SeqCst), 2);
        }
    }
    assert!(baton.held_here());
    assert_eq!(pump.drain_progress(), MainNativeDrain::Drained);
    assert!(matches!(
        waiter.observe(),
        super::super::transport::WaitObservation::Notified(_)
    ));
    owner.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        assert!(
            matches!(
                registry.records[&key].state,
                RecordState::TerminalAwaitingOwner
            ),
            "native settlement is not an owner acknowledgement"
        );
        assert!(matches!(
            registry.records[&key].steps.state_for_test(),
            OwnerStepState::Idle
        ));
        assert!(
            !registry.records[&key].roots.is_empty(),
            "quiescent collection owns root retirement"
        );
    });
    // Test teardown alone drops this already-notified waiter; it cannot grant
    // an owner acknowledgement or retire any registered RootBatch.
    waiter.finish();
}

#[test]
fn ended_owner_cleanup_settles_without_requester_return_or_root_retirement() {
    ended_owner_native_cleanup(CleanupHistory::Immediate);
}

#[test]
fn ended_owner_cleanup_preserves_native_pending_route_and_roots_across_close() {
    ended_owner_native_cleanup(CleanupHistory::PendingAcrossClose);
}
