//! Oracle parity tests for GNU line-editing helper semantics.
//!
//! GNU implements `delete-line` and `ensure-empty-lines` in `lisp/subr.el`
//! in terms of `pos-bol`, `delete-region`, and newline insertion.  These
//! helpers are small, but their point and buffer side effects are used by
//! startup/help/package code.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_delete_line_uses_current_line_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((cases nil))
  (dolist (case '((1 . "one\ntwo\nthree")
                  (3 . "one\ntwo\nthree")
                  (5 . "one\ntwo\nthree")
                  (9 . "one\ntwo\nthree")
                  (14 . "one\ntwo\nthree")
                  (1 . "single")
                  (7 . "single")))
    (with-temp-buffer
      (insert (cdr case))
      (goto-char (car case))
      (let ((before (list (point) (pos-bol) (pos-bol 2))))
        (delete-line)
        (push (list before (point) (buffer-string)) cases))))
  (nreverse cases))
"#;

    let expect = expect_test::expect![[
        r#""OK (((1 1 5) 1 \"two\\nthree\") ((3 1 5) 1 \"two\\nthree\") ((5 5 9) 5 \"one\\nthree\") ((9 9 14) 9 \"one\\ntwo\\n\") ((14 9 14) 9 \"one\\ntwo\\n\") ((1 1 7) 1 \"\") ((7 1 7) 1 \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_ensure_empty_lines_adjusts_prefix_newlines_and_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((cases nil))
  (dolist (case '((1 2 "foo")
                  (4 2 "foo")
                  (5 2 "foo\n")
                  (9 2 "foo\n\n\n\n\n")
                  (7 0 "foo\n\n\n")
                  (2 nil "abc")
                  (1 nil "\n\nabc")))
    (with-temp-buffer
      (insert (nth 2 case))
      (goto-char (nth 0 case))
      (let ((before (point)))
        (ensure-empty-lines (nth 1 case))
        (push (list before (nth 1 case) (point) (buffer-string)) cases))))
  (nreverse cases))
"#;

    let expect = expect_test::expect![[
        r#""OK ((1 2 3 \"\\n\\nfoo\") (4 2 7 \"foo\\n\\n\\n\") (5 2 7 \"foo\\n\\n\\n\") (9 2 7 \"foo\\n\\n\\n\") (7 0 5 \"foo\\n\") (2 nil 4 \"a\\n\\nbc\") (1 nil 2 \"\\n\\n\\nabc\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
