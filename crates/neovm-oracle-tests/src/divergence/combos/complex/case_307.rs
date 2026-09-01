//! Complex combo batch 307 — `time` / `date` deep: encode/decode with
//! timezone offsets, `current-time-zone`, `float-time`, `time-add` with
//! fractional seconds, `parse-time-string` with ISO8601 and informal formats.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx307_encode_decode_with_timezone_offsets() {
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
              (list tz
                    (decoded-time-hour dec)
                    (decoded-time-zone dec))))
          tzs))
"##,
        expect,
    )
}

#[test]
fn div_cx307_time_arithmetic_with_fractional() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 12 2 12 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((t0 (encode-time 0 0 0 1 1 2024 nil))
       (plus-half-day (time-add t0 43200))
       (plus-1.5-days (time-add t0 129600))
       (minus-6h (time-subtract t0 21600)))
  (list (decoded-time-day (decode-time plus-half-day))
        (decoded-time-hour (decode-time plus-half-day))
        (decoded-time-day (decode-time plus-1.5-days))
        (decoded-time-hour (decode-time plus-1.5-days))
        (decoded-time-hour (decode-time minus-6h))))
"##,
        expect,
    )
}

#[test]
fn div_cx307_parse_time_string_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"2024-06-15\" 2024 6 15 nil) (\"2024-06-15 12:30\" 2024 6 15 12) (\"2024-06-15 12:30:45\" 2024 6 15 12) (\"Jun 15, 2024\" 2024 6 15 nil) (\"15 Jun 2024\" 2024 6 15 nil) (\"2024-06-15T12:30:45Z\" 2024 6 15 12) (\"invalid date\" nil nil nil nil))""#
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
          "2024-06-15T12:30:45Z"
          "invalid date"))
"##,
        expect,
    )
}

#[test]
fn div_cx307_format_time_string_with_zone_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"12:00 +0000\" \"13:00 +0100\" \"07:00 -0500\" \"GMT\" \"2024-06-15T12:00:00+0000\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time 0 0 12 15 6 2024 nil 0)))
  (list (format-time-string "%H:%M %z" t0 0)
        (format-time-string "%H:%M %z" t0 3600)
        (format-time-string "%H:%M %z" t0 -18000)
        (format-time-string "%Z" t0 0)
        (format-time-string "%Y-%m-%dT%H:%M:%S%z" t0 0)))
"##,
        expect,
    )
}

#[test]
fn div_cx307_time_to_days_and_seconds_precision() {
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
fn div_cx307_current_time_structure_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((now (current-time)))
  (list (consp now)
        (integerp (car now))
        (integerp (cadr now))
        (stringp (current-time-string))
        (consp (current-time-zone))
        (floatp (float-time))))
"##,
        expect,
    )
}

#[test]
fn div_cx307_calendar_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'calendar)
      (list (calendar-absolute-from-gregorian '(1 1 2024))
            (calendar-absolute-from-gregorian '(12 31 2024))
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
fn div_cx307_time_less_equal_greater_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((t0 (encode-time 0 0 0 1 1 2024 nil))
       (t1 (encode-time 0 0 0 2 1 2024 nil))
       (t2 (encode-time 0 0 0 1 1 2024 nil)))
  (list (time-less-p t0 t1)
        (time-less-p t1 t0)
        (time-equal-p t0 t2)
        (time-equal-p t0 t1)))
"##,
        expect,
    )
}

#[test]
fn div_cx307_format_time_with_padding_flags() {
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
fn div_cx307_time_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((t0 (encode-time 0 0 12 15 6 2024 nil))
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
    )
}
