//! Complex combo batch 146 — `package` / `elpa` / `melpa` / `use-package`
//! deferred loading, `package-autoload-file`, `package-user-dir` queries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx146_package_availability() {
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
            (boundp 'package-archives)
            (boundp 'package-user-dir)
            (boundp 'package-directory-list)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx146_melpa_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t (\"melpa\" . \"https://melpa.org/packages/\") (\"gnu\" . \"https://elpa.gnu.org/packages/\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((archives '(("melpa" . "https://melpa.org/packages/")
                      ("gnu" . "https://elpa.gnu.org/packages/")
                      ("nongnu" . "https://elpa.nongnu.org/packages/"))))
      (list (consp archives)
            (assoc "melpa" archives)
            (assoc "gnu" archives)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx146_use_package_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'use-package)
      (list (fboundp 'use-package)
            (boundp 'use-package-always-ensure)
            (boundp 'use-package-verbose)
            (boundp 'use-package-minimum-reported-time)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx146_package_desc_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'package-desc-from-define)
          (fboundp 'package-desc-p)
          (fboundp 'package-desc-name)
          (fboundp 'package-desc-version)
          (boundp 'package--builtins))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx146_package_version_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'version-to-list)
          (fboundp 'package-version-join)
          (version-to-list "1.2.3")
          (package-version-join '(1 2 3))
          (package-version-join '(1 2 3 4)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx146_use_package_expand_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((expanded (macroexpand
                     '(use-package neo-cx146-fake
                        :ensure t
                        :defer t
                        :bind ("C-c C-a" . neo-cx146-fake-action)
                        :config (message "loaded")))))
      (list (consp expanded)
            (eq (car expanded) 'progn)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx146_package_installed_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'package-installed-p)
          (fboundp 'package-delete)
          (fboundp 'package-list-packages)
          (boundp 'package-selected-packages))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx146_elpa_archive_url_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-variable)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (boundp 'package-archives)
          (consp package-archives)
          (stringp (cdr (car package-archives))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx146_package_activated_list_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (boundp 'package-activated-list)
          (boundp 'package-load-list)
          (boundp 'package-archive-priorities))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx146_package_import_with_existing_directory() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'package-import-keyring)
          (fboundp 'package-refresh-contents)
          (boundp 'package-gnupghome-dir))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx146_use_package_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'package)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Package mega test buffer content")
        (put-text-property 1 7 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (fboundp 'package-initialize)
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
  (error (list :errored (car e))))
"##,
        expect,
    );
}
