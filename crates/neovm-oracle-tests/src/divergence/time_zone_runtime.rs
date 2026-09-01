//! Time-zone handling parity: format-time-string with named/integer zone +
//! %z, decode-time / current-time-zone with integer zone offsets, encode-time
//! with an explicit zone; plus the %:::z colon-offset specifier gap.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn current_time_zone_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 3600 -18000)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (car (current-time-zone '(26150 29968) 0)) (car (current-time-zone '(26150 29968) 3600)) (car (current-time-zone '(26150 29968) -18000)))"##,
        expect,
    );
}

#[test]
fn decode_time_zone_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (14 15 13 7200)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (nth 2 (decode-time '(26150 29968) 0)) (nth 2 (decode-time '(26150 29968) 3600)) (nth 2 (decode-time '(26150 29968) -3600)) (nth 8 (decode-time '(26150 29968) 7200)))"##,
        expect,
    );
}

#[test]
fn encode_time_utc_zone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 30 14 15 3 2024)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let* ((enc (encode-time (list 0 30 14 15 3 2024 nil nil 0)))
        (back (decode-time enc 0)))
  (list (nth 0 back) (nth 1 back) (nth 2 back) (nth 3 back) (nth 4 back) (nth 5 back)))"##,
        expect,
    );
}

#[test]
fn encode_time_with_zone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (11 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let* ((enc (encode-time (list 0 0 12 1 6 2024 nil nil 3600)))
        (dec (decode-time enc 0)))
  (list (nth 2 dec) (nth 1 dec)))"##,
        expect,
    );
}

#[test]
fn fts_integer_zone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"15:32+0100\" \"09:32-0500\" \"14:32+0000\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%H:%M%z" '(26150 29968) 3600) (format-time-string "%H:%M%z" '(26150 29968) -18000) (format-time-string "%H:%M%z" '(26150 29968) 0))"##,
        expect,
    );
}

#[test]
fn fts_named_zone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"14:32 UTC\" \"14:32\" \"+0100\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%H:%M %Z" '(26150 29968) "UTC") (format-time-string "%H:%M" '(26150 29968) t) (format-time-string "%z" '(26150 29968) 3600))"##,
        expect,
    );
}

#[test]
fn divergence_fts_minimal_colon_zone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"+01\" \"+00\" \"+01:01:01\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format-time-string "%:::z" '(26150 29968) 3600)
      (format-time-string "%:::z" '(26150 29968) 0)
      (format-time-string "%::z" '(26150 29968) 3661))"##,
        expect,
    );
}
