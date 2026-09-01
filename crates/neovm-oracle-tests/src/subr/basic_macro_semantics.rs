//! Oracle parity tests for small GNU `subr.el` basic macros.
//!
//! GNU defines `1value` in `lisp/subr.el` as a macro that returns its FORM
//! unchanged.  Its arity and macro predicate behavior matter for Testcover and
//! libraries that inspect macro definitions.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_1value_macro_expands_to_single_form_unchanged() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (fboundp '1value)
 (macrop '1value)
 (macroexpand '(1value (+ 1 2)))
 (eval '(1value (+ 1 2)) t)
 (let ((cell (list 'x)))
   (eq (eval '(1value cell) t) cell))
 (condition-case err
     (macroexpand '(1value))
   (error (list (car err) (cdr err))))
 (condition-case err
     (macroexpand '(1value 1 2))
   (error (list (car err) (cdr err)))))"#;

    let expect = expect_test::expect![[r#""ERR (void-variable cell)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
