//! Main-owned calls use the actual bridge, not main's own receive channel.
use super::super::{
    HostRegistry, RecordState, RestoredHostOwner,
    owner_steps::{ContinuationAccess, OwnerNextStep, OwnerStepError, StepKey},
    pump::FixtureSemanticEvent,
};
use super::*;
use crate::emacs_core::error::{EvalResult, LispCondition, signal};
use crate::emacs_core::eval::green::switch::HostOwnerTurn;
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::intern::intern;
use crate::emacs_core::print::print_value;
use crate::emacs_core::subr::SubrSpec;
use crate::emacs_core::value::Value;
use crate::host_effect::contracts::{
    CallbackStepArguments, CallbackStepResult, OwnerExecutionFrameRecord, OwnerRestoreFrameRecord,
    ReadDeadline,
};
use crate::host_effect::native_pending::{
    self, NativeAdapterStep, NativeCompletionPublisher, NativePublication,
};
use crate::host_effect::owner_step::{OwnerInvocationId, OwnerStepResult, PendingOwnerStep};
use crate::tagged::gc::TaggedHeap;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

fn error(error: impl std::fmt::Display) -> crate::emacs_core::error::Flow {
    signal(LispCondition::Error, vec![Value::string(error.to_string())])
}

fn with_hosts<R>(
    owner: &mut Context,
    apply: impl FnOnce(&mut HostRegistry, &Baton, &TaggedHeap, ThreadKey) -> R,
) -> R {
    let key = owner.green_key().unwrap();
    assert_eq!(key, ThreadKey::Main);
    let baton = owner.green_seat().unwrap().take_owner_step_baton().unwrap();
    let result = owner.with_green_runtime(|runtime| {
        apply(
            runtime.parked.borrow_mut().hosts(),
            &baton,
            &owner.tagged_heap,
            key,
        )
    });
    owner.green_seat().unwrap().restore_owner_step_baton(baton);
    result
}

fn prepare_first_step(owner: &mut Context, parent: HostRequestKey, graph: Value) -> StepKey {
    with_hosts(owner, |registry, baton, heap, key| {
        // SAFETY: graph and these immediate values are live on their original
        // held-Baton owner; registration contains no GC, callback or handoff.
        let (arguments, execution, restore) = unsafe {
            (
                registry.register::<CallbackStepArguments>(baton, heap, key, parent, &[graph]),
                registry.register::<OwnerExecutionFrameRecord>(
                    baton,
                    heap,
                    key,
                    parent,
                    &[Value::fixnum(31), graph],
                ),
                registry.register::<OwnerRestoreFrameRecord>(
                    baton,
                    heap,
                    key,
                    parent,
                    &[Value::T, graph],
                ),
            )
        };
        let step = registry
            .issue_owner_step(
                baton,
                heap,
                parent,
                key,
                OwnerInvocationId::from_owner_id(NonZeroU64::MIN),
            )
            .unwrap();
        let pending = PendingOwnerStep::callback(
            step,
            arguments.unwrap(),
            execution.unwrap(),
            restore.unwrap(),
        )
        .unwrap();
        registry.prepare_owner_step(baton, heap, pending).unwrap();
        step
    })
}

fn prepare_second_step(
    owner: &mut Context,
    access: &ContinuationAccess,
    graph: Value,
) -> Result<(), OwnerStepError> {
    with_hosts(owner, |registry, baton, heap, _| {
        let step = registry.issue_successor(baton, heap, access)?;
        // SAFETY: the registered execution frame keeps graph live under the
        // original owner's Baton throughout this no-GC/no-handoff operation.
        let (arguments, execution, restore) = unsafe {
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
                    &[graph, Value::fixnum(47)],
                )?,
                registry.register_continuation::<OwnerRestoreFrameRecord>(
                    baton,
                    heap,
                    access,
                    &[graph, Value::NIL],
                )?,
            )
        };
        let pending = PendingOwnerStep::callback(step, arguments, execution, restore).unwrap();
        registry
            .prepare_successor(baton, heap, access, pending)
            .unwrap();
        Ok(())
    })
}

