//! Number/float (print precision + read round-trip, ldexp/frexp/copysign,
//! bignum arithmetic, predicates, round/truncate/floor/ceiling modes),
//! format (%s/%S, width/precision, %*, flags, edge types), and reader
//! syntax (radix/char escapes, string escapes, odd symbols, special forms,
//! float syntax) parity, plus the trailing-dot integer divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn float_print_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"0.3\" \"0.30000000000000004\" \"1e-300\" \"1e+300\" \"123456789.12345679\" \"2.220446049250313e-16\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (prin1-to-string 0.3) (prin1-to-string (+ 0.1 0.2))
        (prin1-to-string 1e-300) (prin1-to-string 1e300)
        (prin1-to-string 123456789.123456789) (prin1-to-string 2.220446049250313e-16))"##,
        expect,
    );
}

#[test]
fn float_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((vals '(0.1 0.2 0.3 3.141592653589793 2.718281828459045 1.7976931348623157e308)))
  (cl-every (lambda (v) (= v (car (read-from-string (prin1-to-string v))))) vals))"##,
        expect,
    );
}

#[test]
fn float_special_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (12.0 (0.75 . 4) -3.0 3 1.4142135623730951 1 -5.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (ldexp 1.5 3) (frexp 12.0) (copysign 3.0 -1.0)
        (logb 8.0) (expt 2.0 0.5) (expt -8 (/ 1 3)) (float -5))"##,
        expect,
    );
}

#[test]
fn int_bignum_arith() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (10000000000000000000000000000000000000000 1267650600228229401496703205375 142857142857142857142857142857 1 1606938044258990275541962092341162602522202993782792835301376)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (* (expt 10 20) (expt 10 20)) (- (expt 2 100) 1)
        (/ (expt 10 30) 7) (% (expt 10 30) 7) (expt 2 200))"##,
        expect,
    );
}

#[test]
fn number_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-plusp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (natnump 5) (natnump -1) (zerop 0.0) (cl-plusp 3) (cl-minusp -2)
        (= 1 1.0) (eql 1 1.0) (eql 1.0 1.0) (floatp 1.0) (integerp 1))"##,
        expect,
    );
}

#[test]
fn round_truncate_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 4 -2 2 -3 3 4 3 1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (round 2.5) (round 3.5) (round -2.5) (truncate 2.9)
        (floor -2.1) (ceiling 2.1) (round 7 2) (floor 7 2) (mod 7 3) (mod -7 3))"##,
        expect,
    );
}

#[test]
fn format_edge_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"65\" \"ff\" \"100\" \"0.000000e+00\" \"100000\" \"1e+06\" \"nil\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%d" ?A) (format "%x" 255) (format "%o" 64)
        (format "%e" 0.0) (format "%g" 100000.0) (format "%g" 1000000.0) (format "%S" nil))"##,
        expect,
    );
}

#[test]
fn format_message_pct() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"50%\" \"A\" \"HI\" \"  5|5  |\" \"03.10\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%d%%" 50) (format "%c" ?A) (format "%c%c" 72 73)
        (format "%3d|%-3d|" 5 5) (format "%05.2f" 3.1))"##,
        expect,
    );
}

#[test]
fn format_s_S() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"(1 2 3)\" \"(1 2 3)\" \"str\" \"\\\"str\\\"\" \"65\" \"65\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%s" '(1 2 3)) (format "%S" '(1 2 3))
        (format "%s" "str") (format "%S" "str") (format "%s" ?A) (format "%S" ?A))"##,
        expect,
    );
}

#[test]
fn format_star_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid format operation %*\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%*d" 5 42) (format "%-*d|" 5 42) (format "%.*f" 2 3.14159))"##,
        expect,
    );
}

#[test]
fn format_width_prec() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"        hi|\" \"hi        |\" \"hel\" \"   3.142\" \"+5\" \" 5\" \"0xff\" \"010\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%10s|" "hi") (format "%-10s|" "hi") (format "%.3s" "hello")
        (format "%8.3f" 3.14159) (format "%+d" 5) (format "% d" 5) (format "%#x" 255) (format "%#o" 8))"##,
        expect,
    );
}

#[test]
fn read_float_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1.5 0.5 10000000000.0 -0.0015 1.0e+INF 2.5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "1.5") (read ".5") (read "1e10") (read "-1.5e-3") (read "1.0e+INF") (read "2.5e0"))"##,
        expect,
    );
}

#[test]
fn read_radix_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (31 15 10 35 65 65 233)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "#x1F") (read "#o17") (read "#b1010") (read "#36rZ")
        (read "?\\x41") (read "?\\101") (read "?\\u00e9"))"##,
        expect,
    );
}

#[test]
fn read_special_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ('(1 . 2) `(a ,b) [1 2 3] #'car (1 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "'(1 . 2)") (read "`(a ,b)") (read "[1 2 3]")
        (read "#'car") (car (read-from-string "(1 2) trailing")))"##,
        expect,
    );
}

#[test]
fn read_string_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"a\tb\\nc\" 1 \"AB\" \"linecont\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "\"a\\tb\\nc\"") (length (read "\"\\u00e9\""))
        (read "\"\\x41\\x42\"") (read "\"line\\\ncont\""))"##,
        expect,
    );
}

#[test]
fn read_symbols_odd() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"foo bar\" \\+1 t 1500.0 -0.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (symbol-name (read "foo\\ bar")) (read "\\+1")
        (symbolp (read "1+")) (read "1.5e3") (read "-0.0"))"##,
        expect,
    );
}

#[test]
fn divergence_read_trailing_dot_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 42 -7 t integer t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "1.") (read "42.") (read "-7.")
      (integerp (read "1.")) (type-of (read "10."))
      (eq (read "100.") 100))"##,
        expect,
    );
}
