//! Actual requester/native hops, using the production park and restore path.
use super::owner_steps::{
    ContinuationAccess, OwnerInterruption, OwnerNextStep, OwnerStepError, StepKey,
};
use super::pump::FixtureSemanticEvent;
use super::*;
use crate::emacs_core::error::{EvalResult, LispCondition, signal};
use crate::emacs_core::eval::green::switch::HostOwnerTurn;
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::intern::intern;
use crate::emacs_core::print::print_value;
use crate::emacs_core::subr::SubrSpec;
use crate::emacs_core::value::Value;
use crate::host_effect::contracts::{
    CallbackStepArguments, CallbackStepResult, OwnedNativeData, OwnerExecutionFrameRecord,
    OwnerRestoreFrameRecord, ReadDeadline,
};
use crate::host_effect::native_pending::{self, NativeAdapterStep, NativePublication};
use crate::host_effect::owner_step::{OwnerInvocationId, OwnerStepResult, PendingOwnerStep};
use std::sync::{Arc, Mutex};

fn error(error: impl std::fmt::Display) -> crate::emacs_core::error::Flow {
    signal(LispCondition::Error, vec![Value::string(error.to_string())])
}

/// An original-owner test helper; no registry borrow survives the operation.
fn with_hosts<R>(
    context: &mut Context,
    operation: impl FnOnce(&mut HostRegistry, &Baton, &TaggedHeap, ThreadKey) -> R,
) -> R {
    let owner = context.green_key().unwrap();
    let baton = context
        .green_seat()
        .unwrap()
        .take_owner_step_baton()
        .unwrap();
    let result = context.with_green_runtime(|runtime| {
        operation(
            runtime.parked.borrow_mut().hosts(),
            &baton,
            &context.tagged_heap,
            owner,
        )
    });
    context
        .green_seat()
        .unwrap()
        .restore_owner_step_baton(baton);
    result
}

fn first_step(context: &mut Context, parent: HostRequestKey, graph: Value) -> StepKey {
    with_hosts(context, |registry, baton, heap, owner| {
        // SAFETY: the original owner keeps these live values under its held
        // Baton, with no GC, callback or handoff throughout registration.
        let (arguments, frame, restore) = unsafe {
            (
                registry.register::<CallbackStepArguments>(baton, heap, owner, parent, &[graph]),
                registry.register::<OwnerExecutionFrameRecord>(
                    baton,
                    heap,
                    owner,
                    parent,
                    &[Value::fixnum(17), graph],
                ),
                registry.register::<OwnerRestoreFrameRecord>(
                    baton,
                    heap,
                    owner,
                    parent,
                    &[Value::T, graph],
                ),
            )
        };
        let key = registry
            .issue_owner_step(
                baton,
                heap,
                parent,
                owner,
                OwnerInvocationId::from_owner_id(NonZeroU64::MIN),
            )
            .unwrap();
        let pending =
            PendingOwnerStep::callback(key, arguments.unwrap(), frame.unwrap(), restore.unwrap())
                .unwrap();
        registry.prepare_owner_step(baton, heap, pending).unwrap();
        key
    })
}

