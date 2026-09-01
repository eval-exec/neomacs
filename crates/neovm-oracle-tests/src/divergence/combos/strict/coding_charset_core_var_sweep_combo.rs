//! Strict combo oracle probes, batch 271: coding / charset CORE variable sweep.
//! Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_coding_system_read_write_alist_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'coding-system-for-read)
      (boundp 'coding-system-for-write)
      (boundp 'file-coding-system-alist)
      (boundp 'process-coding-system-alist)
      (boundp 'network-coding-system-alist)
      (boundp 'default-buffer-file-coding-system)
      (boundp 'buffer-file-coding-system)
      (boundp 'inhibit-eol-conversion)
      (boundp 'select-safe-coding-system-function)
      (boundp 'coding-system-require-warning)
      (boundp 'enable-character-translation)
      (boundp 'char-coding-system-table))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t nil t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_eol_mnemonic_charset_priority_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'eol-mnemonic-unix)
      (boundp 'eol-mnemonic-dos)
      (boundp 'eol-mnemonic-mac)
      (boundp 'eol-mnemonic-undecided)
      (boundp 'current-language-environment)
      (boundp 'locale-coding-system)
      (boundp 'current-locale-environment)
      (boundp 'locale-preferred-coding-systems)
      (boundp 'charset-priority-list)
      (boundp 'coding-priority)
      (boundp 'default-process-coding-system)
      (boundp 'default-sendmail-coding-system))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_auto_coding_inhibit_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'auto-coding-alist)
      (boundp 'auto-coding-regexp-alist)
      (boundp 'auto-coding-functions)
      (boundp 'inhibit-iso-escape-detection)
      (boundp 'set-auto-coding-function)
      (boundp 'coding-category-utf-8)
      (boundp 'coding-category-list)
      (boundp 'translation-table-for-input)
      (boundp 'translation-table-for-output)
      (boundp 'standard-translation-table-for-decode)
      (boundp 'standard-translation-table-for-encode)
      (boundp 'charset-revision-table))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
