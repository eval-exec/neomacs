//! Complex combo batch 368 — `time`/`date` ultimate: encode/decode with
//! timezone offsets, format-time-string with all specifiers, parse-time-string
//! variants, time arithmetic, calendar queries, format padding flags.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx368_encode_decode_with_timezone_offsets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((-36000 12 -36000) (-18000 12 -18000) (-3600 12 -3600) (0 12 0) (3600 12 3600) (10800 12 10800) (32400 12 32400))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tzs '(-36000 -18000 -3600 0 3600 10800 32400)))
  (mapcar (lambda (tz)
            (let* ((enc (encode-time 0 0 12 15 6 2024 tz))
                   (dec (decode-time enc tz)))
              (list tz (decoded-time-hour dec) (decoded-time-zone dec))))
          tzs))
"##,
        expect,
    )
}

#[test]
fn div_cx368_format_time_string_all_specifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"2024-01-01\" \"00:00:00\" \"Monday January 01, 2024\" \"001\" \"00\" \"01\" \"2024-01-01T00:00:00-0500\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time 0 0 0 1 1 2024 nil)))
  (list (format-time-string "%Y-%m-%d" t0)
        (format-time-string "%H:%M:%S" t0)
        (format-time-string "%A %B %d, %Y" t0)
        (format-time-string "%j" t0)
        (format-time-string "%U" t0)
        (format-time-string "%W" t0)
        (format-time-string "%Y-%m-%dT%H:%M:%S%z" t0)))
"##,
        expect,
    )
}

#[test]
fn div_cx368_time_arithmetic_add_subtract_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (16 12 6 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((t0 (encode-time 0 0 12 15 6 2024 nil))
       (plus-1day (time-add t0 86400))
       (minus-6h (time-subtract t0 21600)))
  (list (decoded-time-day (decode-time plus-1day))
        (decoded-time-hour (decode-time plus-1day))
        (decoded-time-hour (decode-time minus-6h))
        (time-less-p minus-6h t0)
        (time-less-p t0 plus-1day)
        (time-equal-p t0 (time-subtract plus-1day 86400))))
"##,
        expect,
    )
}

#[test]
fn div_cx368_parse_time_string_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"2024-06-15\" 2024 6 15 nil) (\"2024-06-15 12:30\" 2024 6 15 12) (\"2024-06-15 12:30:45\" 2024 6 15 12) (\"Jun 15, 2024\" 2024 6 15 nil) (\"15 Jun 2024\" 2024 6 15 nil) (\"2024-06-15T12:30:45Z\" 2024 6 15 12))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (condition-case e
              (let ((p (parse-time-string s)))
                (list s (decoded-time-year p) (decoded-time-month p)
                      (decoded-time-day p) (decoded-time-hour p)))
            (error (list s :err (car e)))))
        '("2024-06-15"
          "2024-06-15 12:30"
          "2024-06-15 12:30:45"
          "Jun 15, 2024"
          "15 Jun 2024"
          "2024-06-15T12:30:45Z"))
"##,
        expect,
    )
}

#[test]
fn div_cx368_format_time_string_with_zone_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"12:00 +0000\" \"13:00 +0100\" \"07:00 -0500\" \"GMT\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time 0 0 12 15 6 2024 nil 0)))
  (list (format-time-string "%H:%M %z" t0 0)
        (format-time-string "%H:%M %z" t0 3600)
        (format-time-string "%H:%M %z" t0 -18000)
        (format-time-string "%Z" t0 0)))
"##,
        expect,
    )
}

#[test]
fn div_cx368_time_to_days_and_seconds_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (738886 738887 1 86400.0 1704085200.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time 0 0 0 1 1 2024 nil))
      (t1 (encode-time 0 0 0 2 1 2024 nil)))
  (list (time-to-days t0)
        (time-to-days t1)
        (- (time-to-days t1) (time-to-days t0))
        (float-time (time-subtract t1 t0))
        (float-time t0)))
"##,
        expect,
    )
}

#[test]
fn div_cx368_calendar_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calendar)
      (list (calendar-absolute-from-gregorian '(1 1 2024))
            (calendar-day-of-year '(1 1 2024))
            (calendar-day-of-year '(12 31 2024))
            (calendar-leap-year-p 2024)
            (calendar-leap-year-p 2023)
            (calendar-last-day-of-month 2 2024)
            (calendar-last-day-of-month 2 2023)
            (calendar-day-name '(1 1 2024))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx368_current_time_structure_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((now (current-time)))
  (list (consp now)
        (integerp (car now))
        (integerp (cadr now))
        (stringp (current-time-string))
        (consp (current-time-zone))))
"##,
        expect,
    )
}

#[test]
fn div_cx368_format_time_padding_flags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"5\" \" 5\" \"05\" \"MAY\" \"SUNDAY\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time 5 5 5 5 5 2024 nil)))
  (list (format-time-string "%-d" t0)
        (format-time-string "%_d" t0)
        (format-time-string "%0d" t0)
        (format-time-string "%^B" t0)
        (format-time-string "%^A" t0)))
"##,
        expect,
    )
}

#[test]
fn div_cx368_time_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((t0 (encode-time 0 30 14 16 6 2026 nil))
       (t-str (format-time-string "%Y-%m-%d %H:%M:%S" t0)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Time mega: %s" t-str))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 22)
      (let ((state (list t-str
                         (format-time-string "%H:%M" t0)
                         (time-to-days t0)
                         (decoded-time-day (decode-time t0))
                         (decoded-time-hour (decode-time t0))
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen()
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))))
"##,
        expect,
    )
}
