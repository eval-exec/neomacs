//! Strict combo oracle probes, batch 242: generators (iter). iter-lambda,
//! iter-yield, iter-next, iter-close, and iter-do iteration loop. Neomacs may
//! not support generators -- errors caught defensively so a divergence shows as
//! a caught-error difference.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_iter_lambda_yield_next_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'generator)
(condition-case err
    (let ((gen (funcall (iter-lambda () (iter-yield 1) (iter-yield 2) (iter-yield 3)))))
      (list (iter-next gen)
            (iter-next gen)
            (iter-next gen)
            (condition-case nil (iter-next gen) (iter-end-of-sequence 'ended) (error 'caught))))
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""OK (1 2 3 ended)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_iter_do_loop_collect_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'generator)
(condition-case err
    (let ((gen (funcall (iter-lambda () (dotimes (i 5) (iter-yield (* i i)))))))
      (let ((collected nil))
        (iter-do (v gen) (push v collected))
        (nreverse collected)))
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""OK (0 1 4 9 16)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_iter_close_and_exhaustion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'generator)
(condition-case err
    (let* ((closed nil)
           (gen (funcall (iter-lambda ()
                           (unwind-protect
                               (iter-yield 'first)
                             (setq closed t)))))))
      (let ((first (iter-next gen)))
        (iter-close gen)
        (list first closed)))
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable err)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