fn successor(
    context: &mut Context,
    access: &ContinuationAccess,
    graph: Value,
) -> Result<StepKey, OwnerStepError> {
    with_hosts(context, |registry, baton, heap, _owner| {
        let key = registry.issue_successor(baton, heap, access)?;
        // SAFETY: the active original-owner frame roots graph throughout
        // this no-GC, no-callback, no-handoff continuation registration.
        let (arguments, frame, restore) = unsafe {
            (
                registry.register_continuation::<CallbackStepArguments>(
                    baton,
                    heap,
                    access,
                    &[graph],
                )?,
                registry.register_continuation::<OwnerExecutionFrameRecord>(
                    baton,
                    heap,
                    access,
                    &[Value::fixnum(29), graph],
                )?,
                registry.register_continuation::<OwnerRestoreFrameRecord>(
                    baton,
                    heap,
                    access,
                    &[Value::NIL, graph],
                )?,
            )
        };
        let pending = PendingOwnerStep::callback(key, arguments, frame, restore).unwrap();
        registry
            .prepare_successor(baton, heap, access, pending)
            .unwrap();
        Ok(key)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeStage {
    Start,
    FirstOwner,
    SecondOwner,
    NativeReply,
    Finished,
}

#[derive(Clone, Copy)]
enum RequestHistory {
    Uninterrupted,
    CancelAtFirstOwnerReturn,
}

impl RequestHistory {
    fn interruption(self) -> OwnerInterruption {
        match self {
            Self::Uninterrupted => OwnerInterruption::Uninterrupted,
            Self::CancelAtFirstOwnerReturn => {
                OwnerInterruption::Requested(super::transport::CancelReason::TargetSignal)
            }
        }
    }
}

fn bridge_request(context: &mut Context) -> EvalResult {
    bridge_request_history(context, RequestHistory::Uninterrupted)
}

fn cancelled_bridge_request(context: &mut Context) -> EvalResult {
    bridge_request_history(context, RequestHistory::CancelAtFirstOwnerReturn)
}

fn bridge_request_history(context: &mut Context, history: RequestHistory) -> EvalResult {
    let requester_os = std::thread::current().id();
    let binding = context
        .special_variable_value_by_id(intern("s2-owner-binding"))
        .unwrap();
    let graph = Value::list(vec![Value::string("two owner hops"), Value::fixnum(73)]);
    // SAFETY: graph is this original owner's live value, retained before any
    // collection, callback or handoff through the canonical publication API.
    let call = unsafe { context.green_prepare_host_call(&[graph]) }.map_err(error)?;
    let parent = call.key();
    let first = first_step(context, parent, graph);
    let trace = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&trace);
    let mut stage = NativeStage::Start;
    context.with_green_runtime(|runtime| {
        runtime
            .parked
            .borrow_mut()
            .services
            .install_semantic_fixture_driver(Box::new(move |event, route| {
                let host_os = std::thread::current().id();
                assert_ne!(
                    host_os, requester_os,
                    "native service must not run on the requester"
                );
                observed.lock().unwrap().push((stage, host_os));
                match event {
                    FixtureSemanticEvent::Start { key, native } => {
                        assert_eq!(stage, NativeStage::Start);
                        assert_eq!(key, parent);
                        assert_eq!(native, OwnedNativeData::None);
                        stage = NativeStage::FirstOwner;
                        NativeAdapterStep::RequiresOwner(first)
                    }
                    FixtureSemanticEvent::Owner {
                        key,
                        completion,
                        next,
                        interruption,
                    } => {
                        assert_eq!(
                            interruption,
                            history.interruption(),
                            "pending owner intent is observed before native semantics resume"
                        );
                        assert_eq!(key, parent);
                        assert_eq!(completion.key().parent(), parent);
                        match stage {
                            NativeStage::FirstOwner => {
                                let OwnerNextStep::Prepared(next) = next else {
                                    panic!("requester must publish its second frame")
                                };
                                assert_ne!(completion.key(), next, "each hop has its own nonce");
                                stage = NativeStage::SecondOwner;
                                NativeAdapterStep::RequiresOwner(next)
                            }
                            NativeStage::SecondOwner => {
                                assert!(matches!(next, OwnerNextStep::Absent));
                                let (operation, completion) =
                                    native_pending::pair(key, ReadDeadline::Unbounded, route);
                                assert!(matches!(
                                    completion
                                        .complete(super::transport::NativeOutcome::HostClosed),
                                    NativePublication::Committed(_)
                                ));
                                stage = NativeStage::NativeReply;
                                NativeAdapterStep::Pending(operation)
                            }
                            NativeStage::Start
                            | NativeStage::NativeReply
                            | NativeStage::Finished => panic!("unexpected owner completion"),
                        }
                    }
                    FixtureSemanticEvent::Native { key, outcome } => {
                        assert_eq!(stage, NativeStage::NativeReply);
                        assert_eq!(key, parent);
                        assert!(matches!(
                            outcome,
                            super::transport::NativeOutcome::HostClosed
                        ));
                        stage = NativeStage::Finished;
                        NativeAdapterStep::Complete(outcome)
                    }
                }
            }));
    });
    let consume = |owner: &RestoredHostOwner<'_>,
                   settlement: &super::transport::NativeSettlement| {
        match history {
            RequestHistory::Uninterrupted => assert!(matches!(
                settlement,
                super::transport::NativeSettlement::Completed(
                    super::transport::NativeOutcome::HostClosed
                )
            )),
            RequestHistory::CancelAtFirstOwnerReturn => assert!(matches!(
                settlement,
                super::transport::NativeSettlement::CompletedAfterCancellation {
                    outcome: super::transport::NativeOutcome::HostClosed,
                    cancellation: super::transport::CancelReason::TargetSignal
                }
            )),
        }
        assert_eq!(std::thread::current().id(), requester_os);
        assert_eq!(
            owner
                .context
                .special_variable_value_by_id(intern("s2-owner-binding")),
            Some(binding)
        );
        owner.with_registry(|registry, _| {
            assert!(
                registry.records.contains_key(&parent),
                "final decode precedes retirement"
            )
        });
        Ok(Value::T)
    };
    let mut turn = context.green_host_call_turn(call, OwnedNativeData::None, consume)?;
    let mut owner_hops = 0;
    loop {
        match turn {
            HostOwnerTurn::Final(value) => {
                assert_eq!(owner_hops, 2);
                assert_eq!(
                    trace.lock().unwrap().len(),
                    4,
                    "physical readiness must resume the semantic driver"
                );
                let records = trace.lock().unwrap();
                assert!(records.iter().all(|(_, host)| *host == records[0].1));
                return Ok(Value::list(vec![binding, value, Value::fixnum(owner_hops)]));
            }
            HostOwnerTurn::NeedOwner(step) => {
                owner_hops += 1;
                context
                    .with_owner_execution_frame(step, |frame, claim, access| {
                        assert_eq!(std::thread::current().id(), requester_os);
                        let expected_intent = match owner_hops {
                            1 => OwnerInterruption::Uninterrupted,
                            2 => history.interruption(),
                            _ => panic!("unexpected extra owner hop"),
                        };
                        assert_eq!(frame.interruption().unwrap(), expected_intent);
                        assert_eq!(
                            print_value(&frame.arguments().unwrap()[0]),
                            "(\"two owner hops\" 73)"
                        );
                        frame.execute(|owner| {
                            assert_eq!(
                                owner.special_variable_value_by_id(intern("s2-owner-binding")),
                                Some(binding)
                            );
                            owner.gc_collect_exact();
                            owner
                                .green_park(super::super::scheduler::Wait::Runnable)
                                .unwrap();
                            owner.gc_collect_exact();
                            assert_eq!(std::thread::current().id(), requester_os);
                        });
                        let graph = frame.arguments().unwrap()[0];
                        if owner_hops == 1 {
                            frame.execute(|owner| successor(owner, &access, graph).unwrap());
                            match history {
                                RequestHistory::Uninterrupted => {}
                                RequestHistory::CancelAtFirstOwnerReturn => {
                                    frame.execute(|owner| {
                                        owner
                                            .with_green_runtime(|runtime| {
                                                runtime.parked.borrow_mut().services.cancel(
                                                    parent,
                                                    super::transport::CancelReason::TargetSignal,
                                                )
                                            })
                                            .unwrap();
                                    })
                                }
                            }
                        }
                        frame.execute(|owner| {
                            with_hosts(owner, |registry, baton, heap, _owner| {
                                // SAFETY: this immediate result and the restored frame
                                // stay live during no-GC continuation registration.
                                let result = unsafe {
                                    registry.register_continuation::<CallbackStepResult>(
                                        baton,
                                        heap,
                                        &access,
                                        &[Value::T],
                                    )
                                }
                                .unwrap();
                                let completion =
                                    claim.complete(OwnerStepResult::Callback(result)).unwrap();
                                registry
                                    .complete_owner_step(baton, heap, completion)
                                    .unwrap();
                                assert!(matches!(
                                    registry.records[&parent].state,
                                    RecordState::Published(_)
                                ));
                                assert!(
                                    registry.settle(parent).is_err(),
                                    "an intermediate completion cannot settle its parent"
                                );
                            })
                        });
                    })
                    .map_err(error)?;
                turn = context.green_continue_host_turn(parent, consume)?;
            }
        }
    }
}

#[test]
fn two_owner_hops_and_native_resume_keep_requester_bindings_roots_and_one_parent() {
    run_owner_history(RequestHistory::Uninterrupted);
}

#[test]
fn cancellation_at_owner_return_reaches_native_driver_before_it_resumes_next_step() {
    run_owner_history(RequestHistory::CancelAtFirstOwnerReturn);
}

fn run_owner_history(history: RequestHistory) {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut main = Context::new_with_threading(ThreadingMode::Green);
    match history {
        RequestHistory::Uninterrupted => {
            main.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
                "s2-owner-bridge-request",
                bridge_request
            )));
        }
        RequestHistory::CancelAtFirstOwnerReturn => {
            main.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
                "s2-owner-bridge-request",
                cancelled_bridge_request
            )));
        }
    }
    main.eval_str("(defvar s2-owner-binding 10) (defvar s2-owner-cleanup 0)")
        .unwrap();
    main.start_green_threads().unwrap();
    let result = main.eval_str("(list (thread-join (make-thread (lambda () (let ((s2-owner-binding 37)) (unwind-protect (s2-owner-bridge-request) (setq s2-owner-cleanup (1+ s2-owner-cleanup))))))) s2-owner-binding s2-owner-cleanup)").unwrap();
    assert_eq!(print_value(&result), "((37 t 2) 10 1)");
    main.with_green_runtime(|runtime| {
        assert!(
            runtime.parked.borrow().is_empty(),
            "only final Done retires the one parent and its frame batches"
        )
    });
}

