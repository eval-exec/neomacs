//! Divergence tests: print circle, read circle, gensym deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_print_circle_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"(#1=(1 2) #1#)\" \"[#1=(1 2) #1#]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((x (list 1 2)))
  (let ((print-circle t))
    (list (prin1-to-string (list x x))
          (prin1-to-string (vector x x))))) "#,
        expect,
    );
}

#[test]
fn divergence_print_circle_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"((1 2) (1 2))\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((x (list 1 2)))
  (let ((print-circle nil))
    (list (prin1-to-string (list x x))
          (stringp (prin1-to-string x))))) "#,
        expect,
    );
}

#[test]
fn divergence_read_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"(a (b c) d)\" ((a (b c) d) . 11) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((obj '(a (b c) d)))
  (let ((s (prin1-to-string obj)))
    (list s (read-from-string s)
          (equal obj (car (read-from-string s)))))) "#,
        expect,
    );
}

#[test]
fn divergence_gensym() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((g (gensym)))
  (list (symbolp g)
        (not (null g))
        (null (intern-soft (symbol-name g)))
        (let ((print-gensym t))
          (string-match "gensym" (prin1-to-string g))))) "#,
        expect,
    );
}

#[test]
fn divergence_gensym_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((g (gensym "test-prefix-")))
  (list (symbolp g)
        (string-match "test-prefix" (symbol-name g))
        (not (eq g (gensym "test-prefix-"))))) "#,
        expect,
    );
}

#[test]
fn divergence_print_gensym_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'print-gensym)
  (booleanp print-gensym)
  (boundp 'print-circle)
  (booleanp print-circle)) "#,
        expect,
    );
}

#[test]
fn divergence_print_escape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'print-escape-newlines)
  (boundp 'print-escape-control-characters)
  (boundp 'print-escape-multibyte)
  (boundp 'print-escape-nonascii)
  (booleanp print-escape-newlines)
  (booleanp print-escape-control-characters)) "#,
        expect,
    );
}

#[test]
fn divergence_prin1_princ() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function princ-to-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (stringp (prin1-to-string "hello\nworld"))
  (stringp (princ-to-string "hello\nworld"))
  (not (equal (prin1-to-string "hello\nworld")
              (princ-to-string "hello\nworld")))) "#,
        expect,
    );
}

#[test]
fn divergence_print_array() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"[1 2 3]\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((v [1 2 3])
        (s (prin1-to-string v)))
  (list s
        (equal (read-from-string s) (cons v 7))
        (vectorp (car (read-from-string s))))) "#,
        expect,
    );
}

#[test]
fn divergence_print_string_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"\\\"hello\tworld\\\"\" \"\\\"quote\\\\\\\"inside\\\"\" \"\\\"back\\\\\\\\slash\\\"\" 3 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (prin1-to-string "hello\tworld")
  (prin1-to-string "quote\"inside")
  (prin1-to-string "back\\slash")
  (length (prin1-to-string "a"))
  (> (length (prin1-to-string "\n")) 1)) "#,
        expect,
    );
}
