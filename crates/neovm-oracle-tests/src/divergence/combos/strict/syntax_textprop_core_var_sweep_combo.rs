//! Strict combo oracle probes, batch 263: syntax / text-property CORE variable
//! existence sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_text_property_nonsticky_wrap_line_prefix_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'text-property-default-nonsticky)
      (boundp 'wrap-prefix)
      (boundp 'line-prefix)
      (boundp 'default-text-properties)
      (boundp 'point-before-scroll)
      (boundp 'buffer-invisibility-spec)
      (boundp 'char-property-alias-alist)
      (boundp 'inhibit-point-motion-hooks)
      (boundp 'show-help-function)
      (boundp 'default-transient-mark-mode))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_defun_prompt_regexp_page_outline_regexp_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'open-paren-in-column-0-is-defun-start)
      (boundp 'defun-prompt-regexp)
      (boundp 'outline-regexp)
      (boundp 'outline-heading-end-regexp)
      (boundp 'outline-level)
      (boundp 'page-delimiter)
      (boundp 'paragraph-start)
      (boundp 'left-margin)
      (boundp 'right-margin)
      (boundp 'fill-nobreak-predicate)
      (boundp 'fill-nobreak-invisible))
"##;
    let expect = expect_test::expect![[r#""OK (t t t nil t t t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_syntax_category_table_core_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'standard-syntax-table)
      (boundp 'text-mode-syntax-table)
      (boundp 'emacs-lisp-mode-syntax-table)
      (boundp 'standard-category-table)
      (boundp 'word-combining-categories)
      (boundp 'word-separating-categories)
      (boundp 'parse-sexp-ignore-comments)
      (boundp 'parse-sexp-lookup-properties)
      (boundp 'multibyte-syntax-as-symbol)
      (boundp 'nonascii-translation-table)
      (boundp 'character-fold-table))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t nil t t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
