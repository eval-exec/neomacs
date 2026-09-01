//! Oracle parity tests for GNU `generator.el` semantics.
//!
//! GNU implements generators through a CPS transformation in
//! `lisp/emacs-lisp/generator.el`.  These tests cover yielded values,
//! end-of-sequence payloads, sent values, delegation, `iter-do`, and
//! cleanup via `iter-close`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_generator_basic_iteration_and_end_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'generator)
  (iter-defun neomacs-oracle-gen-basic (n)
    (dotimes (i n 'done)
      (iter-yield i)))
  (let ((it (neomacs-oracle-gen-basic 4))
        out)
    (condition-case err
        (while t
          (push (iter-next it) out))
      (iter-end-of-sequence
       (push (list 'end (cdr err)) out)))
    (nreverse out)))
"#;

    let expect = expect_test::expect![[r#""OK (0 1 2 3 (end done))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_generator_sent_values_and_independent_iterators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'generator)
  (iter-defun neomacs-oracle-gen-echo ()
    (let ((first (iter-yield 'ask-first))
          (second (iter-yield 'ask-second)))
      (list first second)))
  (let ((a (neomacs-oracle-gen-echo))
        (b (neomacs-oracle-gen-echo)))
    (list
     (iter-next a)
     (iter-next a 'alpha)
     (condition-case err
         (iter-next a 'beta)
       (iter-end-of-sequence (list 'end-a (cdr err))))
     (iter-next b)
     (iter-next b 'one)
     (condition-case err
         (iter-next b 'two)
       (iter-end-of-sequence (list 'end-b (cdr err)))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (ask-first ask-second (end-a (alpha beta)) ask-first ask-second (end-b (one two)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_generator_yield_from_and_iter_do_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'generator)
  (iter-defun neomacs-oracle-gen-child ()
    (iter-yield 'child-1)
    (iter-yield 'child-2)
    'child-done)
  (iter-defun neomacs-oracle-gen-parent ()
    (iter-yield 'parent-start)
    (let ((child-result (iter-yield-from (neomacs-oracle-gen-child))))
      (iter-yield (list 'child-result child-result)))
    'parent-done)
  (let ((it (neomacs-oracle-gen-parent))
        values)
    (let ((done (iter-do (value it)
                  (push value values))))
      (list (nreverse values) done))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((parent-start child-1 child-2 (child-result child-done)) parent-done)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_generator_close_runs_cleanup_and_then_ends() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'generator)
  (let ((events nil))
    (iter-defun neomacs-oracle-gen-cleanup ()
      (unwind-protect
          (progn
            (push 'entered events)
            (iter-yield 'first)
            (push 'after-first events)
            (iter-yield 'second)
            'finished)
        (push 'cleanup events)))
    (let ((it (neomacs-oracle-gen-cleanup)))
      (list
       (iter-next it)
       events
       (iter-close it)
       events
       (condition-case err
           (iter-next it)
         (iter-end-of-sequence (list 'end (cdr err))))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (first (entered) (cleanup entered) (cleanup entered) (end nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
