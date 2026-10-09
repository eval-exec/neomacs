//! Snapshots between GNU `mapcar1` callbacks retain the original length,
//! completed result slots and the next tail. In particular, mutation during
//! a callback changes the tail read AFTER that callback, and must not cause a
//! second proper-list prewalk on resume.

use super::{MapCallee, MapSink, map_sequence_length, mapcar1_eval, mapcar1_eval_from};
use crate::emacs_core::error::{Flow, FlowKind, FlowResultExt};
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::Value;

#[derive(Clone, Copy, Debug)]
enum Mutation {
    None,
    Shorten,
    Dotted,
    Circular,
    DisconnectHead,
}

#[derive(Debug, PartialEq, Eq)]
struct MappingOutcome {
    mapped: usize,
    calls: Vec<i64>,
    results: String,
}

/// Recreate a snapshot after exactly `split` callbacks, or at the earlier
/// normal stop if mutation shortened the list. Prefix execution deliberately
/// uses GNU's source order independently of the resumable loop.
fn list_snapshot(split: usize, mutation: Mutation, collect: bool, stress: bool) -> MappingOutcome {
    let mut eval = Context::new();
    eval.gc_stress = stress;
    let roots = eval.save_vm_roots();
    let sequence = Value::list((0..6).map(Value::fixnum).collect());
    eval.push_vm_frame_root(sequence);
    let len = map_sequence_length(sequence).expect("validated original length");
    let changed = sequence.cons_cdr().cons_cdr();
    let circle_target = sequence.cons_cdr();
    // Both mutation targets stay reachable from sequence until consumed at
    // index 2. They are never read afterward. DisconnectHead leaves later
    // cursors reachable ONLY through the loop's reusable cursor slot.
    let base = eval.reserve_vm_frame_root_slots(len);
    let mut calls = Vec::new();
    let mut callback = |eval: &mut Context, item: Value| {
        let index = calls.len();
        calls.push(item.as_fixnum().expect("list element"));
        if index == 2 {
            match mutation {
                Mutation::None => {}
                Mutation::Shorten => changed.set_cdr(Value::NIL),
                Mutation::Dotted => changed.set_cdr(Value::fixnum(99)),
                Mutation::Circular => changed.set_cdr(circle_target),
                Mutation::DisconnectHead => sequence.set_cdr(Value::NIL),
            }
        }
        // GC after allocation checks both the completed result prefix and
        // a disconnected current tail across subsequent callbacks.
        if stress {
            eval.eval_str("(progn (cons nil nil) (garbage-collect))")
                .expect("stress callback collects");
        }
        Ok(Value::cons(item, Value::fixnum(index as i64)))
    };
    let mut values = if collect {
        MapSink::RootSlots(base)
    } else {
        MapSink::Discard
    };
    let mapped = if split == 0 {
        mapcar1_eval(&mut eval, len, values, sequence, &mut callback)
            .expect("uninterrupted mapping")
    } else {
        let cursor_root = eval.push_vm_frame_root_slot(sequence);
        let mut tail = sequence;
        let mut index = 0;
        while index < split && index < len && tail.is_cons() {
            eval.set_vm_frame_root_slot(cursor_root, tail);
            let value = callback(&mut eval, tail.cons_car()).expect("prefix callback");
            values.store(&mut eval, index, value);
            // This is the snapshot boundary: the result was stored and the
            // cdr was read after all callback side effects.
            tail = tail.cons_cdr();
            index += 1;
        }
        // A stale prefix cursor must not accidentally protect the resumed
        // tail: only the resumed loop's own cursor root should keep it alive.
        eval.set_vm_frame_root_slot(cursor_root, Value::NIL);
        mapcar1_eval_from(&mut eval, len, values, sequence, tail, index, &mut callback)
            .expect("resumed mapping")
    };
    let results = if collect {
        print_value(&Value::list_from_slice(
            eval.vm_frame_root_slots(base, mapped),
        ))
    } else {
        String::new()
    };
    eval.restore_vm_roots(roots);
    MappingOutcome {
        mapped,
        calls,
        results,
    }
}

fn every_list_snapshot_matches(mutation: Mutation, stress: bool) {
    for collect in [false, true] {
        let uninterrupted = list_snapshot(0, mutation, collect, stress);
        for split in 1..=6 {
            assert_eq!(
                list_snapshot(split, mutation, collect, stress),
                uninterrupted,
                "split={split}, mutation={mutation:?}, collect={collect}, stress={stress}"
            );
        }
    }
}

#[test]
fn map_resume_at_every_list_index_matches_uninterrupted_mapping() {
    every_list_snapshot_matches(Mutation::None, false);
}

#[test]
fn map_resume_after_callback_shortens_list_uses_saved_length_and_next_tail() {
    every_list_snapshot_matches(Mutation::Shorten, false);
}

#[test]
fn map_resume_after_callback_creates_dotted_tail_stops_without_revalidation() {
    every_list_snapshot_matches(Mutation::Dotted, false);
}

#[test]
fn map_resume_after_callback_creates_cycle_keeps_original_iteration_bound() {
    every_list_snapshot_matches(Mutation::Circular, false);
}

#[test]
fn map_resume_roots_completed_results_and_disconnected_cursor_under_gc_stress() {
    crate::test_utils::init_test_tracing();
    every_list_snapshot_matches(Mutation::DisconnectHead, true);
}

