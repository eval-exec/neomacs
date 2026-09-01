//! Oracle parity tests for GNU `truncate-string-to-width` semantics.
//!
//! GNU implements this in `lisp/international/mule-util.el` on top of
//! `char-width` and `string-width`.  These cases pin the observable behavior
//! around start columns, padding, explicit ellipses, and display-property
//! ellipsis mode.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_truncate_ascii_start_and_end_columns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (truncate-string-to-width "abcdefghij" 5)
 (truncate-string-to-width "abcdefghij" 5 2)
 (truncate-string-to-width "abcdefghij" 0)
 (truncate-string-to-width "abcdefghij" 20 8))
"#;

    let expect = expect_test::expect![[r#""OK (\"abcde\" \"cde\" \"\" \"ij\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_truncate_wide_chars_and_padding_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (concat "a" (char-to-string #x4e2d) "b"
                 (char-to-string #x6587) "c")))
  (list
   (string-width s)
   (truncate-string-to-width s 1)
   (truncate-string-to-width s 2)
   (truncate-string-to-width s 3)
   (truncate-string-to-width s 4)
   (truncate-string-to-width s 3 1 ?.)
   (truncate-string-to-width s 4 2 ?_)))
"#;

    let expect = expect_test::expect![[r#""OK (7 \"a\" \"a\" \"a中\" \"a中b\" \"中\" \"_b\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_truncate_padding_when_string_is_too_short() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (truncate-string-to-width "" 4 nil ?.)
 (truncate-string-to-width "ab" 5 nil ?_)
 (truncate-string-to-width "ab" 5 10 ?x)
 (truncate-string-to-width "ab" 5 1 ?-))
"#;

    let expect = expect_test::expect![[r#""OK (\"....\" \"ab___\" \"xxxxx\" \"b---\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_truncate_explicit_ellipsis_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (concat "abcd" (char-to-string #x4e2d) "efgh")))
  (list
   (truncate-string-to-width s 4 nil nil "...")
   (truncate-string-to-width s 5 nil nil "...")
   (truncate-string-to-width s 6 nil nil "<>")
   (truncate-string-to-width "abc" 2 nil nil "...")
   (truncate-string-to-width "abc" 2 nil nil "")))
"#;

    let expect = expect_test::expect![[r#""OK (\"a...\" \"ab...\" \"abcd<>\" \"ab\" \"ab\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_truncate_ellipsis_text_property_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (copy-sequence "abcdefghij")))
  (put-text-property 2 7 'face 'bold s)
  (let ((r (truncate-string-to-width s 5 nil nil "..." t)))
    (list r
          (substring-no-properties r)
          (text-properties-at 2 r)
          (text-properties-at 5 r)
          (get-text-property 5 'display r))))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"abcdefghij\" 2 7 (face bold display \"...\") 7 10 (display \"...\")) \"abcdefghij\" (face bold display \"...\") (face bold display \"...\") \"...\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_truncate_argument_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((cases
       (list
        (lambda () (truncate-string-to-width "abc" "2"))
        (lambda () (truncate-string-to-width "abc" 2 "1"))
        (lambda () (truncate-string-to-width "abc" 2 nil "x"))
        (lambda () (truncate-string-to-width 123 2)))))
  (mapcar
   (lambda (fn)
     (condition-case err
         (funcall fn)
       (error (list (car err) (cadr err)))))
   cases))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument number-or-marker-p) (wrong-type-argument number-or-marker-p) \"ab\" (wrong-type-argument sequencep))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
