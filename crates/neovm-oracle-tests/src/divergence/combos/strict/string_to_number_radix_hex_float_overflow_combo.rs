//! Strict combo oracle probes, batch 198: number<->string conversion. string-
//! to-number with radix (2/8/10/16), hex 0x prefix, float parse, leading sign,
//! trailing junk, empty/invalid, scientific notation, and number-to-string of
//! integers/floats/negative-zero/bignum.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_string_to_number_radix_hex_junk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (string-to-number "42")
      (string-to-number "ff" 16)
      (string-to-number "FF" 16)
      (string-to-number "0xff")
      (string-to-number "777" 8)
      (string-to-number "101" 2)
      (string-to-number "-5")
      (string-to-number "+5")
      (string-to-number "123abc")
      (string-to-number "")
      (string-to-number "abc")
      (string-to-number "  42")
      (string-to-number "1e10")
      (string-to-number "3.14")
      (string-to-number "0xZZ"))
"##;
    let expect =
        expect_test::expect![[r#""OK (42 255 255 0 511 5 -5 5 123 0 0 42 10000000000.0 3.14 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_number_to_string_int_float_bignum_negzero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (number-to-string 42)
      (number-to-string -42)
      (number-to-string 0)
      (number-to-string 3.14)
      (number-to-string -0.0)
      (number-to-string (expt 2 64))
      (number-to-string 1/3)
      (number-to-string -1/4)
      (number-to-string most-positive-fixnum)
      (number-to-string (1+ most-positive-fixnum))
      (number-to-string 1e100)
      (number-to-string 1e-10))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable 1/3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_number_format_to_string_overflow_inf_nan() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (format "%d" 42)
      (format "%d" (1+ most-positive-fixnum))
      (format "%x" 255)
      (format "%o" 64)
      (format "%b" 10)
      (format "%d" (/ 1 0.0))
      (number-to-string (/ 1.0 0.0))
      (number-to-string (/ -1.0 0.0))
      (number-to-string (/ 0.0 0.0))
      (format "%f" (/ 1.0 0.0)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"42\" \"2305843009213693952\" \"ff\" \"100\" \"1010\" \"inf\" \"1.0e+INF\" \"-1.0e+INF\" \"-0.0e+NaN\" \"inf\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