fn local_immediate(owner: &mut Context) -> EvalResult {
    let main_os = std::thread::current().id();
    let binding = owner
        .special_variable_value_by_id(intern("s2-local-binding"))
        .unwrap();
    let graph = Value::list(vec![binding, Value::string("local immediate")]);
    // SAFETY: this original main owner retains graph before GC or handoff.
    let call = unsafe { owner.green_prepare_host_call(&[graph]) }.map_err(error)?;
    let parent = call.key();
    let starts = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&starts);
    let control = owner.with_green_runtime(|runtime| Arc::clone(&runtime.control));
    let original_wait = control.wait_of(ThreadKey::Main);
    owner.with_green_runtime(|runtime| {
        runtime
            .parked
            .borrow_mut()
            .services
            .install_semantic_fixture_driver(Box::new(move |event, _| {
                assert_eq!(std::thread::current().id(), main_os);
                match event {
                    FixtureSemanticEvent::Start { key, native } => {
                        assert_eq!(key, parent);
                        assert_eq!(native, OwnedNativeData::None);
                        assert_eq!(observed.fetch_add(1, Ordering::SeqCst), 0);
                        NativeAdapterStep::Complete(NativeOutcome::HostClosed)
                    }
                    FixtureSemanticEvent::Owner { .. } | FixtureSemanticEvent::Native { .. } => {
                        panic!("immediate main call must not manufacture a continuation")
                    }
                }
            }));
    });
    let decodes = AtomicUsize::new(0);
    let result = owner.green_host_call(call, OwnedNativeData::None, |restored, settlement| {
        assert_eq!(decodes.fetch_add(1, Ordering::SeqCst), 0);
        assert_eq!(std::thread::current().id(), main_os);
        assert_eq!(restored.owner, ThreadKey::Main);
        assert_eq!(
            restored
                .context
                .special_variable_value_by_id(intern("s2-local-binding")),
            Some(binding),
        );
        assert!(matches!(
            settlement,
            NativeSettlement::Completed(NativeOutcome::HostClosed)
        ));
        restored.with_registry(|registry, _| assert!(registry.records.contains_key(&parent)));
        Ok(Value::T)
    })?;
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(decodes.load(Ordering::SeqCst), 1);
    assert_eq!(control.wait_of(ThreadKey::Main), original_wait);
    owner.with_green_runtime(|runtime| assert!(runtime.parked.borrow().is_empty()));
    Ok(Value::list(vec![binding, result]))
}

/// A nested child performs a real host request while the outer main callback
/// waits in thread-join. Its request must use the receive-owned pump without
/// borrowing or settling the suspended parent's semantic continuation.
fn nested_child_host_request(owner: &mut Context) -> EvalResult {
    let requester_os = std::thread::current().id();
    let requester = owner.green_key().unwrap();
    assert!(matches!(requester, ThreadKey::Worker(_)));
    let binding = owner
        .special_variable_value_by_id(intern("s2-local-binding"))
        .unwrap();
    assert_eq!(print_value(&binding), "child");
    let graph = Value::list(vec![Value::string("nested child host graph"), binding]);
    // SAFETY: this original child owner holds its live graph under its Baton,
    // with no collection, callback or ownership handoff before publication.
    let call = unsafe { owner.green_prepare_host_call(&[graph]) }.map_err(error)?;
    let parent = call.key();
    owner.green_host_call(
        call,
        OwnedNativeData::Text("nested child host effect".into()),
        |restored, settlement| {
            assert_eq!(std::thread::current().id(), requester_os);
            assert_eq!(restored.owner, requester);
            assert_eq!(
                restored
                    .context
                    .special_variable_value_by_id(intern("s2-local-binding")),
                Some(binding)
            );
            assert!(matches!(
                settlement,
                NativeSettlement::Completed(NativeOutcome::HostClosed)
            ));
            restored.with_registry(|registry, _| assert!(registry.records.contains_key(&parent)));
            Ok(Value::T)
        },
    )
}

