//! Strict combo oracle probes, batch 145: field text-property motion
//! (constrain-to-field / field-beginning / field-end / field-string with
//! boundary edges), replace-regexp-in-string with function repl + subexp +
//! literal flag, deep format specifier matrix (floats/bignum/char/pad/%S vs
//! %s), hash-table custom :test 'equal with fresh-cons keys + remhash, and
//! string-width / char-width across combining marks, CJK and emoji.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_field_property_motion_boundary_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAAABBBBBCCCCC")
  ;; field 'x' covers the five B's (positions 6..10), 'y' the five C's (11..15)
  (put-text-property 6 11 'field 'field-x)
  (put-text-property 11 16 'field 'field-y)
  (list (save-excursion (goto-char 1) (field-beginning))
        (save-excursion (goto-char 1) (field-end))
        (save-excursion (goto-char 6) (field-beginning))
        (save-excursion (goto-char 6) (field-end))
        (save-excursion (goto-char 8) (field-string))
        (save-excursion (goto-char 11) (field-beginning))
        (save-excursion (goto-char 11) (field-end))
        (save-excursion (goto-char 16) (field-beginning))
        ;; constrain-to-field: moving from inside field-x (8) toward 13 (field-y)
        (save-excursion (goto-char 8) (constrain-to-field 13 1))
        ;; constrain-to-field staying within same field
        (save-excursion (goto-char 7) (constrain-to-field 9 7))
        (get-char-property 8 'field)
        (get-char-property 11 'field)))
"##;
    let expect = expect_test::expect![[
        r#""OK (1 6 1 6 #(\"BBBBB\" 0 5 (field field-x)) 6 11 11 6 9 field-x field-y)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_replace_regexp_in_string_function_subexp_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (replace-regexp-in-string "\\b\\w" 'upcase "hello world foo")
      (replace-regexp-in-string "[a-z]+" (lambda (m) (concat "<" m ">")) "a bb ccc")
      (replace-regexp-in-string "\\(\\w+\\) \\(\\w+\\)" "\\2 \\1" "hello world")
      ;; LITERAL non-nil => replacement taken literally, no \\& / \\1 expansion
      (replace-regexp-in-string "[a-z]+" "\\&-\\&" "ab cd" t)
      ;; fixed-case preserve, capitalized replacement
      (replace-regexp-in-string "hello" "HELLO" "Hello hello HELLO"))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"Hello World Foo\" \"<a> <bb> <ccc>\" \"world hello\" \"ab-ab cd-cd\" \"HELLO HELLO HELLO\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_specifier_matrix_floats_bignum_pad() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (format "%5.2f|%-5.2f|%+5.2f|%05.2f" 3.14159 3.14159 3.14159 3.14159)
      (format "%g|%e|%.3e|%g" 0.0001 123456789.0 1234567.89 0.5)
      (format "%c|%d|%o|%x|#x%x|#o%o" 65 255 8 255 255 8)
      (format "%05d|%5d|%-5d|%+d" 42 42 42 42)
      (format "%s|%S|%c" "a\"b" "a\"b" 98)
      (format "%020d" (expt 2 64))
      (format "%.10g" 1.0)
      (format "%5s|%-5s|" "ab" "ab"))
"##;
    let expect = expect_test::expect![[
        r#""OK (\" 3.14|3.14 |+3.14|03.14\" \"0.0001|1.234568e+08|1.235e+06|0.5\" \"A|255|10|ff|#xff|#o10\" \"00042|   42|42   |+42\" \"a\\\"b|\\\"a\\\\\\\"b\\\"|b\" \"18446744073709551616\" \"1\" \"   ab|ab   |\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_hash_table_equal_test_fresh_cons_remhash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((h (make-hash-table :test 'equal))
       (k1 (list 1 2 3))
       (k2 (list 1 2 3)))
  (puthash k1 'first h)
  (puthash k2 'second h)
  (let ((c1 (hash-table-count h))
        (v1 (gethash k1 h))
        (v2 (gethash (list 1 2 3) h))
        (v3 (gethash (list 9 9) h 'missing)))
    (remhash k1 h)
    (let ((c2 (hash-table-count h))
          (v4 (gethash k2 h 'gone)))
      (clrhash h)
      (list c1 v1 v2 v3 c2 v4 (hash-table-count h)
            (hash-table-p h)
            (eq (hash-table-test h) 'equal)))))
"##;
    let expect = expect_test::expect![[r#""OK (1 second second missing 0 gone 0 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_string_char_width_combining_cjk_emoji() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((combining "ä")
      (cjk "日本語")
      (emoji "🐶")
      (zwj "👩‍👩"))
  (list (string-width "abc")
        (string-width cjk)
        (string-width combining)
        (char-width ?a)
        (char-width ?日)
        (length cjk)
        (string-bytes cjk)
        (length emoji)
        (string-width emoji)
        (string-width zwj)
        ;; truncate-no-properties vs substring across multibyte
        (substring cjk 0 2)
        (aref cjk 1)))
"##;
    let expect = expect_test::expect![[r#""OK (3 6 1 1 2 3 9 1 2 4 \"日本\" 26412)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
