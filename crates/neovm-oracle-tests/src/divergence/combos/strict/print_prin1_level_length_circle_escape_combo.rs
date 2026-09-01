//! Strict combo oracle probes, batch 168: print/prin1 depth + escape control.
//! print-level / print-length truncation with nested lists, print-circle of a
//! truly circular list, print-escape-nonascii / print-escape-newline, nested
//! vectors, dotted pairs, and print-quoted quoting control.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_print_level_length_truncation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((data '((1 2 3 (4 5 (6 7 8) 9) 10) (a b c d e f g h i j))))
  (list (let ((print-level 2)) (prin1-to-string data))
        (let ((print-length 3)) (prin1-to-string data))
        (let ((print-level 1) (print-length 2)) (prin1-to-string data))
        (let ((print-level nil) (print-length nil)) (prin1-to-string data))
        (let ((print-length 0)) (prin1-to-string '(a b c)))))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"((1 2 3 ... 10) (a b c d e f g h i j))\" \"((1 2 3 ...) (a b c ...))\" \"(... ...)\" \"((1 2 3 (4 5 (6 7 8) 9) 10) (a b c d e f g h i j))\" \"(...)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_print_circle_circular_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((print-circle t))
  (let ((x (list 1 2 3)))
    (setcdr (cddr x) x)
    (list (prin1-to-string x))))
"##;
    let expect = expect_test::expect![[r##""OK (\"#1=(1 2 3 . #1#)\")""##]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_print_escape_nonascii_newline_quoted_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (let ((print-escape-nonascii t)) (prin1-to-string "héllo wörld"))
      (let ((print-escape-nonascii nil)) (prin1-to-string "héllo wörld"))
      (let ((print-escape-newline t)) (prin1-to-string "line1\nline2\ttab"))
      (let ((print-escape-newline nil)) (prin1-to-string "a\nb"))
      (prin1-to-string [1 [2 [3 [4]]]])
      (prin1-to-string '(a . (b . c)))
      (prin1-to-string (cons 'a (cons 'b 'c)))
      (let ((print-quoted t)) (prin1-to-string '(quote foo)))
      (let ((print-quoted nil)) (prin1-to-string '(quote foo)))
      (prin1-to-string '#$(1 2 3))
      (princ-to-string "literal")
      (prin1-to-string ?A))
"##;
    let expect = expect_test::expect![[r#""ERR (invalid-function 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