/// The peer can execute only after main actually releases the Baton. Its
/// collection witnesses parked main's roots; the native helper owns no Lisp.
fn complete_local_pending(owner: &mut Context) -> EvalResult {
    assert!(matches!(owner.green_key(), Some(ThreadKey::Worker(_))));
    owner.gc_collect_exact();
    owner.with_green_runtime(|runtime| {
        runtime
            .parked
            .borrow_mut()
            .services
            .finish_fixture_gc()
            .unwrap();
    });
    Ok(Value::T)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainStage {
    Start,
    FirstOwner,
    SecondOwner,
    PendingNative,
    Finished,
}

fn local_two_hops(owner: &mut Context) -> EvalResult {
    let main_os = std::thread::current().id();
    let binding = owner
        .special_variable_value_by_id(intern("s2-local-binding"))
        .unwrap();
    let graph = Value::list(vec![
        Value::string("local graph survives nested waits"),
        binding,
    ]);
    // SAFETY: the original main owner retains graph before any GC or handoff.
    let call = unsafe { owner.green_prepare_host_call(&[graph]) }.map_err(error)?;
    let parent = call.key();
    let first = prepare_first_step(owner, parent, graph);
    let collections = Arc::new(AtomicUsize::new(0));
    let (collected, collection) = mpsc::channel();
    let (publish, publication) = mpsc::channel::<NativeCompletionPublisher>();
    let native_helper = std::thread::spawn(move || {
        let producer = publication.recv_timeout(Duration::from_secs(10)).unwrap();
        let peer: FixtureGcCompleted = collection.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_ne!(peer.worker, main_os);
        assert_ne!(std::thread::current().id(), main_os);
        assert!(matches!(
            producer.complete(NativeOutcome::HostClosed),
            NativePublication::Committed(_)
        ));
    });
    let child_calls = Arc::new(AtomicUsize::new(0));
    let child_observed = Arc::clone(&child_calls);
    let trace = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&trace);
    let mut stage = MainStage::Start;
    owner.with_green_runtime(|runtime| {
        let mut parked = runtime.parked.borrow_mut();
        parked
            .services
            .install_fixture_completion(collected, Arc::clone(&collections));
        parked
            .services
            .install_semantic_fixture_driver(Box::new(move |event, route| {
                assert_eq!(std::thread::current().id(), main_os);
                if let FixtureSemanticEvent::Start { key, native } = &event {
                    if *key != parent {
                        assert_eq!(stage, MainStage::FirstOwner);
                        assert_eq!(
                            native,
                            &OwnedNativeData::Text("nested child host effect".into())
                        );
                        assert_eq!(child_observed.fetch_add(1, Ordering::SeqCst), 0);
                        return NativeAdapterStep::Complete(NativeOutcome::HostClosed);
                    }
                }
                observed.lock().unwrap().push(stage);
                match event {
                    FixtureSemanticEvent::Start { key, native } => {
                        assert_eq!(stage, MainStage::Start);
                        assert_eq!(key, parent);
                        assert_eq!(native, OwnedNativeData::None);
                        stage = MainStage::FirstOwner;
                        NativeAdapterStep::RequiresOwner(first)
                    }
                    FixtureSemanticEvent::Owner {
                        key,
                        completion,
                        next,
                        interruption,
                    } => {
                        assert_eq!(key, parent);
                        assert_eq!(completion.key().owner(), ThreadKey::Main);
                        assert_eq!(
                            interruption,
                            super::super::owner_steps::OwnerInterruption::Uninterrupted
                        );
                        match stage {
                            MainStage::FirstOwner => {
                                let OwnerNextStep::Prepared(second) = next else {
                                    panic!("local requester must prepare exactly one successor")
                                };
                                assert_ne!(completion.key(), second);
                                stage = MainStage::SecondOwner;
                                NativeAdapterStep::RequiresOwner(second)
                            }
                            MainStage::SecondOwner => {
                                assert!(matches!(next, OwnerNextStep::Absent));
                                let (operation, producer) =
                                    native_pending::pair(key, ReadDeadline::Unbounded, route);
                                publish.send(producer).unwrap();
                                stage = MainStage::PendingNative;
                                NativeAdapterStep::Pending(operation)
                            }
                            MainStage::Start | MainStage::PendingNative | MainStage::Finished => {
                                panic!("unexpected local owner completion")
                            }
                        }
                    }
                    FixtureSemanticEvent::Native { key, outcome } => {
                        assert_eq!(stage, MainStage::PendingNative);
                        assert_eq!(key, parent);
                        assert_eq!(outcome, NativeOutcome::HostClosed);
                        stage = MainStage::Finished;
                        NativeAdapterStep::Complete(outcome)
                    }
                }
            }));
    });
    let decodes = AtomicUsize::new(0);
    let decode = |restored: &RestoredHostOwner<'_>, settlement: &NativeSettlement| {
        assert_eq!(decodes.fetch_add(1, Ordering::SeqCst), 0);
        assert_eq!(std::thread::current().id(), main_os);
        assert_eq!(restored.owner, ThreadKey::Main);
        assert_eq!(
            restored
                .context
                .special_variable_value_by_id(intern("s2-local-binding")),
            Some(binding),
        );
        assert!(matches!(
            settlement,
            NativeSettlement::Completed(NativeOutcome::HostClosed)
        ));
        restored.with_registry(|registry, _| {
            assert!(
                registry.records.contains_key(&parent),
                "decode precedes final root retirement"
            );
        });
        Ok(Value::T)
    };
    let mut turn = owner.green_host_call_turn(call, OwnedNativeData::None, decode)?;
    let mut hops = 0;
    let result = loop {
        match turn {
            HostOwnerTurn::Final(value) => break value,
            HostOwnerTurn::NeedOwner(step) => {
                hops += 1;
                assert_eq!(
                    decodes.load(Ordering::SeqCst),
                    0,
                    "no intermediate terminal decode"
                );
                assert_eq!(step.owner(), ThreadKey::Main);
                owner.with_owner_execution_frame(step, |frame, claim, access| {
                    assert_eq!(std::thread::current().id(), main_os);
                    assert_eq!(print_value(&frame.arguments().unwrap()[0]),
                        "(\"local graph survives nested waits\" 61)");
                    frame.execute(|owner| {
                        assert_eq!(owner.green_key(), Some(ThreadKey::Main));
                        assert_eq!(owner.special_variable_value_by_id(intern("s2-local-binding")), Some(binding));
                        owner.gc_collect_exact();
                        if hops == 1 {
                            // A genuine nested join restores the same callback
                            // frame after another Lisp owner collects. The bare
                            // Context has no subr.el macros; use the core `if`
                            // so the nested host request actually executes.
                            assert_eq!(print_value(&owner.eval_str("(thread-last-error t)").unwrap()), "nil");
                            let child = owner.eval_str("(thread-join (make-thread (lambda () (let ((s2-local-binding 'child)) (garbage-collect) (if (eq (s2-local-child-request) t) nil (signal 'error nil)) 'nested-done))))").unwrap();
                            assert_eq!(print_value(&child), "nested-done");
                            assert_eq!(print_value(&owner.eval_str("(thread-last-error)").unwrap()), "nil", "the nested owner must finish without a hidden Lisp error");
                            owner.gc_collect_exact();
                        } else {
                            assert_eq!(hops, 2);
                            // This newly ready worker does not run at the
                            // artificial continuation hop. The pending native
                            // executor's actual release gives it its first turn.
                            owner.eval_str("(setq s2-local-pending-peer (make-thread (lambda () (s2-local-complete-pending))))").unwrap();
                        }
                    });
                    assert_eq!(collections.load(Ordering::SeqCst), 0);
                    let graph = frame.arguments().unwrap()[0];
                    if hops == 1 {
                        frame.execute(|owner| prepare_second_step(owner, &access, graph).unwrap());
                    }
                    frame.execute(|owner| {
                        with_hosts(owner, |registry, baton, heap, _| {
                            // SAFETY: this original owner's registered frame
                            // roots its live values through immediate publication.
                            let result = unsafe {
                                registry.register_continuation::<CallbackStepResult>(
                                    baton, heap, &access, &[Value::T],
                                )
                            }.unwrap();
                            let completion = claim.complete(OwnerStepResult::Callback(result)).unwrap();
                            registry.complete_owner_step(baton, heap, completion).unwrap();
                            assert!(matches!(registry.records[&parent].state, RecordState::Published(_)));
                            assert!(registry.settle(parent).is_err(), "intermediate completion is not terminal");
                        });
                    });
                }).map_err(error)?;
                turn = owner.green_continue_host_turn(parent, decode)?;
            }
        }
    };
    native_helper.join().unwrap();
    assert_eq!(child_calls.load(Ordering::SeqCst), 1);
    let joined = owner
        .eval_str("(thread-join s2-local-pending-peer)")
        .unwrap();
    assert_eq!(print_value(&joined), "t");
    assert_eq!(hops, 2);
    assert_eq!(decodes.load(Ordering::SeqCst), 1);
    assert_eq!(collections.load(Ordering::SeqCst), 1);
    assert_eq!(
        *trace.lock().unwrap(),
        vec![
            MainStage::Start,
            MainStage::FirstOwner,
            MainStage::SecondOwner,
            MainStage::PendingNative
        ]
    );
    owner.with_green_runtime(|runtime| assert!(runtime.parked.borrow().is_empty()));
    Ok(Value::list(vec![binding, result, Value::fixnum(hops)]))
}

