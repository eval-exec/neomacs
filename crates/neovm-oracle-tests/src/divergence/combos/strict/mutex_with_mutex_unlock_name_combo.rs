//! Strict combo oracle probes, batch 337: mutex / with-mutex deep.
//! make-mutex, with-mutex acquire/release, mutex-name, mutex-owner,
//! and condition-variable-signal without wait.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_mutex_with_mutex_acquire_release_owner() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(condition-case err
    (let ((m (make-mutex "probe-mutex")))
      (list (mutexp m)
            (mutex-name m)
            (with-mutex m
              (list 'inside (eq (mutex-owner m) (current-thread))))
            (mutex-lock m)
            (mutex-unlock m)
            'done))
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""OK (caught void-function)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_condition_variable_signal_notify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(condition-case err
    (let* ((m (make-mutex "probe-cv-mutex"))
           (cv (make-condition-variable m "probe-cv")))
      (list (condition-variable-p cv)
            (condition-variable-name cv)
            (with-mutex m
              (condition-variable-signal cv)
              'signaled)
            'after))
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""OK (caught void-function)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_mutex_unlock_without_lock_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(condition-case err
    (let ((m (make-mutex "probe-unlock-err")))
      (mutex-unlock m)
      'unexpected-success)
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""OK (caught error)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
