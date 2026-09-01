//! Complex combo batch 83 — time / date arithmetic: encode-time,
//! decode-time, current-time, format-time-string, time-add, time-subtract,
//! time-less-p, time-to-days, and parse-time-string edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx83_encode_decode_time_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (30 45 12 15 6 2024 6 t -14400)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((encoded (encode-time 30 45 12 15 6 2024 nil))
       (decoded (decode-time encoded)))
  (list (decoded-time-second decoded)
        (decoded-time-minute decoded)
        (decoded-time-hour decoded)
        (decoded-time-day decoded)
        (decoded-time-month decoded)
        (decoded-time-year decoded)
        (decoded-time-weekday decoded)
        (decoded-time-dst decoded)
        (decoded-time-zone decoded)))
"##,
        expect,
    );
}

#[test]
fn div_cx83_format_time_string_various_formats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"2024-01-01\" \"00:00:00\" \"2024-01-01T00:00:00-0500\" \"Monday, January 01, 2024\" \"001\" \"00\" \"01\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time 0 0 0 1 1 2024 nil)))
  (list (format-time-string "%Y-%m-%d" t0)
        (format-time-string "%H:%M:%S" t0)
        (format-time-string "%Y-%m-%dT%H:%M:%S%z" t0)
        (format-time-string "%A, %B %d, %Y" t0)
        (format-time-string "%j" t0)
        (format-time-string "%U" t0)
        (format-time-string "%W" t0)))
"##,
        expect,
    );
}

#[test]
fn div_cx83_time_arithmetic_add_subtract() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((0 0 13 15 6 2024 6 t -14400) (0 0 11 15 6 2024 6 t -14400) (0 30 12 15 6 2024 6 t -14400) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((t0 (encode-time 0 0 12 15 6 2024 nil))
       (plus-1h (time-add t0 3600))
       (minus-1h (time-subtract t0 3600))
       (plus-30m (time-add t0 (* 30 60))))
  (list (decode-time plus-1h)
        (decode-time minus-1h)
        (decode-time plus-30m)
        (time-less-p minus-1h t0)
        (time-less-p t0 plus-1h)
        (time-equal-p t0 (time-subtract plus-1h 3600))))
"##,
        expect,
    );
}

#[test]
fn div_cx83_parse_time_string_formats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((2024 6 15 nil nil nil) (2024 6 15 12 30 0) (2024 6 15 12 30 45) (2024 6 15 12 30 45) (2024 6 15 nil nil nil) (2024 6 15 nil nil nil) (2024 6 15 nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (condition-case e
              (let ((parsed (parse-time-string s)))
                (list (decoded-time-year parsed)
                      (decoded-time-month parsed)
                      (decoded-time-day parsed)
                      (decoded-time-hour parsed)
                      (decoded-time-minute parsed)
                      (decoded-time-second parsed)))
            (error (cons :err (car e)))))
        '("2024-06-15"
          "2024-06-15 12:30"
          "2024-06-15 12:30:45"
          "2024-06-15T12:30:45"
          "Jun 15, 2024"
          "15 Jun 2024"
          "Saturday, 15 June 2024"))
"##,
        expect,
    );
}

#[test]
fn div_cx83_time_to_days_and_to_seconds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (738886 1704085200.0 1704085200.0 738888)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time 0 0 0 1 1 2024 nil)))
  (list (time-to-days t0)
        (time-to-seconds t0)
        (float-time t0)
        (1+ (time-to-days (encode-time 0 0 0 2 1 2024 nil)))))
"##,
        expect,
    );
}

#[test]
fn div_cx83_current_time_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((now (current-time)))
  (list (consp now)
        (integerp (car now))
        (integerp (cadr now))
        (consp (current-time-string))
        (stringp (current-time-string))
        (floatp (float-time))))
"##,
        expect,
    );
}

#[test]
fn div_cx83_format_time_string_with_zone() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"12:00 GMT\" \"12:00 +0000\" \"07:00 -05\" \"13:00 +0100\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time 0 0 12 15 6 2024 nil 0)))
  (list (format-time-string "%H:%M %Z" t0 0)
        (format-time-string "%H:%M %z" t0 0)
        (format-time-string "%H:%M %Z" t0 -18000)
        (format-time-string "%H:%M %z" t0 3600)))
"##,
        expect,
    );
}

#[test]
fn div_cx83_iso8601_parse_and_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((iso-str "2024-06-15T12:30:45Z")
           (parsed (iso8601-parse iso-str))
           (encoded (encode-time (decoded-time-second parsed)
                                  (decoded-time-minute parsed)
                                  (decoded-time-hour parsed)
                                  (decoded-time-day parsed)
                                  (decoded-time-month parsed)
                                  (decoded-time-year parsed)
                                  0))
           (re-str (format-time-string "%Y-%m-%dT%H:%M:%SZ" encoded 0)))
      (list parsed re-str
            (decoded-time-year parsed)
            (decoded-time-month parsed)
            (decoded-time-day parsed)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx83_time_difference_in_units() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2775600.0 46260.0 771.0 32.125)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((t1 (encode-time 0 0 12 15 6 2024 nil))
       (t2 (encode-time 0 0 15 17 7 2024 nil))
       (diff-seconds (time-to-seconds (time-subtract t2 t1))))
  (list diff-seconds
        (/ diff-seconds 60)
        (/ diff-seconds 3600)
        (/ diff-seconds 86400)))
"##,
        expect,
    );
}

#[test]
fn div_cx83_time_with_fractional_seconds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((t t t t t) (0 . 1000000) \"00.000\" \"00.000000\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((t0 (encode-time '(30 0 0) 0 12 15 6 2024 nil))
       (now (current-time)))
  (list (list (consp now)
              (= (length now) 4)
              (catch 'all-integers
                (dolist (part now t)
                  (unless (integerp part)
                    (throw 'all-integers nil))))
              (<= 0 (nth 2 now) 999999)
              (<= 0 (nth 3 now) 999999999999))
        (encode-time '(0 0 0) 0 0 1 1 2024 nil)
        (format-time-string "%S.%3N" t0)
        (format-time-string "%S.%6N" t0)))
"##,
        expect,
    );
}

#[test]
fn div_cx83_format_time_with_padding_and_flags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"5\" \" 5\" \"05\" \"5\" \" 5\" \"MAY\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time 5 5 5 5 5 2024 nil)))
  (list (format-time-string "%-d" t0)
        (format-time-string "%_d" t0)
        (format-time-string "%0d" t0)
        (format-time-string "%-m" t0)
        (format-time-string "%_m" t0)
        (format-time-string "%^B" t0)))
"##,
        expect,
    );
}

#[test]
fn div_cx83_time_loop_arithmetic_with_marker_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((base (encode-time 0 0 0 1 1 2024 nil))
       (steps (cl-loop for i from 0 below 5
                       collect (decode-time (time-add base (* i 86400))))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Time test buffer")
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 4))
          (ov (make-overlay 2 8)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 1 10)
      (let ((state (list (mapcar #'decoded-time-day steps)
                         (mapcar #'decoded-time-month steps)
                         (mapcar #'decoded-time-year steps)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
