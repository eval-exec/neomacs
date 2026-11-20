//! E4-S0 frame arena, record and root-schema unit tests (plain data only).

use super::super::owner_bindings::{BindingKind, Channel, SlotValue, UndoTarget};
use super::frames::*;
use crate::emacs_core::eval::Context;
use crate::test_utils::unit_runtime;
use neovm_value::forward::TlsIndex;
use neovm_value::intern::NIL_SYM_ID;
use neovm_value::tagged::gc::RendezvousError;
use neovm_value::tagged::transport::SharedRoot;
use neovm_value::value::Value;
use std::num::NonZeroU32;

fn budget() -> PollBudget {
    PollBudget::new(NonZeroU32::new(8).expect("nonzero poll interval"))
}

fn interp_frame(a: i64, b: i64, c: i64, d: i64) -> InterpFrame {
    InterpFrame::new(
        Value::fixnum(a),
        Value::fixnum(b),
        Value::fixnum(c),
        Value::fixnum(d),
        Value::NIL,
        1,
        HandlerMark::EMPTY,
        OwnerRecordMark::EMPTY,
        budget(),
    )
}

fn vm_frame(function: i64, code: i64, cursor: OperandCursor) -> VmFrame {
    VmFrame::new(
        Value::fixnum(function),
        Value::fixnum(code),
        Pc::new(0),
        cursor,
        OwnerRecordMark::EMPTY,
        HandlerMark::EMPTY,
        budget(),
    )
}

fn handler_record(frame: FrameId, tag: i64, handlers: i64) -> HandlerRecord {
    HandlerRecord::new(
        HandlerKind::ConditionCase,
        Value::fixnum(tag),
        Value::fixnum(handlers),
        frame,
        OwnerRecordMark::EMPTY,
    )
}

fn slot_record(saved: i64) -> SlotRecord {
    SlotRecord::new(
        NIL_SYM_ID,
        TlsIndex::VOID,
        Channel::Default,
        BindingKind::Let,
        UndoTarget::Previous(SlotValue::new(Value::fixnum(saved))),
        SlotGeneration::new(NonZeroU32::MIN),
    )
}

#[test]
fn arena_pushes_pops_and_refuses_stale_handles() {
    let mut arena = FrameArena::new(8);
    let interp = arena
        .push_interp(interp_frame(1, 2, 3, 4))
        .expect("first frame");
    let vm = arena
        .push_vm(vm_frame(5, 6, OperandCursor::new(0, 0)))
        .expect("second frame");
    assert_eq!(arena.depth(), 2);
    assert_ne!(interp.generation(), vm.generation());

    // The VM frame is the top; the interp handle is not yet poppable.
    assert!(arena.pop(interp).is_none());
    assert!(arena.pop(vm).is_some());
    assert!(arena.pop(interp).is_some());
    assert_eq!(arena.depth(), 0);

    // A handle from the previous occupancy cannot name the reused slot.
    let reused = arena
        .push_interp(interp_frame(7, 8, 9, 10))
        .expect("reuse slot");
    assert!(arena.pop(vm).is_none());
    assert!(arena.pop(reused).is_some());
}

#[test]
fn frame_id_generation_exhaustion_refuses_without_wrapping() {
    let mut arena = FrameArena::new(4);
    let first = arena
        .push_interp(interp_frame(1, 2, 3, 4))
        .expect("first frame");
    arena.exhaust_frame_generations_for_test();
    assert_eq!(
        arena.push_vm(vm_frame(5, 6, OperandCursor::new(0, 0))),
        Err(FramePublishError::GenerationExhausted)
    );
    assert_eq!(arena.depth(), 1);
    assert!(arena.pop(first).is_some());
    assert_eq!(arena.depth(), 0);
    assert_eq!(
        arena.push_interp(interp_frame(7, 8, 9, 10)),
        Err(FramePublishError::GenerationExhausted)
    );
    assert_eq!(arena.depth(), 0);
}

#[test]
fn call_depth_limit_refuses_overflow_and_recovers_after_pop() {
    let mut arena = FrameArena::new(1);
    let first = arena
        .push_interp(interp_frame(1, 2, 3, 4))
        .expect("depth one admitted");
    let error = arena
        .push_vm(vm_frame(5, 6, OperandCursor::new(0, 0)))
        .expect_err("depth two refused");
    match error {
        FramePublishError::Depth(depth) => assert_eq!(depth.limit(), 1),
        FramePublishError::GenerationExhausted => {
            panic!("depth refusal reported as generation exhaustion")
        }
    }
    assert!(arena.pop(first).is_some());
    assert!(
        arena
            .push_vm(vm_frame(5, 6, OperandCursor::new(0, 0)))
            .is_ok()
    );
}

