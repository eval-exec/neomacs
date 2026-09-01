//! Generator (iter) divergences (generator.el CPS vs neovm-core).
//!
//! Confirmed entry point: generator end-of-sequence signaling and iterator
//! protocol details. Probes iter-yield/iter-next variants, iter-do, iter-close,
//! iter-defun, yield-from, cleanup-on-close, and repeated-next-past-end.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_agen_basic_yield_next_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 (:eos . :done))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'generator)
  (let ((g (funcall (iter-lambda () (iter-yield 1) (iter-yield 2) :done))))
    (list (iter-next g) (iter-next g)
          (condition-case e (iter-next g)
            (iter-end-of-sequence (cons :eos (cdr e)))
            (error (cons :err (car e)))))))
"##,
        expect,
    );
}

#[test]
fn div_agen_iter_next_explicit_end_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 (:eos . :eof))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'generator)
  (let ((g (funcall (iter-lambda () (iter-yield 1) (iter-yield 2)))))
    (list (iter-next g) (iter-next g)
          (condition-case e (iter-next g :eof)
            (iter-end-of-sequence (cons :eos (cdr e)))
            (error (cons :err (car e)))))))
"##,
        expect,
    );
}

#[test]
fn div_agen_iter_do_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (t) nil (let (cps-current-state-5 cps-current-value-4 cps-state-terminal-6 cps-state-iter-yield-7 cps-state-atom-8 cps-state-iter-yield-9 cps-state-atom-10 cps-state-iter-yield-11 cps-state-atom-12) (setq cps-state-terminal-6 #'(lambda nil (signal 'iter-end-of-sequence cps-current-value-4))) (setq cps-state-iter-yield-7 #'(lambda nil (progn (setq cps-current-state-5 cps-state-terminal-6) (throw 'cps--yield cps-current-value-4)))) (setq cps-state-atom-8 #'(lambda nil (setq cps-current-value-4 (prog1 3 (setq cps-current-state-5 cps-state-iter-yield-7))))) (setq cps-state-iter-yield-9 #'(lambda nil (progn (setq cps-current-state-5 cps-state-atom-8) (throw 'cps--yield cps-current-value-4)))) (setq cps-state-atom-10 #'(lambda nil (setq cps-current-value-4 (prog1 2 (setq cps-current-state-5 cps-state-iter-yield-9))))) (setq cps-state-iter-yield-11 #'(lambda nil (progn (setq cps-current-state-5 cps-state-atom-10) (throw 'cps--yield cps-current-value-4)))) (setq cps-state-atom-12 #'(lambda nil (setq cps-current-value-4 (prog1 1 (setq cps-current-state-5 cps-state-iter-yield-11))))) (setq cps-current-state-5 cps-state-atom-12) (let ((iterator #'(lambda (op value) (cond ((eq op :close) (progn (setq cps-current-state-5 cps-state-terminal-6) (setq cps-current-value-4 nil))) ((eq op :next) (setq cps-current-value-4 value) (let ((yielded nil)) (unwind-protect (prog1 (catch 'cps--yield (while t (funcall cps-current-state-5))) (setq yielded t)) (if yielded nil (progn (setq cps-current-state-5 cps-state-terminal-6) (setq cps-current-value-4 nil)))))) (t (error \"Unknown iterator operation %S\" op)))))) nil iterator))) 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'generator)
  (let (acc)
    (iter-do (x (iter-lambda () (iter-yield 1) (iter-yield 2) (iter-yield 3)))
      (push x acc))
    (nreverse acc)))
"##,
        expect,
    );
}

#[test]
fn div_agen_iter_close() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :closed""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'generator)
  (let ((g (funcall (iter-lambda () (iter-yield 1) (iter-yield 2)))))
    (iter-next g)
    (iter-close g)
    :closed))
"##,
        expect,
    );
}

#[test]
fn div_agen_iter_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'generator)
  (iter-defun neo-igen (n) (dotimes (i n) (iter-yield i)))
  (let ((g (neo-igen 4)))
    (list (iter-next g) (iter-next g) (iter-next g) (iter-next g))))
"##,
        expect,
    );
}

#[test]
fn div_agen_infinite_generator_external_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'generator)
  (let ((g (funcall (iter-lambda () (let ((i 0)) (while t (iter-yield (setq i (1+ i)))))))))
    (list (iter-next g) (iter-next g) (iter-next g))))
"##,
        expect,
    );
}

#[test]
fn div_agen_cleanup_on_close() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :ran""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'generator)
  (let (cleaned)
    (let ((g (funcall (iter-lambda () (unwind-protect (iter-yield 1) (setq cleaned :ran))))))
      (iter-next g)
      (iter-close g))
    cleaned))
"##,
        expect,
    );
}

#[test]
fn div_agen_repeated_next_past_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:eos1 :eos2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'generator)
  (let ((g (funcall (iter-lambda () (iter-yield 1)))))
    (iter-next g)
    (list (condition-case e (iter-next g) (iter-end-of-sequence :eos1) (error :other1))
          (condition-case e (iter-next g) (iter-end-of-sequence :eos2) (error :other2)))))
"##,
        expect,
    );
}

#[test]
fn div_agen_yield_from_delegation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'generator)
  (let ((g (funcall (iter-lambda ()
                      (iter-yield-from (funcall (iter-lambda () (iter-yield 1) (iter-yield 2))))
                      (iter-yield 3)))))
    (list (iter-next g) (iter-next g) (iter-next g))))
"##,
        expect,
    );
}

#[test]
fn div_agen_generator_final_value_then_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:a (:eos . :final))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'generator)
  (let ((g (funcall (iter-lambda () (iter-yield :a) :final))))
    (list (iter-next g)
          (condition-case e (iter-next g)
            (iter-end-of-sequence (cons :eos (cdr e)))
            (error (cons :err (car e)))))))
"##,
        expect,
    );
}

#[test]
fn div_agen_iter_next_end_of_sequence_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'generator)
  (let ((g (funcall (iter-lambda () (iter-yield 1))))
    (iter-next g)
    (iter-next g (lambda () :custom-eos))))
"##,
        expect,
    );
}
