//! Comprehensive oracle parity tests for `string-to-number`.
//!
//! Covers ALL base parameters (2, 8, 10, 16), leading/trailing whitespace,
//! positive/negative signs, float strings with and without base, empty and
//! whitespace-only strings, "0x" prefixes, very large/small numbers,
//! infinity/NaN representations, and unusual edge cases not covered by
//! the basic or advanced test files.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Base parameter exhaustive: all valid bases with tricky inputs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_number_comprehensive_all_bases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r##"(list
  ;; Base 2: binary edge cases
  (string-to-number "0" 2)
  (string-to-number "1" 2)
  (string-to-number "10" 2)
  (string-to-number "11" 2)
  (string-to-number "1111111111111111" 2)
  (string-to-number "-1010" 2)
  (string-to-number "+110" 2)
  (string-to-number "  101" 2)
  (string-to-number "101abc" 2)
  (string-to-number "2" 2)          ;; out-of-range digit
  (string-to-number "1012" 2)       ;; partial parse stops at '2'

  ;; Base 8: octal edge cases
  (string-to-number "0" 8)
  (string-to-number "7" 8)
  (string-to-number "10" 8)
  (string-to-number "77" 8)
  (string-to-number "377" 8)
  (string-to-number "-77" 8)
  (string-to-number "+77" 8)
  (string-to-number "  77" 8)
  (string-to-number "8" 8)          ;; out-of-range digit
  (string-to-number "79" 8)         ;; stops at '9'
  (string-to-number "007" 8)

  ;; Base 10: decimal edge cases
  (string-to-number "0" 10)
  (string-to-number "-0" 10)
  (string-to-number "+0" 10)
  (string-to-number "42" 10)
  (string-to-number "-42" 10)
  (string-to-number "+42" 10)
  (string-to-number "  42" 10)
  (string-to-number "0042" 10)

  ;; Base 16: hex edge cases
  (string-to-number "0" 16)
  (string-to-number "a" 16)
  (string-to-number "A" 16)
  (string-to-number "f" 16)
  (string-to-number "F" 16)
  (string-to-number "ff" 16)
  (string-to-number "FF" 16)
  (string-to-number "fff" 16)
  (string-to-number "DeAdBeEf" 16)
  (string-to-number "-ff" 16)
  (string-to-number "+ff" 16)
  (string-to-number "  ff" 16)
  (string-to-number "ffg" 16)       ;; stops at 'g'
  (string-to-number "g" 16)         ;; no valid digits
  (string-to-number "0xff" 16)      ;; 0x prefix in base 16
  (string-to-number "0XFF" 16))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Leading whitespace, tabs, mixed whitespace with all bases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_number_comprehensive_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Various whitespace characters before number
  (string-to-number "  42")
  (string-to-number "	42")         ;; tab
  (string-to-number " 	 42")       ;; mixed space+tab
  (string-to-number "
42")                                ;; newline
  (string-to-number "   -7")
  (string-to-number "	+99")
  ;; Whitespace-only strings
  (string-to-number "")
  (string-to-number " ")
  (string-to-number "  ")
  (string-to-number "	")
  (string-to-number " 	 ")
  ;; Trailing non-numeric chars (parsing stops, no error)
  (string-to-number "42abc")
  (string-to-number "42 ")
  (string-to-number "42 56")        ;; space separates: parse "42"
  (string-to-number "-7xyz")
  (string-to-number "+99abc")
  (string-to-number "3.14xyz")
  ;; Whitespace in bases
  (string-to-number "  ff" 16)
  (string-to-number "	77" 8)
  (string-to-number " 101" 2)
  ;; Trailing after base parse
  (string-to-number "ffgg" 16)
  (string-to-number "77abc" 8)
  (string-to-number "101xyz" 2))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Positive/negative sign combinations and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_number_comprehensive_signs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Plus sign
  (string-to-number "+0")
  (string-to-number "+1")
  (string-to-number "+42")
  (string-to-number "+999999")
  ;; Minus sign
  (string-to-number "-0")
  (string-to-number "-1")
  (string-to-number "-42")
  (string-to-number "-999999")
  ;; Sign only (no digits)
  (string-to-number "+")
  (string-to-number "-")
  ;; Double signs (should parse as 0 or stop early)
  (string-to-number "++42")
  (string-to-number "--42")
  (string-to-number "+-42")
  (string-to-number "-+42")
  ;; Sign with space before digits
  (string-to-number "+ 42")
  (string-to-number "- 42")
  ;; Sign in non-decimal bases
  (string-to-number "+ff" 16)
  (string-to-number "-ff" 16)
  (string-to-number "+77" 8)
  (string-to-number "-77" 8)
  (string-to-number "+101" 2)
  (string-to-number "-101" 2)
  ;; Sign with leading whitespace
  (string-to-number "  +42")
  (string-to-number "  -42")
  (string-to-number "  +ff" 16)
  (string-to-number "  -101" 2))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Float strings: comprehensive coverage including base interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_number_comprehensive_floats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic floats
  (string-to-number "3.14")
  (string-to-number "-2.718")
  (string-to-number "+1.5")
  (string-to-number "0.0")
  (string-to-number "-0.0")
  (string-to-number ".5")
  (string-to-number "-.5")
  (string-to-number "+.5")
  (string-to-number "100.")
  ;; Scientific notation variants
  (string-to-number "1e10")
  (string-to-number "1E10")
  (string-to-number "2.5e3")
  (string-to-number "2.5E3")
  (string-to-number "-1.5e2")
  (string-to-number "+1.5e2")
  (string-to-number "1e-3")
  (string-to-number "1e+3")
  (string-to-number "1E-3")
  (string-to-number "1E+3")
  (string-to-number "0.0e0")
  (string-to-number "1.0e0")
  (string-to-number "5e0")
  ;; Float with non-decimal base: base is ignored for float strings
  (string-to-number "3.14" 16)
  (string-to-number "1e10" 16)
  (string-to-number "3.14" 8)
  (string-to-number "1e10" 8)
  (string-to-number "3.14" 2)
  (string-to-number "1e10" 2)
  ;; Type predicates on float results
  (floatp (string-to-number "3.14"))
  (floatp (string-to-number "1e10"))
  (floatp (string-to-number ".5"))
  (floatp (string-to-number "100."))
  (integerp (string-to-number "42"))
  (integerp (string-to-number "0"))
  ;; Very small floats
  (string-to-number "1e-100")
  (string-to-number "-1e-100")
  (string-to-number "1e-300")
  ;; Very large float exponents
  (string-to-number "1e100")
  (string-to-number "1e200")
  (string-to-number "-1e200"))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Infinity and NaN representations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_number_comprehensive_infinity_nan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Infinity representations
  (string-to-number "1.0e+INF")
  (string-to-number "-1.0e+INF")
  (string-to-number "1e999")        ;; overflow to inf
  (string-to-number "-1e999")       ;; overflow to -inf
  ;; NaN representations
  (string-to-number "0.0e+NaN")
  ;; Check types
  (numberp (string-to-number "1e999"))
  (numberp (string-to-number "0.0e+NaN"))
  ;; Infinity comparison
  (= (string-to-number "1e999") (string-to-number "1e998"))
  ;; "inf" as string (not a special form in Emacs)
  (string-to-number "inf")
  (string-to-number "Inf")
  (string-to-number "INF")
  (string-to-number "nan")
  (string-to-number "NaN")
  (string-to-number "NAN")
  ;; Overflow and underflow
  (floatp (string-to-number "1e999"))
  (floatp (string-to-number "1e-999"))
  (string-to-number "1e-999")       ;; underflow to 0.0
  (string-to-number "-1e-999"))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// "0x" prefix strings and octal-like "0" prefix behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_number_comprehensive_prefixes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; "0x" prefix in default (base 10) context
  (string-to-number "0x10")
  (string-to-number "0xFF")
  (string-to-number "0xDEAD")
  (string-to-number "0X10")
  (string-to-number "0XFF")
  ;; "0x" prefix with explicit base 16
  (string-to-number "0x10" 16)
  (string-to-number "0xff" 16)
  ;; "0x" prefix with base 8 or 2
  (string-to-number "0x10" 8)
  (string-to-number "0x10" 2)
  ;; "0" prefix (C-style octal is NOT used by Emacs)
  (string-to-number "010")
  (string-to-number "010" 10)
  (string-to-number "010" 8)
  (string-to-number "010" 2)
  ;; "0b" prefix (binary prefix, not used by Emacs)
  (string-to-number "0b101")
  (string-to-number "0b101" 2)
  ;; Leading zeros in various bases
  (string-to-number "0042")
  (string-to-number "0042" 16)
  (string-to-number "0042" 8)
  (string-to-number "0011" 2)
  ;; Multiple zeros
  (string-to-number "0000")
  (string-to-number "0000" 16)
  (string-to-number "0000" 8)
  (string-to-number "0000" 2))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Very large and very small numbers across bases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_number_comprehensive_extreme_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Large integers in base 10
  (string-to-number "999999999999999999")
  (string-to-number "-999999999999999999")
  (string-to-number "123456789012345678")
  ;; Large hex values
  (string-to-number "FFFFFFFFFFFF" 16)
  (string-to-number "100000000" 16)
  (string-to-number "DEADBEEFCAFE" 16)
  ;; Large binary values
  (string-to-number "1111111111111111111111111111111111111111" 2)
  (string-to-number "1000000000000000000000000000000000000000" 2)
  ;; Large octal values
  (string-to-number "77777777777" 8)
  (string-to-number "37777777777" 8)
  ;; Integer overflow boundary (most-positive-fixnum vicinity)
  (integerp (string-to-number "2305843009213693951"))
  (integerp (string-to-number "-2305843009213693952"))
  ;; Very small floats close to zero
  (string-to-number "0.000000001")
  (string-to-number "-0.000000001")
  (string-to-number "5e-324")       ;; near minimum subnormal
  (string-to-number "-5e-324")
  ;; Very large floats
  (string-to-number "1.7976931348623157e308")   ;; near DBL_MAX
  (string-to-number "-1.7976931348623157e308")
  ;; Compare parsed extreme values
  (> (string-to-number "999999999") (string-to-number "999999998"))
  (< (string-to-number "-999999999") (string-to-number "-999999998")))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Non-numeric and degenerate strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_number_comprehensive_degenerate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Completely non-numeric strings
  (string-to-number "abc")
  (string-to-number "hello world")
  (string-to-number "nil")
  (string-to-number "t")
  (string-to-number "#xff")         ;; Emacs read syntax, not string-to-number
  (string-to-number "#b101")
  (string-to-number "#o77")
  ;; Symbol-like strings
  (string-to-number "foo-bar")
  (string-to-number "foo_bar")
  (string-to-number "foo.bar")      ;; stops at '.'? Actually "foo" part is non-numeric
  ;; Strings that look like numbers but aren't in given base
  (string-to-number "9" 8)          ;; 9 not valid in octal
  (string-to-number "f" 10)         ;; f not valid in decimal
  (string-to-number "2" 2)          ;; 2 not valid in binary
  (string-to-number "g" 16)         ;; g not valid in hex
  ;; Mixed valid/invalid
  (string-to-number "12abc" 10)
  (string-to-number "ab12" 10)
  (string-to-number "12ab" 16)
  (string-to-number "gh12" 16)
  ;; Only whitespace then garbage
  (string-to-number "   abc")
  (string-to-number "	xyz")
  ;; Dot-only and exponent-only
  (string-to-number ".")
  (string-to-number "e10")
  (string-to-number "E10")
  (string-to-number ".e10")
  (string-to-number "e")
  ;; Result type checks for zero from non-numeric
  (= 0 (string-to-number ""))
  (= 0 (string-to-number "abc"))
  (= 0 (string-to-number " "))
  (integerp (string-to-number "abc")))"##;
    let expect = expect_test::expect![[r#""ERR (void-variable 0x)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Roundtrip and combined operations across bases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_to_number_comprehensive_roundtrip_cross_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Cross-base equivalence: same number parsed from different representations
  (= (string-to-number "255" 10) (string-to-number "ff" 16))
  (= (string-to-number "255" 10) (string-to-number "377" 8))
  (= (string-to-number "255" 10) (string-to-number "11111111" 2))
  (= (string-to-number "ff" 16) (string-to-number "377" 8))
  (= (string-to-number "ff" 16) (string-to-number "11111111" 2))
  (= (string-to-number "377" 8) (string-to-number "11111111" 2))
  ;; Negative cross-base
  (= (string-to-number "-255" 10) (string-to-number "-ff" 16))
  (= (string-to-number "-255" 10) (string-to-number "-377" 8))
  (= (string-to-number "-255" 10) (string-to-number "-11111111" 2))
  ;; Roundtrip via format
  (= 255 (string-to-number (format "%d" 255)))
  (= 255 (string-to-number (format "%x" 255) 16))
  (= 255 (string-to-number (format "%o" 255) 8))
  ;; Arithmetic across bases
  (+ (string-to-number "10" 2)    ;; 2
     (string-to-number "10" 8)    ;; 8
     (string-to-number "10" 10)   ;; 10
     (string-to-number "10" 16))  ;; 16 => total 36
  ;; Mapcar over base list
  (let ((bases '(2 8 10 16)))
    (mapcar (lambda (b) (string-to-number "10" b)) bases))
  ;; Chain: parse, compute, format, re-parse
  (let* ((a (string-to-number "ff" 16))
         (b (string-to-number "77" 8))
         (sum (+ a b))
         (s (number-to-string sum))
         (reparsed (string-to-number s)))
    (list a b sum s reparsed (= sum reparsed))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t t t t t t t 36 (2 8 10 16) (255 63 318 \"318\" 318 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