fn main_owner() -> Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut owner = Context::new_with_threading(ThreadingMode::Green);
    owner.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "s2-local-immediate",
        local_immediate,
    )));
    owner.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "s2-local-two-hops",
        local_two_hops,
    )));
    owner.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "s2-local-complete-pending",
        complete_local_pending,
    )));
    owner.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "s2-local-child-request",
        nested_child_host_request,
    )));
    owner.eval_str("(defvar s2-local-binding 10) (defvar s2-local-cleanup 0) (defvar s2-local-pending-peer nil)").unwrap();
    owner.start_green_threads().unwrap();
    owner
}

#[test]
fn actual_local_main_immediate_dispatch_restores_binding_and_decodes_once() {
    let mut owner = main_owner();
    let result = owner
        .eval_str("(list (let ((s2-local-binding 43)) (s2-local-immediate)) s2-local-binding)")
        .unwrap();
    assert_eq!(print_value(&result), "((43 t) 10)");
}

#[test]
fn actual_local_main_two_hops_survive_nested_join_gc_and_real_native_pending() {
    let mut owner = main_owner();
    let result = owner.eval_str("(list (let ((s2-local-binding 61)) (unwind-protect (s2-local-two-hops) (setq s2-local-cleanup (1+ s2-local-cleanup)))) s2-local-binding s2-local-cleanup)").unwrap();
    assert_eq!(print_value(&result), "((61 t 2) 10 1)");
}
