//! Strict combo oracle probes, batch 301: file operations deep (shared tempdir).
//! file-modes / set-file-modes, make-symbolic-link / file-symlink-p,
//! file-directory-p / file-regular-p, and directory-files-recursively.
//! Uses assert_oracle_parity_with_shared_tempdir_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_file_modes_set_symlink_directory_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((dir (or (getenv "NEOVM_ORACLE_TEST_TMPDIR") temporary-file-directory))
       (sub (expand-file-name "probe-fileops-301/" dir)))
  (when (file-exists-p sub) (delete-directory sub t))
  (make-directory (expand-file-name "nested/" sub) t)
  (write-region "content\n" nil (expand-file-name "f1.txt" sub) nil 'silent)
  (write-region "nested-content\n" nil (expand-file-name "nested/deep.txt" sub) nil 'silent)
  (let ((modes (file-modes (expand-file-name "f1.txt" sub))))
    (set-file-modes (expand-file-name "f1.txt" sub) 448)
    (prog1
        (list (integerp modes)
              (= (logand (file-modes (expand-file-name "f1.txt" sub)) 511) 448)
              (file-regular-p (expand-file-name "f1.txt" sub))
              (file-directory-p sub)
              (sort (directory-files-recursively sub "\\.txt\\'") #'string<)
              (file-symlink-p (expand-file-name "f1.txt" sub)))
      (delete-directory sub t)))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(form, expect);
}

#[test]
fn div_v8_file_symlink_make_resolve() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((dir (or (getenv "NEOVM_ORACLE_TEST_TMPDIR") temporary-file-directory))
       (sub (expand-file-name "probe-symlink-301/" dir)))
  (when (file-exists-p sub) (delete-directory sub t))
  (make-directory sub t)
  (write-region "target\n" nil (expand-file-name "target.txt" sub) nil 'silent)
  (make-symbolic-link "target.txt" (expand-file-name "link.txt" sub) t)
  (prog1
      (list (file-symlink-p (expand-file-name "link.txt" sub))
            (file-exists-p (expand-file-name "link.txt" sub))
            (with-temp-buffer
              (insert-file-contents (expand-file-name "link.txt" sub))
              (buffer-string)))
    (delete-directory sub t)))
"##;
    let expect = expect_test::expect![[r#""OK (\"target.txt\" t \"target\\n\")""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(form, expect);
}

#[test]
fn div_v8_file_copy_rename_attributes_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((dir (or (getenv "NEOVM_ORACLE_TEST_TMPDIR") temporary-file-directory))
       (sub (expand-file-name "probe-copy-301/" dir)))
  (when (file-exists-p sub) (delete-directory sub t))
  (make-directory sub t)
  (write-region "original\n" nil (expand-file-name "orig.txt" sub) nil 'silent)
  (copy-file (expand-file-name "orig.txt" sub) (expand-file-name "copy.txt" sub))
  (rename-file (expand-file-name "copy.txt" sub) (expand-file-name "renamed.txt" sub))
  (prog1
      (list (file-exists-p (expand-file-name "orig.txt" sub))
            (file-exists-p (expand-file-name "copy.txt" sub))
            (file-exists-p (expand-file-name "renamed.txt" sub))
            (with-temp-buffer
              (insert-file-contents (expand-file-name "renamed.txt" sub))
              (buffer-string)))
    (delete-directory sub t)))
"##;
    let expect = expect_test::expect![[r#""OK (t nil t \"original\\n\")""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(form, expect);
}
