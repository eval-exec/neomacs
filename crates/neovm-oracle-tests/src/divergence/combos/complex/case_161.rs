//! Complex combo batch 161 — `time` / `date` / `current-time` /
//! `current-time-string` / `current-time-zone` / `format-time-string` /
//! `encode-time` / `decode-time` in different timezones.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx161_current_time_zone_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function multiple-value-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((now (current-time)))
  (multiple-value-bind (offset name)
      (current-time-zone now)
    (list (integerp offset)
          (or (stringp name) (null name))
          offset name)))
"##,
        expect,
    );
}

#[test]
fn div_cx161_format_time_string_with_zone_offset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"16:00 +0000\" \"17:00 +0100\" \"11:00 -0500\" \"16:00 GMT\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time 0 0 12 15 6 2024 nil)))
  (list (format-time-string "%H:%M %z" t0 0)
        (format-time-string "%H:%M %z" t0 3600)
        (format-time-string "%H:%M %z" t0 -18000)
        (format-time-string "%H:%M %Z" t0 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx161_encode_decode_round_trip_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((26221 50474) (26221 50474) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((encoded-1 (encode-time 30 45 12 15 6 2024 nil))
       (decoded-1 (decode-time encoded-1))
       (encoded-2 (encode-time (decoded-time-second decoded-1)
                                (decoded-time-minute decoded-1)
                                (decoded-time-hour decoded-1)
                                (decoded-time-day decoded-1)
                                (decoded-time-month decoded-1)
                                (decoded-time-year decoded-1)
                                nil)))
  (list encoded-1 encoded-2
        (= (float-time encoded-1) (float-time encoded-2))))
"##,
        expect,
    );
}

#[test]
fn div_cx161_time_add_subtract_with_days() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 31 25 1 2023)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((t0 (encode-time 0 0 0 1 1 2024 nil))
       (plus-1day (time-add t0 86400))
       (plus-30days (time-add t0 (* 30 86400)))
       (minus-7days (time-subtract t0 (* 7 86400))))
  (list (decoded-time-day (decode-time plus-1day))
        (decoded-time-day (decode-time plus-30days))
        (decoded-time-day (decode-time minus-7days))
        (decoded-time-month (decode-time plus-30days))
        (decoded-time-year (decode-time minus-7days))))
"##,
        expect,
    );
}

#[test]
fn div_cx161_time_less_p_with_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((t0 (encode-time 0 0 0 1 1 2024 nil))
       (t-past (time-subtract t0 86400))
       (t-future (time-add t0 86400)))
  (list (time-less-p t-past t0)
        (time-less-p t0 t-future)
        (time-less-p t-future t0)
        (not (time-less-p t0 t-past))))
"##,
        expect,
    );
}

#[test]
fn div_cx161_time_to_days_seconds_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time 0 0 0 1 1 2024 nil)))
  (list (integerp (time-to-days t0))
        (integerp (time-to-seconds t0))
        (floatp (float-time t0))
        (integerp (1+ (time-to-days t0)))))
"##,
        expect,
    );
}

#[test]
fn div_cx161_current_time_structure_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((now (current-time)))
  (list (consp now)
        (integerp (car now))
        (stringp (current-time-string))
        (consp (current-time-zone))))
"##,
        expect,
    );
}

#[test]
fn div_cx161_format_time_string_with_padding_flags() {
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
    );
}

#[test]
fn div_cx161_iso8601_parse_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((iso-str "2024-06-15T12:30:45Z")
           (parsed (iso8601-parse iso-str)))
      (list (decoded-time-year parsed)
            (decoded-time-month parsed)
            (decoded-time-day parsed)
            (decoded-time-hour parsed)
            (decoded-time-minute parsed)
            (decoded-time-second parsed)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx161_format_time_string_with_fractional_seconds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"00\" \"00.000000000\" \"00.000\" \"00.000000\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time '(30 0 0) 0 12 15 6 2024 nil)))
  (list (format-time-string "%S" t0)
        (format-time-string "%S.%N" t0)
        (format-time-string "%S.%3N" t0)
        (format-time-string "%S.%6N" t0)))
"##,
        expect,
    );
}

#[test]
fn div_cx161_time_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((t0 (encode-time 0 0 12 15 6 2024 nil))
       (time-str (format-time-string "%Y-%m-%d %H:%M:%S" t0)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Time mega: %s" time-str))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list time-str
                         (format-time-string "%H:%M" t0)
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
