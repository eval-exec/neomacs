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

/// P2.3 O1: an observation call beneath two recursive, fib-shaped compiled
/// levels must see each activation's symbol and its original arguments.
#[test]
fn oracle_inline_o1_mapbacktrace_two_recursive_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::inlined_call_semantics::check(
        r#"(progn
  (defalias 'inline-o1-observe
    (lambda ()
      (let (frames)
        (mapbacktrace
         (lambda (evald f args flags)
           (when (memq f '(inline-o1-observe inline-o1-fib inline-o1-top))
             (push (list evald f args flags) frames))))
        (nreverse frames))))
  (defalias 'inline-o1-fib
    (byte-compile (lambda (n observe)
      (if (< n 2)
          (if observe (inline-o1-observe) n)
        (if observe
            (inline-o1-fib (1- n) observe)
          (+ (inline-o1-fib (1- n) nil) (inline-o1-fib (- n 2) nil)))))))
  (defalias 'inline-o1-top (byte-compile (lambda (n flag) (inline-o1-fib n flag))))
  (dotimes (_ 8) (inline-o1-top 4 nil))
  (inline-o1-top 3 t))"#,
        expect_test::expect![[
            r#""OK ((t inline-o1-observe nil nil) (t inline-o1-fib (1 t) nil) (t inline-o1-fib (2 t) nil) (t inline-o1-fib (3 t) nil) (t inline-o1-top (3 t) nil))""#
        ]],
    );
}

/// P2.3 O2: BASE starts from the nearest recursive activation, then counts
/// into its callers. The frame function is the called symbol, including
/// when BASE itself is an alias of that function.
#[test]
fn oracle_inline_o2_backtrace_base_finds_nearest_inline_activation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::inlined_call_semantics::check(
        r#"(progn
  (defalias 'inline-o2-observe
    (lambda ()
      (list (backtrace-frame 0 'inline-o2-inner)
            (backtrace-frame 1 'inline-o2-inner)
            (backtrace-frame 2 'inline-o2-inner)
            (backtrace-frame 0 'inline-o2-alias)
            (backtrace-frame 0 'inline-o2-top))))
  (defalias 'inline-o2-inner
    (byte-compile (lambda (n flag)
      (if (= n 0)
          (if flag (inline-o2-observe) 0)
        (inline-o2-inner (1- n) flag)))))
  (defalias 'inline-o2-alias 'inline-o2-inner)
  (defalias 'inline-o2-top (byte-compile (lambda (n flag) (inline-o2-inner n flag))))
  (dotimes (_ 8) (inline-o2-top 2 nil))
  (inline-o2-top 2 t))"#,
        expect_test::expect![[
            r#""OK ((t inline-o2-inner 0 t) (t inline-o2-inner 1 t) (t inline-o2-inner 2 t) (t inline-o2-inner 0 t) (t inline-o2-top 2 t))""#
        ]],
    );
}
