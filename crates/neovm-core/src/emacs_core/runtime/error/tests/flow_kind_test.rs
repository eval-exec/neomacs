//! The P0.12 Stage-A flow API: `FlowKind` behind the `Flow` alias, the
//! borrowed `FlowRef` view, the accessors and the `Result` views. These pin
//! the contract a one-word `Flow` must keep (same views, same `Debug` text).

use super::{
    EvalError, Flow, FlowKind, FlowRef, FlowResultExt, LispCondition, flow_from_eval_error,
    map_flow, signal,
};
use crate::emacs_core::eval::ShutdownRequest;
use crate::emacs_core::{Context, Value};

fn every_kind() -> Vec<Flow> {
    let mut flows = vec![
        signal(
            LispCondition::Error,
            vec![Value::string("flow-kind payload")],
        ),
        Flow::throw(Value::symbol("flow-kind-tag"), Value::string("thrown")),
        Flow::thread_blocked(
            Value::symbol("blocker"),
            Value::list(vec![Value::fixnum(1)]),
        ),
    ];
    for exit_code in [i32::MIN, -1, 0, 1, i32::MAX] {
        for restart in [false, true] {
            flows.push(Flow::shutdown(ShutdownRequest { exit_code, restart }));
        }
    }
    flows
}

#[test]
fn every_kind_round_trips_through_from_kind_and_into_kind() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    for flow in every_kind() {
        let before = format!("{flow:?}");
        let back = Flow::from_kind(flow.into_kind());
        assert_eq!(format!("{back:?}"), before);
    }
}

#[test]
fn debug_text_matches_the_owned_enum() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    for flow in every_kind() {
        assert_eq!(
            format!("{flow:?}"),
            format!("{:?}", flow.clone().into_kind())
        );
    }
    let text = format!(
        "{:?}",
        Flow::shutdown(ShutdownRequest {
            exit_code: 3,
            restart: true
        })
    );
    assert_eq!(
        text,
        "Shutdown(ShutdownRequest { exit_code: 3, restart: true })"
    );
}

#[test]
fn borrowed_view_and_accessors_agree_with_the_owned_variant() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    for flow in every_kind() {
        let view = flow.kind();
        match flow.clone().into_kind() {
            FlowKind::Signal(sig) => {
                let FlowRef::Signal(seen) = view else {
                    panic!("signal view: {view:?}")
                };
                assert_eq!(seen.symbol, sig.symbol);
                assert_eq!(seen.data, sig.data);
                assert!(flow.is_signal());
                assert!(!flow.is_throw() && !flow.is_thread_blocked() && !flow.is_shutdown());
                assert_eq!(flow.as_signal().map(|s| s.symbol), Some(sig.symbol));
                assert!(flow.as_throw().is_none());
                assert!(flow.shutdown_request().is_none());
            }
            FlowKind::Throw(thrown) => {
                let FlowRef::Throw(seen) = view else {
                    panic!("throw view: {view:?}")
                };
                assert_eq!((seen.tag, seen.value), (thrown.tag, thrown.value));
                assert!(flow.is_throw());
                assert!(!flow.is_signal() && !flow.is_thread_blocked() && !flow.is_shutdown());
                let t = flow.as_throw().expect("as_throw");
                assert_eq!((t.tag, t.value), (thrown.tag, thrown.value));
                assert!(flow.as_signal().is_none());
            }
            FlowKind::ThreadBlocked(blocked) => {
                let FlowRef::ThreadBlocked(seen) = view else {
                    panic!("thread-blocked view: {view:?}")
                };
                assert_eq!(seen.blocker, blocked.blocker);
                assert!(flow.is_thread_blocked());
                assert!(!flow.is_signal() && !flow.is_throw() && !flow.is_shutdown());
                assert_eq!(
                    flow.as_thread_blocked().map(|b| b.remaining_forms),
                    Some(blocked.remaining_forms)
                );
            }
            FlowKind::Shutdown(request) => {
                let FlowRef::Shutdown(seen) = view else {
                    panic!("shutdown view: {view:?}")
                };
                assert_eq!(seen, request);
                assert!(flow.is_shutdown());
                assert_eq!(flow.shutdown_request(), Some(request));
                assert!(flow.as_signal().is_none() && flow.as_throw().is_none());
            }
        }
    }
}

