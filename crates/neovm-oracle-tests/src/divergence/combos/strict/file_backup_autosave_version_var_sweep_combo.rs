//! Strict combo oracle probes, batch 257: file / backup / auto-save / version
//! control variable existence sweep. Any nil-in-Neomacs/t-in-GNU is a missing
//! variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_backup_by_copying_version_control_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'backup-by-copying)
      (boundp 'backup-by-copying-when-linked)
      (boundp 'backup-by-copying-when-mismatch)
      (boundp 'backup-by-copying-when-privileged-mismatch)
      (boundp 'version-control)
      (boundp 'kept-old-versions)
      (boundp 'kept-new-versions)
      (boundp 'delete-old-versions)
      (boundp 'dired-kept-versions)
      (boundp 'make-backup-files)
      (boundp 'make-backup-file-name-function)
      (boundp 'backup-directory-alist)
      (boundp 'tramp-backup-directory-alist))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_autosave_visit_require_final_newline_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'auto-save-default)
      (boundp 'auto-save-visited-file-name)
      (boundp 'auto-save-interval)
      (boundp 'auto-save-timeout)
      (boundp 'auto-save-list-file-name)
      (boundp 'auto-save-list-file-prefix)
      (boundp 'buffer-offer-save)
      (boundp 'find-file-visit-truename)
      (boundp 'require-final-newline)
      (boundp 'mode-require-final-newline)
      (boundp 'write-region-inhibit-fsync)
      (boundp 'confirm-nonexistent-file-or-buffer))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_find_file_large_file_warn_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'large-file-warning-threshold)
      (boundp 'find-file-suppress-same-file-warnings)
      (boundp 'find-file-visit-truename)
      (boundp 'find-file-existing-other-name)
      (boundp 'find-file-literally)
      (boundp 'find-file-not-found-functions)
      (boundp 'find-file-hook)
      (boundp 'find-file-literally)
      (boundp 'inhibit-file-name-handlers)
      (boundp 'inhibit-file-name-operation)
      (boundp 'file-precious-flag)
      (boundp 'delete-by-moving-to-trash))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