/// Deliberately end a real requester with an unfinished owner step. This models
/// an owner unwinding out of the adapter; no fabricated exit or settlement is used.
fn requester_ends_with_owned_step(context: &mut Context) -> EvalResult {
    use super::pump::FixtureCleanupEvent;

    let requester_os = std::thread::current().id();
    let binding = context
        .special_variable_value_by_id(intern("s2-ended-binding"))
        .unwrap();
    let graph = Value::list(vec![Value::string("ended worker graph"), binding]);
    // SAFETY: this original requester's live graph is retained under its held
    // Baton before any callback, collection or handoff.
    let call = unsafe { context.green_prepare_host_call(&[graph]) }.map_err(error)?;
    let parent = call.key();
    let step = first_step(context, parent, graph);
    context.with_green_runtime(|runtime| {
        runtime
            .parked
            .borrow_mut()
            .services
            .install_cleanup_fixture_driver(
                Box::new(move |event, _route| {
                    assert_ne!(std::thread::current().id(), requester_os);
                    match event {
                        FixtureSemanticEvent::Start { key, native } => {
                            assert_eq!(key, parent);
                            assert_eq!(native, OwnedNativeData::None);
                            NativeAdapterStep::RequiresOwner(step)
                        }
                        FixtureSemanticEvent::Owner { .. }
                        | FixtureSemanticEvent::Native { .. } => {
                            panic!("an ended requester has no Lisp completion")
                        }
                    }
                }),
                Box::new(move |event, _route| {
                    assert_ne!(std::thread::current().id(), requester_os);
                    match event {
                        FixtureCleanupEvent::OwnerEnded { key, step: actual } => {
                            assert_eq!(key, parent);
                            assert_eq!(actual, step);
                            NativeAdapterStep::Complete(super::transport::NativeOutcome::HostClosed)
                        }
                        FixtureCleanupEvent::Native { .. } => panic!("cleanup was synchronous"),
                    }
                }),
            );
    });
    let turn =
        context.green_host_call_turn::<Value>(call, OwnedNativeData::None, |_owner, _reply| {
            panic!("a dead owner must not receive a final decoded reply")
        })?;
    let HostOwnerTurn::NeedOwner(actual) = turn else {
        panic!("fixture must publish its owned continuation")
    };
    assert_eq!(actual, step);
    context
        .with_owner_execution_frame(step, |frame, _claim, _access| {
            assert_eq!(std::thread::current().id(), requester_os);
            assert_eq!(
                print_value(&frame.arguments().unwrap()[0]),
                "(\"ended worker graph\" 37)"
            );
            frame.execute(|owner| {
                owner.gc_collect_exact();
                assert_eq!(
                    owner.special_variable_value_by_id(intern("s2-ended-binding")),
                    Some(binding)
                );
            });
            // The consumed owner frame is unwound without publishing completion.
            // Its parent/ports stay retained until native-only cleanup settles.
        })
        .map_err(error)?;
    Ok(binding)
}

