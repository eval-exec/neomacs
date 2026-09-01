//! Oracle parity tests for GNU transposition command semantics.
//!
//! GNU implements `transpose-chars`, `transpose-words`, `transpose-sexps`,
//! and `transpose-lines` in `lisp/simple.el` through `transpose-subr`.
//! These tests compare text, point, mark, and error behavior.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_transpose_chars_eol_and_negative_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (eol negative)
  (with-temp-buffer
    (insert "abc\n")
    (goto-char (line-end-position))
    (transpose-chars 1)
    (setq eol (list (buffer-string) (point))))
  (with-temp-buffer
    (insert "abcd")
    (goto-char 4)
    (transpose-chars -1)
    (setq negative (list (buffer-string) (point))))
  (list eol negative))
"#;

    let expect = expect_test::expect![[r#""OK ((\"ab\\nc\" 5) (\"acbd\" 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_transpose_words_forward_backward_and_mark_arg_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (forward backward marked)
  (with-temp-buffer
    (insert "one two three")
    (goto-char 5)
    (transpose-words 1)
    (setq forward (list (buffer-string) (point))))
  (with-temp-buffer
    (insert "one two three")
    (goto-char 9)
    (transpose-words -1)
    (setq backward (list (buffer-string) (point))))
  (with-temp-buffer
    (insert "alpha beta gamma")
    (goto-char 2)
    (push-mark 13 t t)
    (transpose-words 0)
    (setq marked (list (buffer-string) (point) (mark t))))
  (list forward backward marked))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"two one three\" 8) (\"two one three\" 4) (\"gamma beta alpha\" 17 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_transpose_lines_positive_negative_and_mark_arg_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (positive negative marked)
  (with-temp-buffer
    (insert "a\nb\nc\n")
    (goto-char 3)
    (transpose-lines 1)
    (setq positive (list (buffer-string) (point))))
  (with-temp-buffer
    (insert "a\nb\nc\n")
    (goto-char 5)
    (transpose-lines -1)
    (setq negative (list (buffer-string) (point))))
  (with-temp-buffer
    (insert "a\nb\nc\n")
    (goto-char 1)
    (push-mark 5 t t)
    (transpose-lines 0)
    (setq marked (list (buffer-string) (point) (mark t))))
  (list positive negative marked))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"b\\na\\nc\\n\" 5) (\"b\\na\\nc\\n\" 3) (\"c\\nb\\na\\n\" 5 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_transpose_sexps_symbols_lists_and_interactive_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (symbols lists error-shape)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "alpha beta gamma")
    (goto-char 7)
    (transpose-sexps 1)
    (setq symbols (list (buffer-string) (point))))
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "(one) (two) (three)")
    (goto-char 7)
    (transpose-sexps 1)
    (setq lists (list (buffer-string) (point))))
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "(one")
    (goto-char (point-max))
    (setq error-shape
          (condition-case err
              (progn (transpose-sexps 1 t) 'no-error)
            (error (list (car err) (cadr err))))))
  (list symbols lists error-shape))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"beta alpha gamma\" 11) (\"(two) (one) (three)\" 12) no-error)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
