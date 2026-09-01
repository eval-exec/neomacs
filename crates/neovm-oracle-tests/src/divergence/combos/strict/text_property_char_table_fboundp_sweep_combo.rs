//! Strict combo oracle probes, batch 281: text-property + char-table fboundp
//! sweep. Any nil-in-Neomacs/t-in-GNU is a missing-function bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_text_property_fboundp_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (fboundp 'add-face-text-property)
      (fboundp 'add-text-properties)
      (fboundp 'set-text-properties)
      (fboundp 'remove-text-properties)
      (fboundp 'remove-list-of-text-properties)
      (fboundp 'put-text-property)
      (fboundp 'get-text-property)
      (fboundp 'get-char-property)
      (fboundp 'get-pos-property)
      (fboundp 'next-single-property-change)
      (fboundp 'next-single-char-property-change)
      (fboundp 'next-property-change)
      (fboundp 'next-char-property-change)
      (fboundp 'previous-single-property-change)
      (fboundp 'text-property-any)
      (fboundp 'text-property-not-all)
      (fboundp 'text-property-search-forward)
      (fboundp 'text-property-search-backward)
      (fboundp 'propertize)
      (fboundp 'set-properties)
      (fboundp 'properties-at-point))
"##;
    let expect =
        expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t t t nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_table_extra_slot_fboundp_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (fboundp 'make-char-table)
      (fboundp 'char-table-p)
      (fboundp 'char-table-subtype)
      (fboundp 'char-table-parent)
      (fboundp 'set-char-table-parent)
      (fboundp 'char-table-extra-slot)
      (fboundp 'set-char-table-extra-slot)
      (fboundp 'char-table-range)
      (fboundp 'set-char-table-range)
      (fboundp 'char-table-extra-slots)
      (fboundp 'map-char-table)
      (fboundp 'optimize-char-table)
      (fboundp 'category-table-p)
      (fboundp 'category-table)
      (fboundp 'set-category-table)
      (fboundp 'define-category)
      (fboundp 'category-doc-string)
      (fboundp 'category-set-mnemonics)
      (fboundp 'modify-category-entry)
      (fboundp 'char-category-set))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t nil t t t t t t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_syntax_table_fboundp_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (fboundp 'syntax-table)
      (fboundp 'set-syntax-table)
      (fboundp 'make-syntax-table)
      (fboundp 'modify-syntax-entry)
      (fboundp 'char-syntax)
      (fboundp 'syntax-after)
      (fboundp 'syntax-class-to-char)
      (fboundp 'skip-syntax-forward)
      (fboundp 'skip-syntax-backward)
      (fboundp 'forward-word)
      (fboundp 'backward-word)
      (fboundp 'forward-sexp)
      (fboundp 'backward-sexp)
      (fboundp 'forward-list)
      (fboundp 'backward-list)
      (fboundp 'up-list)
      (fboundp 'down-list)
      (fboundp 'backward-up-list)
      (fboundp 'scan-lists)
      (fboundp 'scan-sexps)
      (fboundp 'parse-partial-sexp))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
