//! Complex combo batch 255 — `magit` / `forge` / `transient` / `git-commit` /
//! `vc-git` availability and repository state queries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx255_magit_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'magit)
          (fboundp 'magit-status)
          (fboundp 'magit-dispatch)
          (boundp 'magit repository-directories))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx255_forge_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'forge)
          (fboundp 'forge-dispatch)
          (boundp 'forge-database-file))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx255_transient_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'transient)
      (list (fboundp 'transient-define-prefix)
            (fboundp 'transient-insert-suffix)
            (boundp 'transient-levels)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx255_git_commit_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored file-missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'git-commit)
      (list (fboundp 'git-commit-mode)
            (boundp 'git-commit-summary-max-length)
            (boundp 'git-commit-fill-column)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx255_vc_git_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'vc-git)
      (list (fboundp 'vc-git-registered)
            (fboundp 'vc-git-state)
            (fboundp 'vc-git-command)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx255_log_edit_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'log-edit)
      (fboundp 'log-edit-mode)
      (boundp 'log-edit-confirm))
"##,
        expect,
    )
}

#[test]
fn div_cx255_vc_dir_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'vc-dir)
      (list (fboundp 'vc-dir)
            (boundp 'vc-dir-backend)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx255_vc_annotate_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'vc-annotate)
      (fboundp 'vc-annotate-display)
      (boundp 'vc-annotate-color-map))
"##,
        expect,
    )
}

#[test]
fn div_cx255_diff_mode_navigation_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'diff-mode)
      (list (fboundp 'diff-hunk-next)
            (fboundp 'diff-hunk-prev)
            (fboundp 'diff-file-next)
            (fboundp 'diff-file-prev)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx255_vc_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'vc)
      (require 'diff-mode)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "VC/magit mega test buffer content here")
        (put-text-property 1 5 'face 'bold)
        (let ((m (set-marker (make-marker) 10))
              (ov (make-overlay 4 18)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 25)
          (let ((state (list (fboundp 'vc-dir)
                             (fboundp 'vc-annotate)
                             (boundp 'vc-handled-backends)
                             (featurep 'magit)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}
