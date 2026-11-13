//! Re-homed C3 import-gate test.
//!
//! The graph-closure predicates are C4 read-only observers and live in value's
//! test-support surface (`symbol_authority::closed_graph`), because an
//! inherent `impl KeeperSymbols` must be defined in the value crate; defining
//! it here hit E0116 (inherent impl for a foreign type). The test body and its
//! assertions are unchanged.
use neovm_value::tagged::gc::mutator_registration::symbol_authority::closed_graph::closed_graph;
use neovm_value::value::Value;

#[test]
fn p3sym_c3_import_gate_checks_transitive_payloads_and_cycles() {
    let mut context = crate::test_utils::new_context();
    let graph = context.eval_str("'(1 (2 . 3))").unwrap();
    assert!(closed_graph([graph], 32));
    assert!(!closed_graph([graph], 1));
    let cycle = context
        .eval_str("(let ((x (cons 1 nil))) (setcdr x x) x)")
        .unwrap();
    assert!(closed_graph([cycle], 32));
    let vector = context.eval_str("'(1 . [2])").unwrap();
    assert!(!closed_graph([vector], 32));
    let table = context.eval_str("(list (make-hash-table))").unwrap();
    assert!(!closed_graph([table], 32));
    assert!(closed_graph([Value::UNBOUND], 1));
}
