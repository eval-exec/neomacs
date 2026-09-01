//! Complex combo batch 218 — `projectile` / `treemacs` / `lsp-mode` /
//! `eglot` / `dap-mode` / `company` / `corfu` / `cape` development tool
//! availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx218_projectile_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'projectile)
          (fboundp 'projectile-mode)
          (fboundp 'projectile-find-file)
          (boundp 'projectile-project-root))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx218_lsp_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'lsp-mode)
          (fboundp 'lsp)
          (boundp 'lsp-headerline-breadcrumb-enable))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx218_eglot_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eglot)
      (list (fboundp 'eglot)
            (fboundp 'eglot-ensure)
            (boundp 'eglot-server-programs)
            (boundp 'eglot-autoshutdown)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx218_company_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'company)
          (fboundp 'company-mode)
          (fboundp 'global-company-mode)
          (boundp 'company-backends)
          (boundp 'company-idle-delay))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx218_corfu_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'corfu)
          (fboundp 'corfu-mode)
          (boundp 'corfu-auto)
          (boundp 'corfu-count))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx218_cape_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'cape)
          (fboundp 'cape-dict-file)
          (fboundp 'completion-at-point)
          (boundp 'cape-dict-file))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx218_dap_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'dap-mode)
          (fboundp 'dap-mode)
          (fboundp 'dap-debug))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx218_treemacs_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'treemacs)
          (fboundp 'treemacs)
          (boundp 'treemacs-width))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx218_flycheck_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'flycheck)
          (fboundp 'flycheck-mode)
          (boundp 'flycheck-checkers)
          (boundp 'flycheck-check-syntax-automatically))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx218_devtools_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'eglot)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Devtools mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (fboundp 'eglot)
                             (boundp 'eglot-server-programs)
                             (featurep 'company)
                             (boundp 'company-backends)
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
    );
}
