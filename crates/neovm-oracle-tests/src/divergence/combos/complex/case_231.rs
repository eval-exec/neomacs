//! Complex combo batch 231 — `revert-buffer` / `auto-revert-mode` /
//! `find-file` with wildcards / `recover-file` / `backup` policies.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx231_revert_buffer_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'revert-buffer)
      (fboundp 'revert-buffer-quick)
      (boundp 'revert-buffer-function)
      (boundp 'revert-without-query)
      (boundp 'buffer-stale-function))
"##,
        expect,
    );
}

#[test]
fn div_cx231_auto_revert_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'autorevert)
      (list (fboundp 'auto-revert-mode)
            (fboundp 'global-auto-revert-mode)
            (boundp 'auto-revert-interval)
            (boundp 'auto-revert-stop-on-user-input)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx231_find_file_with_wildcards() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t (\"a.txt\" \"b.txt\" \"c.txt\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-cx231-wild" t)))
  (unwind-protect
      (progn
        (dolist (name '("a.txt" "b.txt" "c.txt"))
          (write-region "x" nil (expand-file-name name dir) nil 'silent))
        (condition-case e
            (let ((files (sort (file-expand-wildcards
                                (expand-file-name "*.txt" dir))
                               #'string<)))
              (list (consp files)
                    (= (length files) 3)
                    (not (memq nil (mapcar #'file-name-absolute-p files)))
                    (mapcar #'file-name-nondirectory files)))
          (error (list :errored (car e)))))
    (ignore-errors (delete-directory dir t))))
"##,
        expect,
    );
}

#[test]
fn div_cx231_backup_policies_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'make-backup-files)
      (boundp 'backup-by-copying)
      (boundp 'version-control)
      (boundp 'kept-new-versions)
      (boundp 'kept-old-versions)
      (boundp 'backup-directory-alist))
"##,
        expect,
    );
}

#[test]
fn div_cx231_recover_file_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'recover-file)
      (fboundp 'recover-session)
      (boundp 'auto-save-list-file-prefix))
"##,
        expect,
    );
}

#[test]
fn div_cx231_auto_save_configuration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'auto-save-default)
      (boundp 'auto-save-interval)
      (boundp 'auto-save-timeout)
      (boundp 'auto-save-visited-file-name)
      (boundp 'auto-save-list-file-name))
"##,
        expect,
    );
}

#[test]
fn div_cx231_lock_file_configuration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'create-lockfiles)
      (boundp 'lock-file-name-transforms)
      (fboundp 'lock-buffer)
      (fboundp 'unlock-buffer))
"##,
        expect,
    );
}

#[test]
fn div_cx231_buffer_stale_function_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'buffer-stale-function)
      (boundp 'revert-buffer-function)
      (boundp 'before-revert-hook)
      (boundp 'after-revert-hook))
"##,
        expect,
    );
}

#[test]
fn div_cx231_find_file_literally_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'find-file-literally)
      (fboundp 'find-file-no-select)
      (boundp 'find-file-literally)
      (boundp 'large-file-warning-threshold))
"##,
        expect,
    );
}

#[test]
fn div_cx231_revert_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((auto-save-default nil)
      (make-backup-files nil))
  (let* ((path (make-temp-file "neo-cx231-mega"))
         (data "Revert mega test café 世界 content"))
    (with-temp-buffer
      (insert data)
      (write-region (point-min) (point-max) path nil 'silent))
    (let ((buf (find-file-noselect path)))
      (with-current-buffer buf
        (buffer-enable-undo)
        (insert " MODIFIED")
        (put-text-property 1 5 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (widen)
            (set-buffer-modified-p nil)
            (kill-buffer buf)
            (delete-file path)
            (list state)))))
  )
"##,
        expect,
    );
}