#[test]
fn as_signal_mut_edits_the_payload_in_place() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let mut flow = signal(LispCondition::Error, vec![Value::fixnum(7)]);
    flow.as_signal_mut().expect("a signal").search_complete = true;
    let FlowKind::Signal(sig) = flow.into_kind() else {
        panic!("still a signal")
    };
    assert!(sig.search_complete);

    let mut thrown = Flow::throw(Value::symbol("tag"), Value::NIL);
    assert!(thrown.as_signal_mut().is_none());
}

#[test]
fn signal_boxed_rewraps_a_taken_payload() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let flow = signal(LispCondition::WrongTypeArgument, vec![Value::fixnum(1)]);
    let before = format!("{flow:?}");
    let FlowKind::Signal(sig) = flow.into_kind() else {
        panic!("a signal")
    };
    assert_eq!(format!("{:?}", Flow::signal_boxed(sig)), before);
}

#[test]
fn result_views_expose_the_kind_on_the_error_side() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let ok: Result<i64, Flow> = Ok(5);
    assert!(matches!(ok.kinded_ref(), Ok(&5)));
    assert!(matches!(ok.kinded(), Ok(5)));

    let err: Result<i64, Flow> = Err(Flow::throw(Value::symbol("k"), Value::fixnum(9)));
    match err.kinded_ref() {
        Err(FlowRef::Throw(t)) => assert_eq!(t.value, Value::fixnum(9)),
        other => panic!("borrowed view: {other:?}"),
    }
    match err.kinded() {
        Err(FlowKind::Throw(t)) => assert_eq!(t.value, Value::fixnum(9)),
        other => panic!("owned view: {other:?}"),
    }
}

#[test]
fn eval_error_round_trip_per_kind() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let signal_error = map_flow(signal(LispCondition::Error, vec![Value::fixnum(1)]));
    let back = flow_from_eval_error(signal_error);
    assert!(back.is_signal());
    assert_eq!(
        back.as_signal().map(|s| s.data.clone()),
        Some(vec![Value::fixnum(1)])
    );

    let thrown = map_flow(Flow::throw(Value::symbol("k"), Value::fixnum(2)));
    assert!(matches!(thrown, EvalError::UncaughtThrow { .. }));
    let back = flow_from_eval_error(thrown);
    assert_eq!(back.as_throw().map(|t| t.value), Some(Value::fixnum(2)));

    let request = ShutdownRequest {
        exit_code: -1,
        restart: true,
    };
    let back = flow_from_eval_error(map_flow(Flow::shutdown(request)));
    assert_eq!(back.shutdown_request(), Some(request));
}

