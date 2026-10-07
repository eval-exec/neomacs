//! P0.12 §11 ownership and carrier representation tests, in both configurations.
//! Register this as a child of error/mod.rs so private registry fields are visible.

use super::{
    EvalError, EvalResult, Flow, FlowKind, FlowRef, InFlightRegistryHandle, InFlightRootPin,
    LispCondition, SignalData, ThreadBlockedData, ThrowData, current_in_flight_registry_handle,
    flow_from_eval_error, is_signal_result, map_flow, signal, with_signal_result,
};
use crate::emacs_core::eval::{Context, ResumeTarget, ShutdownRequest};
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::Value;
use std::collections::BTreeSet;
use std::mem::size_of;

// A unique inferred marker exists only if Flow does NOT implement the trait.
// If it becomes Send/Sync, the two candidates make this assertion ambiguous.
// This is checked by normal cargo check/nextest compilation in both configs,
// without cargo test doctests or a new static_assertions dependency.
macro_rules! assert_not_impl {
    ($ty:ty, $bound:path) => {
        const _: fn() = || {
            trait AmbiguousIfImpl<A> {
                fn marker() {}
            }
            impl<T: ?Sized> AmbiguousIfImpl<()> for T {}
            struct Implements;
            impl<T: ?Sized + $bound> AmbiguousIfImpl<Implements> for T {}
            let _ = <$ty as AmbiguousIfImpl<_>>::marker;
        };
    };
}
assert_not_impl!(Flow, Send);
assert_not_impl!(Flow, Sync);

#[derive(Debug)]
struct RegistryState {
    live: usize,
    total: usize,
    free: usize,
}

fn registry_state(registry: &InFlightRegistryHandle) -> RegistryState {
    let table = registry.lock();
    let free: BTreeSet<usize> = table.free.iter().copied().collect();
    assert_eq!(free.len(), table.free.len(), "a slot was released twice");
    for &slot in &free {
        assert!(slot < table.slots.len(), "free slot outside its registry");
        assert!(table.slots[slot].is_none(), "live slot on the free list");
    }
    let live = table.slots.iter().filter(|slot| slot.is_some()).count();
    assert_eq!(
        live + free.len(),
        table.slots.len(),
        "an unowned slot leaked"
    );
    RegistryState {
        live,
        total: table.slots.len(),
        free: free.len(),
    }
}

fn owning_pin(flow: &Flow) -> &InFlightRootPin {
    let roots = match flow.kind() {
        FlowRef::Signal(payload) => &payload.pin,
        FlowRef::Throw(payload) => &payload.pin,
        FlowRef::ThreadBlocked(payload) => &payload.pin,
        FlowRef::Shutdown(_) => panic!("Shutdown has no registry pin"),
    };
    roots.pin.as_ref().expect("heap payload has a pin")
}

fn owning_pin_slot(flow: &Flow) -> usize {
    owning_pin(flow).slot
}

fn owned_payload_address(kind: &FlowKind) -> Option<usize> {
    match kind {
        FlowKind::Signal(payload) => Some(std::ptr::from_ref(&**payload).cast::<()>() as usize),
        FlowKind::Throw(payload) => Some(std::ptr::from_ref(&**payload).cast::<()>() as usize),
        FlowKind::ThreadBlocked(payload) => {
            Some(std::ptr::from_ref(&**payload).cast::<()>() as usize)
        }
        FlowKind::Shutdown(_) => None,
    }
}

fn borrowed_payload_address(flow: &Flow) -> Option<usize> {
    match flow.kind() {
        FlowRef::Signal(payload) => Some(std::ptr::from_ref(payload).cast::<()>() as usize),
        FlowRef::Throw(payload) => Some(std::ptr::from_ref(payload).cast::<()>() as usize),
        FlowRef::ThreadBlocked(payload) => Some(std::ptr::from_ref(payload).cast::<()>() as usize),
        FlowRef::Shutdown(_) => None,
    }
}

fn heap_kinds() -> Vec<FlowKind> {
    vec![
        FlowKind::Signal(Box::new(SignalData::new(
            intern("error"),
            vec![Value::string("flow word signal")],
            Some(Value::list(vec![Value::string("raw signal data")])),
            false,
        ))),
        FlowKind::Throw(Box::new(ThrowData::new(
            Value::symbol("flow-word-tag"),
            Value::string("flow word throw"),
        ))),
        FlowKind::ThreadBlocked(Box::new(ThreadBlockedData::new(
            Value::string("flow word blocker"),
            Value::list(vec![Value::fixnum(31)]),
        ))),
    ]
}

