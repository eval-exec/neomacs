//! Divergence tests: reader edge cases, read-from-string, syntax deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_read_from_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (((a b c) . 7) (42 . 2) (\"hello\" . 7) (foo-bar . 7))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (read-from-string "(a b c)")
  (read-from-string "42")
  (read-from-string "\"hello\"")
  (read-from-string "foo-bar")) "#,
        expect,
    );
}

#[test]
fn divergence_read_from_string_offset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integerp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (read-from-string "  (a b) (c d)" t nil)
  (read-from-string "42 99" t nil)) "#,
        expect,
    );
}

#[test]
fn divergence_read_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (([1 2 3] . 7) (a . b) 7 (65 . 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (read-from-string "[1 2 3]")
  (car (read-from-string "(a . b)"))
  (cdr (read-from-string "(a . b)"))
  (read-from-string "?A")) "#,
        expect,
    );
}

#[test]
fn divergence_read_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp (foo . 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (intern-soft (symbol-name (read-from-string "foo")))
  (symbol-name (car (read-from-string ":hello")))
  (keywordp (car (read-from-string ":hello")))) "#,
        expect,
    );
}

#[test]
fn divergence_print_readably() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"\\\"hello\\\\nworld\\\"\" \"\\\"tab\\\\11here\\\"\" \"\\\"normal\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-escape-newlines t)
        (print-escape-control-characters t))
  (list (prin1-to-string "hello\nworld")
        (prin1-to-string "tab\there")
        (prin1-to-string "normal"))) "#,
        expect,
    );
}

#[test]
fn divergence_print_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"(1 2 3 ...)\" \"\\\"abcdefghij\\\"\" \"[1 2 3 ...]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-length 3))
  (list (prin1-to-string '(1 2 3 4 5))
        (prin1-to-string "abcdefghij")
        (prin1-to-string [1 2 3 4 5]))) "#,
        expect,
    );
}

#[test]
fn divergence_print_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"(a (b ...))\" \"(1 2 3)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((print-level 2))
  (list (prin1-to-string '(a (b (c (d)))))
        (prin1-to-string '(1 2 3)))) "#,
        expect,
    );
}

#[test]
fn divergence_print_circle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((x (list 1 2)))
  (nconc x x)
  (let ((print-circle t))
    (list (stringp (prin1-to-string x))
          (> (length (prin1-to-string x)) 0)))) "#,
        expect,
    );
}

#[test]
fn divergence_print_gensym() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((sym (make-symbol "gensym-test")))
  (let ((print-gensym t))
    (list (stringp (prin1-to-string sym))
          (> (length (prin1-to-string sym)) 0)
          (string-match "gensym-test" (prin1-to-string sym))))) "#,
        expect,
    );
}

#[test]
fn divergence_format_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"00042\" \"hi        \" \"+5\" \"ff\" \"10\" \"3.140000e+00\" \"3.140000\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "%05d" 42)
  (format "%-10s" "hi")
  (format "%+d" 5)
  (format "%x" 255)
  (format "%o" 8)
  (format "%e" 3.14)
  (format "%f" 3.14)) "#,
        expect,
    );
}
