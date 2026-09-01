//! Complex combo batch 281 — `package` / `straight` / `el-get` /
//! `quelpa` package manager availability; `mu4e` / `notmuch` email
//! clients; `pdf-tools` / `doc-view` document viewing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx281_package_full_api_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'package)
      (list (fboundp 'package-initialize)
            (fboundp 'package-refresh-contents)
            (fboundp 'package-install)
            (fboundp 'package-delete)
            (fboundp 'package-list-packages)
            (fboundp 'package-installed-p)
            (fboundp 'package-desc-p)
            (fboundp 'package-desc-name)
            (boundp 'package-archives)
            (boundp 'package-user-dir)
            (boundp 'package-selected-packages)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx281_straight_el_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'straight)
          (fboundp 'straight-use-package)
          (boundp 'straight-use-package-by-default))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx281_el_get_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'el-get)
          (fboundp 'el-get-install)
          (boundp 'el-get-recipe-path))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx281_quelpa_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'quelpa)
          (fboundp 'quelpa)
          (boundp 'quelpa-dir))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx281_mu4e_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'mu4e)
          (fboundp 'mu4e)
          (boundp 'mu4e-mu-binary))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx281_notmuch_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'notmuch)
          (fboundp 'notmuch)
          (boundp 'notmuch-command))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx281_pdf_tools_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'pdf-tools)
          (fboundp 'pdf-tools-install)
          (featurep 'pdf-view)
          (fboundp 'pdf-view-mode))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx281_doc_view_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'doc-view)
      (list (fboundp 'doc-view-mode)
            (boundp 'doc-view-ghostscript-program)
            (boundp 'doc-view-cache-directory)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx281_org_roam_deft_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (featurep 'org-roam)
      (fboundp 'org-roam-mode)
      (featurep 'deft)
      (fboundp 'deft)
      (featurep 'org-brain)
      (fboundp 'org-brain-mode))
"##,
        expect,
    )
}

#[test]
fn div_cx281_package_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((avail (list (fboundp 'package-initialize)
                   (featurep 'straight)
                   (featurep 'mu4e)
                   (featurep 'pdf-tools)
                   (fboundp 'doc-view-mode))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Package/email/pdf mega test buffer content")
    (put-text-property 1 8 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((state (list avail
                         (boundp 'package-archives)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
