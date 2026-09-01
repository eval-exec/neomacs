//! Oracle parity tests for GNU backtrace frame inspection.
//!
//! GNU implements `mapbacktrace` and `backtrace-frame--internal` in
//! `src/eval.c`; `backtrace-frame` and `backtrace-frames` are Lisp wrappers in
//! `lisp/subr.el`.  The shape of evaluated frames is user-visible Elisp data.

use crate::common::{
    assert_ok_eq, eval_oracle_and_neovm, return_if_neovm_enable_oracle_proptest_not_set,
};

#[test]
fn oracle_backtrace_frame_base_counts_from_nearest_activation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defun neomacs--oracle-bt-target (x y)
    (list
     (let ((frame (backtrace-frame 0 'neomacs--oracle-bt-target)))
       (list (car frame) (cadr frame) (cddr frame)))
     (let ((frame (backtrace-frame 1 'neomacs--oracle-bt-target)))
       (list (car frame) (cadr frame)))
     (let ((frames (backtrace-frames 'neomacs--oracle-bt-target)))
       (list (consp frames) (caar frames) (cadar frames)))
     (mapbacktrace (lambda (&rest _frame) nil)
                   'neomacs--oracle-bt-target)))
  (unwind-protect
      (neomacs--oracle-bt-target 3 4)
    (fmakunbound 'neomacs--oracle-bt-target)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t neomacs--oracle-bt-target (3 4)) (nil unwind-protect) (t t neomacs--oracle-bt-target) nil)""#
    ]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(
        "((t neomacs--oracle-bt-target (3 4)) (nil unwind-protect) (t t neomacs--oracle-bt-target) nil)",
        &oracle,
        &neovm,
    );
}