#[test]
fn real_requester_exit_drives_native_cleanup_before_join_and_quiescent_root_drain() {
    use super::owner_steps::OwnerStepState;
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut main = Context::new_with_threading(ThreadingMode::Green);
    main.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "s2-ended-owner-request",
        requester_ends_with_owned_step
    )));
    main.eval_str("(defvar s2-ended-binding 10) (defvar s2-ended-cleanup 0)")
        .unwrap();
    main.start_green_threads().unwrap();
    let result = main.eval_str("(list (thread-join (make-thread (lambda () (let ((s2-ended-binding 37)) (unwind-protect (s2-ended-owner-request) (setq s2-ended-cleanup (1+ s2-ended-cleanup))))))) s2-ended-binding s2-ended-cleanup)").unwrap();
    assert_eq!(print_value(&result), "(37 10 1)");
    main.gc_collect_exact();
    main.with_green_runtime(|runtime| {
        let mut roots = runtime.parked.borrow_mut();
        let registry = roots.hosts();
        assert_eq!(
            registry.records.len(),
            1,
            "the native reply is not an owner acknowledgement"
        );
        let record = registry.records.values().next().unwrap();
        assert!(matches!(record.state, RecordState::TerminalAwaitingOwner));
        assert!(matches!(
            record.steps.state_for_test(),
            OwnerStepState::Idle
        ));
        let mut retained = Vec::new();
        for batch in &record.roots {
            let reader = batch.reader(&main.tagged_heap).unwrap();
            reader.append_roots(&mut retained);
            reader.finish().unwrap();
        }
        assert!(
            retained
                .iter()
                .any(|value| print_value(value) == "(\"ended worker graph\" 37)")
        );
        assert!(roots.services.owner_continuation_keys().is_empty());
    });
    main.green_teardown();
    main.with_green_runtime(|runtime| assert!(runtime.parked.borrow().is_empty()));
}

