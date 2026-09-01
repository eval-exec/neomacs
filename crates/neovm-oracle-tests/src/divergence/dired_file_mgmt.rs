//! Divergence tests: dired, file management, directory operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_dired_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'dired)
  (fboundp 'dired-other-window)
  (fboundp 'dired-jump)
  (fboundp 'dired-mark)
  (fboundp 'dired-unmark)
  (featurep 'dired))"#,
        expect,
    );
}

#[test]
fn divergence_dired_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'dired-next-line)
  (fboundp 'dired-previous-line)
  (fboundp 'dired-next-dirline)
  (fboundp 'dired-prev-dirline))"#,
        expect,
    );
}

#[test]
fn divergence_dired_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'dired-do-copy)
  (fboundp 'dired-do-rename)
  (fboundp 'dired-do-delete)
  (fboundp 'dired-do-shell-command)
  (fboundp 'dired-do-async-shell-command))"#,
        expect,
    );
}

#[test]
fn divergence_dired_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'dired-sort-toggle-or-edit)
  (fboundp 'dired-toggle-marks)
  (fboundp 'dired-mark-files-regexp)
  (boundp 'dired-listing-switches)
  (stringp dired-listing-switches)) "#,
        expect,
    );
}

#[test]
fn divergence_dired_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'insert-directory)
  (fboundp 'dired-insert-set-properties)
  (fboundp 'dired-get-filename)) "#,
        expect,
    );
}

#[test]
fn divergence_dired_revert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'dired-revert)
  (fboundp 'revert-buffer)
  (boundp 'revert-without-query)
  (listp revert-without-query)) "#,
        expect,
    );
}

#[test]
fn divergence_find_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'find-file)
  (fboundp 'find-file-other-window)
  (fboundp 'find-file-read-only)
  (fboundp 'write-file)
  (fboundp 'save-buffer)
  (fboundp 'save-some-buffers))"#,
        expect,
    );
}

#[test]
fn divergence_auto_save() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'auto-save-mode)
  (boundp 'auto-save-default)
  (boundp 'auto-save-interval)
  (boundp 'auto-save-timeout)
  (integerp auto-save-interval)
  (numberp auto-save-timeout)) "#,
        expect,
    );
}

#[test]
fn divergence_backup_files() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'make-backup-files)
  (boundp 'backup-by-copying)
  (boundp 'version-control)
  (fboundp 'backup-buffer)
  (fboundp 'find-backup-file-name)) "#,
        expect,
    );
}

#[test]
fn divergence_file_locks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'lock-file)
  (fboundp 'unlock-file)
  (fboundp 'ask-user-about-lock)
  (boundp 'lock-file-name-transforms)) "#,
        expect,
    );
}

#[test]
fn divergence_recentf_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable recentf-max-saved-items)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'recentf-cleanup)
  (fboundp 'recentf-add-file)
  (fboundp 'recentf-remove-if-non-kept)
  (boundp 'recentf-max-saved-items)
  (integerp recentf-max-saved-items)) "#,
        expect,
    );
}
