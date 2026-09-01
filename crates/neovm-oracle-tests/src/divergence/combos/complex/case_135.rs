//! Complex combo batch 135 — `compile` / `grep` / `xref` / `project` /
//! `find-dired` / `lgrep` / `rgrep` availability and basic queries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx135_compile_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'compile)
      (list (fboundp 'compile)
            (fboundp 'recompile)
            (boundp 'compilation-error-regexp-alist)
            (boundp 'compile-command)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx135_grep_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'grep)
      (list (fboundp 'grep)
            (fboundp 'lgrep)
            (fboundp 'rgrep)
            (boundp 'grep-command)
            (boundp 'grep-find-template)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx135_xref_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'xref)
      (list (fboundp 'xref-find-definitions)
            (fboundp 'xref-find-references)
            (fboundp 'xref-pop-marker-stack)
            (boundp 'xref-marker-ring-length)
            (boundp 'xref-search-program)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx135_project_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'project)
      (list (fboundp 'project-current)
            (fboundp 'project-root)
            (fboundp 'project-find-file)
            (boundp 'project-find-functions)
            (boundp 'project-vc-extra-root-markers)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx135_find_dired_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'find-dired)
      (list (fboundp 'find-dired)
            (fboundp 'find-name-dired)
            (fboundp 'find-grep-dired)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx135_lgrep_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :ran""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((dir (make-temp-file "neo-cx135-lgrep" t))
           (f (expand-file-name "test.txt" dir)))
      (with-temp-buffer
        (insert "alpha\nbeta\ngamma\nalpha-beta\n")
        (write-region (point-min) (point-max) f nil 'silent))
      (let ((grep-template "grep -nH -e <R> <F>")
            (grep-find-template nil))
        (list (stringp grep-template)))
      (delete-directory dir t)
      :ran)
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx135_xref_backend_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'xref-backend-functions)
          (boundp 'xref-backend-functions)
          (boundp 'xref-show-definitions-function))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx135_project_root_via_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((root (make-temp-file "neo-cx135-root" t))
           (marker (expand-file-name ".git" root))
           (sub (expand-file-name "deep/sub/dir" root)))
      (make-directory marker t)
      (make-directory sub t)
      (let ((project-find-functions
             (list (lambda (dir)
                     (let ((m (locate-dominating-file dir ".git")))
                       (when m (cons 'transient m)))))))
        (let ((proj (project-current nil sub)))
          (delete-directory root t)
          (list proj (consp proj) (when (consp proj) (cdr proj))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx135_compilation_buffer_name_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (boundp 'compilation-buffer-name-function)
          (boundp 'compilation-save-buffers-predicate)
          (boundp 'compilation-ask-about-save)
          (boundp 'compilation-scroll-output))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx135_xref_query_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'xref-query-replace-in-results)
          (fboundp 'xref--read-from-minibuffer)
          (boundp 'xref-history-storage))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx135_project_ignore_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (boundp 'project-ignores)
          (boundp 'project-grep-ignore-files)
          (boundp 'project-files-cache))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx135_compile_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'compile)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Compilation mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (fboundp 'compile)
                             (boundp 'compile-command)
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
