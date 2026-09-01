//! Complex combo batch 57 — fresh subsystem edges: diff-mode, smerge-mode,
//! electric-quote-mode, org-table formulas, project, xref, saveplace,
//! savehist, recentf, desktop, enriched text.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx57_diff_mode_parse_hunks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t diff-header nil 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,4 @@\n context\n-removed\n+added\n+new line\n")
      (diff-mode)
      (font-lock-fontify-buffer)
      (list (eq major-mode 'diff-mode)
            (get-text-property 1 'face)
            (get-text-property (point-min) 'diff-old)
            (next-single-property-change (point-min) 'face)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx57_smerge_mode_parse_conflict() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil :no-conflict)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "<<<<<<< HEAD\nmine\n=======\ntheirs\n>>>>>>> branch\n")
      (smerge-mode)
      (list (eq major-mode 'smerge-mode)
            (condition-case e2 (smerge-get-current) (error :no-conflict))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx57_electric_quote_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"It's a 'test' string\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (electric-quote-mode 1)
      (insert "It's a 'test' string")
      (buffer-string))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx57_org_table_formula() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| 1 | 2 |\\n| 3 | 4 |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "| 1 | 2 |\n| 3 | 4 |\n")
        (goto-char 1)
        (org-table-recalculate 'all)
        (buffer-string)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx57_project_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'project)
      (list (fboundp 'project-current)
            (fboundp 'project-root)
            (fboundp 'project-prompt-project-dir)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx57_xref_backend_api() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'xref)
      (list (fboundp 'xref-find-definitions)
            (fboundp 'xref-find-references)
            (fboundp 'xref-backend-functions)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx57_saveplace_save_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'saveplace)
      (list (boundp 'save-place-mode)
            (fboundp 'save-place-to-alist)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx57_savehist_save_minibuffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments condition-case 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    );
}

#[test]
fn div_cx57_recentf_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r##"
(condition-case e
    (progn (require 'recentf)
      (list (boundp 'recentf-mode)
            (boundp 'recentf-max-saved-items)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx57_desktop_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'desktop)
      (list (boundp 'desktop-save-mode)
            (fboundp 'desktop-save)
            (fboundp 'desktop-read)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx57_enriched_text_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'enriched)
      (with-temp-buffer
        (insert "Content-Type: text/enriched\n\ngood <bold>bold</bold> text")
        (enriched-decode (point-min) (point-max))
        (list (buffer-string) (text-properties-at 0) (text-properties-at 14))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx57_treesit_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'treesit-parser-create)
      (fboundp 'treesit-node-type)
      (featurep 'treesit))
"##,
        expect,
    );
}

#[test]
fn div_cx57_eglot_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'eglot)
      (boundp 'eglot-mode))
"##,
        expect,
    );
}

#[test]
fn div_cx57_org_src_block_execute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"
(condition-case e
    (progn (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n")
        (org-babel-execute-src-block)))
  (error (list :result :errored)))
"##,
    );
}

#[test]
fn div_cx57_org_export_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Table of Contents\\n_________________\\n\\n1. Heading\\n.. 1. Sub\\n\\n\\n1 Heading\\n=========\\n\\n1.1 Sub\\n~~~~~~~\\n\\n  Text here\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'org)
      (require 'ox-ascii)
      (with-temp-buffer
        (org-mode)
        (insert "* Heading\n** Sub\nText here\n")
        (let ((output (org-ascii-export-as-ascii)))
          (if output
              (with-current-buffer output
                (prog1 (buffer-string) (kill-buffer output)))
            :no-output))))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx57_glasses_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 18""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (glasses-mode 1)
      (insert "camelCaseVariable snake_case")
      (goto-char 1)
      (forward-word 1)
      (point))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx57_so_long_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :completed""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'so-long)
      (with-temp-buffer
        (insert (make-string 10000 ?x))
        (so-long))
      :completed)
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx57_longlines_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:not-available)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (longlines-mode 1)
      (insert "very long line that would wrap")
      (buffer-string))
  (error (list :not-available)))
"##,
        expect,
    );
}

#[test]
fn div_cx57_subword_superword_kill_undo_marker_overlay_narrow_display_textprop_evaporate_env_exitcode_coding_timer_weak_hash_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX57=v" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX57"))))
        (exit-code
         (let ((p (make-process :name "neo-cx57-ec" :command '("sh" "-c" "exit 3")))
           (accept-process-output p 2)
           (process-exit-status p))))
    (sit-for 0.01)
    (let ((weak-ht (make-hash-table :weakness 'key :test 'eq)))
      (puthash (cons 1 nil) :v weak-ht)
      (garbage-collect)
      (condition-case e
          (with-temp-buffer
            (buffer-enable-undo)
            (insert "pre myCamelCaseVar snake_case_var rest end")
            (put-text-property 1 3 'face 'bold)
            (put-text-property 4 6 'display "XX")
            (let ((ov (make-overlay 5 35)) (m (set-marker (make-marker) 15)))
              (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
              (narrow-to-region 3 43)
              (subword-mode 1)
              (goto-char 5) (kill-word 1)
              (undo-boundary)
              (goto-char 5) (upcase-word 1)
              (let ((state (list (buffer-string) (marker-position m)
                                 (overlayp ov) (overlay-start ov)
                                 (text-properties-at 1) (current-column))))
                (subword-mode -1) (undo)
                (list env-val exit-code timer-fired state (buffer-string)
                      (marker-position m) (overlayp ov) (overlay-start ov)
                      (text-properties-at 1) (current-column)
                      (hash-table-count weak-ht)))))
        (error (list env-val exit-code timer-fired :errored)))))
"##,
        expect,
    );
}
