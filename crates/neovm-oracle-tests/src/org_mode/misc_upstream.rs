//! Ported upstream ERT tests from org-mode's smaller test files (9.7.11).
//!
//! Covers: test-org-clock, test-org-list, test-org-footnote,
//! test-org-timer, test-org-duration, test-org-num, test-org-colview,
//! test-org-archive, test-org-datetree, test-org-macro, test-oc.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ── Clock: org-clock-into-drawer ─────────────────────────────────────

#[test]
fn upstream_org_clock_into_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-into-drawer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; nil clock-into-drawer.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min))
       (let ((org-clock-into-drawer nil) (org-log-into-drawer nil))
         (org-clock-into-drawer)))
     ;; String clock-into-drawer.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min))
       (let ((org-clock-into-drawer "FOO") (org-log-into-drawer nil))
         (org-clock-into-drawer)))
     ;; Number clock-into-drawer.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min))
       (let ((org-clock-into-drawer 2) (org-log-into-drawer nil))
         (org-clock-into-drawer))))))"##,
        expect,
    );
}

// ── List: list-ending ────────────────────────────────────────────────

#[test]
fn upstream_org_list_ending() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; With two blank lines.
     (with-temp-buffer (org-mode) (insert "- item\n\n\n  Text")
       (goto-line 4) (org-in-item-p))
     ;; With text less indented.
     (with-temp-buffer (org-mode) (insert "- item\nText")
       (goto-line 2) (org-in-item-p))
     ;; In blocks: ignored.
     (with-temp-buffer (org-mode)
       (insert "- item\n  #+begin_quote\n\n\nText at column 0\n  #+end_quote\n Text")
       (goto-line 7) (org-in-item-p)))))"##,
        expect,
    );
}

// ── List: cycle-bullet ───────────────────────────────────────────────

#[test]
fn upstream_org_cycle_list_bullet_spec() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"  + item\" \"1. item\" \"+ item\" \"- item\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-plain-list-ordered-item-terminator t))
    (list
     ;; Cycle through bullets.
     (with-temp-buffer (org-mode) (insert "  - item")
       (goto-char (point-min))
       (org-cycle-list-bullet) (buffer-string))
     ;; Argument: specific bullet.
     (with-temp-buffer (org-mode) (insert "- item")
       (goto-char (point-min))
       (org-cycle-list-bullet "1.") (buffer-string))
     ;; Argument: integer.
     (with-temp-buffer (org-mode) (insert "1. item")
       (goto-char (point-min))
       (org-cycle-list-bullet 1) (buffer-string))
     ;; Argument: previous.
     (with-temp-buffer (org-mode) (insert "+ item")
       (goto-char (point-min))
       (org-cycle-list-bullet 'previous) (buffer-string)))))"##,
        expect,
    );
}

// ── Footnote: org-footnote-new ───────────────────────────────────────

#[test]
fn upstream_org_footnote_new() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Test[fn:1]\\n\\n[fn:1] \\n\" \"Test[fn::]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-footnote-auto-label t)
        (org-footnote-section nil))
    (list
     ;; Create footnote with auto label.
     (with-temp-buffer (org-mode) (insert "Test")
       (goto-char (point-max))
       (org-footnote-new) (buffer-string))
     ;; Anonymous footnote.
     (with-temp-buffer (org-mode) (insert "Test")
       (goto-char (point-max))
       (let ((org-footnote-auto-label 'anonymous))
         (org-footnote-new)) (buffer-string)))))"##,
        expect,
    );
}

// ── Footnote: org-footnote-delete ────────────────────────────────────

#[test]
fn upstream_org_footnote_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Don’t know which footnote to remove\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-footnote-section nil))
    (list
     ;; Delete regular footnote.
     (with-temp-buffer (org-mode)
       (insert "Paragraph[fn:1]\n\n[fn:1] Definition")
       (goto-char (point-min))
       (search-forward "[fn:1]")
       (org-footnote-delete)
       (org-trim (buffer-string)))
     ;; Delete anonymous footnote.
     (with-temp-buffer (org-mode)
       (insert "Para[fn::def]")
       (goto-char (point-min))
       (search-forward "[fn::")
       (org-footnote-delete)
       (org-trim (buffer-string))))))"##,
        expect,
    );
}