#[test]
fn handler_and_bind_marks_truncate_only_above_the_mark() {
    let mut arena = FrameArena::new(4);
    let frame = arena.push_interp(interp_frame(1, 2, 3, 4)).expect("frame");

    let handler_mark = arena.handlers_mut().mark();
    arena.handlers_mut().push(handler_record(frame, 10, 11));
    arena.handlers_mut().push(handler_record(frame, 12, 13));
    assert_eq!(arena.handlers_mut().len(), 2);
    assert_eq!(arena.handlers_mut().truncate(handler_mark), 2);
    assert_eq!(arena.handlers_mut().mark(), handler_mark);

    let bind_mark = arena.bindings_mut().mark();
    arena.bindings_mut().push(slot_record(20));
    assert_eq!(arena.bindings_mut().len(), 1);
    assert_eq!(arena.bindings_mut().truncate(bind_mark), 1);
    assert_eq!(arena.bindings_mut().mark(), bind_mark);
}

#[test]
fn root_walk_publishes_every_frame_record_and_registry_root() {
    let mut arena = FrameArena::new(4);
    let frame = arena
        .push_interp(interp_frame(11, 12, 13, 14))
        .expect("interp frame");
    let operand_start = arena.push_operand(Value::fixnum(15));
    let second = arena.push_operand(Value::fixnum(16));
    assert_eq!(operand_start.index(), 0);
    let vm = VmFrame::new(
        Value::fixnum(17),
        Value::fixnum(18),
        Pc::new(0),
        OperandCursor::new(operand_start.index(), 2),
        OwnerRecordMark::EMPTY,
        HandlerMark::EMPTY,
        budget(),
    );
    arena.push_vm(vm).expect("vm frame");
    arena.handlers_mut().push(handler_record(frame, 19, 20));
    arena.bindings_mut().push(slot_record(21));
    let rooted = arena
        .roots_mut()
        .push(RootGroup::Misc, Value::fixnum(22))
        .expect("registry root");
    assert_eq!(
        arena.roots_mut().lookup(rooted).and_then(Value::as_fixnum),
        Some(22)
    );

    let mut seen = Vec::new();
    arena.trace_roots(&mut |group, value| {
        seen.push((group.name(), value.as_fixnum()));
    });
    assert_eq!(second.index(), 1);
    for expected in [
        ("eval_temp", 11),
        ("eval_temp", 12),
        ("eval_temp", 13),
        ("eval_temp", 14),
        ("vm_frames", 17),
        ("vm_frames", 18),
        ("vm_frames", 15),
        ("vm_frames", 16),
        ("handlers", 19),
        ("handlers", 20),
        ("specpdl", 21),
        ("misc", 22),
    ] {
        assert!(
            seen.iter()
                .any(|entry| *entry == (expected.0, Some(expected.1))),
            "missing root {expected:?} in {seen:?}"
        );
    }
    assert!(
        seen.iter().any(|entry| *entry == ("eval_temp", None)),
        "the progn accumulator is rooted, in {seen:?}"
    );
    assert_eq!(seen.len(), 13, "each root visited exactly once");
}

#[test]
fn operand_segment_refuses_out_of_range_cursors() {
    let mut arena = FrameArena::new(1);
    let first = arena.push_operand(Value::fixnum(1));
    let second = arena.push_operand(Value::fixnum(2));
    assert_eq!(arena.operand(first).and_then(Value::as_fixnum), Some(1));
    assert_eq!(arena.operand(second).and_then(Value::as_fixnum), Some(2));
    let window = arena.operand_segment(OperandCursor::new(1, 1));
    assert_eq!(window.len(), 1);
    assert_eq!(window[0].as_fixnum(), Some(2));
    assert!(arena.operand_segment(OperandCursor::new(2, 4)).is_empty());
}

