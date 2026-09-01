//! Strict combo oracle probes, batch 30: advice flavors and ordering
//! (:before/:after/:around/:override/:filter-args), pcase-let,
//! cl-macrolet/cl-symbol-macrolet, and format-seconds/decode-iso8601.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_g5_advice_flavor_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 (around-in before orig after around-out))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (defun probe-adv-a (x) (push 'orig log) x)
  (let ((before (lambda (&rest _r) (push 'before log)))
        (after (lambda (_r) (push 'after log)))
        (around (lambda (fn x)
                  (push 'around-in log)
                  (prog1 (funcall fn x) (push 'around-out log)))))
    (advice-add 'probe-adv-a :before before)
    (advice-add 'probe-adv-a :after after)
    (advice-add 'probe-adv-a :around around)
    (unwind-protect
        (list (probe-adv-a 5) (nreverse log))
      (advice-remove 'probe-adv-a before)
      (advice-remove 'probe-adv-a after)
      (advice-remove 'probe-adv-a around)
      (fmakunbound 'probe-adv-a))))
"##,
        expect,
    );
}

#[test]
fn div_g5_advice_override_and_filter_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-number-of-arguments (closure (t) (x) x) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defun probe-adv-b (x) x)
  (let ((ov (lambda (x) (list 'overridden x)))
        (fa (lambda (args) (list (car args) 'filtered))))
    (advice-add 'probe-adv-b :override ov)
    (let ((r1 (probe-adv-b 5)))
      (advice-remove 'probe-adv-b ov)
      (advice-add 'probe-adv-b :filter-args fa)
      (let ((r2 (probe-adv-b 5)))
        (advice-remove 'probe-adv-b fa)
        (list r1 r2 (probe-adv-b 5))))))
"##,
        expect,
    );
}

#[test]
fn div_g5_pcase_let_and_macros() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-macrolet)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (pcase-let ((`(,a ,b) '(1 2))) (+ a b))
      (cl-macrolet ((probe-double (x) `(* 2 ,x)))
        (probe-double 5))
      (cl-symbol-macrolet ((probe-sym 42))
        probe-sym))
"##,
        expect,
    );
}

#[test]
fn div_g5_format_seconds_and_iso8601() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function decode-iso8601-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format-seconds "%mm %ss" 125)
      (format-seconds "%h" 7200)
      (format-seconds "%yy %dd" 397)
      (decode-iso8601-string "2025-06-15T12:30:45")
      (decode-iso8601-string "2025-06-15"))
"##,
        expect,
    );
}
