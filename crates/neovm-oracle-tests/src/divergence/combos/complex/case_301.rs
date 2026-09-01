//! Complex combo batch 301 — `winner-mode` / `tab-bar` actual tab
//! operations / `project` detection with markers / `xref` backend /
//! `flymake` diagnostic creation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx301_winner_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'winner)
      (list (fboundp 'winner-mode)
            (fboundp 'winner-undo)
            (fboundp 'winner-redo)
            (boundp 'winner-ring-size)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx301_tab_bar_tab_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'tab-bar-new-tab)
          (fboundp 'tab-bar-switch-to-tab)
          (fboundp 'tab-bar-close-tab)
          (fboundp 'tab-bar-move-tab)
          (boundp 'tab-bar-show)
          (boundp 'tab-bar-tabs-function))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx301_project_detection_with_git_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((root (make-temp-file "neo-cx301-proj" t))
           (git-dir (expand-file-name ".git" root))
           (sub (expand-file-name "src/deep" root)))
      (make-directory git-dir t)
      (make-directory sub t)
      (let ((project-find-functions
             (list (lambda (dir)
                     (let ((m (locate-dominating-file dir ".git")))
                       (when m (cons 'transient m)))))))
        (let ((proj (project-current nil sub)))
          (delete-directory root t)
          (list (consp proj)
                (when (consp proj) (cdr proj))
                (eq (car proj) 'transient))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx301_xref_backend_functions_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'xref)
      (list (boundp 'xref-backend-functions)
            (fboundp 'xref-find-definitions)
            (fboundp 'xref-find-references)
            (fboundp 'xref--create-xref)
            (fboundp 'xref-make)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx301_flymake_diagnostic_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'flymake)
      (with-temp-buffer
        (insert "some code here")
        (let ((diag (flymake-make-diagnostic (current-buffer)
                                              5 9
                                              :warning
                                              "test warning message")))
          (list (flymake--diag-p diag)
                (flymake--diag-buffer diag)
                (flymake--diag-type diag)
                (flymake--diag-text diag)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx301_compile_error_regexp_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable compilation-error-regexp-alist)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((test-lines '("file.el:42:10:Error: undefined variable"
                     "file.el:99: Warning: unused variable"
                     "/path/to/file.el:1:1: run-time error")))
  (mapcar (lambda (line)
            (let (result)
              (dolist (re compilation-error-regexp-alist)
                (when (and (not result)
                           (string-match (car (assoc (car re)
                                                    compilation-error-regexp-alist-alist))
                                         line))
                  (setq result (list (match-beginning 0) (match-end 0)))))
              (list line result)))
          test-lines))
"##,
        expect,
    )
}

#[test]
fn div_cx301_window_configuration_save_restore_via_winner() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((config (current-window-configuration))
      (n-before (length (window-list))))
  (split-window)
  (let ((n-after-split (length (window-list))))
    (set-window-configuration config)
    (let ((n-restored (length (window-list))))
      (list n-before n-after-split n-restored
            (= n-before n-restored)))))
"##,
        expect,
    )
}

#[test]
fn div_cx301_project_files_query_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'project)
      (list (fboundp 'project-files)
            (fboundp 'project-current)
            (fboundp 'project-roots)
            (boundp 'project-vc-extra-root-markers)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx301_flymake_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'flymake)
      (list (fboundp 'flymake-mode)
            (fboundp 'flymake-start)
            (fboundp 'flymake-goto-next-error)
            (boundp 'flymake-fringe-indicator-position)
            (boundp 'flymake-no-changes-timeout)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx301_project_flymake_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'project)
      (require 'flymake)
      (require 'winner)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Project/flymake/winner mega test buffer content")
        (put-text-property 1 8 'face 'bold)
        (let ((m (set-marker (make-marker) 10))
              (ov (make-overlay 4 18)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 25)
          (let ((state (list (fboundp 'project-current)
                             (fboundp 'flymake-mode)
                             (fboundp 'winner-mode)
                             (fboundp 'flymake-make-diagnostic)
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
