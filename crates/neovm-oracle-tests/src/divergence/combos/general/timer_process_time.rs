//! Divergence tests: timer + process + buffer + callback combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_timer_idle_create_cancel() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 9 29)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-timer-result-xxx nil)
  (let ((timer (run-with-idle-timer 1 nil
                  (lambda () (setq test-timer-result-xxx 'fired)))))
    (list (timerp timer)
          (null test-timer-result-xxx)
          (cancel-timer timer)
          (null test-timer-result-xxx)
          (timerp timer)))) #"#,
        expect,
    );
}

#[test]
fn divergence_current_time_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 11 51)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((time (current-time)))
    (list (listp time)
          (>= (length time) 2)
          (integerp (car time))
          (>= (car time) 0)
          (format-time-string "%Y-%m-%d" time)
          (stringp (format-time-string "%Y-%m-%d" time))
          (= (length (format-time-string "%Y-%m-%d" time)) 10)
          (time-equal-p time time)
          (<= (float-time) (+ (float-time) 1))))) #"#,
        expect,
    );
}

#[test]
fn divergence_time_subtract_add() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 10 56)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let* ((t1 (current-time))
         (t2 (time-add t1 60)))
    (list (time-less-p t1 t2)
          (null (time-less-p t2 t1))
          (float-time (time-subtract t2 t1))
          (>= (float-time (time-subtract t2 t1)) 59)
          (<= (float-time (time-subtract t2 t1)) 61)
          (time-equal-p (time-add t1 0) t1)
          (= (float-time (time-subtract t2 t1)) 60)))) #"#,
        expect,
    );
}

#[test]
fn divergence_process_connection_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 9 47)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((proc (start-process "test-conn-xxx" nil "echo" "test")))
    (set-process-query-on-exit-flag proc nil)
    (while (process-live-p proc) (accept-process-output proc 1))
    (list (processp proc)
          (memq (process-type proc) '(real network serial))
          (eq (process-status proc) 'exit)
          (null (process-live-p proc))
          (= (process-exit-status proc) 0)))) #"#,
        expect,
    );
}

#[test]
fn divergence_encode_time_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 16 37)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let* ((encoded (encode-time 30 15 12 1 1 2024 nil))
         (decoded (decode-time encoded)))
    (list (nth 0 decoded)
          (= (nth 0 decoded) 30)
          (nth 1 decoded)
          (= (nth 1 decoded) 15)
          (nth 2 decoded)
          (= (nth 2 decoded) 12)
          (nth 3 decoded)
          (= (nth 3 decoded) 1)
          (nth 4 decoded)
          (= (nth 4 decoded) 1)
          (nth 5 decoded)
          (= (nth 5 decoded) 2024)
          (= (length decoded) 9)))) #"#,
        expect,
    );
}

#[test]
fn divergence_process_buffer_live() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 15 50)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " test-pbl-xxx")))
    (with-current-buffer buf
      (let ((proc (start-process "test-pbl-xxx" buf "echo" "alive")))
        (set-process-query-on-exit-flag proc nil)
        (while (process-live-p proc) (accept-process-output proc 1))
        (let ((output (buffer-string))
              (pbuf (process-buffer proc)))
          (list (bufferp pbuf)
                (eq pbuf buf)
                (buffer-live-p pbuf)
                (string-match "alive" output)
                (> (length output) 0)
                (kill-buffer buf)
                (not (buffer-live-p pbuf)))))))) #"#,
        expect,
    );
}

#[test]
fn divergence_format_time_string_zones() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 7 77)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((fixed-time '(26150 29968)))
    (list (stringp (format-time-string "%H:%M:%S" fixed-time))
          (= (length (format-time-string "%H:%M:%S" fixed-time)) 8)
          (stringp (format-time-string "%s" fixed-time))
          (> (string-to-number (format-time-string "%s" fixed-time)) 0)
          (string-match "^[0-9]+$" (format-time-string "%s" fixed-time))))) #"#,
        expect,
    );
}

#[test]
fn divergence_timer_duration_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 8 69)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (seconds-to-time 60)
        (equal (seconds-to-time 60) (list 0 60))
        (time-to-seconds (seconds-to-time 30))
        (= (time-to-seconds (seconds-to-time 30)) 30)
        (float-time (seconds-to-time 3600))
        (= (float-time (seconds-to-time 3600)) 3600)
        (<= (abs (- (float-time) (float-time (current-time)))) 1))) #"#,
        expect,
    );
}

#[test]
fn divergence_process_list_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (void-function set-process-query-on-exight-flag)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((before (length (process-list))))
    (let ((p1 (start-process "test-pl1-xxx" nil "sleep" "0.1"))
          (p2 (start-process "test-pl2-xxx" nil "sleep" "0.1")))
      (set-process-query-on-exit-flag p1 nil)
      (set-process-query-on-exight-flag p2 nil)
      (let ((during (length (process-list))))
        (while (or (process-live-p p1) (process-live-p p2))
          (accept-process-output nil 0.2))
        (list (>= during (+ before 2))
              (processp p1)
              (processp p2)
              (eq (process-status p1) 'exit)
              (eq (process-status p2) 'exit)))))) #"#,
        expect,
    );
}

#[test]
fn divergence_decode_time_components() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 14 33)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((now (decode-time)))
    (list (>= (nth 0 now) 0)
          (<= (nth 0 now) 60)
          (>= (nth 1 now) 0)
          (<= (nth 1 now) 59)
          (>= (nth 2 now) 0)
          (<= (nth 2 now) 23)
          (>= (nth 3 now) 1)
          (<= (nth 3 now) 31)
          (>= (nth 4 now) 1)
          (<= (nth 4 now) 12)
          (> (nth 5 now) 2020)
          (= (length now) 9)))) #"#,
        expect,
    );
}
