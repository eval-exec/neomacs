//! Strict combo oracle probes, batch 276: mule / multibyte / locale CORE
//! variable sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_multibyte_enable_language_environment_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'default-enable-multibyte-characters)
      (boundp 'enable-multibyte-characters)
      (boundp 'current-language-environment)
      (boundp 'current-locale-environment)
      (boundp 'default-input-method)
      (boundp 'nonascii-translation-table)
      (boundp 'nonascii-insert-offset)
      (boundp 'unibyte-display-via-language-environment)
      (boundp 'latin1-char-display)
      (boundp 'current-iso639-language)
      (boundp 'locale-coding-system)
      (boundp 'language-info-alist))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t t t nil nil t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_charset_unify_translation_dos_codepage_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'charset-list)
      (boundp 'charset-history)
      (boundp 'unify-8859-on-decoding-mode)
      (boundp 'unify-8859-on-encoding-mode)
      (boundp 'ccl-program-regs)
      (boundp 'dos-codepage)
      (boundp 'dos-locale)
      (boundp 'default-korean-keyboard)
      (boundp 'current-input-method)
      (boundp 'current-input-method-title)
      (boundp 'iso-2022-ctl)
      (boundp 'mule-version))
"##;
    let expect = expect_test::expect![[r#""OK (t nil nil nil nil nil nil t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_charset_priority_coding_detect_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'charset-priority-list)
      (boundp 'coding-priority)
      (boundp 'auto-coding-alist)
      (boundp 'auto-coding-regexp-alist)
      (boundp 'auto-coding-functions)
      (boundp 'detect-coding-with-priority)
      (boundp 'detect-coding-region-function)
      (boundp 'code-page-alist)
      (boundp 'current-language-environment)
      (boundp 'charset-revision-table)
      (boundp 'char-coding-system-table)
      (boundp 'inhibit-iso-escape-detection))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil t t t nil nil nil t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