#[derive(Clone, Copy)]
enum UnusedInitialHistory {
    SchedulerClosedBeforeAdmission,
    ActorClosedBeforePublication,
    ReceiverClosedBeforePublication,
    NativeCompletesWithoutOwner,
}

fn scheduler_refused_initial(context: &mut Context) -> EvalResult {
    unused_initial_request(
        context,
        UnusedInitialHistory::SchedulerClosedBeforeAdmission,
    )
}

fn actor_refused_initial(context: &mut Context) -> EvalResult {
    unused_initial_request(context, UnusedInitialHistory::ActorClosedBeforePublication)
}

fn disconnected_initial(context: &mut Context) -> EvalResult {
    unused_initial_request(
        context,
        UnusedInitialHistory::ReceiverClosedBeforePublication,
    )
}

fn native_ignores_initial(context: &mut Context) -> EvalResult {
    unused_initial_request(context, UnusedInitialHistory::NativeCompletesWithoutOwner)
}

fn unused_initial_request(context: &mut Context, history: UnusedInitialHistory) -> EvalResult {
    let requester_os = std::thread::current().id();
    let binding = context
        .special_variable_value_by_id(intern("s2-initial-binding"))
        .unwrap();
    let graph = Value::list(vec![Value::string("unused initial graph"), binding]);
    // SAFETY: the original requester keeps these values live under its held
    // Baton across no-callback preparation/registration before the first park.
    let call = unsafe { context.green_prepare_host_call(&[graph]) }.map_err(error)?;
    let parent = call.key();
    let _step = first_step(context, parent, graph);
    context.with_green_runtime(|runtime| {
        runtime
            .parked
            .borrow_mut()
            .services
            .install_semantic_fixture_driver(Box::new(move |event, _route| {
                assert!(
                    matches!(history, UnusedInitialHistory::NativeCompletesWithoutOwner),
                    "refusal must not invoke native work"
                );
                match event {
                    FixtureSemanticEvent::Start { key, native } => {
                        assert_eq!(key, parent);
                        assert_eq!(native, OwnedNativeData::None);
                        assert_ne!(std::thread::current().id(), requester_os);
                        NativeAdapterStep::Complete(super::transport::NativeOutcome::HostClosed)
                    }
                    FixtureSemanticEvent::Owner { .. } | FixtureSemanticEvent::Native { .. } => {
                        panic!("unused initial steps must not run an owner callback")
                    }
                }
            }));
        match history {
            UnusedInitialHistory::SchedulerClosedBeforeAdmission => runtime.control.close(),
            UnusedInitialHistory::ActorClosedBeforePublication => {
                runtime.parked.borrow_mut().services.close().unwrap();
            }
            UnusedInitialHistory::ReceiverClosedBeforePublication => {
                runtime
                    .parked
                    .borrow_mut()
                    .services
                    .retire_publication_receiver_for_test();
            }
            UnusedInitialHistory::NativeCompletesWithoutOwner => {}
        }
    });
    let turn =
        context.green_host_call_turn(call, OwnedNativeData::None, |restored, settlement| {
            assert_eq!(std::thread::current().id(), requester_os);
            assert_eq!(
                restored
                    .context
                    .special_variable_value_by_id(intern("s2-initial-binding")),
                Some(binding)
            );
            assert!(matches!(
                settlement,
                super::transport::NativeSettlement::Completed(
                    super::transport::NativeOutcome::HostClosed
                )
            ));
            restored.with_registry(|registry, _| {
                assert!(
                    registry.records.contains_key(&parent),
                    "final decode precedes acknowledged root retirement"
                );
            });
            Ok(Value::T)
        })?;
    match turn {
        HostOwnerTurn::Final(value) => Ok(value),
        HostOwnerTurn::NeedOwner(_) => panic!("a never-requested initial step cannot run Lisp"),
    }
}