// ── Timer: secs-to-hms ───────────────────────────────────────────────

#[test]
fn upstream_org_timer_secs_to_hms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"0:00:30\" \"0:02:10\" \"1:01:30\" \"-1:01:30\" 30 130 3690)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-timer)
  (list
   (org-timer-secs-to-hms 30)
   (org-timer-secs-to-hms 130)
   (org-timer-secs-to-hms 3690)
   (org-timer-secs-to-hms -3690)
   ;; Round-trip.
   (org-timer-hms-to-secs (org-timer-secs-to-hms 30))
   (org-timer-hms-to-secs (org-timer-secs-to-hms 130))
   (org-timer-hms-to-secs (org-timer-secs-to-hms 3690))))"##,
        expect,
    );
}

// ── Timer: fix-incomplete ────────────────────────────────────────────

#[test]
fn upstream_org_timer_fix_incomplete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1:02:03\" \"0:02:03\" \"0:00:03\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-timer)
  (list
   (org-timer-fix-incomplete "1:02:03")
   (org-timer-fix-incomplete "02:03")
   (org-timer-fix-incomplete "03")))"##,
        expect,
    );
}

// ── Timer: change-times-in-region ────────────────────────────────────

#[test]
fn upstream_org_timer_change_times() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\\n1:31:15\\n4:00:55\" \"\\n-1:30:25\\n0:59:15\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-timer)
  (let ((org-mode-hook nil))
    (list
     ;; Add time.
     (with-temp-buffer (org-mode)
       (insert "\n0:00:25\n2:30:05")
       (org-timer-change-times-in-region (point-min) (point-max) "1:30:50")
       (buffer-string))
     ;; Subtract time.
     (with-temp-buffer (org-mode)
       (insert "\n0:00:25\n2:30:05")
       (org-timer-change-times-in-region (point-min) (point-max) "-1:30:50")
       (buffer-string)))))"##,
        expect,
    );
}

// ── Duration: to-minutes ─────────────────────────────────────────────

#[test]
fn upstream_org_duration_to_minutes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (61.0 80.5 130.0 1502.0 150.0 2.0 0.0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-duration)
  (list
   (org-duration-to-minutes "1:01")
   (org-duration-to-minutes "1:20:30")
   (org-duration-to-minutes "2h 10min")
   (org-duration-to-minutes "1d 1:02")
   (org-duration-to-minutes "2.5h")
   (org-duration-to-minutes "2")
   (org-duration-to-minutes "")
   (floatp (org-duration-to-minutes "1:01"))))"##,
        expect,
    );
}

// ── Duration: from-minutes ───────────────────────────────────────────

