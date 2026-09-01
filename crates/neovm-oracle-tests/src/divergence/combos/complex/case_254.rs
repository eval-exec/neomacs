//! Complex combo batch 254 — `dir-locals` / `project` root finding /
//! `vc` backend detection / `compilation-mode` error navigation /
//! `flymake` diagnostics availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx254_dir_locals_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t ((neo-cx254-var . \"test-value\")) ((neo-cx254-elisp . t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((dir-locals-class-alist nil)
      (dir-locals-directory-cache nil))
  (let* ((dir (make-temp-file "neo-cx254-dirlocals" t))
         (locals-file (expand-file-name ".dir-locals.el" dir)))
    (unwind-protect
        (progn
          (with-temp-buffer
            (insert "((nil . ((neo-cx254-var . \"test-value\")))
                      (emacs-lisp-mode . ((neo-cx254-elisp . t))))")
            (write-region (point-min) (point-max) locals-file nil 'silent))
          (condition-case e
              (let* ((class (dir-locals-read-from-file dir))
                     (variables (dir-locals-get-class-variables class)))
                (list (symbolp class)
                      (cdr (assq nil variables))
                      (cdr (assq 'emacs-lisp-mode variables))))
            (error (list :errored (car e)))))
      (when (file-exists-p dir) (delete-directory dir t)))))
"##,
        expect,
    )
}

#[test]
fn div_cx254_dir_locals_find() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'hack-dir-local-variables)
          (fboundp 'hack-dir-local-variables-non-file-buffer)
          (boundp 'dir-locals-file)
          (boundp 'enable-dir-local-variables)
          (boundp 'enable-remote-dir-variables))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx254_project_root_finding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'project)
      (let* ((root (make-temp-file "neo-cx254-proj" t))
             (git-dir (expand-file-name ".git" root))
             (sub (expand-file-name "src/sub" root))
             (deep (expand-file-name "deep/dir" sub)))
        (make-directory git-dir t)
        (make-directory deep t)
        (let ((project-find-functions
               (list (lambda (dir)
                       (let ((m (locate-dominating-file dir ".git")))
                         (when m (cons 'transient m)))))))
          (let ((proj (project-current nil deep)))
            (delete-directory root t)
            (list (consp proj)
                  (when (consp proj) (cdr proj))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx254_vc_backend_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'vc)
      (list (fboundp 'vc-backend)
            (fboundp 'vc-responsible-backend)
            (fboundp 'vc-root-dir)
            (boundp 'vc-handled-backends)
            (boundp 'vc-follow-symlinks)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx254_compilation_mode_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'compile)
      (list (fboundp 'compilation-mode)
            (fboundp 'next-error)
            (fboundp 'previous-error)
            (fboundp 'first-error)
            (boundp 'compilation-error-regexp-alist)
            (boundp 'compilation-auto-jump-to-first-error)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx254_flymake_diagnostics_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'flymake)
      (list (fboundp 'flymake-mode)
            (fboundp 'flymake-start)
            (fboundp 'flymake-goto-next-error)
            (fboundp 'flymake-goto-prev-error)
            (boundp 'flymake-fringe-indicator-position)
            (boundp 'flymake-no-changes-timeout)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx254_flymake_diagnostic_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'flymake-make-diagnostic)
          (fboundp 'flymake-diagnostics)
          (fboundp 'flymake-diagnostic-buffer)
          (fboundp 'flymake-diagnostic-text)
          (boundp 'flymake-diagnostic-functions))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx254_next_error_navigation_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'next-error)
      (fboundp 'previous-error)
      (fboundp 'next-error-select-buffer)
      (boundp 'next-error-highlight)
      (boundp 'next-error-recenter)
      (boundp 'next-error-hook))
"##,
        expect,
    )
}

#[test]
fn div_cx254_project_files_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'project)
      (list (fboundp 'project-files)
            (fboundp 'project-current)
            (fboundp 'project-root)
            (boundp 'project-vc-extra-root-markers)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx254_project_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'project)
      (require 'compile)
      (require 'flymake)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Project/compile/flymake mega test buffer content")
        (put-text-property 1 8 'face 'bold)
        (let ((m (set-marker (make-marker) 10))
              (ov (make-overlay 4 18)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 25)
          (let ((state (list (fboundp 'project-current)
                             (fboundp 'flymake-mode)
                             (boundp 'compilation-error-regexp-alist)
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
