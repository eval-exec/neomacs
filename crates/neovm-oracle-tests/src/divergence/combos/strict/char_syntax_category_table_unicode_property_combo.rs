//! Strict combo oracle probes, batch 381: char-syntax/category/unicode property
//! combo. char-syntax across custom/standard tables, char-category-set,
//! get-char-code-property 'bidi-class/'script, and char-width.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_char_syntax_category_unicode_property_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (char-syntax ?a)
      (char-syntax ? )
      (char-syntax ?()
      (char-syntax ?\")
      (char-category-set ?a)
      (char-category-set ?1)
      (get-char-code-property ?a 'script)
      (get-char-code-property ?日 'script)
      (get-char-code-property ?a 'bidi-class)
      (get-char-code-property ?a 'general-category)
      (char-width ?a)
      (char-width ?日))
"##;
    let expect = expect_test::expect![[
        r#""OK (119 32 40 34 #&128\"\\0\\0\\0\\0\\0@\\0\\0\\0\u{10}\\0\\0\u{2}\u{10}\u{4}\\0\" #&128\"\\0\\0\\0\\0\\0@@\\0\\0\\0\\0\\0\u{2}\u{10}\u{4}\\0\" nil nil L Ll 1 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_modify_syntax_entry_skip_syntax_custom_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((st (make-syntax-table)))
  (modify-syntax-entry ?_ "w" st)
  (modify-syntax-entry ?# "'" st)
  (modify-syntax-entry ?< "(>" st)
  (modify-syntax-entry ?> ")<" st)
  (with-temp-buffer
    (set-syntax-table st)
    (insert "hello_world #prefix <nested>")
    (goto-char 1)
    (let ((w (progn (skip-syntax-forward "w") (point)))
          (s2 (progn (skip-syntax-forward " ") (point)))
          (p2 (progn (skip-syntax-forward "'") (point))))
      (list w s2 p2))))
"##;
    let expect = expect_test::expect![[r#""OK (12 13 14)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_code_property_name_decimal_digit_old_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (get-char-code-property ?A 'name)
      (get-char-code-property ?\n 'name)
      (get-char-code-property ?日 'name)
      (get-char-code-property ?0 'decimal-digit-value)
      (get-char-code-property ?9 'decimal-digit-value)
      (get-char-code-property ?a 'numeric-value)
      (get-char-code-property ?5 'numeric-value)
      (get-char-code-property ?Ä 'lowercase)
      (get-char-code-property ?a 'uppercase))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"LATIN CAPITAL LETTER A\" nil \"CJK IDEOGRAPH-65E5\" 0 9 nil 5 228 65)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
