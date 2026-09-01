//! Oracle parity tests for comprehensive number conversion operations.
//!
//! Covers: `string-to-number` with various bases (2, 8, 10, 16),
//! `number-to-string`, float-to-int conversions via `truncate`/`floor`/
//! `ceiling`/`round`, edge cases (empty string, non-numeric, whitespace,
//! leading zeros), hex/octal/binary literal parsing, `format` with numeric
//! specifiers, and mixed int/float arithmetic coercion.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 1. string-to-number with explicit base arguments (2, 8, 10, 16)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numconv_string_to_number_bases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Binary (base 2)
  (string-to-number "1010" 2)
  (string-to-number "11111111" 2)
  (string-to-number "0" 2)
  (string-to-number "1" 2)
  ;; Octal (base 8)
  (string-to-number "77" 8)
  (string-to-number "0" 8)
  (string-to-number "377" 8)
  (string-to-number "12" 8)
  ;; Decimal (base 10, default)
  (string-to-number "42")
  (string-to-number "42" 10)
  (string-to-number "-100" 10)
  (string-to-number "0" 10)
  ;; Hexadecimal (base 16)
  (string-to-number "ff" 16)
  (string-to-number "FF" 16)
  (string-to-number "0" 16)
  (string-to-number "deadbeef" 16)
  (string-to-number "1a2b" 16))"#;
    let expect = expect_test::expect![[
        r#""OK (10 255 0 1 63 0 255 10 42 42 -100 0 255 255 0 3735928559 6699)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 2. number-to-string for integers and floats
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numconv_number_to_string_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (number-to-string 0)
  (number-to-string 1)
  (number-to-string -1)
  (number-to-string 42)
  (number-to-string -999)
  (number-to-string 1000000)
  ;; Floats
  (number-to-string 0.0)
  (number-to-string 3.14)
  (number-to-string -2.718)
  (number-to-string 1.0e5)
  (number-to-string 1.5e-3)
  ;; Roundtrip: number->string->number
  (string-to-number (number-to-string 12345))
  (string-to-number (number-to-string -67890)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"0\" \"1\" \"-1\" \"42\" \"-999\" \"1000000\" \"0.0\" \"3.14\" \"-2.718\" \"100000.0\" \"0.0015\" 12345 -67890)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 3. float-to-int conversions: truncate, floor, ceiling, round
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numconv_float_to_int_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((vals '(3.7 3.2 3.5 -3.7 -3.2 -3.5 0.0 0.5 -0.5
                       4.5 5.5 100.9 -100.9 1.0 -1.0)))
  (list
   (mapcar #'truncate vals)
   (mapcar #'floor vals)
   (mapcar #'ceiling vals)
   (mapcar #'round vals)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 3 3 -3 -3 -3 0 0 0 4 5 100 -100 1 -1) (3 3 3 -4 -4 -4 0 0 -1 4 5 100 -101 1 -1) (4 4 4 -3 -3 -3 0 1 0 5 6 101 -100 1 -1) (4 3 4 -4 -3 -4 0 0 0 4 6 101 -101 1 -1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 4. string-to-number edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numconv_string_to_number_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Empty string
  (string-to-number "")
  ;; Non-numeric strings
  (string-to-number "hello")
  (string-to-number "abc")
  ;; Leading whitespace
  (string-to-number "  42")
  (string-to-number " -7")
  ;; Trailing non-numeric characters
  (string-to-number "42abc")
  (string-to-number "123.456xyz")
  ;; Leading zeros
  (string-to-number "007")
  (string-to-number "00100")
  ;; Plus sign
  (string-to-number "+42")
  ;; Whitespace only
  (string-to-number "   ")
  ;; Dot only
  (string-to-number ".")
  ;; Multiple signs
  (string-to-number "--42")
  ;; Float strings
  (string-to-number "3.14")
  (string-to-number ".5")
  (string-to-number "1e10")
  (string-to-number "1.5e-3"))"#;
    let expect = expect_test::expect![[
        r#""OK (0 0 0 42 -7 42 123.456 7 100 42 0 0 0 3.14 0.5 10000000000.0 0.0015)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 5. Hexadecimal, octal, binary literal parsing in Elisp reader
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numconv_literal_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Hex literals
  #xff
  #x0
  #x1A2B
  #xDEAD
  ;; Octal literals
  #o77
  #o0
  #o377
  #o10
  ;; Binary literals
  #b1010
  #b0
  #b11111111
  #b1
  ;; Verify types
  (integerp #xff)
  (integerp #o77)
  (integerp #b1010)
  ;; Arithmetic with literals
  (+ #xff 1)
  (+ #o10 #b1010)
  (* #x10 #o10))"#;
    let expect =
        expect_test::expect![[r#""OK (255 0 6699 57005 63 0 255 8 10 0 255 1 t t t 256 18 128)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 6. format with numeric specifiers %d, %x, %o, %e, %f, %g
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numconv_format_numeric_specifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; %d decimal
  (format "%d" 42)
  (format "%d" -42)
  (format "%d" 0)
  (format "%05d" 42)
  (format "%+d" 42)
  ;; %x hexadecimal
  (format "%x" 255)
  (format "%X" 255)
  (format "%08x" 255)
  (format "%x" 0)
  ;; %o octal
  (format "%o" 8)
  (format "%o" 255)
  (format "%o" 0)
  ;; %e scientific notation
  (format "%e" 3.14)
  (format "%e" 0.001)
  ;; %f fixed point
  (format "%f" 3.14)
  (format "%.2f" 3.14159)
  (format "%.0f" 3.7)
  ;; Multiple specifiers
  (format "dec=%d hex=%x oct=%o" 42 42 42)
  ;; Padding
  (format "|%10d|" 42)
  (format "|%-10d|" 42))"#;
    let expect = expect_test::expect![[
        r#""OK (\"42\" \"-42\" \"0\" \"00042\" \"+42\" \"ff\" \"FF\" \"000000ff\" \"0\" \"10\" \"377\" \"0\" \"3.140000e+00\" \"1.000000e-03\" \"3.140000\" \"3.14\" \"4\" \"dec=42 hex=2a oct=52\" \"|        42|\" \"|42        |\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 7. Mixed int/float arithmetic coercion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numconv_mixed_arithmetic_coercion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; int + float -> float
  (+ 1 1.5)
  (floatp (+ 1 1.5))
  ;; int * float -> float
  (* 3 2.0)
  (floatp (* 3 2.0))
  ;; int / int -> int (truncated)
  (/ 7 2)
  (integerp (/ 7 2))
  ;; int / float -> float
  (/ 7 2.0)
  (floatp (/ 7 2.0))
  ;; float - int -> float
  (- 5.5 3)
  ;; Comparisons across types
  (= 3 3.0)
  (< 2 2.5)
  (> 3.0 2)
  ;; Chain of mixed ops
  (+ 1 2.0 3 4.0)
  (* 2 3.0 4)
  ;; Explicit float conversion
  (float 42)
  (floatp (float 42))
  ;; Explicit truncation
  (truncate 7.9)
  (integerp (truncate 7.9)))"#;
    let expect =
        expect_test::expect![[r#""OK (2.5 t 6.0 t 3 t 3.5 t 2.5 t t t 10.0 24.0 42.0 t 7 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 8. string-to-number with various bases and invalid digits
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numconv_base_invalid_digits() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Binary with invalid digit '2'
  (string-to-number "1012" 2)
  ;; Octal with invalid digit '9'
  (string-to-number "789" 8)
  ;; Hex with invalid chars
  (string-to-number "xyz" 16)
  ;; Valid prefix then invalid
  (string-to-number "1010xyz" 2)
  (string-to-number "77abc" 8)
  ;; Base edge cases
  (string-to-number "10" 2)
  (string-to-number "10" 8)
  (string-to-number "10" 10)
  (string-to-number "10" 16)
  ;; Negative with base
  (string-to-number "-ff" 16)
  (string-to-number "-1010" 2)
  (string-to-number "-77" 8))"#;
    let expect = expect_test::expect![[r#""OK (5 7 0 10 63 2 8 10 16 -255 -10 -63)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 9. Roundtrip conversions: number -> string -> number
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numconv_roundtrip_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((integers '(0 1 -1 42 -42 999 -999 100000 -100000))
      (test-roundtrip-int
       (lambda (n)
         (= n (string-to-number (number-to-string n))))))
  (list
   ;; All integer roundtrips succeed
   (mapcar test-roundtrip-int integers)
   ;; Format-based roundtrips
   (mapcar (lambda (n)
             (= n (string-to-number (format "%d" n))))
           integers)
   ;; Hex roundtrip via format
   (mapcar (lambda (n)
             (= n (string-to-number (format "%x" n) 16)))
           '(0 1 15 16 255 256 4096))
   ;; Octal roundtrip via format
   (mapcar (lambda (n)
             (= n (string-to-number (format "%o" n) 8)))
           '(0 1 7 8 63 64 511))))"#;
    let expect = expect_test::expect![[
        r#""OK ((t t t t t t t t t) (t t t t t t t t t) (t t t t t t t) (t t t t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 10. Comprehensive floor/ceiling/round/truncate with edge values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numconv_rounding_edge_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Exact integers passed to rounding functions
  (truncate 5)
  (floor 5)
  (ceiling 5)
  (round 5)
  ;; Banker's rounding (round-half-to-even)
  (round 0.5)
  (round 1.5)
  (round 2.5)
  (round 3.5)
  (round 4.5)
  (round -0.5)
  (round -1.5)
  (round -2.5)
  ;; Very small values
  (truncate 0.001)
  (floor 0.001)
  (ceiling 0.001)
  (round 0.001)
  (truncate -0.001)
  (floor -0.001)
  (ceiling -0.001)
  (round -0.001)
  ;; Values just above/below integer boundaries
  (floor 2.9999)
  (ceiling 2.0001)
  (truncate -2.9999)
  (floor -2.0001))"#;
    let expect =
        expect_test::expect![[r#""OK (5 5 5 5 0 2 2 4 4 0 -2 -2 0 0 1 0 0 -1 0 0 2 3 -2 -3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 11. Number type predicates after conversions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numconv_type_predicates_after_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; string-to-number returns integer for integer strings
  (integerp (string-to-number "42"))
  ;; string-to-number returns float for float strings
  (floatp (string-to-number "3.14"))
  ;; truncate/floor/ceiling/round return integer
  (integerp (truncate 3.7))
  (integerp (floor 3.7))
  (integerp (ceiling 3.7))
  (integerp (round 3.7))
  ;; float returns float
  (floatp (float 42))
  ;; Type after mixed arithmetic
  (integerp (+ 1 2))
  (floatp (+ 1 2.0))
  (integerp (* 3 4))
  (floatp (* 3 4.0))
  (integerp (/ 10 3))
  (floatp (/ 10 3.0))
  ;; number-to-string always returns string
  (stringp (number-to-string 42))
  (stringp (number-to-string 3.14)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 12. Number formatting with format for comprehensive patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_numconv_format_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Width and precision
  (format "%10.2f" 3.14159)
  (format "%-10.2f" 3.14159)
  (format "%010.2f" 3.14159)
  ;; Character from number
  (format "%c" 65)
  (format "%c" 97)
  (format "%c" 48)
  ;; Multiple number formats in one string
  (format "int=%d float=%.2f hex=%x" 42 3.14 255)
  ;; %s with numbers (converts via princ)
  (format "%s" 42)
  (format "%s" 3.14)
  ;; %S with numbers (converts via prin1)
  (format "%S" 42)
  (format "%S" 3.14)
  ;; Padding with zeros
  (format "%03d:%02d:%02d" 9 5 7)
  ;; Negative numbers with width
  (format "%10d" -42)
  (format "%-10d" -42))"#;
    let expect = expect_test::expect![[
        r#""OK (\"      3.14\" \"3.14      \" \"0000003.14\" \"A\" \"a\" \"0\" \"int=42 float=3.14 hex=ff\" \"42\" \"3.14\" \"42\" \"3.14\" \"009:05:07\" \"       -42\" \"-42       \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