#[test]
fn frame_kind_enumeration_covers_the_arena_frames() {
    let mut arena = FrameArena::new(2);
    arena
        .push_interp(interp_frame(1, 2, 3, 4))
        .expect("interp frame");
    arena
        .push_vm(vm_frame(5, 6, OperandCursor::new(0, 0)))
        .expect("vm frame");
    let present = arena.frame_kinds().collect::<Vec<_>>();
    assert_eq!(present, FrameKind::ALL.to_vec());
    for kind in FrameKind::ALL {
        assert!(!kind.root_groups().is_empty());
        for group in kind.root_groups() {
            assert!(RootGroup::ALL.contains(group));
        }
    }
}

#[test]
fn poll_budget_fires_once_per_interval() {
    let mut budget = PollBudget::new(NonZeroU32::new(2).expect("nonzero interval"));
    assert!(!budget.consume());
    assert!(budget.consume());
    assert!(!budget.consume());
    assert!(budget.consume());
    assert_eq!(budget.remaining(), 2);
}

#[test]
fn stale_root_handle_is_refused_after_the_slot_is_reused() {
    let mut registry = RootRegistry::new();
    let first = registry
        .push(RootGroup::Bc, Value::fixnum(1))
        .expect("first root");
    let top = registry
        .push(RootGroup::Misc, Value::fixnum(2))
        .expect("top root");
    assert_eq!(registry.lookup(first).and_then(Value::as_fixnum), Some(1));
    assert_eq!(registry.pop(first), Err(RootRevokeError::Stale));
    assert_eq!(registry.lookup(first).and_then(Value::as_fixnum), Some(1));

    assert_eq!(registry.pop(top).expect("pop top").as_fixnum(), Some(2));
    assert!(registry.lookup(top).is_none());

    assert_eq!(registry.truncate(0).expect("clear"), 1);
    assert!(registry.lookup(first).is_none());

    let reused = registry
        .push(RootGroup::JitWindow, Value::fixnum(9))
        .expect("reuse slot");
    assert_eq!(reused.index(), first.index());
    assert_ne!(reused.generation(), first.generation());
    assert!(registry.lookup(first).is_none());
    assert!(registry.lookup(top).is_none());
    assert_eq!(registry.lookup(reused).and_then(Value::as_fixnum), Some(9));
}

#[test]
fn root_handle_generation_exhaustion_refuses_without_wrapping() {
    let mut registry = RootRegistry::new();
    let live = registry
        .push(RootGroup::EvalTemp, Value::fixnum(4))
        .expect("live root");
    registry.exhaust_generations_for_test();
    assert_eq!(
        registry.push(RootGroup::Bc, Value::fixnum(5)),
        Err(RootPublishError::GenerationExhausted)
    );
    assert_eq!(
        registry.truncate(0),
        Err(RootPublishError::GenerationExhausted)
    );
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.lookup(live).and_then(Value::as_fixnum), Some(4));
    assert_eq!(
        registry.pop(live),
        Err(RootRevokeError::GenerationExhausted)
    );
    assert_eq!(registry.lookup(live).and_then(Value::as_fixnum), Some(4));
}

#[test]
fn root_group_scan_visits_each_group_once() {
    let mut registry = RootRegistry::new();
    registry
        .push(RootGroup::EvalTemp, Value::fixnum(1))
        .expect("first eval root");
    registry
        .push(RootGroup::EvalTemp, Value::fixnum(2))
        .expect("second eval root");
    registry
        .push(RootGroup::Misc, Value::fixnum(3))
        .expect("misc root");

    let mut seen = Vec::new();
    registry.trace_root_groups(&mut |group, values| {
        seen.push((group.name(), values.len()));
    });
    assert_eq!(
        seen,
        [
            ("vm_frames", 0),
            ("eval_temp", 2),
            ("bc", 0),
            ("jit_window", 0),
            ("handlers", 0),
            ("specpdl", 0),
            ("misc", 1),
        ]
    );
}

#[test]
fn root_group_names_are_the_canonical_e3_roster() {
    let names = RootGroup::ALL
        .iter()
        .map(|group| group.name())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "vm_frames",
            "eval_temp",
            "bc",
            "jit_window",
            "handlers",
            "specpdl",
            "misc",
        ]
    );
}

fn loaded_arena() -> FrameArena {
    let mut arena = FrameArena::new(4);
    let frame = arena
        .push_interp(interp_frame(11, 12, 13, 14))
        .expect("interp frame");
    arena.push_operand(Value::fixnum(15));
    arena.push_operand(Value::fixnum(16));
    arena
        .push_vm(vm_frame(17, 18, OperandCursor::new(0, 2)))
        .expect("vm frame");
    arena.handlers_mut().push(handler_record(frame, 19, 20));
    arena.bindings_mut().push(slot_record(21));
    arena
        .roots_mut()
        .push(RootGroup::Misc, Value::fixnum(22))
        .expect("registry root");
    arena
}