#[test]
fn flow_is_one_word_and_eval_result_two() {
    #[cfg(feature = "flow-word")]
    {
        assert_eq!(size_of::<Flow>(), 8);
        assert_eq!(size_of::<Option<Flow>>(), 8);
        assert_eq!(size_of::<EvalResult>(), 16);
        assert_eq!(size_of::<Option<EvalResult>>(), 16);
        assert_eq!(size_of::<Result<(), Flow>>(), 8);
    }
    #[cfg(not(feature = "flow-word"))]
    {
        assert_eq!(size_of::<Flow>(), size_of::<FlowKind>());
        assert_eq!(
            size_of::<EvalResult>(),
            size_of::<Result<Value, FlowKind>>()
        );
    }
}

#[test]
fn every_kind_round_trips_through_the_word() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    for kind in heap_kinds() {
        let expected = format!("{kind:?}");
        let address = owned_payload_address(&kind);
        let flow = Flow::from_kind(kind);
        assert_eq!(borrowed_payload_address(&flow), address);
        eval.gc_collect_exact();
        assert_eq!(format!("{flow:?}"), expected);
        let restored = flow.into_kind();
        assert_eq!(
            owned_payload_address(&restored),
            address,
            "packing must preserve the owning box"
        );
        assert_eq!(format!("{restored:?}"), expected);
    }
    for exit_code in [i32::MIN, -1, 0, 1, i32::MAX] {
        for restart in [false, true] {
            let request = ShutdownRequest { exit_code, restart };
            let flow = Flow::from_kind(FlowKind::Shutdown(request));
            assert_eq!(flow.shutdown_request(), Some(request));
            assert!(matches!(flow.kind(), FlowRef::Shutdown(seen) if seen == request));
            assert!(matches!(flow.into_kind(), FlowKind::Shutdown(seen) if seen == request));
        }
    }
}

#[test]
fn debug_text_matches_the_owned_enum() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let mut flows: Vec<Flow> = heap_kinds().into_iter().map(Flow::from_kind).collect();
    for exit_code in [i32::MIN, -1, 0, 1, i32::MAX] {
        for restart in [false, true] {
            flows.push(Flow::shutdown(ShutdownRequest { exit_code, restart }));
        }
    }
    for flow in flows {
        assert_eq!(
            format!("{flow:?}"),
            format!("{:?}", flow.clone().into_kind())
        );
    }
}

#[test]
fn clone_takes_a_fresh_pin_and_drop_releases_it() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let registry = current_in_flight_registry_handle();
    // Construct each kind separately: unrelated live payloads must not mask
    // a missing clone pin or a duplicate slot release.
    for kind_index in 0..3 {
        let before = registry_state(&registry);
        let flow = match kind_index {
            0 => signal(LispCondition::Error, vec![Value::string("clone signal")]),
            1 => Flow::throw(Value::symbol("clone-tag"), Value::string("clone throw")),
            _ => Flow::thread_blocked(
                Value::string("clone blocker"),
                Value::list(vec![Value::fixnum(7)]),
            ),
        };
        assert_eq!(registry_state(&registry).live, before.live + 1);
        let cloned = flow.clone();
        assert_ne!(owning_pin_slot(&flow), owning_pin_slot(&cloned));
        assert_eq!(registry_state(&registry).live, before.live + 2);
        drop(flow);
        assert_eq!(registry_state(&registry).live, before.live + 1);
        drop(cloned);
        let after = registry_state(&registry);
        assert_eq!(after.live, before.live);
        assert_eq!(after.free, before.free + after.total - before.total);
    }
}

#[test]
fn into_kind_transfers_the_pin_until_the_owned_payload_drops() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let registry = current_in_flight_registry_handle();
    let before = registry_state(&registry);
    let flow = signal(LispCondition::Error, vec![Value::string("owned view pin")]);
    let slot = owning_pin_slot(&flow);
    let owned = flow.into_kind();
    assert_eq!(registry_state(&registry).live, before.live + 1);
    let FlowKind::Signal(payload) = &owned else {
        panic!("Signal owned view")
    };
    assert_eq!(payload.pin.pin.as_ref().unwrap().slot, slot);
    drop(owned);
    assert_eq!(registry_state(&registry).live, before.live);
}

