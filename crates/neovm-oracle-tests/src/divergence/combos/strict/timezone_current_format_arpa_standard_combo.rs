//! Strict combo oracle probes, batch 239: timezone + time-zone handling.
//! current-time-zone, format-time-string %Z, set-time-zone-rule round-trip,
//! and timezone-make-date-arpa-standard / zone offset.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_current_time_zone_format_z_explicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((tz (getenv "TZ")))
  (unwind-protect
      (progn
        (set-time-zone-rule "UTC0")
        (let ((fixed (encode-time 0 0 12 15 3 2025 nil -1 nil)))
          (list (format-time-string "%Z" fixed)
                (format-time-string "%z" fixed)
                (car (current-time-zone fixed))
                (cadr (current-time-zone fixed))
                (format-time-string "%Y-%m-%dT%H:%M:%S%z" fixed))))
    (set-time-zone-rule tz)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"UTC\" \"+0000\" 0 \"UTC\" \"2025-03-15T12:00:00+0000\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_timezone_make_date_arpa_standard_offset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((tz (getenv "TZ")))
  (unwind-protect
      (progn
        (set-time-zone-rule "UTC0")
        (let ((fixed (encode-time 0 30 9 15 3 2025 nil -1 nil)))
          (list (format-time-string "%a, %d %b %Y %H:%M:%S %z" fixed)
                (format-time-string "%Y%m%dT%H%M%S" fixed)
                (decode-time fixed)
                (current-time-zone fixed))))
    (set-time-zone-rule tz)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"Sat, 15 Mar 2025 09:30:00 +0000\" \"20250315T093000\" (0 30 9 15 3 2025 6 nil 0) (0 \"UTC\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_set_time_zone_rule_restore_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((tz (getenv "TZ")))
  (unwind-protect
      (let ((fixed (encode-time 0 0 0 1 1 2025 nil -1 nil)))
        (set-time-zone-rule "UTC0")
        (let ((utc-z (format-time-string "%H%M" fixed)))
          (set-time-zone-rule "PST8PDT")
          (let ((pst-z (format-time-string "%H%M" fixed)))
            (set-time-zone-rule tz)
            (list utc-z pst-z (>= (length utc-z) 4) (>= (length pst-z) 4)))))
    (set-time-zone-rule tz)))
"##;
    let expect = expect_test::expect![[r#""OK (\"0500\" \"2100\" t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