fn run_unused_initial_history(history: UnusedInitialHistory) {
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut main = Context::new_with_threading(ThreadingMode::Green);
    match history {
        UnusedInitialHistory::SchedulerClosedBeforeAdmission => {
            main.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
                "s2-unused-initial-request",
                scheduler_refused_initial
            )))
        }
        UnusedInitialHistory::ActorClosedBeforePublication => {
            main.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
                "s2-unused-initial-request",
                actor_refused_initial
            )))
        }
        UnusedInitialHistory::ReceiverClosedBeforePublication => {
            main.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
                "s2-unused-initial-request",
                disconnected_initial
            )))
        }
        UnusedInitialHistory::NativeCompletesWithoutOwner => {
            main.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
                "s2-unused-initial-request",
                native_ignores_initial
            )))
        }
    }
    main.eval_str("(defvar s2-initial-binding 10)").unwrap();
    main.start_green_threads().unwrap();
    let result = match history {
        UnusedInitialHistory::SchedulerClosedBeforeAdmission => main.eval_str("(list (let ((s2-initial-binding 37)) (s2-unused-initial-request)) s2-initial-binding)"),
        UnusedInitialHistory::ActorClosedBeforePublication | UnusedInitialHistory::ReceiverClosedBeforePublication | UnusedInitialHistory::NativeCompletesWithoutOwner => main.eval_str("(list (thread-join (make-thread (lambda () (let ((s2-initial-binding 37)) (s2-unused-initial-request))))) s2-initial-binding)"),
    }.unwrap();
    assert_eq!(print_value(&result), "(t 10)");
    main.gc_collect_exact();
    main.with_green_runtime(|runtime| assert!(runtime.parked.borrow().is_empty()));
}

#[test]
fn refused_initial_step_restores_requester_before_acknowledging_and_retiring_roots() {
    run_unused_initial_history(UnusedInitialHistory::SchedulerClosedBeforeAdmission);
}

#[test]
fn actor_closed_initial_step_restores_requester_without_native_or_owner_callback() {
    run_unused_initial_history(UnusedInitialHistory::ActorClosedBeforePublication);
}

#[test]
fn native_terminal_without_owner_step_restores_requester_and_retires_its_unused_frame() {
    run_unused_initial_history(UnusedInitialHistory::NativeCompletesWithoutOwner);
}

#[test]
fn disconnected_actor_initial_step_restores_requester_without_native_or_owner_callback() {
    run_unused_initial_history(UnusedInitialHistory::ReceiverClosedBeforePublication);
}
