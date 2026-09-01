//! UTF-8 / multibyte *character property* divergence probes.
//!
//! Probes Unicode property tables that GNU loads from the Unicode database:
//! `get-char-code-property` (general-category, bidi-class, case mapping),
//! `char-script`, `char-to-name`, `char-category`.  A UTF-8-internal reimpl
//! frequently lacks or simplifies these tables, making them high-yield
//! divergence targets.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_utf8_general_category_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (Ll Lu Nd Ll Lo Lo So Zs)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (get-char-code-property ?a 'general-category)
      (get-char-code-property ?A 'general-category)
      (get-char-code-property ?1 'general-category)
      (get-char-code-property ?é 'general-category)
      (get-char-code-property ?\x3042 'general-category)
      (get-char-code-property ?\x4e2d 'general-category)
      (get-char-code-property #x1f600 'general-category)
      (get-char-code-property ?\s 'general-category))
"#,
        expect,
    );
}

#[test]
fn div_utf8_unicode_case_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (get-char-code-property ?a 'lowercase)
      (get-char-code-property ?A 'uppercase)
      (get-char-code-property ?é 'lowercase)
      (get-char-code-property ?ß 'uppercase)
      (get-char-code-property ?\x4e2d 'lowercase))
"#,
        expect,
    );
}

#[test]
fn div_utf8_bidi_class_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (L R AL NSM)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (get-char-code-property ?a 'bidi-class)
      (get-char-code-property ?\x5d0 'bidi-class)
      (get-char-code-property ?\x627 'bidi-class)
      (get-char-code-property ?\x300 'bidi-class))
"#,
        expect,
    );
}

#[test]
fn div_utf8_char_script_classification() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-script)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (char-script ?a)
      (char-script ?é)
      (char-script ?\x3042)
      (char-script ?\x4e2d)
      (char-script ?\x627)
      (char-script ?\x5d0)
      (char-script #x1f600))
"#,
        expect,
    );
}

#[test]
fn div_utf8_char_to_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"LATIN SMALL LETTER A\" \"LATIN SMALL LETTER E WITH ACUTE\" \"HIRAGANA LETTER A\" \"CJK IDEOGRAPH-4E2D\" \"GRINNING FACE\" \"COMBINING GRAVE ACCENT\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (char-to-name ?a)
      (char-to-name ?é)
      (char-to-name ?\x3042)
      (char-to-name ?\x4e2d)
      (char-to-name #x1f600)
      (char-to-name ?\x300))
"#,
        expect,
    );
}

#[test]
fn div_utf8_char_category_bits() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-category)""#]];
    // Emacs char-category table (independent of Unicode properties).
    crate::common::assert_oracle_parity_expect(
        r#"
(list (char-category ?a)
      (char-category ?A)
      (char-category ?1)
      (char-category ?é)
      (char-category ?\x3042)
      (char-category ?\s))
"#,
        expect,
    );
}

#[test]
fn div_utf8_get_char_code_property_canonical_decomposition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil (101 769) (117 776))""#]];
    // Precomposed chars should decompose to their combining sequence.
    crate::common::assert_oracle_parity_expect(
        r#"
(list (get-char-code-property ?á 'canonical-class)
      (get-char-code-property ?é 'decomposition)
      (get-char-code-property ?ü 'decomposition))
"#,
        expect,
    );
}

#[test]
fn div_utf8_char_script_table_membership() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-script-table-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (char-script-table-p (char-script ?a))
      (sort (delete-dups (mapcar #'char-script "aA1 éあ中")) #'string<))
"#,
        expect,
    );
}
