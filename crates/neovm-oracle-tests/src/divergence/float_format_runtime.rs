//! Float formatting parity: %g trailing-zero stripping, %e/%.Ne exponent,
//! %.Nf rounding (round-half-to-even), width/pad/sign/space flags, -0.0,
//! %d/%x/%o of bignums, very large/small floats, number-to-string +
//! string-to-number roundtrip; plus the %E/%G uppercase-conversion divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn ff_float_to_string_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((vals '(0.1 0.2 0.3 1.5 3.14159265358979 1e100 1e-100)))
  (cl-every (lambda (v) (= v (string-to-number (number-to-string v)))) vals))"##,
        expect,
    );
}

#[test]
fn ff_format_d_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"1180591620717411303424\" \"-1180591620717411303424\" \"10000000000000000\" \"10000000000\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%d" (expt 2 70)) (format "%d" (- (expt 2 70)))
        (format "%x" (expt 2 64)) (format "%o" (expt 2 30)))"##,
        expect,
    );
}

#[test]
fn ff_format_e_exponent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"1.000000e+00\" \"1.234568e+04\" \"1.230000e-04\" \"1.00e+03\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%e" 1.0) (format "%e" 12345.678) (format "%e" 0.000123) (format "%.2e" 999.9))"##,
        expect,
    );
}

#[test]
fn ff_format_f_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"2\" \"4\" \"3.142\" \"0.100000\" \"0.3333333333\" \"0\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%.0f" 2.5) (format "%.0f" 3.5) (format "%.3f" 3.14159)
        (format "%f" 0.1) (format "%.10f" (/ 1.0 3.0)) (format "%.0f" 0.5))"##,
        expect,
    );
}

#[test]
fn ff_format_f_width_pad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"      3.14\" \"3.14      |\" \"0000003.14\" \"+3.14\" \" 3.14\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%10.2f" 3.14) (format "%-10.2f|" 3.14)
        (format "%010.2f" 3.14) (format "%+.2f" 3.14) (format "% .2f" 3.14))"##,
        expect,
    );
}

#[test]
fn ff_format_g_trailing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"1\" \"1.5\" \"100000\" \"1e+06\" \"0.0001\" \"1e-05\" \"1.23457\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%g" 1.0) (format "%g" 1.5) (format "%g" 100000.0)
        (format "%g" 1000000.0) (format "%g" 0.0001) (format "%g" 0.00001) (format "%g" 1.23456789))"##,
        expect,
    );
}

#[test]
fn ff_format_large_small_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"1e+308\" \"1e-308\" \"100000000000000000000.000000\" \"1.80e+308\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%g" 1e308) (format "%g" 1e-308) (format "%f" 1e20)
        (format "%.2e" 1.7976931348623157e308))"##,
        expect,
    );
}

#[test]
fn ff_format_negative_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"-0.000000\" \"-0\" \"-0.000000e+00\" \"-0.0\" \"0\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%f" -0.0) (format "%g" -0.0) (format "%e" -0.0)
        (format "%.1f" -0.04) (format "%d" -0))"##,
        expect,
    );
}

#[test]
fn ff_format_percent_combos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\" 95.5%\" \"3/4\" \"+0042\" \"ff FF\" \"0b101\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%5.1f%%" 95.5) (format "%d/%d" 3 4)
        (format "%+05d" 42) (format "%x %X" 255 255) (format "%#b" 5))"##,
        expect,
    );
}

#[test]
fn ff_number_to_string_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"1.0\" \"0.1\" \"1e+20\" \"0.6666666666666666\" \"-0.0\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (number-to-string 1.0) (number-to-string 0.1)
        (number-to-string 1e20) (number-to-string (/ 2.0 3.0)) (number-to-string -0.0))"##,
        expect,
    );
}

#[test]
fn divergence_format_uppercase_e_g() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (err err)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (format "%E" 1.0) (error 'err))
      (condition-case e (format "%G" 1500.0) (error 'err)))"##,
        expect,
    );
}
