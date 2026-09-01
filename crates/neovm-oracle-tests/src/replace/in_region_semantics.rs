//! Oracle parity tests for GNU `subr.el` in-region replacement helpers.
//!
//! GNU implements `replace-string-in-region` and
//! `replace-regexp-in-region` in Elisp in `lisp/subr.el`.  They default
//! START to point, END to point-max, narrow around the target range, force
//! case-sensitive matching, preserve point via `save-excursion`, and signal
//! explicit boundary errors before narrowing.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_replace_string_in_region_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((case-fold-search t)
      (cases nil))
  (with-temp-buffer
    (insert "foo Foo foo")
    (goto-char 5)
    (let ((ret (replace-string-in-region "foo" "bar")))
      (push (list 'default-start ret (point) (buffer-string)) cases)))
  (with-temp-buffer
    (insert "xx foo foo xx")
    (goto-char 1)
    (let ((ret (replace-string-in-region "foo" "z" 4 11)))
      (push (list 'explicit-region ret (point) (buffer-string)) cases)))
  (with-temp-buffer
    (insert "abc")
    (goto-char 2)
    (let ((ret (replace-string-in-region "missing" "x")))
      (push (list 'missing ret (point) (buffer-string)) cases)))
  (with-temp-buffer
    (insert "abc")
    (push (condition-case err
              (replace-string-in-region "a" "x" 0 nil)
            (error (list 'start-error (car err) (cadr err))))
          cases))
  (with-temp-buffer
    (insert "abc")
    (push (condition-case err
              (replace-string-in-region "a" "x" nil 5)
            (error (list 'end-error (car err) (cadr err))))
          cases))
  (nreverse cases))
"#;

    let expect = expect_test::expect![[
        r#""OK ((default-start 1 5 \"foo Foo bar\") (explicit-region 2 1 \"xx z z xx\") (missing nil 2 \"abc\") (start-error error \"Start before start of buffer\") (end-error error \"End after end of buffer\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_replace_regexp_in_region_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((case-fold-search t)
      (cases nil))
  (with-temp-buffer
    (insert "a1 A2 a3")
    (goto-char 4)
    (let ((ret (replace-regexp-in-region "a\\([0-9]\\)" "x\\1-\\&")))
      (push (list 'default-start ret (point) (buffer-string)) cases)))
  (with-temp-buffer
    (insert "foo-12 bar-34 baz")
    (goto-char 1)
    (let ((ret (replace-regexp-in-region "\\([a-z]+\\)-\\([0-9]+\\)"
                                         "\\2:\\1"
                                         1 14)))
      (push (list 'explicit-region ret (point) (buffer-string)) cases)))
  (with-temp-buffer
    (insert "a b")
    (let ((ret (replace-regexp-in-region "\\(a\\)\\|\\(b\\)" "<\\1/\\2>" 1 4)))
      (push (list 'unmatched-subexp ret (buffer-string)) cases)))
  (with-temp-buffer
    (insert "abc")
    (let ((ret (replace-regexp-in-region "z+" "x")))
      (push (list 'missing ret (point) (buffer-string)) cases)))
  (with-temp-buffer
    (insert "abc")
    (push (condition-case err
              (replace-regexp-in-region "a" "x" 0 nil)
            (error (list 'start-error (car err) (cadr err))))
          cases))
  (with-temp-buffer
    (insert "abc")
    (push (condition-case err
              (replace-regexp-in-region "a" "x" nil 5)
            (error (list 'end-error (car err) (cadr err))))
          cases))
  (nreverse cases))
"#;

    let expect = expect_test::expect![[
        r#""OK ((default-start 1 4 \"a1 A2 x3-a3\") (explicit-region 2 1 \"12:foo 34:bar baz\") (unmatched-subexp 2 \"<a/> </b>\") (missing nil 4 \"abc\") (start-error error \"Start before start of buffer\") (end-error error \"End after end of buffer\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
