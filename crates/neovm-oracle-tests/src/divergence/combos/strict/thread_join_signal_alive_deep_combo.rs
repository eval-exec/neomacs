//! Strict combo oracle probes, batch 338: thread-join / signal / timeout deep.
//! thread-join returns thread result, thread-signal delivery, main-thread
//! identity, and current-thread uniqueness.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_thread_join_returns_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(condition-case err
    (let ((th (make-thread (lambda () (+ 1 2)) "probe-join-result")))
      (list (threadp th)
            (eq (thread-join th) 3)
            (thread-alive-p th)))
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""OK (caught void-function)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_thread_name_current_main_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(condition-case err
    (let ((th (make-thread (lambda () (thread-name (current-thread))) "probe-named")))
      (list (eq (current-thread) (current-thread))
            (eq (main-thread) (main-thread))
            (threadp (main-thread))
            (thread-join th)))
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""OK (caught void-function)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_thread_error_propagation_alive_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(condition-case err
    (let ((th (make-thread (lambda () (+ 1 2)) "probe-normal")))
      (let ((result (thread-join th)))
        (list (threadp th)
              (or (eq result 3) (null result))
              (not (thread-alive-p th)))))
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""OK (caught void-function)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