fn outcome(result: Result<Value, Flow>) -> String {
    let result = result.kinded();
    match result {
        Ok(value) => print_value(&value),
        Err(FlowKind::Signal(sig)) => format!(
            "{} {:?}",
            sig.symbol_name(),
            sig.data.iter().map(print_value).collect::<Vec<_>>()
        ),
        Err(flow) => format!("{flow:?}"),
    }
}

#[test]
fn map_resume_initial_dotted_and_circular_lists_fail_before_any_callback() {
    for circular in [false, true] {
        for mapcar in [false, true] {
            let mut eval = Context::new();
            let roots = eval.save_vm_roots();
            eval.eval_str("(setq map-resume-call-count 0)").unwrap();
            let callback = eval
                .eval_str("(lambda (x) (setq map-resume-call-count (1+ map-resume-call-count)) x)")
                .unwrap();
            eval.push_vm_frame_root(callback);
            let sequence = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
            eval.push_vm_frame_root(sequence);
            sequence.cons_cdr().set_cdr(if circular {
                sequence
            } else {
                Value::fixnum(99)
            });
            let validation = outcome(map_sequence_length(sequence).map(|_| Value::NIL));
            let result = if mapcar {
                super::builtin_mapcar_2(&mut eval, callback, sequence)
            } else {
                super::builtin_mapc_2(&mut eval, callback, sequence)
            };
            assert!(result.is_err(), "initial invalid list must signal");
            assert_eq!(outcome(result), validation);
            assert_eq!(
                eval.eval_str("map-resume-call-count").unwrap(),
                Value::fixnum(0),
                "length prewalk must precede callback entry"
            );
            eval.restore_vm_roots(roots);
        }
    }
}

#[test]
fn map_resume_at_every_indexed_sequence_index_preserves_result_prefix() {
    for source in ["[1 2 3 4]", "\"aλ😀\"", "(bool-vector t nil t nil)", "nil"] {
        let mut eval = Context::new();
        let roots = eval.save_vm_roots();
        let sequence = eval.eval_str(source).unwrap();
        eval.push_vm_frame_root(sequence);
        let len = map_sequence_length(sequence).unwrap();
        let base = eval.reserve_vm_frame_root_slots(len);
        let mapped = mapcar1_eval(
            &mut eval,
            len,
            MapSink::RootSlots(base),
            sequence,
            |_, item| Ok(item),
        )
        .unwrap();
        let expected = print_value(&Value::list_from_slice(
            eval.vm_frame_root_slots(base, mapped),
        ));
        for split in 0..=len {
            let resumed_base = eval.reserve_vm_frame_root_slots(len);
            for index in 0..split {
                let prefix = eval.vm_frame_root_slots(base, len)[index];
                eval.set_vm_frame_root_slot(resumed_base + index, prefix);
            }
            let mapped = mapcar1_eval_from(
                &mut eval,
                len,
                MapSink::RootSlots(resumed_base),
                sequence,
                Value::NIL,
                split,
                |_, item| Ok(item),
            )
            .unwrap();
            assert_eq!(mapped, len, "{source}, split={split}");
            assert_eq!(
                print_value(&Value::list_from_slice(
                    eval.vm_frame_root_slots(resumed_base, mapped)
                )),
                expected,
                "{source}, split={split}"
            );
        }
        eval.restore_vm_roots(roots);
    }
}

fn redefining_callback_snapshot(split: usize) -> String {
    let mut eval = crate::test_utils::runtime_startup_context();
    let roots = eval.save_vm_roots();
    eval.eval_str(
        "(defun map-resume-redefined (x)
           (when (= x 2)
             (fset 'map-resume-redefined (lambda (x) (+ x 100))))
           (+ x 10))",
    )
    .unwrap();
    let func = Value::symbol("map-resume-redefined");
    let sequence = Value::list((0..6).map(Value::fixnum).collect());
    eval.push_vm_frame_root(func);
    eval.push_vm_frame_root(sequence);
    let len = map_sequence_length(sequence).unwrap();
    let base = eval.reserve_vm_frame_root_slots(len);
    let cursor_root = eval.push_vm_frame_root_slot(sequence);
    let callee = MapCallee::resolve(&mut eval, func);
    let mut cursor = sequence;
    for index in 0..split {
        eval.set_vm_frame_root_slot(cursor_root, cursor);
        let value = callee.call(&mut eval, cursor.cons_car()).unwrap();
        eval.set_vm_frame_root_slot(base + index, value);
        cursor = cursor.cons_cdr();
    }
    // A resumed HOF resolves the original designator again; later calls must
    // observe its current cell, including a redefinition by the last prefix
    // callback. The running old callback itself still completed with old code.
    let callee = MapCallee::resolve(&mut eval, func);
    let mapped = mapcar1_eval_from(
        &mut eval,
        len,
        MapSink::RootSlots(base),
        sequence,
        cursor,
        split,
        |eval, item| callee.call(eval, item),
    )
    .unwrap();
    let result = print_value(&Value::list_from_slice(
        eval.vm_frame_root_slots(base, mapped),
    ));
    eval.restore_vm_roots(roots);
    result
}

#[test]
fn map_resume_observes_callback_redefinition_at_every_start_index() {
    let expected = redefining_callback_snapshot(0);
    for split in 1..=6 {
        assert_eq!(
            redefining_callback_snapshot(split),
            expected,
            "split={split}"
        );
    }
}
