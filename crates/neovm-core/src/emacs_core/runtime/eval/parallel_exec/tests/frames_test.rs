//! E4-S0 frame arena, record and root-schema unit tests (plain data only).

use super::super::owner_bindings::{BindingKind, Channel, SlotValue, UndoTarget};
use super::frames::*;
use crate::emacs_core::forward::TlsIndex;
use crate::emacs_core::intern::NIL_SYM_ID;
use crate::emacs_core::value::Value;
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
fn call_depth_limit_refuses_overflow_and_recovers_after_pop() {
    let mut arena = FrameArena::new(1);
    let first = arena
        .push_interp(interp_frame(1, 2, 3, 4))
        .expect("depth one admitted");
    let error = arena
        .push_vm(vm_frame(5, 6, OperandCursor::new(0, 0)))
        .expect_err("depth two refused");
    assert_eq!(error.limit(), 1);
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
    let rooted = arena.roots_mut().push(RootGroup::Misc, Value::fixnum(22));
    assert_eq!(rooted.get().as_fixnum(), Some(22));

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
    assert_eq!(seen.len(), 12, "each root visited exactly once");
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
