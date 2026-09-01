//! Divergence tests: timer + idle + time function combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_time_add_subtract_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((t1 (current-time))
        (t2 (time-add t1 100))
        (t3 (time-subtract t2 100)))
  (list (time-equal-p t1 t3)
        (not (time-equal-p t1 t2))
        (time-less-p t1 t2)
        (not (time-less-p t2 t1))
        (> (float-time t2) (float-time t1)))) "#,
        expect,
    );
}

#[test]
fn divergence_format_time_string_specifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"2024-06-15 14:45:30\" \"Saturday June 15\" \"167\" \"24\" t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((time (encode-time 30 45 14 15 6 2024 nil)))
  (list (format-time-string "%Y-%m-%d %H:%M:%S" time)
        (format-time-string "%A %B %d" time)
        (format-time-string "%j" time)
        (format-time-string "%W" time)
        (string= (format-time-string "%Y" time) "2024")
        (string= (format-time-string "%m" time) "06")
        (string= (format-time-string "%d" time) "15"))) "#,
        expect,
    );
}

#[test]
fn divergence_encode_decode_time_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 0 12 25 12 2023 t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((time (encode-time 0 0 12 25 12 2023 nil))
        (decoded (decode-time time)))
  (list (nth 0 decoded)  ;; seconds
        (nth 1 decoded)  ;; minutes
        (nth 2 decoded)  ;; hours
        (nth 3 decoded)  ;; day
        (nth 4 decoded)  ;; month
        (nth 5 decoded)  ;; year
        (= (nth 0 decoded) 0)
        (= (nth 2 decoded) 12)
        (= (nth 3 decoded) 25)
        (= (nth 4 decoded) 12)
        (= (nth 5 decoded) 2023))) "#,
        expect,
    );
}

#[test]
fn divergence_float_time_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((now (current-time))
        (ft (float-time now))
        (back (seconds-to-time ft)))
  (list (floatp ft)
        (> ft 0)
        (time-equal-p now back)
        (<= (abs (- (float-time now) (float-time back))) 0.001))) "#,
        expect,
    );
}

#[test]
fn divergence_timer_create_cancel_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((timers nil))
  (dotimes (i 5)
    (push (run-at-time 3600 nil (lambda ())) timers))
  (let ((count (length timers)))
    (dolist (t timers) (cancel-timer t))
    (list count (= count 5)))) "#,
        expect,
    );
}

#[test]
fn divergence_idle_timer_cancel() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((timer (run-with-idle-timer 600 t (lambda ()))))
  (let ((result (timerp timer)))
    (cancel-timer timer)
    (list result (not (timerp timer))))) "#,
        expect,
    );
}

#[test]
fn divergence_with_timeout_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (with-timeout (1 'timed-out) (+ 1 2))
  (= (with-timeout (1 'timed-out) (+ 1 2)) 3)) "#,
        expect,
    );
}

#[test]
fn divergence_time_less_p_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((t1 '(26154 32144 123456 789000))
        (t2 '(26154 32144 123456 789001))
        (t3 '(26154 32145 0 0)))
  (list (time-less-p t1 t2)
        (time-less-p t2 t3)
        (not (time-less-p t2 t1))
        (time-less-p t1 t3)
        (not (time-less-p t3 t1)))) "#,
        expect,
    );
}

#[test]
fn divergence_current_time_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 20 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ts (current-time-string)))
  (list (stringp ts)
        (= (length ts) 24)
        (string-match "20[0-9][0-9]" ts)
        (> (string-match "20[0-9][0-9]" ts) 0))) "#,
        expect,
    );
}

#[test]
fn divergence_format_seconds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"0 years 0 days 1 hour:1 minute:1 second\" \"1 hour:1 minute:1 second\" \"2 minutes:5 seconds\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format-seconds "%Y %D %H:%M:%S" (* 3661 1.0))
  (format-seconds "%H:%M:%S" 3661)
  (format-seconds "%M:%S" 125)
  (= (string-to-number (format-seconds "%S" 45)) 45)) "#,
        expect,
    );
}
