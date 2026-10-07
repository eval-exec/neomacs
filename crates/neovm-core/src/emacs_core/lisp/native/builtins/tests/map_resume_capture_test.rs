//! Capture selection is local to each resumed mapping activation. GNU
//! `mapcar1` (src/fns.c) stores callback results before advancing the current
//! tail; completed prefix reads precede the resumed activation's capture.

use super::{MapResultVec, MapSink, map_sequence_length, mapcar1_eval_from};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use crate::tagged::collection_reads::capture;

fn list_cells(mut cursor: Value) -> Vec<Value> {
    let mut cells = Vec::new();
    while cursor.is_cons() {
        cells.push(cursor);
        cursor = cursor.cons_cdr();
    }
    cells
}

#[test]
fn map_resume_capture_observes_every_resumed_cell_and_preserves_result_prefix() {
    crate::test_utils::init_test_tracing();
    let len = 4;
    for collect in [false, true] {
        for split in 0..=len {
            for changed_index in 0..len {
                let mut eval = Context::new();
                let roots = eval.save_vm_roots();
                let sequence = Value::list((0..len as i64).map(Value::fixnum).collect());
                eval.push_vm_frame_root(sequence);
                // Validation and prefix execution must remain outside the
                // capture: otherwise they would mask a missing resumed read.
                assert_eq!(map_sequence_length(sequence).unwrap(), len);
                let cells = list_cells(sequence);
                let base = eval.reserve_vm_frame_root_slots(len);
                let mut collected = MapResultVec::new();
                let mut sink = if collect {
                    MapSink::Collect(&mut collected)
                } else {
                    MapSink::RootSlots(base)
                };
                let mut calls = Vec::new();
                let mut cursor = sequence;
                for index in 0..split {
                    let item = cursor.cons_car().as_fixnum().unwrap();
                    calls.push(item);
                    sink.store(&mut eval, index, Value::fixnum(100 + item));
                    cursor = cursor.cons_cdr();
                }

                let (mapped, reads) = capture(|| {
                    mapcar1_eval_from(&mut eval, len, sink, sequence, cursor, split, |_, item| {
                        let item = item.as_fixnum().unwrap();
                        assert_eq!(item, calls.len() as i64);
                        calls.push(item);
                        Ok(Value::fixnum(100 + item))
                    })
                });
                assert_eq!(mapped.unwrap(), len);
                assert_eq!(calls, (0..len as i64).collect::<Vec<_>>());
                let expected: Vec<_> = (0..len as i64)
                    .map(|index| Value::fixnum(100 + index))
                    .collect();
                if collect {
                    assert_eq!(collected.as_slice(), expected.as_slice());
                } else {
                    assert_eq!(eval.vm_frame_root_slots(base, len), expected.as_slice());
                }
                let reads = reads.expect("read-only resumed mapping has coherent reads");
                assert!(reads.unchanged());
                cells[changed_index].set_car(Value::fixnum(999));
                assert_eq!(
                    reads.unchanged(),
                    changed_index < split,
                    "collect={collect}, split={split}, changed cell={changed_index}"
                );
                eval.restore_vm_roots(roots);
            }
        }
    }
}

#[test]
fn map_resume_capture_nested_callbacks_keep_dependencies_and_absolute_indices() {
    crate::test_utils::init_test_tracing();
    let len = 4;
    for split in 0..=len {
        let mut eval = Context::new();
        let roots = eval.save_vm_roots();
        let sources: Vec<_> = (0..len as i64)
            .map(|index| Value::cons(Value::fixnum(11 + index), Value::NIL))
            .collect();
        let sequence = Value::list(sources.clone());
        eval.push_vm_frame_root(sequence);
        assert_eq!(map_sequence_length(sequence).unwrap(), len);
        let cells = list_cells(sequence);
        let base = eval.reserve_vm_frame_root_slots(len);
        let mut sink = MapSink::RootSlots(base);
        let mut cursor = sequence;
        let mut calls = Vec::new();
        for index in 0..split {
            let item = cursor.cons_car();
            calls.push(item);
            let value = item.cons_car().as_fixnum().unwrap();
            sink.store(&mut eval, index, Value::fixnum(value + index as i64 * 100));
            cursor = cursor.cons_cdr();
        }
        let mut callback_reads = Vec::new();
        assert!(!crate::tagged::collection_reads::is_active());
        let mapped = mapcar1_eval_from(&mut eval, len, sink, sequence, cursor, split, |_, item| {
            let index = calls.len();
            assert_eq!(item, sources[index]);
            calls.push(item);
            // Only the callback's ordinary accessor runs inside this
            // nested capture; the inactive traversal resumes afterward.
            let (value, reads) = capture(|| item.cons_car());
            callback_reads.push((index, reads.expect("pure callback has coherent reads")));
            Ok(Value::fixnum(
                value.as_fixnum().unwrap() + index as i64 * 100,
            ))
        })
        .unwrap();
        assert_eq!(mapped, len);
        assert_eq!(calls, sources);
        let expected: Vec<_> = (0..len as i64)
            .map(|index| Value::fixnum(11 + index + index * 100))
            .collect();
        assert_eq!(eval.vm_frame_root_slots(base, mapped), expected.as_slice());
        assert!(!crate::tagged::collection_reads::is_active());
        assert_eq!(callback_reads.len(), len - split);
        for source in &sources[..split] {
            source.set_car(Value::T);
        }
        for cell in cells {
            cell.set_car(Value::NIL);
        }
        for (_, reads) in &callback_reads {
            assert!(
                reads.unchanged(),
                "prefix and traversal are outside callback captures"
            );
        }
        for (index, reads) in callback_reads {
            sources[index].set_car(Value::T);
            assert!(!reads.unchanged(), "split={split}, callback index={index}");
        }
        eval.restore_vm_roots(roots);
    }
}
