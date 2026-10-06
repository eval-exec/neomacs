//! Runtime constants remain literal data, as in GNU Fmake_byte_code.
use crate::emacs_core::{eval::Context, print::print_value};

#[test]
fn p6_make_byte_code_keeps_hash_literal_shaped_constant_verbatim() {
    let mut context = Context::new();
    let result = context.eval_str(r#"(let* ((constant '(make-hash-table-from-literal '(hash-table))) (constants (vector constant)) (function (make-byte-code 0 "\300\207" constants 1))) (list (eq (funcall function) constant) (equal (aref function 2) constants)))"#).unwrap();
    assert_eq!(print_value(&result), "(t t)");
}

#[test]
fn p6_byte_code_keeps_large_hash_literal_shaped_constant_verbatim() {
    let mut context = Context::new();
    let result = context.eval_str(r#"(byte-code "\300\207" [(make-hash-table-from-literal '(hash-table size 2305843009213693951))] 1)"#).unwrap();
    assert!(result.is_cons(), "constant must remain an ordinary list");
    let expected = context
        .eval_str(r#"'(make-hash-table-from-literal '(hash-table size 2305843009213693951))"#)
        .unwrap();
    let equality = crate::emacs_core::builtins::builtin_equal(vec![result, expected]).unwrap();
    assert!(equality.is_truthy());
}
