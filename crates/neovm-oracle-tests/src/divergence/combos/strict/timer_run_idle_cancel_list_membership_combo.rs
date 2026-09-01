//! Strict combo oracle probes, batch 174: timer object API (deterministic
//! parts only). We avoid printing raw timer objects since their vector slots
//! embed absolute wall-clock microseconds and an internal incarnation counter,
//! which differ between engines by construction. Instead we assert boolean
//! membership (consp memq), timerp, repeat-delay, and incarnation, which are
//! deterministic. run-with-timer / run-with-idle-timer, timer-list /
//! timer-idle-list membership, cancel-timer removal.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_timer_run_cancel_list_membership() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((t1 (run-with-timer 600 nil (lambda () (message "x")))))
  (unwind-protect
      (let ((t2 (run-with-idle-timer 600 nil (lambda () nil)))
            (t3 (run-with-timer 600 30 (lambda () nil))))
        (list (timerp t1)
              (timerp t2)
              (timerp 'not-a-timer)
              (timerp 42)
              (consp (memq t1 timer-list))
              (consp (memq t2 timer-idle-list))
              (consp (memq t3 timer-list))
              (timer--repeat-delay t3)
              (timer-incarnation t1)
              (progn (cancel-timer t1) (consp (memq t1 timer-list)))
              (progn (cancel-timer t2) (consp (memq t2 timer-idle-list)))
              (progn (cancel-timer t3) (consp (memq t3 timer-list)))))
    (when (timerp t1) (cancel-timer t1))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function timer-incarnation)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_timer_cancel_async_signal_idletime_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((fired 0)
       (t1 (run-with-timer 600 nil (lambda () (setq fired (1+ fired))))))
  (unwind-protect
      (let ((before-cancel (consp (memq t1 timer-list))))
        (cancel-timer t1)
        (list before-cancel
              (consp (memq t1 timer-list))
              fired
              (numberp (float-time))
              (<= fired 1)))
    (when (timerp t1) (cancel-timer t1))))
"##;
    let expect = expect_test::expect![[r#""OK (t nil 0 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_timer_attributes_repeat_idle_indexed_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((log nil)
       (t1 (run-with-timer 600 60 (lambda () (push 'tick log)))))
  (unwind-protect
      (list (timerp t1)
            (timerp (run-with-idle-timer 600 t (lambda ())))
            (timer--repeat-delay t1)
            (functionp (timer--function t1))
            (eq (timer--repeat-delay t1) 60))
    (when (timerp t1) (cancel-timer t1))))
"##;
    let expect = expect_test::expect![[r#""OK (t t 60 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
