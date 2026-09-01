//! Complex combo batch 160 — `package` / `elpa` / `package-upload` /
//! `package-keyring-update` / `package-desc-p` / `package-load-all` /
//! `load-history-roundtrip` / `eval-and-compile` / `eval-when-compile`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx160_package_desc_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'package)
      (let ((desc (package-desc-from-define
                   "neo-cx160-fake" '(1 2 3)
                   "Summary line."
                   '((emacs (25 1)) (cl-lib (0 5)))
                   'misc)))
        (list (package-desc-p desc)
              (package-desc-name desc)
              (package-desc-version desc)
              (package-desc-summary desc)
              (package-desc-requirements desc))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx160_package_version_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function version-list->)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (version-to-list "1.2.3")
      (version-to-list "1.2.3.4")
      (version-to-list "1.2")
      (version-list-< '(1 2 3) '(1 2 4))
      (version-list-= '(1 2 3) '(1 2 3))
      (version-list-> '(1 2 4) '(1 2 3)))
"##,
        expect,
    );
}

#[test]
fn div_cx160_package_version_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function package-version-join)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (package-version-join '(1 2 3))
      (package-version-join '(1 2 3 4))
      (package-version-join '(1))
      (condition-case e (package-version-join '()) (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx160_eval_and_compile_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t :both)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (eval-and-compile
    (defvar neo-cx160-evalc :both))
  (list (boundp 'neo-cx160-evalc)
        neo-cx160-evalc))
"##,
        expect,
    );
}

#[test]
fn div_cx160_eval_when_compile_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defvar neo-cx160-when-compile (eval-when-compile (+ 1 2 3)))
  (list (integerp neo-cx160-when-compile)
        neo-cx160-when-compile))
"##,
        expect,
    );
}

#[test]
fn div_cx160_load_history_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(require 'cl-lib)
(let* ((cl-path (locate-library "cl-lib"))
       (entry (cl-find-if (lambda (e) (equal (car e) cl-path)) load-history)))
  (list (consp entry)
        (stringp (car entry))
        (consp (cdr entry))
        (> (length entry) 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx160_elpa_archive_path_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'package-user-dir)
      (boundp 'package-directory-list)
      (stringp package-user-dir)
      (consp package-directory-list))
"##,
        expect,
    );
}

#[test]
fn div_cx160_package_archive_priorities_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable package-archive-priorities)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'package-archive-priorities)
      (consp package-archive-priorities)
      (boundp 'package-menu-hide-low-priority))
"##,
        expect,
    );
}

#[test]
fn div_cx160_package_init_helper_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'package-initialize)
      (fboundp 'package-activate-all)
      (fboundp 'package-activate)
      (boundp 'package-alist)
      (boundp 'package--builtins))
"##,
        expect,
    );
}

#[test]
fn div_cx160_load_suffixes_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (\".elc\" \".el\") (\".el\") t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (consp load-suffixes)
          (member ".elc" load-suffixes)
          (member ".el" load-suffixes)
          (consp load-file-rep-suffixes)
          (boundp 'load-path))
"##,
        expect,
    );
}

#[test]
fn div_cx160_with_temp_buffer_window_eval_macro_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Eval/compile mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((eval-when-result (eval-when-compile (* 6 7)))
            (eval-and-result (eval-and-compile (+ 10 20))))
        (let ((state (list eval-when-result eval-and-result
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx160_package_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'package)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Package version-join mega test content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (package-version-join '(1 2 3))
                             (version-to-list "2.0.5")
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
