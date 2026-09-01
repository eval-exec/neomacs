//! Oracle parity tests for GNU warning-suppression macro semantics.
//!
//! GNU implements `with-demoted-errors` in `lisp/subr.el` and the compiler
//! warning wrappers in `lisp/emacs-lisp/byte-run.el`.  These forms have small
//! interpreter-visible edge semantics beyond their byte-compiler purpose:
//! missing and non-string demotion formats are rewritten through GNU's
//! `macroexp-warn-and-return` path, while warning wrappers still evaluate like
//! ordinary progn/last-value forms when interpreted.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_with_demoted_errors_macro_rewrite_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (macroexpand '(with-demoted-errors (error "missing-format")))
 (macroexpand '(with-demoted-errors nil (error "nil-format")))
 (macroexpand '(with-demoted-errors))
 (list
  (with-demoted-errors (error "missing-format"))
  (with-demoted-errors nil (error "nil-format"))
  (with-demoted-errors)))
"#;

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_warning_wrapper_interpreter_values_and_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil))
  (list
   (with-no-warnings
    (push 'a log)
    (push 'b log)
    (copy-sequence log))
   log
   (with-suppressed-warnings ((obsolete old-fun)
                              (free-vars maybe-free))
     (push 'c log)
     (push 'd log)
     (copy-sequence log))
   log
   (macroexpand '(with-suppressed-warnings ((obsolete old-fun))
                   (list 1 2)
                   (+ 3 4)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((b a) (b a) (d c b a) (d c b a) (progn (list 1 2) (+ 3 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
