//! Oracle parity tests for GNU `subr.el` boolean helper contracts.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_xor_ignore_always_gnu_subr_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el defines `xor` as a defsubst that returns the single
    // non-nil object, and `ignore`/`always` as &rest functions that still
    // receive all already-evaluated arguments.
    let form = r#"(list
 (xor nil nil)
 (xor nil :right)
 (xor :left nil)
 (xor :left :right)
 (xor 0 nil)
 (let ((trace nil))
   (list (xor (progn (push 'a trace) nil)
              (progn (push 'b trace) 'right))
         (nreverse trace)))
 (let ((trace nil))
   (list (ignore (push 'a trace)
                 (push 'b trace))
         (nreverse trace)))
 (let ((trace nil))
   (list (always (push 'a trace)
                 (push 'b trace))
         (nreverse trace))))"#;
    let expect = expect_test::expect![
        r#""OK (nil :right :left nil 0 (right (a b)) (nil (a b)) (t (a b)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}