#[test]
fn gdl_eval_error_memory_round_trip_keeps_original_binding_through_clone_and_gc() {
    use super::{
        SignalDelivery, collect_in_flight_flow_gc_roots, make_signal_binding_value,
        memory_signal_from_binding_value,
    };
    use crate::emacs_core::eval::ResumeTarget;

    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    // This descriptor is not stored in a Lisp variable. Only the in-flight
    // signal owns a collector-visible root for its original cons.
    let binding = ctx
        .eval_str("(cons (make-symbol \"gdl-roundtrip-local-error\") (cons (vector 7) 99))")
        .unwrap();
    let symbol = binding.cons_car().as_symbol_id().unwrap();
    let tail = binding.cons_cdr();
    let vector = tail.cons_car();
    let target = ResumeTarget::SafeFuncall {
        condition_stack_base: 7,
    };
    let mut flow = memory_signal_from_binding_value(binding);
    let signal = flow.as_signal_mut().unwrap();
    signal.selected_resume = Some(target.clone());
    signal.search_complete = true;

    let original_error = map_flow(flow);
    let error = original_error.clone();
    drop(original_error);
    let mut roots = Vec::new();
    collect_in_flight_flow_gc_roots(&mut roots, ctx.tagged_heap.identity());
    assert!(
        roots.iter().any(|root| root.bits() == binding.bits()),
        "the public error clone must pin the original memory descriptor"
    );
    ctx.gc_collect_exact();
    for value in [binding, tail, vector] {
        assert!(ctx.tagged_heap.owns_heap_value_for_test(value));
    }

    let back = flow_from_eval_error(error);
    let signal = back.as_signal().unwrap();
    assert!(matches!(
        signal.delivery(),
        SignalDelivery::MemoryExhausted(_)
    ));
    assert_eq!(signal.symbol, symbol);
    assert_eq!(signal.raw_data, Some(tail));
    assert_eq!(make_signal_binding_value(signal).bits(), binding.bits());
    assert_eq!(signal.selected_resume, Some(target));
    assert!(signal.search_complete);
    ctx.gc_collect_exact();
    assert!(ctx.tagged_heap.owns_heap_value_for_test(binding));
    assert!(ctx.tagged_heap.owns_heap_value_for_test(vector));
    assert_eq!(binding.cons_car().as_symbol_id(), Some(symbol));
    assert_eq!(tail.cons_cdr(), Value::fixnum(99));
    assert_eq!(vector.as_vector_data().unwrap()[0], Value::fixnum(7));
}

#[test]
fn gdl_eval_error_round_trip_keeps_hook_suppression() {
    use super::{SignalDelivery, signal_suppressed};

    crate::test_utils::init_test_tracing();
    let _ctx = Context::new();
    let back = flow_from_eval_error(map_flow(signal_suppressed(
        LispCondition::Error,
        vec![Value::fixnum(2)],
    )));
    let signal = back.as_signal().unwrap();
    assert!(matches!(signal.delivery(), SignalDelivery::HookSuppressed));
    assert_eq!(signal.data, vec![Value::fixnum(2)]);
    assert!(signal.selected_resume.is_none());
    assert!(!signal.search_complete);
}

#[test]
fn gdl_eval_error_round_trip_keeps_completed_handler_selection() {
    use super::{SignalDelivery, signal_suppressed};
    use crate::emacs_core::eval::ResumeTarget;

    crate::test_utils::init_test_tracing();
    let _ctx = Context::new();
    for mut flow in [
        signal(LispCondition::Error, vec![Value::fixnum(3)]),
        signal_suppressed(LispCondition::Error, vec![Value::fixnum(4)]),
    ] {
        let target = ResumeTarget::InterpreterConditionCase {
            handler_index: 3,
            condition_stack_base: 2,
        };
        let suppressed = matches!(
            flow.as_signal().unwrap().delivery(),
            SignalDelivery::HookSuppressed
        );
        let signal = flow.as_signal_mut().unwrap();
        signal.selected_resume = Some(target.clone());
        signal.search_complete = true;
        // A second public boundary models repeated loader cleanup crossings;
        // none may restart handler-bind search or discard the chosen target.
        let back = flow_from_eval_error(map_flow(flow_from_eval_error(map_flow(flow))));
        let signal = back.as_signal().unwrap();
        assert_eq!(signal.selected_resume, Some(target));
        assert!(signal.search_complete);
        assert_eq!(
            matches!(signal.delivery(), SignalDelivery::HookSuppressed),
            suppressed
        );
    }
}

#[test]
fn gdl_eval_error_public_signal_constructor_starts_ordinary_unselected() {
    use super::SignalDelivery;

    crate::test_utils::init_test_tracing();
    let _ctx = Context::new();
    let back = flow_from_eval_error(EvalError::signal(
        Value::symbol("error").as_symbol_id().unwrap(),
        vec![Value::fixnum(5)],
        None,
    ));
    let signal = back.as_signal().unwrap();
    assert!(matches!(signal.delivery(), SignalDelivery::Ordinary));
    assert!(signal.selected_resume.is_none());
    assert!(!signal.search_complete);
}