#[test]
fn clone_keeps_the_original_context_registry_after_activation_changes() {
    crate::test_utils::init_test_tracing();
    let _first = Context::new();
    let first_registry = current_in_flight_registry_handle();
    let first_before = registry_state(&first_registry);
    let original = signal(
        LispCondition::Error,
        vec![Value::string("original context")],
    );
    let _second = Context::new();
    let second_registry = current_in_flight_registry_handle();
    let second_before = registry_state(&second_registry);
    assert!(!std::sync::Arc::ptr_eq(
        &first_registry.table,
        &second_registry.table
    ));
    let cloned = original.clone();
    assert!(std::sync::Arc::ptr_eq(
        &owning_pin(&original).registry.table,
        &first_registry.table
    ));
    assert!(std::sync::Arc::ptr_eq(
        &owning_pin(&cloned).registry.table,
        &first_registry.table
    ));
    assert!(!std::sync::Arc::ptr_eq(
        &owning_pin(&cloned).registry.table,
        &second_registry.table
    ));
    assert_eq!(registry_state(&first_registry).live, first_before.live + 2);
    assert_eq!(registry_state(&second_registry).live, second_before.live);
    drop(original);
    assert_eq!(registry_state(&first_registry).live, first_before.live + 1);
    assert_eq!(registry_state(&second_registry).live, second_before.live);
    drop(cloned);
    assert_eq!(registry_state(&first_registry).live, first_before.live);
    assert_eq!(registry_state(&second_registry).live, second_before.live);
}

#[test]
fn shutdown_has_no_registry_pin() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let registry = current_in_flight_registry_handle();
    let before = registry_state(&registry);
    for exit_code in [i32::MIN, -1, 0, 1, i32::MAX] {
        for restart in [false, true] {
            let flow = Flow::shutdown(ShutdownRequest { exit_code, restart });
            let cloned = flow.clone();
            drop(flow.into_kind());
            drop(cloned);
        }
    }
    let after = registry_state(&registry);
    assert_eq!(
        (after.live, after.total, after.free),
        (before.live, before.total, before.free)
    );
}

#[test]
fn as_signal_mut_edits_in_place() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let mut flow = signal(LispCondition::Error, vec![Value::string("mutable payload")]);
    let address = borrowed_payload_address(&flow);
    let target = ResumeTarget::InterpreterConditionCase {
        handler_index: 7,
        condition_stack_base: 11,
    };
    let payload = flow.as_signal_mut().expect("signal payload");
    payload.search_complete = true;
    payload.selected_resume = Some(target.clone());
    assert_eq!(borrowed_payload_address(&flow), address);
    let FlowKind::Signal(payload) = flow.into_kind() else {
        panic!("Signal owned view")
    };
    assert!(payload.search_complete);
    assert_eq!(payload.selected_resume, Some(target));
}

#[test]
fn eval_error_round_trip_per_kind() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let mut errors = vec![
        EvalError::signal(
            intern("error"),
            vec![Value::string("public signal")],
            Some(Value::list(vec![Value::string("raw public data")])),
        ),
        EvalError::uncaught_throw(
            Value::symbol("public-throw-tag"),
            Value::string("public throw"),
        ),
    ];
    for exit_code in [i32::MIN, -1, 0, 1, i32::MAX] {
        for restart in [false, true] {
            errors.push(EvalError::Shutdown(ShutdownRequest { exit_code, restart }));
        }
    }
    for error in errors {
        let expected = format!("{error:?}");
        let returned = map_flow(flow_from_eval_error(error));
        assert_eq!(format!("{returned:?}"), expected);
    }
}

