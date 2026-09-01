//! format conversion edges: %c (emoji/codepoint 0/wide), %x/%o/%X of negatives
//! and bignums, %d of floats (truncation) + type errors, %s of all types, %.*f
//! and %*.*f star args, %#x/%#o of 0, %N$ field reuse, %c width/pad.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn format_c_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"A\" \"日\" \"\\0\" \"😀\" 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%c" 65) (format "%c" ?日) (format "%c" 0)
        (format "%c" 128512) (length (format "%c" ?😀)))"##,
        expect,
    );
}

#[test]
fn format_char_string_mix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"[abc]\" \"    x\" \"y    |\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "[%c%c%c]" ?a ?b ?c)
        (format "%5c" ?x) (format "%-5c|" ?y))"##,
        expect,
    );
}

#[test]
fn format_d_nonint() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"3\" \"-3\" \"3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (list (format "%d" 3.9) (format "%d" -3.9) (format "%d" 3.2)) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn format_d_string_err() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (error error \"1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (format "%d" "x") (error (car e)))
        (condition-case e (format "%c" "y") (error (car e)))
        (condition-case e (format "%x" 1.5) (error (car e))))"##,
        expect,
    );
}

#[test]
fn format_field_reuse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"a-a-b\" \"2 1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%1$s-%1$s-%2$s" "a" "b")
        (format "%2$d %1$d" 1 2))"##,
        expect,
    );
}

#[test]
fn format_o_x_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"0\" \"0\" \"0\" \"0\" \"0\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%x" 0) (format "%o" 0) (format "%#x" 0) (format "%#o" 0) (format "%X" 0))"##,
        expect,
    );
}

#[test]
fn format_s_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"42\" \"3.14\" \"nil\" \"sym\" \"(1 2)\" \"[1 2]\" \"65\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%s" 42) (format "%s" 3.14) (format "%s" nil)
        (format "%s" 'sym) (format "%s" '(1 2)) (format "%s" [1 2]) (format "%s" ?A))"##,
        expect,
    );
}

#[test]
fn format_star_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid format operation %*\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%.*f" 3 3.14159) (format "%*.*f" 10 2 3.14159)
        (format "%-*d|" 5 7))"##,
        expect,
    );
}

#[test]
fn format_x_bignum_neg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"10000000000000000\" \"-10000000000000000\" \"-1267650600228229401496703205376\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%x" (expt 2 64)) (format "%x" (- (expt 2 64)))
        (format "%d" (- (expt 2 100))))"##,
        expect,
    );
}

#[test]
fn format_x_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"-1\" \"-ff\" \"-10\" \"-10\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (format "%x" -1) (format "%x" -255) (format "%o" -8) (format "%X" -16))"##,
        expect,
    );
}