#[test]
fn upstream_org_duration_from_minutes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"1:00\" \"1:01:30\" \"1:01\" \"1h\" \"1h 0min\" \"50min\" \"0h 50min\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-duration)
  (list
   (let ((org-duration-format 'h:mm)) (org-duration-from-minutes 60))
   (let ((org-duration-format 'h:mm:ss)) (org-duration-from-minutes 61.5))
   (let ((org-duration-format 'h:mm)) (org-duration-from-minutes 61.5))
   (let ((org-duration-format '(("h" . nil) ("min" . nil)))) (org-duration-from-minutes 60))
   (let ((org-duration-format '(("h" . nil) ("min" . t)))) (org-duration-from-minutes 60))
   (let ((org-duration-format '(("h" . nil) ("min" . nil)))) (org-duration-from-minutes 50))
   (let ((org-duration-format '(("h" . t) ("min" . t)))) (org-duration-from-minutes 50))))"##,
        expect,
    );
}

// ── Duration: org-duration-p ─────────────────────────────────────────

#[test]
fn upstream_org_duration_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 0 0 0 0 0 0 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-duration)
  (list
   (org-duration-p "3:12")
   (org-duration-p "123:12")
   (org-duration-p "1:23:45")
   (org-duration-p "3d 3h 4min")
   (org-duration-p "3d3h4min")
   (org-duration-p "3d 13:35")
   (org-duration-p "2.35h")
   (org-duration-p "2 h")
   ;; Invalid.
   (org-duration-p "3::12")
   (org-duration-p "3:2")))"##,
        expect,
    );
}

// ── Colview: compile-format / uncompile-format ───────────────────────

#[test]
fn upstream_org_colview_compile_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"ITEM\" \"ITEM\" nil nil nil)) ((\"ITEM\" \"ITEM\" nil nil nil) (\"TODO\" \"TODO\" nil nil nil)) ((\"ITEM\" \"ITEM\" 10 nil nil)) ((\"ITEM\" \"some title\" nil nil nil)) ((\"ITEM\" \"ITEM\" nil \"+\" nil)) ((\"ITEM\" \"ITEM\" nil \"+\" \"%.1f\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-colview)
  (list
   ;; Single element.
   (org-columns-compile-format "%ITEM")
   ;; Two elements.
   (org-columns-compile-format "%ITEM %TODO")
   ;; With width.
   (org-columns-compile-format "%10ITEM")
   ;; With title.
   (org-columns-compile-format "%ITEM(some title)")
   ;; With operator.
   (org-columns-compile-format "%ITEM{+}")
   ;; With operator and printf.
   (org-columns-compile-format "%ITEM{+;%.1f}")))"##,
        expect,
    );
}

#[test]
fn upstream_org_colview_uncompile_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"%ITEM\" \"%ITEM %TODO\" \"%10ITEM\" \"%ITEM(some title)\" \"%ITEM{+}\" \"%ITEM{+;%.1f}\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-colview)
  (list
   ;; Single element.
   (org-columns-uncompile-format '(("ITEM" "ITEM" nil nil nil)))
   ;; Two elements.
   (org-columns-uncompile-format '(("ITEM" "ITEM" nil nil nil) ("TODO" "TODO" nil nil nil)))
   ;; With width.
   (org-columns-uncompile-format '(("ITEM" "ITEM" 10 nil nil)))
   ;; With title.
   (org-columns-uncompile-format '(("ITEM" "some title" nil nil nil)))
   ;; With operator.
   (org-columns-uncompile-format '(("ITEM" "ITEM" nil "+" nil)))
   ;; With operator and printf.
   (org-columns-uncompile-format '(("ITEM" "ITEM" nil "+" "%.1f")))))"##,
        expect,
    );
}

// ── Macro: macro-replace-all ─────────────────────────────────────────

#[test]
fn upstream_org_macro_replace_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-macro)
  (let ((org-mode-hook nil))
    (list
     ;; Standard.
     (with-temp-buffer (org-mode)
       (insert "#+MACRO: A B\n1 {{{A}}} 3")
       (goto-char (point-min))
       (org-macro-initialize-templates)
       (org-macro-replace-all org-macro-templates)
       (buffer-string))
     ;; With arguments.
     (with-temp-buffer (org-mode)
       (insert "#+MACRO: macro $1 $2\n{{{macro(some,text)}}}")
       (goto-char (point-min))
       (org-macro-initialize-templates)
       (org-macro-replace-all org-macro-templates)
       (buffer-string))
     ;; Nested macros.
     (with-temp-buffer (org-mode)
       (insert "#+MACRO: in inner\n#+MACRO: out {{{in}}} outer\n{{{out}}}")
       (goto-char (point-min))
       (org-macro-initialize-templates)
       (org-macro-replace-all org-macro-templates)
       (buffer-string)))))"##,
        expect,
    );
}

// ── Archive: org-archive-subtree ─────────────────────────────────────

#[test]
fn upstream_org_archive_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"No file associated to buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-archive)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Top\n** DONE One\n** TODO Two")
      (goto-char (point-min))
      (forward-line 1)
      (org-archive-subtree)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Datetree: find-date-create ───────────────────────────────────────

#[test]
fn upstream_org_datetree_find_date_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* 2012\\n\\n** 2012-03 March\\n\\n*** 2012-03-29 Thursday\" \"* 2012\\n\\n** 2012-03 March\\n\\n*** 2012-03-29 Thursday\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-datetree)
  (let ((org-mode-hook nil)
        (org-datetree-add-timestamp nil)
        (org-blank-before-new-entry '((heading . t))))
    (list
     ;; Create from empty.
     (with-temp-buffer (org-mode)
       (org-datetree-find-date-create '(3 29 2012))
       (org-trim (buffer-string)))
     ;; Don't duplicate year.
     (with-temp-buffer (org-mode)
       (insert "* 2012\n")
       (org-datetree-find-date-create '(3 29 2012))
       (org-trim (buffer-string))))))"##,
        expect,
    );
}

// ── OC: register/unregister processor ────────────────────────────────

#[test]
fn upstream_oc_register_processor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 1 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'oc)
  (list
   ;; Register.
   (let ((org-cite--processors nil))
     (org-cite-register-processor 'name)
     (and (assq 'name org-cite--processors) t))
   ;; Duplicate register.
   (let ((org-cite--processors nil))
     (org-cite-register-processor 'name)
     (org-cite-register-processor 'name)
     (length org-cite--processors))
   ;; Unregister.
   (let ((org-cite--processors nil))
     (org-cite-register-processor 'name)
     (org-cite-unregister-processor 'name)
     org-cite--processors)))"##,
        expect,
    );
}

// ── Fold: hide-drawer-toggle ─────────────────────────────────────────

#[test]
fn upstream_org_fold_hide_drawer_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-fold)
  (let ((org-mode-hook nil))
    (list
     ;; Hide drawer.
     (with-temp-buffer (org-mode) (insert ":drawer:\ncontents\n:end:")
       (goto-char (point-min))
       (org-fold-show-all)
       (org-fold-hide-drawer-toggle)
       (get-char-property (line-end-position) 'invisible))
     ;; Show drawer.
     (with-temp-buffer (org-mode) (insert ":drawer:\ncontents\n:end:")
       (goto-char (point-min))
       (org-fold-hide-drawer-toggle)
       (org-fold-hide-drawer-toggle 'off)
       (get-char-property (line-end-position) 'invisible))
     ;; Hide unconditionally.
     (with-temp-buffer (org-mode) (insert ":drawer:\ncontents\n:end:")
       (goto-char (point-min))
       (org-fold-hide-drawer-toggle t)
       (get-char-property (line-end-position) 'invisible)))))"##,
        expect,
    );
}

// ── Fold: hide-block-toggle ──────────────────────────────────────────

#[test]
fn upstream_org_fold_hide_block_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-fold)
  (let ((org-mode-hook nil))
    (list
     ;; Hide block.
     (with-temp-buffer (org-mode)
       (insert "#+BEGIN_CENTER\ncontents\n#+END_CENTER")
       (goto-char (point-min))
       (org-fold-hide-block-toggle)
       (get-char-property (line-end-position) 'invisible))
     ;; Show block.
     (with-temp-buffer (org-mode)
       (insert "#+BEGIN_CENTER\ncontents\n#+END_CENTER")
       (goto-char (point-min))
       (org-fold-hide-block-toggle)
       (org-fold-hide-block-toggle 'off)
       (get-char-property (line-end-position) 'invisible))
     ;; Hide unconditionally.
     (with-temp-buffer (org-mode)
       (insert "#+BEGIN_CENTER\ncontents\n#+END_CENTER")
       (goto-char (point-min))
       (org-fold-hide-block-toggle t)
       (get-char-property (line-end-position) 'invisible)))))"##,
        expect,
    );
}

// ── Num: org-num-mode ────────────────────────────────────────────────

#[test]
fn upstream_org_num_max_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"1 \" 0 2 (face org-level-1)) #(\"1.1 \" 0 4 (face org-level-2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-num)
  (let ((org-mode-hook nil)
        (org-num-max-level 2))
    (with-temp-buffer (org-mode) (insert "* H1\n** H2\n*** H3")
      (goto-char (point-min))
      (org-num-mode 1)
      (sort (mapcar (lambda (o) (overlay-get o 'after-string))
                    (overlays-in (point-min) (point-max)))
            #'string-lessp))))"##,
        expect,
    );
}

// ── Src: org-edit-special ────────────────────────────────────────────

#[test]
fn upstream_org_edit_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-src)
  (let ((org-mode-hook nil)
        (org-edit-src-content-indentation 2)
        (org-src-preserve-indentation nil))
    (with-temp-buffer (org-mode)
      (insert "\n#+begin_src emacs-lisp\n  (message hello)\n#+end_src\n")
      (goto-char (point-min))
      (forward-line 1)
      (org-edit-special)
      (insert "blah")
      (org-edit-src-exit)
      (buffer-string))))"##,
        expect,
    );
}
