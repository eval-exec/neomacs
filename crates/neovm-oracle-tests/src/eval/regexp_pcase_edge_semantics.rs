//! Oracle parity for eval, regexp-quote, match-string, and pcase.
//! GNU src/eval.c, src/search.c, lisp/emacs-lisp/pcase.el.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

// --- eval (in-process) ---

#[test]
fn oracle_eval_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(eval '(+ 1 2))"#, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_eval_quoted_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(eval '(list 1 2 3))"#, expect);
    assert_ok_eq("(1 2 3)", &o, &n);
}

#[test]
fn oracle_eval_self_evaluating() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(eval 42)"#, expect);
    assert_ok_eq("42", &o, &n);
}

// --- regexp-quote (via binary, needs full library) ---

#[test]
fn oracle_regexp_quote_escapes_special_chars_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a\\\\.b\\\\*c\\\\[d]e\\\\^f\\\\$g\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(regexp-quote "a.b*c[d]e^f$g")"#, expect);
    // prin1 of regexp-quoted string: each backslash is printed as \\
    assert_ok_eq("\"a\\\\.b\\\\*c\\\\[d]e\\\\^f\\\\$g\"", &o, &n);
}

#[test]
fn oracle_regexp_quote_no_special_chars_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(regexp-quote "hello")"#, expect);
    assert_ok_eq("\"hello\"", &o, &n);
}

// --- match-string (via binary) ---

#[test]
fn oracle_match_string_after_string_match_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (string-match "[a-z]+" "hello world") (match-string 0 "hello world"))"#,
        expect,
    );
    assert_ok_eq("\"hello\"", &o, &n);
}

// --- pcase (via binary, needs full library) ---

#[test]
fn oracle_pcase_literal_match_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK forty-two""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(pcase 42 (1 'one) (42 'forty-two) (_ 'other))"#,
        expect,
    );
    assert_ok_eq("forty-two", &o, &n);
}

#[test]
fn oracle_pcase_wildcard_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK other""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(pcase 99 (1 'one) (_ 'other))"#, expect);
    assert_ok_eq("other", &o, &n);
}
