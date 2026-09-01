//! Strict combo oracle probes, batch 227: Unicode char-code properties.
//! get-char-code-property over general-category/numeric-value/bidi-class/
//! script/lowercase/uppercase for latin/digit/accented/CJK/space, and
//! char-script-chars.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_get_char_code_property_general_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (get-char-code-property ?A 'general-category)
      (get-char-code-property ?a 'general-category)
      (get-char-code-property ?1 'general-category)
      (get-char-code-property ?  'general-category)
      (get-char-code-property ?é 'general-category)
      (get-char-code-property ?日 'general-category)
      (get-char-code-property ?_ 'general-category)
      (get-char-code-property ?, 'general-category)
      (get-char-code-property ?\n 'general-category))
"##;
    let expect = expect_test::expect![[r#""OK (Lu Ll Nd Zs Ll Lo Pc Po Cc)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_get_char_code_property_numeric_script_bidi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (get-char-code-property ?5 'numeric-value)
      (get-char-code-property ?a 'numeric-value)
      (get-char-code-property ?a 'script)
      (get-char-code-property ?é 'script)
      (get-char-code-property ?日 'script)
      (get-char-code-property ?A 'script)
      (get-char-code-property ?a 'bidi-class)
      (get-char-code-property ?a 'lowercase)
      (get-char-code-property ?A 'uppercase)
      (get-char-code-property ?5 'bidi-class))
"##;
    let expect = expect_test::expect![[r#""OK (5 nil nil nil nil nil L nil nil EN)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_script_chars_and_decimal_digit_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (consp (char-script-chars 'latin))
      (memq ?a (char-script-chars 'latin))
      (get-char-code-property ?0 'decimal-digit-value)
      (get-char-code-property ?9 'decimal-digit-value)
      (get-char-code-property ?a 'name)
      (get-char-code-property ?日 'name)
      (get-char-code-property ?\n 'name)
      (get-char-code-property ?a 'old-name))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function char-script-chars)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
