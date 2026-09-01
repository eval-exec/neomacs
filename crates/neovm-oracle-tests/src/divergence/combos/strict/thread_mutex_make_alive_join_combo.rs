//! Strict combo oracle probes, batch 185: threading and mutexes. make-thread /
//! threadp / thread-alive-p, make-mutex / mutexp / mutex-lock / mutex-unlock,
//! condition-variable, and thread-join. Neomacs may lack native thread support
//! (errors caught) -- a divergence is recorded as a failing test per
//! convention.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_thread_make_alive_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((result nil))
  (condition-case err
      (let ((th (make-thread (lambda () (setq result 'ran)) "probe-thread")))
        (list (threadp th)
              (threadp 'not-thread)
              (thread-name th)
              (thread-join th)
              result))
    (error (list 'caught (car err) result))))
"##;
    let expect = expect_test::expect![[r#""OK (t nil \"probe-thread\" ran ran)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_mutex_lock_unlock_condition_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(condition-case err
    (let ((mx (make-mutex "probe-mutex"))
          (cv (make-condition-variable nil "probe-cv")))
      (mutex-lock mx)
      (let ((after-lock (mutexp mx)))
        (mutex-unlock mx)
        (list (mutexp mx)
              after-lock
              (condition-variable-p cv)
              (condition-variable-name cv)
              (mutex-name mx))))
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""OK (caught wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_thread_main_thread_current_signal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(condition-case err
    (let ((main (main-thread)))
      (list (threadp main)
            (eq main (current-thread))
            (thread-alive-p main)
            (eq (current-thread) (current-thread))))
  (error (list 'caught (car err))))
"##;
    let expect = expect_test::expect![[r#""OK (caught void-function)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