/// Publish, then hand the batch to the caller. A reserve failure returns
/// before the acknowledgement closure runs.
fn publish_then_acknowledge(
    arena: &FrameArena,
    context: &Context,
    out: &mut Vec<SharedRoot>,
    acknowledge: impl FnOnce(&[SharedRoot]),
) -> Result<(), RendezvousError> {
    arena.publish_scan(context.tagged_heap(), out)?;
    acknowledge(out);
    Ok(())
}

fn shared_fixnum(context: &Context, value: i64) -> SharedRoot {
    // SAFETY: `value` is an immediate. The context thread is the mutator of
    // this heap, and an immediate needs no heap object to stay reachable.
    unsafe { SharedRoot::new(context.tagged_heap(), Value::fixnum(value)) }
}

#[test]
fn publish_scan_of_an_empty_arena_appends_nothing() {
    let context = Context::new(unit_runtime());
    let arena = FrameArena::new(1);
    let mut out = Vec::new();
    arena
        .publish_scan(context.tagged_heap(), &mut out)
        .expect("empty scan");
    assert!(out.is_empty());
}

#[test]
fn publish_scan_copies_every_owner_source_once() {
    let context = Context::new(unit_runtime());
    let arena = loaded_arena();
    let mut out = vec![shared_fixnum(&context, 1)];
    let prefix = out[0].parallel_word();
    let mut acknowledged = false;
    publish_then_acknowledge(&arena, &context, &mut out, |roots| {
        acknowledged = true;
        assert_eq!(roots.len(), 14);
    })
    .expect("scan");
    assert!(acknowledged);
    assert_eq!(out[0].parallel_word(), prefix);
    assert_eq!(out.len(), 14);
    let published: Vec<usize> = out[1..].iter().map(SharedRoot::parallel_word).collect();
    for expected in [11, 12, 13, 14, 17, 18, 15, 16, 19, 20, 21, 22] {
        let word = Value::fixnum(expected).bits();
        assert!(
            published.contains(&word),
            "missing fixnum {expected} in {published:?}"
        );
    }
    assert_eq!(published.len(), 13);
    assert!(
        published.contains(&Value::NIL.bits()),
        "publish_scan carries the progn accumulator"
    );
}

#[test]
fn publish_scan_publishes_the_operand_stack_once() {
    let context = Context::new(unit_runtime());
    let mut arena = FrameArena::new(4);
    arena.push_operand(Value::fixnum(15));
    arena
        .push_vm(vm_frame(1, 2, OperandCursor::new(0, 1)))
        .expect("first vm frame");
    arena
        .push_vm(vm_frame(3, 4, OperandCursor::new(0, 1)))
        .expect("second vm frame");
    let mut out = Vec::new();
    let mut acknowledged = false;
    publish_then_acknowledge(&arena, &context, &mut out, |_| acknowledged = true).expect("scan");
    assert!(acknowledged);
    let operand = Value::fixnum(15).bits();
    assert_eq!(
        out.iter()
            .filter(|root| root.parallel_word() == operand)
            .count(),
        1,
        "a shared operand cursor must not publish the stack twice"
    );
    assert_eq!(out.len(), 5);
}

#[test]
fn publish_scan_reserve_failure_does_not_acknowledge() {
    let context = Context::new(unit_runtime());
    let mut arena = FrameArena::new(2);
    arena.push_interp(interp_frame(1, 2, 3, 4)).expect("frame");
    arena.fail_next_scan_reserve_for_test();
    let mut out = vec![shared_fixnum(&context, 9)];
    let prefix = out[0].parallel_word();
    let mut acknowledged = false;
    let error = publish_then_acknowledge(&arena, &context, &mut out, |_| acknowledged = true)
        .expect_err("reserve failure");
    assert_eq!(error, RendezvousError::Capacity);
    assert!(!acknowledged, "acknowledgement must not run");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].parallel_word(), prefix);
    publish_then_acknowledge(&arena, &context, &mut out, |_| {}).expect("flag cleared");
    assert_eq!(out.len(), 6);
    assert_eq!(
        out.iter()
            .filter(|root| root.parallel_word() == Value::NIL.bits())
            .count(),
        1,
        "publish_scan carries the progn accumulator"
    );
}
