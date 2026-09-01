//! Complex combo batch 101 — package manager / project / xref / vc /
//! directory tracking / desktop / saveplace / recentf availability
//! and state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx101_package_availability_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'package)
      (list (fboundp 'package-initialize)
            (fboundp 'package-refresh-contents)
            (fboundp 'package-install)
            (fboundp 'package-list-packages)
            (boundp 'package-archives)
            (boundp 'package-user-dir)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx101_use_package_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'use-package)
      (list (fboundp 'use-package)
            (boundp 'use-package-always-ensure)
            (boundp 'use-package-verbose)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx101_project_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'project)
      (list (fboundp 'project-current)
            (fboundp 'project-root)
            (fboundp 'project-find-file)
            (boundp 'project-find-functions)
            (boundp 'project-roots)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx101_xref_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'xref)
      (list (fboundp 'xref-find-definitions)
            (fboundp 'xref-find-references)
            (fboundp 'xref-pop-marker-stack)
            (boundp 'xref-marker-ring-length)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx101_vc_availability_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'vc)
      (list (fboundp 'vc-dir)
            (fboundp 'vc-diff)
            (fboundp 'vc-log-incoming)
            (fboundp 'vc-print-log)
            (boundp 'vc-handled-backends)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx101_saveplace_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'saveplace)
      (list (fboundp 'save-place-mode)
            (boundp 'save-place-file)
            (boundp 'save-place-limit)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx101_recentf_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'recentf)
      (list (fboundp 'recentf-mode)
            (fboundp 'recentf-add-file)
            (fboundp 'recentf-include-p)
            (boundp 'recentf-max-saved-items)
            (boundp 'recentf-list)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx101_desktop_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'desktop)
      (list (fboundp 'desktop-save)
            (fboundp 'desktop-read)
            (fboundp 'desktop-clear)
            (boundp 'desktop-dirname)
            (boundp 'desktop-restore-frames)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx101_savehist_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'savehist)
      (list (fboundp 'savehist-mode)
            (boundp 'savehist-file)
            (boundp 'savehist-additional-variables)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx101_project_root_finding_with_marker_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((root (make-temp-file "neo-cx101-root" t))
           (git-dir (expand-file-name ".git" root))
           (sub (expand-file-name "subdir" root))
           (deep (expand-file-name "deep" sub)))
      (make-directory git-dir t)
      (make-directory deep t)
      (let ((project-find-functions
             (list (lambda (dir)
                     (let ((marker (locate-dominating-file dir ".git")))
                       (when marker (cons 'transient marker)))))))
        (with-temp-buffer
          (buffer-enable-undo)
          (insert "Project mega test buffer content")
          (put-text-property 1 7 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 4 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 18)
            (let ((state (list (buffer-string)
                               (marker-position m)
                               (overlay-start ov) (overlay-end ov)
                               (text-properties-at 1))))
              (undo)
              (widen)
              (delete-directory root t)
              (list state (buffer-string) (marker-position m)
                    (overlay-start ov) (overlay-end ov)
                    (text-properties-at 1)))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx101_dired_basic_buffer_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'dired)
      (list (fboundp 'dired)
            (fboundp 'dired-jump)
            (boundp 'dired-listing-switches)
            (boundp 'dired-dwim-target)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx101_editorconfig_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'editorconfig)
          (fboundp 'editorconfig-mode)
          (boundp 'editorconfig-exec-path))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx101_flymake_flycheck_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t :no-flycheck)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (condition-case e (progn (require 'flymake) t) (error :no-flymake))
 (condition-case e (progn (require 'flycheck) t) (error :no-flycheck)))
"##,
        expect,
    );
}
