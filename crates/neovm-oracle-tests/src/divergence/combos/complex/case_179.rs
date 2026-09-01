//! Complex combo batch 179 — `timer` / `idle-timer` / `event-loop`
//! interactions with `input-pending-p`, `sit-for`, `accept-process-output`,
//! `redisplay` state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx179_timer_creation_and_cancel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:fired))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (fired)
  (let ((timer (run-with-timer 0 nil (lambda () (push :fired fired)))))
    (sit-for 0.01)
    (cancel-timer timer)
    (list (nreverse fired))))
"##,
        expect,
    );
}

#[test]
fn div_cx179_repeat_timer_fires_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (fired)
  (let ((timer (run-with-timer 0 0.001 (lambda () (push :tick fired)))))
    (sit-for 0.02)
    (cancel-timer timer))
  (list (> (length fired) 1)
        (catch 'bad
          (dolist (x fired t)
            (unless (eq x :tick)
              (throw 'bad nil))))))
"##,
        expect,
    );
}

#[test]
fn div_cx179_idle_timer_does_not_fire_during_busy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (fired)
  (let ((idle (run-with-idle-timer 0.05 nil (lambda () (push :idle fired)))))
    (sit-for 0.01)
    (let ((short fired))
      (sit-for 0.1)
      (cancel-timer idle)
      (list short (nreverse fired)))))
"##,
        expect,
    );
}

#[test]
fn div_cx179_input_pending_p_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((unread-command-events nil))
  (list (input-pending-p)
        (fboundp 'input-pending-p)
        (fboundp 'sit-for)))
"##,
        expect,
    );
}

#[test]
fn div_cx179_sit_for_returns_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (sit-for 0)
      (sit-for 0.001)
      (fboundp 'sit-for))
"##,
        expect,
    );
}

#[test]
fn div_cx179_timer_list_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((before timer-list))
  (let ((timer (run-with-timer 100 nil (lambda () :never))))
    (let ((after-add (length timer-list)))
      (cancel-timer timer)
      (let ((after-cancel (length timer-list)))
        (list (>= after-add (1+ (length before)))
              (<= after-cancel after-add)))))
"##,
        expect,
    );
}

#[test]
fn div_cx179_idle_timer_list_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((before timer-idle-list))
  (let ((idle (run-with-idle-timer 100 nil (lambda () :never))))
    (let ((after-add (length timer-idle-list)))
      (cancel-timer idle)
      (let ((after-cancel (length timer-idle-list)))
        (list (>= after-add (1+ (length before)))
              (<= after-cancel after-add)))))
"##,
        expect,
    );
}

#[test]
fn div_cx179_timer_predicate_and_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((timer (run-with-timer 100 nil (lambda () :never))))
  (list (timerp timer)
        (timer--time timer)
        (timer--repeat-delay timer)
        (timer--function timer))
  (cancel-timer timer))
"##,
        expect,
    );
}

#[test]
fn div_cx179_with_timeout_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'with-timeout)
          (fboundp 'with-timeout-suspend)
          (fboundp 'with-timeout-unsuspend))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx179_timer_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (fired)
  (let ((timer (run-with-timer 0 nil (lambda () (push :fired fired)))))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Timer mega test buffer content")
      (put-text-property 1 6 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (sit-for 0.01)
        (let ((state (list (nreverse fired)
                           (timerp timer)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (cancel-timer timer)
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}
