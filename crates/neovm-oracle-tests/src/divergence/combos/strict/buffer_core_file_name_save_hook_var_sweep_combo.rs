//! Strict combo oracle probes, batch 266: buffer CORE variable existence sweep
//! (file-name, auto-save, display-count, save hooks). Any nil-in-Neomacs/t-in-
//! GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_buffer_file_name_truename_coding_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'buffer-file-name)
      (boundp 'buffer-file-truename)
      (boundp 'buffer-file-coding-system)
      (boundp 'buffer-file-coding-system-auto-detect)
      (boundp 'buffer-file-number)
      (boundp 'buffer-file-numbers-unique)
      (boundp 'buffer-auto-save-file-name)
      (boundp 'buffer-backed-up)
      (boundp 'buffer-saved-size)
      (boundp 'buffer-display-count)
      (boundp 'buffer-display-time)
      (boundp 'buffer-save-without-query)
      (boundp 'buffer-read-only))
"##;
    let expect = expect_test::expect![[r#""OK (t t t nil t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_save_hook_format_write_file_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'before-save-hook)
      (boundp 'after-save-hook)
      (boundp 'write-file-functions)
      (boundp 'write-contents-functions)
      (boundp 'before-change-functions)
      (boundp 'after-change-functions)
      (boundp 'first-change-hook)
      (boundp 'buffer-access-fontify-functions)
      (boundp 'buffer-access-fontified-region)
      (boundp 'fontification-functions)
      (boundp 'file-local-name))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_buffer_name_list_default_directory_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'default-directory)
      (boundp 'buffer-list)
      (boundp 'buffer-name)
      (boundp 'buffer-name-history)
      (boundp 'inhibit-buffer-hooks)
      (boundp 'kill-buffer-query-functions)
      (boundp 'kill-buffer-hook)
      (boundp 'buffer-list-update-hook)
      (boundp 'temp-buffer-setup-hook)
      (boundp 'temp-buffer-show-hook)
      (boundp 'change-major-mode-hook)
      (boundp 'set-buffer-major-mode-hook))
"##;
    let expect = expect_test::expect![[r#""OK (t nil nil t nil t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
