//! Number & object print-representation divergence probes (calibration).
//!
//! Probes float formatting (decimal, scientific, special values), bignum
//! arithmetic/printing, fixnum boundaries, char escapes in prin1, number
//! bases, symbol-name printing, and nested structure printing — areas where
//! two Lisp implementations commonly diverge in textual representation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- float formatting -------------------------------------------------------

#[test]
fn div_np_float_decimal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"0.1\" \"3.14\" \"100.0\" \"0.5\" \"-2.5\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (prin1-to-string 0.1)
      (prin1-to-string 3.14)
      (prin1-to-string 100.0)
      (prin1-to-string 0.5)
      (prin1-to-string -2.5))
"##,
        expect,
    );
}

#[test]
fn div_np_float_division_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"0.3333333333333333\" \"3.142857142857143\" \"0.14285714285714285\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (prin1-to-string (/ 1.0 3))
      (prin1-to-string (/ 22.0 7))
      (prin1-to-string (/ 1.0 7)))
"##,
        expect,
    );
}

#[test]
fn div_np_float_scientific_extremes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"10000000000.0\" \"1e-10\" \"1e+100\" \"1.5e+300\" \"1.5e-300\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (prin1-to-string 1e10)
      (prin1-to-string 1e-10)
      (prin1-to-string 1e100)
      (prin1-to-string 1.5e300)
      (prin1-to-string 1.5e-300))
"##,
        expect,
    );
}

#[test]
fn div_np_float_special_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1.0e+INF\" \"-1.0e+INF\" \"-0.0e+NaN\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (prin1-to-string (/ 1.0 0.0))
      (prin1-to-string (/ -1.0 0.0))
      (prin1-to-string (/ 0.0 0.0))
      (eq (number-to-string (/ 0.0 0.0)) (number-to-string (/ 0.0 0.0))))
"##,
        expect,
    );
}

#[test]
fn div_np_float_format_directives() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"3.140000\" \"3.14\" \"1.234568e+04\" \"0.0001\" \"0.3333333333\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%f" 3.14)
      (format "%.2f" 3.14159)
      (format "%e" 12345.678)
      (format "%g" 0.0001)
      (format "%.10f" (/ 1.0 3)))
"##,
        expect,
    );
}

// --- bignum & fixnum boundaries ---------------------------------------------

#[test]
fn div_np_bignum_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (600000000000000000000 18446744073709551616 340282366920938463463374607431768211456 2305843009213693952 -2305843009213693953)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (* 6 (expt 10 20))
      (expt 2 64)
      (expt 2 128)
      (1+ most-positive-fixnum)
      (1- most-negative-fixnum))
"##,
        expect,
    );
}

#[test]
fn div_np_fixnum_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (0 0 2305843009213693952 -2305843009213693953 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list most-positive-fixnum
      most-negative-fixnum
      (1+ most-positive-fixnum)
      (1- most-negative-fixnum)
      (fixnump (1+ most-positive-fixnum)))
"##,
        expect,
    );
}

#[test]
fn div_np_integer_division_and_mod() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 -3 -3 2 -2 3 -3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (/ 17 5)
      (/ -17 5)
      (/ 17 -5)
      (% 17 5)
      (% -17 5)
      (mod -17 5)
      (mod 17 -5))
"##,
        expect,
    );
}

// --- number bases & formatting ----------------------------------------------

#[test]
fn div_np_number_bases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments number-to-string 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%#o" 64)
      (format "%#x" 255)
      (format "%#X" 255)
      (format "%#b" 10)
      (number-to-string 255 16)
      (number-to-string 64 2))
"##,
        expect,
    );
}

#[test]
fn div_np_string_to_number_bases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (255 511 10 1500.0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-to-number "ff" 16)
      (string-to-number "777" 8)
      (string-to-number "1010" 2)
      (string-to-number "1.5e3")
      (string-to-number "0x1f"))
"##,
        expect,
    );
}

// --- char escapes in prin1 --------------------------------------------------

#[test]
fn div_np_prin1_char_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\"a\tb\\nc\\\"\" \"\\\"a\\\\\\\"b\\\"\" \"\\\"\\\\\\\\back\\\"\" \"\\\"\\0\u{1}\u{1b}\u{7f}\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (prin1-to-string "a\tb\nc")
      (prin1-to-string "a\"b")
      (prin1-to-string "\\back")
      (prin1-to-string (string 0 1 27 127)))
"##,
        expect,
    );
}

#[test]
fn div_np_prin1_multibyte_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (let ((print-escape-nonascii t)) (prin1-to-string "café"))
      (let ((print-escape-multibyte t)) (prin1-to-string "café世界"))
"##,
        expect,
    );
}

// --- symbol & nested printing -----------------------------------------------

#[test]
fn div_np_symbol_special_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable spaces|)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (prin1-to-string 'foo-bar)
      (prin1-to-string 'foo_bar)
      (prin1-to-string 'foo?)
      (prin1-to-string '+)
      (prin1-to-string '1+)
      (prin1-to-string '|has spaces|))
"##,
        expect,
    );
}

#[test]
fn div_np_nested_structure_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"([1 2 (a . b)] \\\"str\\\" sym 3.14 t nil)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(prin1-to-string (list (vector 1 2 (cons 'a 'b)) "str" 'sym 3.14 t nil))
"##,
        expect,
    );
}

#[test]
fn div_np_plist_alist_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"((\\\"a\\\" . 1) (\\\"b\\\" . 2))\" \"(:x 1 :y 2 :z 3)\" \"#s(hash-table test equal)\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (prin1-to-string '(("a" . 1) ("b" . 2)))
      (prin1-to-string '(:x 1 :y 2 :z 3))
      (prin1-to-string (make-hash-table :test 'equal)))
"##,
        expect,
    );
}

// --- numeric comparison edge cases ------------------------------------------

#[test]
fn div_np_float_equality_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (= 0.0 -0.0)
      (eq 0.0 -0.0)
      (< 0.0 -0.0)
      (eql 0.0 -0.0)
      (= 1 (* 3 (/ 1.0 3)))
      (eql most-positive-fixnum (1+ most-positive-fixnum)))
"##,
        expect,
    );
}

#[test]
fn div_np_round_truncate_floor_ceil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 -3 2 -2 2 -2 2.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (round 2.7)
      (round -2.7)
      (truncate 2.7)
      (truncate -2.7)
      (floor 2.7)
      (ceiling -2.7)
      (ffloor 2.7))
"##,
        expect,
    );
}
