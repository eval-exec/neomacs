//! Divergence tests: syntax table, parse-partial, scan-lists deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_syntax_table_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'make-syntax-table)
  (fboundp 'copy-syntax-table)
  (fboundp 'set-syntax-table)
  (fboundp 'syntax-table)
  (fboundp 'modify-syntax-entry))"#,
        expect,
    );
}

#[test]
fn divergence_char_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (119 32 40 41 34 60)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (char-syntax ?a)
  (char-syntax ? )
  (char-syntax ?()
  (char-syntax ?))
  (char-syntax ?\")
  (char-syntax ?\;)) "#,
        expect,
    );
}

#[test]
fn divergence_parse_partial_sexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (scan-error \"Unbalanced parentheses\" 1 21)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(foo (bar baz) quux)")
  (list (parse-partial-sexp 1 5)
        (parse-partial-sexp 1 20)
        (scan-lists 1 1 0)
        (scan-lists 1 1 1))) "#,
        expect,
    );
}

#[test]
fn divergence_forward_sexps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 21 11 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(foo bar) (baz quux)")
  (goto-char 1)
  (forward-sexp 1)
  (let ((pos1 (point)))
    (forward-sexp 1)
    (list pos1 (point)
          (progn (backward-sexp 1) (point))
          (progn (backward-sexp 1) (point))))) "#,
        expect,
    );
}

#[test]
fn divergence_scan_lists_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (scan-error \"Unbalanced parentheses\" 1 26)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(a (b c) d (e (f g) h) i)")
  (list (scan-lists 1 1 0)
        (scan-lists 1 2 0)
        (scan-lists 1 -1 0)
        (scan-lists 1 1 1))) "#,
        expect,
    );
}

#[test]
fn divergence_forward_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 18 nil 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "foo ; comment\nbar")
  (list (forward-comment 1)
        (point)
        (forward-comment -1)
        (point))) "#,
        expect,
    );
}

#[test]
fn divergence_syntax_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments aref 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((st (syntax-table)))
  (list (aref (syntax-table) ?a)
        (aref (syntax-table) ?()
        (aref (syntax-table) ?)
        (syntax-class (aref (syntax-table) ?a))
        (syntax-class (aref (syntax-table) ?()) ))) "#,
        expect,
    );
}

#[test]
fn divergence_indent_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t (1 1 17 nil nil nil 0 nil nil (1) nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(defun foo ()\n  (bar))")
  (list (fboundp 'calculate-lisp-indent)
        (fboundp 'lisp-indent-function)
        (parse-partial-sexp 1 22))) "#,
        expect,
    );
}

#[test]
fn divergence_matching_paren() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 41 40 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'matching-paren)
  (matching-paren ?()
  (matching-paren ?))
  (matching-paren ?a)
  (matching-paren ?{)) "#,
        expect,
    );
}

#[test]
fn divergence_syntax_ppss() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(foo \"bar\\\"baz\" quux)")
  (let ((ppss (parse-partial-sexp 1 21)))
    (list (nth 0 ppss)
          (nth 3 ppss)
          (nth 8 ppss)))) "#,
        expect,
    );
}
