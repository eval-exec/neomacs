use super::*;
use crate::tagged::collection_reads::capture;

/// Capture checks concern every collection the scan touches, including cells
/// whose car does not contribute to the result. Changing an earlier cell must
/// invalidate a result found farther down the list.
fn assert_all_visited_cells_recorded(scan: impl Fn(Value) -> EvalResult, visited: usize) {
    for index in 0..visited {
        let list = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
        let mut source = list;
        for _ in 0..index {
            source = source.cons_cdr();
        }
        let (result, reads) = capture(|| scan(list));
        assert!(result.is_ok());
        let reads = reads.expect("a pure scan retains a coherent capture");
        assert!(reads.unchanged());
        source.set_car(Value::fixnum(99));
        assert!(!reads.unchanged(), "visited cell {index} must be observed");
    }
}

#[test]
fn collection_scan_active_scope_keeps_every_visited_cell() {
    crate::test_utils::init_test_tracing();
    // Each test is also run with the hoisting knob enabled. These calls must
    // then choose the observing variant because capture is already active.
    let scans: [fn(Value) -> EvalResult; 10] = [
        |list| builtin_memq_values(Value::fixnum(9), list, false),
        |list| builtin_memq_values(Value::symbol("collection-scan-absent"), list, true),
        |list| builtin_member_values(Value::fixnum(9), list, false),
        |list| builtin_memql_values(Value::fixnum(9), list, false),
        builtin_length_value,
        |list| builtin_length_lt(vec![list, Value::fixnum(6)]),
        |list| builtin_length_eq(vec![list, Value::fixnum(6)]),
        |list| builtin_nth_values(Value::fixnum(2), list),
        |list| bytecode_nth_values(Value::fixnum(2), list),
        |list| builtin_delq_values(Value::fixnum(9), list, false),
    ];
    for scan in scans {
        assert_all_visited_cells_recorded(scan, 3);
    }
    assert_all_visited_cells_recorded(|list| builtin_nthcdr_values(Value::fixnum(2), list), 2);
}

#[test]
fn collection_scan_active_scope_keeps_alist_and_entry_dependencies() {
    crate::test_utils::init_test_tracing();
    let scans: [fn(Value) -> EvalResult; 5] = [
        |list| builtin_assq_values(Value::symbol("collection-scan-absent"), list, false),
        |list| builtin_assq_values(Value::symbol("collection-scan-absent"), list, true),
        |list| {
            crate::emacs_core::misc::builtin_rassq_values(
                Value::symbol("collection-scan-absent"),
                list,
                false,
            )
        },
        |list| {
            crate::emacs_core::misc::builtin_rassq_values(
                Value::symbol("collection-scan-absent"),
                list,
                true,
            )
        },
        |list| assoc_values(Value::symbol("collection-scan-absent"), list, false),
    ];
    for scan in scans {
        for index in 0..3 {
            for mutate_entry in [false, true] {
                let entries = [
                    Value::cons(Value::symbol("collection-scan-a"), Value::fixnum(1)),
                    Value::cons(Value::symbol("collection-scan-b"), Value::fixnum(2)),
                    Value::cons(Value::symbol("collection-scan-c"), Value::fixnum(3)),
                ];
                let alist = Value::list(entries.to_vec());
                let mut cell = alist;
                for _ in 0..index {
                    cell = cell.cons_cdr();
                }
                let source = if mutate_entry { entries[index] } else { cell };
                let (result, reads) = capture(|| scan(alist));
                assert!(result.is_ok());
                let reads = reads.expect("alist scan has no writes");
                source.set_cdr(Value::T);
                assert!(!reads.unchanged(), "entry={mutate_entry}, index={index}");
            }
        }
    }
}

#[test]
fn collection_scan_positioned_symbol_matches_keep_prefix_dependencies() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let key = Value::symbol("collection-scan-positioned");
    let positioned = eval
        .tagged_heap
        .alloc_symbol_with_pos(key, Value::fixnum(17));
    let first = Value::cons(Value::fixnum(1), Value::cons(positioned, Value::NIL));
    let (answer, reads) = capture(|| builtin_memq_values(key, first, true));
    assert_eq!(answer.unwrap().bits(), first.cons_cdr().bits());
    first.set_car(Value::fixnum(2));
    assert!(!reads.unwrap().unchanged());

    for reverse in [false, true] {
        let skipped = Value::cons(Value::fixnum(1), Value::fixnum(2));
        let entry = if reverse {
            Value::cons(Value::fixnum(3), positioned)
        } else {
            Value::cons(positioned, Value::fixnum(3))
        };
        let alist = Value::list(vec![skipped, entry]);
        let (answer, reads) = capture(|| {
            if reverse {
                crate::emacs_core::misc::builtin_rassq_values(key, alist, true)
            } else {
                builtin_assq_values(key, alist, true)
            }
        });
        assert_eq!(answer.unwrap().bits(), entry.bits());
        skipped.set_car(Value::T);
        assert!(!reads.unwrap().unchanged());
    }
}

#[test]
fn collection_scan_destructive_traversals_still_journal_writes() {
    crate::test_utils::init_test_tracing();
    let list = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    let (result, reads) = capture(|| builtin_delq_values(Value::fixnum(2), list, false));
    assert!(result.is_ok());
    assert!(reads.is_none(), "delq rewrites a cell after observing it");

    let list = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    let appended = Value::list(vec![Value::fixnum(3)]);
    let (result, reads) = capture(|| builtin_nconc_slice_values(&[list, appended]));
    assert!(result.is_ok());
    assert!(reads.is_none(), "nconc rewrites the observed last cell");
}
