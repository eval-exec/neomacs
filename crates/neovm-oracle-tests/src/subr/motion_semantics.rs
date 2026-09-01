//! Oracle parity tests for GNU `subr.el' movement helpers.
//!
//! These functions are Lisp wrappers over syntax-table and word-motion
//! primitives.  Their return values and edge behavior are visible to Elisp.

use crate::common::{
    assert_ok_eq, eval_oracle_and_neovm, return_if_neovm_enable_oracle_proptest_not_set,
};

#[test]
fn oracle_subr_motion_helpers_pin_syntax_and_edge_returns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (with-temp-buffer
   (insert "  alpha_beta\n\t gamma")
   (goto-char 1)
   (list (forward-whitespace 1) (point)
         (forward-symbol 1) (point)
         (forward-whitespace 1) (point)
         (forward-symbol 1) (point)))
 (with-temp-buffer
   (insert "  alpha_beta\n\t gamma")
   (goto-char (point-max))
   (list (forward-symbol -1) (point)
         (forward-whitespace -1) (point)
         (forward-symbol -1) (point)
         (forward-whitespace -1) (point)))
 (with-temp-buffer
   (insert "abc---  zz")
   (goto-char 1)
   (list (forward-same-syntax) (point)
         (forward-same-syntax) (point)
         (forward-same-syntax) (point)
         (forward-same-syntax -1) (point)))
 (with-temp-buffer
   (insert "foo bar")
   (goto-char 1)
   (list (forward-word-strictly 1) (point)
         (backward-word-strictly 1) (point)
         (forward-word-strictly 5) (point))))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 3 13 13 14 14 21 21) (nil 16 nil 14 nil 3 nil 1) (nil 4 nil 7 nil 9 nil 7) (t 4 t 1 nil 8))""#
    ]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(
        "((3 3 13 13 14 14 21 21) (nil 16 nil 14 nil 3 nil 1) (nil 4 nil 7 nil 9 nil 7) (t 4 t 1 nil 8))",
        &oracle,
        &neovm,
    );
}
