//! Oracle parity tests for GNU `subr.el` docstring line formatting helpers.
//!
//! GNU uses `internal--format-docstring-line` while bootstrapping Lisp
//! docstrings.  Its observable contract combines `format`, a newline
//! rejection path, and recursive single-line filling controlled by
//! `fill-column`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_internal_format_docstring_line_fill_column_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((fill-column 12))
  (list
   (internal--format-docstring-line "short")
   (internal--format-docstring-line "hello %s" "world")
   (internal--format-docstring-line "alpha beta gamma")
   (internal--format-docstring-line "alpha  beta")
   (internal--format-docstring-line "abcdefghijklm")
   (let ((fill-column 80))
     (internal--format-docstring-line "alpha beta gamma"))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"short\" \"hello world\" \"alpha beta\\ngamma\" \"alpha  beta\" \"abcdefghijklm\" \"alpha beta gamma\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_internal_format_docstring_line_rejects_newlines_after_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar
 (lambda (thunk)
   (condition-case err
       (funcall thunk)
     (error (list (car err) (cadr err)))))
 (list
  (lambda () (internal--format-docstring-line "line1\nline2"))
  (lambda () (internal--format-docstring-line "line%s" "\nbreak"))
  (lambda () (internal--format-docstring-line "%s" "ok"))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((error \"Unable to fill string containing newline: \\\"line1\\nline2\\\"\") \"line\\nbreak\" \"ok\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_internal_fill_string_single_line_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((fill-column 10))
  (list
   (internal--fill-string-single-line "")
   (internal--fill-string-single-line "short")
   (internal--fill-string-single-line "one two three")
   (internal--fill-string-single-line "one  two")
   (internal--fill-string-single-line " leading space")
   (internal--fill-string-single-line "trailing ")
   (let ((fill-column 5))
     (internal--fill-string-single-line "ab cd ef"))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"\" \"short\" \"one two\\nthree\" \"one  two\" \" leading\\nspace\" \"trailing \" \"ab cd\\nef\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
