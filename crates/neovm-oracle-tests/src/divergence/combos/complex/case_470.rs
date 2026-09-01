/// Batch 470: rfc2047, rfc2231, time-date, format-spec, cookies, sha1, hexl.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx470_rfc2047_encode_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'rfc2047)
  (list (fboundp 'rfc2047-encode-string)
        (fboundp 'rfc2047-decode-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx470_rfc2231_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'rfc2231)
  (list (fboundp 'rfc2231-parse-param-value)
        (fboundp 'rfc2231-get-value)))
"##,
        expect,
    );
}

#[test]
fn div_cx470_time_date_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (739053 (26223 12072) 739053 168)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'time-date)
  (list (date-to-day "2024-06-16")
        (date-to-time "2024-06-16 14:30:00")
        (time-to-days (encode-time 0 0 0 16 6 2024 nil))
        (time-to-day-in-year (encode-time 0 0 0 16 6 2024 nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx470_format_spec_modifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Invalid format string\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'format-spec)
  (let ((spec (format-spec-make ?a "hello" ?b "world" ?n 42)))
    (list (format-spec "%a %b" spec)
          (format-spec "%n" spec)
          (format-spec "%(one%)" spec)
          (format-spec "%a" (format-spec-make ?a (format-spec-make ?b "test"))))))
"##,
        expect,
    );
}

#[test]
fn div_cx470_cookies_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"cookie\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'cookie)
  (list (fboundp 'cookie) (fboundp 'cookie-handle-cookie-line)))
"##,
        expect,
    );
}

#[test]
fn div_cx470_sha1_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d\" \"aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d\" \"5d41402abc4b2a76b9719d911017c592\" \"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (sha1 "hello")
      (secure-hash 'sha1 "hello")
      (secure-hash 'md5 "hello")
      (secure-hash 'sha256 "hello"))
"##,
        expect,
    );
}

#[test]
fn div_cx470_hexl_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'hexl)
  (list (fboundp 'hexl-mode) (fboundp 'hexl-find-file)))
"##,
        expect,
    );
}

#[test]
fn div_cx470_encode_time_with_decoded() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (26223 12072)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(encode-time (decode-time (encode-time 0 30 14 16 6 2024 nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx470_decoded_time_second() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2024 6 16 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((dt (decode-time (encode-time 0 0 0 16 6 2024 nil))))
  (list (decoded-time-year dt) (decoded-time-month dt)
        (decoded-time-day dt) (decoded-time-hour dt)
        (decoded-time-second dt)))
"##,
        expect,
    );
}

#[test]
fn div_cx470_time_add_subtract_days() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (encode-time 0 0 0 1 1 2024 nil))
      (day (seconds-to-time 86400)))
  (list (time-less-p (time-add t1 day) t1)
        (time-less-p t1 (time-add t1 day))
        (time-equal-p t1 t1)))
"##,
        expect,
    );
}

#[test]
fn div_cx470_time_zones() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (26222 54208)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (encode-time 0 0 12 16 6 2024 "UTC")
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx470_seconds_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"0s\" \"60.00s\" \"60.00m\" \"24.00h\" \"61.02m\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (seconds-to-string 0)
      (seconds-to-string 60)
      (seconds-to-string 3600)
      (seconds-to-string 86400)
      (seconds-to-string 3661))
"##,
        expect,
    );
}

#[test]
fn div_cx470_days_in_month() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (31 29 28)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'calendar)
  (list (calendar-last-day-of-month 1 2024)
        (calendar-last-day-of-month 2 2024)
        (calendar-last-day-of-month 2 2023))
"##,
        expect,
    );
}

#[test]
fn div_cx470_smtpmail_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'smtpmail)
  (list (boundp 'smtpmail-default-smtp-server)
        (boundp 'smtpmail-smtp-service)))
"##,
        expect,
    );
}

#[test]
fn div_cx470_sasl_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'sasl)
  (list (fboundp 'sasl-find-mechanism)
        (boundp 'sasl-mechanisms)))
"##,
        expect,
    );
}
