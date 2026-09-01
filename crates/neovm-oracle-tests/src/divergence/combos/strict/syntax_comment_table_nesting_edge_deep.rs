//! Strict combo oracle probes, batch 108: syntax table edge cases — comment
//! syntax classes, nesting syntax, syntax-pp cache, forward-comment across
//! comment styles, and syntax-table text-property interaction.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_s2_syntax_comment_classes_forward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function syntax-pp)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "code ; comment\nmore ;; double\n")
  (goto-char 1)
  (list (syntax-after 6)
        (char-syntax ?\;)
        (progn (forward-comment 1) (point))
        (progn (forward-comment 1) (point))
        (nth 4 (syntax-pp 6))
        (nth 4 (syntax-pp 1))))
"####,
        expect,
    );
}

#[test]
fn div_s2_syntax_table_text_property_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function syntax-pp)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(with-temp-buffer
  (insert "(a (b) c)")
  (put-text-property 2 3 'syntax-table (string-to-syntax "_"))
  (goto-char 1)
  (list (nth 0 (syntax-pp 1))
        (nth 0 (syntax-pp 2))
        (scan-sexps 1 1)
        (nth 0 (parse-partial-sexp 1 4))))
"####,
        expect,
    );
}

#[test]
fn div_s2_syntax_nesting_depth_and_list_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function syntax-pp)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(with-temp-buffer
  (insert "(((a) b) (c (d)))")
  (goto-char 1)
  (list (car (syntax-pp 1))
        (car (syntax-pp 5))
        (car (syntax-pp 10))
        (condition-case err (scan-lists 1 1 0) (scan-error (car err)))
        (condition-case err (scan-lists 1 -1 0) (scan-error (car err)))
        (down-list)
        (point)
        (down-list)
        (point)
        (up-list -1)
        (point)))
"####,
        expect,
    );
}

#[test]
fn div_s2_syntax_string_fence_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function syntax-pp)""#]];
    crate::common::assert_oracle_parity_expect(
        r####"
(with-temp-buffer
  (insert "(concat \"hello world\" foo)")
  (goto-char 1)
  (list (nth 3 (syntax-pp 10))
        (nth 3 (syntax-pp 20))
        (nth 3 (syntax-pp 24))
        (char-syntax ?\")
        (syntax-after 10)))
"####,
        expect,
    );
}