#[test]
fn is_signal_result_evaluates_its_expression_once_and_releases_the_temporary() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let registry = current_in_flight_registry_handle();
    let before = registry_state(&registry);
    let calls = std::cell::Cell::new(0);
    let seen = is_signal_result!({
        calls.set(calls.get() + 1);
        Err::<Value, Flow>(signal(
            LispCondition::Error,
            vec![Value::string("temporary signal")],
        ))
    });
    assert!(seen);
    assert_eq!(calls.get(), 1);
    assert_eq!(registry_state(&registry).live, before.live);
    // A place expression must remain owned after the borrowed predicate.
    let result: EvalResult = Err(signal(
        LispCondition::Error,
        vec![Value::string("retained signal")],
    ));
    let slot = owning_pin_slot(result.as_ref().unwrap_err());
    assert!(is_signal_result!(result));
    assert_eq!(owning_pin_slot(result.as_ref().unwrap_err()), slot);
    assert_eq!(registry_state(&registry).live, before.live + 1);
    drop(result);
    assert_eq!(registry_state(&registry).live, before.live);
}

#[test]
fn with_signal_result_transfers_the_original_box_and_pin_without_cloning() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let registry = current_in_flight_registry_handle();
    let before = registry_state(&registry);
    let flow = signal(
        LispCondition::Error,
        vec![Value::string("macro owned signal")],
    );
    let address = borrowed_payload_address(&flow).unwrap();
    let slot = owning_pin_slot(&flow);
    let result: EvalResult = Err(flow);
    let calls = std::cell::Cell::new(0);
    let forwarded = with_signal_result!({
        calls.set(calls.get() + 1);
        result
    }, payload => {
        assert_eq!(std::ptr::from_ref(&*payload).cast::<()>() as usize, address);
        assert_eq!(payload.pin.pin.as_ref().unwrap().slot, slot);
        assert!(std::sync::Arc::ptr_eq(&payload.pin.pin.as_ref().unwrap().registry.table, &registry.table));
        assert_eq!(registry_state(&registry).live, before.live + 1);
        Err(Flow::signal_boxed(payload))
    });
    assert_eq!(calls.get(), 1);
    let flow = forwarded.expect_err("signal owner forwarded");
    assert_eq!(borrowed_payload_address(&flow), Some(address));
    assert_eq!(owning_pin_slot(&flow), slot);
    assert_eq!(registry_state(&registry).live, before.live + 1);
    drop(flow);
    assert_eq!(registry_state(&registry).live, before.live);
}

#[test]
fn with_signal_result_forwards_throw_thread_blocked_shutdown_and_ok_owners() {
    crate::test_utils::init_test_tracing();
    let _eval = Context::new();
    let registry = current_in_flight_registry_handle();
    for case in 0..4 {
        let before = registry_state(&registry);
        let result: EvalResult = match case {
            0 => Err(Flow::throw(
                Value::symbol("macro-throw"),
                Value::string("macro throw owner"),
            )),
            1 => Err(Flow::thread_blocked(
                Value::string("macro blocker"),
                Value::list(vec![Value::fixnum(19)]),
            )),
            2 => Err(Flow::shutdown(ShutdownRequest {
                exit_code: -123,
                restart: true,
            })),
            _ => Ok(Value::string("macro Ok owner")),
        };
        let expected = format!("{result:?}");
        let address = result.as_ref().err().and_then(borrowed_payload_address);
        let slot = result
            .as_ref()
            .err()
            .filter(|flow| !flow.is_shutdown())
            .map(|flow| owning_pin_slot(flow));
        let held = registry_state(&registry).live;
        let calls = std::cell::Cell::new(0);
        let forwarded = with_signal_result!({
            calls.set(calls.get() + 1);
            result
        }, _signal => panic!("non-signal fallback must not enter the signal body"));
        assert_eq!(calls.get(), 1);
        assert_eq!(format!("{forwarded:?}"), expected);
        assert_eq!(
            registry_state(&registry).live,
            held,
            "fallback must neither clone nor release its pin"
        );
        assert_eq!(
            forwarded.as_ref().err().and_then(borrowed_payload_address),
            address
        );
        assert_eq!(
            forwarded
                .as_ref()
                .err()
                .filter(|flow| !flow.is_shutdown())
                .map(|flow| owning_pin_slot(flow)),
            slot
        );
        if let Some(flow) = forwarded.as_ref().err().filter(|flow| !flow.is_shutdown()) {
            assert!(std::sync::Arc::ptr_eq(
                &owning_pin(flow).registry.table,
                &registry.table
            ));
        }
        drop(forwarded);
        assert_eq!(registry_state(&registry).live, before.live);
    }
}
