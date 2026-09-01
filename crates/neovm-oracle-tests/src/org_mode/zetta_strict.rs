//! Zetta-strict combo tests for org-mode extreme edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-deadline combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_deadline_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H\\nDEADLINE: <2012-03-29 Thu>\" \"* H\\nDEADLINE: <2014-03-04 Tue>\" \"* H\\nDEADLINE: <2012-03-29 Thu +2y>\" \"* H\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-adapt-indentation nil))
    (list
     ;; Insert new deadline.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-deadline nil "<2012-03-29>") (buffer-string))
     ;; Replace existing.
     (with-temp-buffer (org-mode) (insert "* H\nDEADLINE: <2012-03-29>")
       (goto-char (point-min)) (org-deadline nil "<2014-03-04>") (buffer-string))
     ;; With repeater.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-deadline nil "<2012-03-29 +2y>") (buffer-string))
     ;; Remove with C-u.
     (with-temp-buffer (org-mode) (insert "* H\nDEADLINE: <2012-03-29>")
       (goto-char (point-min)) (org-deadline '(4)) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-schedule combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_schedule_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H\\nSCHEDULED: <2012-03-29 Thu>\" \"* H\\nSCHEDULED: <2014-03-04 Tue>\" \"* H\\nSCHEDULED: <2012-03-29 Thu +2y>\" \"* H\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-adapt-indentation nil))
    (list
     ;; Insert new schedule.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-schedule nil "<2012-03-29>") (buffer-string))
     ;; Replace existing.
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2012-03-29>")
       (goto-char (point-min)) (org-schedule nil "<2014-03-04>") (buffer-string))
     ;; With repeater.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-schedule nil "<2012-03-29 +2y>") (buffer-string))
     ;; Remove with C-u.
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2012-03-29>")
       (goto-char (point-min)) (org-schedule '(4)) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-set-property combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_set_property_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\":PROPERTIES:\\n:TEST: t\\n:END:\\n\" \"* H\\n:PROPERTIES:\\n:TEST: t\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Set property on empty buffer.
     (with-temp-buffer (org-mode)
       (let ((org-property-format "%s %s")) (org-set-property "TEST" "t"))
       (buffer-string))
     ;; Set property on headline.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min))
       (let ((org-adapt-indentation nil) (org-property-format "%s %s"))
         (org-set-property "TEST" "t"))
       (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-delete-property combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_delete_property_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\" \":PROPERTIES:\\n:TEST1: t\\n:END:\" \"* H\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Delete from drawer.
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:TEST: t\n:END:")
       (goto-char (point-min)) (org-delete-property "TEST") (buffer-string))
     ;; Delete one of two.
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:TEST1: t\n:TEST2: t\n:END:")
       (goto-char (point-min)) (org-delete-property "TEST2") (buffer-string))
     ;; Delete from headline.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:TEST: t\n:END:")
       (goto-char (point-min)) (org-delete-property "TEST") (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-entry-get combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_entry_get_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1\" \"1\" \"1\" \"1 2 3\" \"\" nil \"1\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Regular get.
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "A"))
     ;; From headline.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "A"))
     ;; Ignore case.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "a"))
     ;; Extended values.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A+: 2\n:A: 1\n:A+: 3\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "A"))
     ;; Empty value.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A:\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "A"))
     ;; nil value.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: nil\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "A"))
     ;; Inheritance.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:\n** H2")
       (goto-char (point-max)) (org-entry-get (point) "A" t))
     ;; Not found.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "B")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-entry-put combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_entry_put_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"* TODO H\" 0 8 (org-todo-head \"TODO\")) #(\"* H\" 0 3 (org-todo-head nil)) \"* [#A] H\" \"* H\\n:PROPERTIES:\\n:A:        2\\n:END:\" \"* H\\n:PROPERTIES:\\n:A:        1\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Set TODO property.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-entry-put (point) "TODO" "TODO") (buffer-string))
     ;; Remove TODO.
     (with-temp-buffer (org-mode) (insert "* TODO H")
       (goto-char (point-min)) (org-entry-put (point) "TODO" nil) (buffer-string))
     ;; Set priority.
     (with-temp-buffer (org-mode) (insert "* [#B] H")
       (goto-char (point-min)) (org-entry-put (point) "PRIORITY" "A") (buffer-string))
     ;; Set regular property.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-put (point) "A" "2") (buffer-string))
     ;; Set property without drawer.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-entry-put (point) "A" "1") (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-refile-get-targets combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_refile_get_targets_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Project A\" \"Design\" \"UI\" \"Implementation\" \"Project B\" \"Testing\" \"Unit tests\" \"Integration tests\" \"Archive\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (let ((org-mode-hook nil)
        (org-refile-targets '((nil :maxlevel . 3))))
    (with-temp-buffer (org-mode)
      (insert "* Project A\n** Design\n*** UI\n** Implementation\n* Project B\n** Testing\n*** Unit tests\n*** Integration tests\n* Archive :ARCHIVE:")
      (goto-char (point-min))
      (mapcar (lambda (r) (car r))
              (org-refile-get-targets)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-clock-table combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_clock_table_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #<killed buffer>""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-clock)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Task\n:LOGBOOK:\nCLOCK: [2023-10-13 Fri 10:00]--[2023-10-13 Fri 11:30] =>  1:30\n:END:")
      (goto-char (point-min))
      (let ((table (org-clock-get-table-data (current-buffer) '(:maxlevel 2))))
        (car table)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-footnote-action combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_footnote_action_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"Text[fn:1]\\n\\n* Footnotes\\n\\n[fn:1] \\n\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Create footnote.
     (with-temp-buffer (org-mode) (insert "Text")
       (goto-char (point-max)) (org-footnote-action) (buffer-string))
     ;; Go to definition.
     (with-temp-buffer (org-mode)
       (insert "Text[fn:1]\n\n[fn:1] Definition.")
       (goto-char (point-min)) (search-forward "[fn:1]")
       (org-footnote-action) (looking-at "Definition.")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-list-struct combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_list_struct_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- item1\n- item2\n  - sub1\n  - sub2\n- item3")
      (goto-char (point-min))
      (let ((struct (org-list-struct)))
        (length struct)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-toggle-checkbox combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_toggle_checkbox_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"- item\" \"- [ ] item\" \"- [ ] item\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Toggle on.
     (with-temp-buffer (org-mode) (insert "- item")
       (goto-char (point-min)) (org-toggle-checkbox) (buffer-string))
     ;; Toggle off.
     (with-temp-buffer (org-mode) (insert "- [X] item")
       (goto-char (point-min)) (org-toggle-checkbox) (buffer-string))
     ;; Toggle to intermediate.
     (with-temp-buffer (org-mode) (insert "- [X] item")
       (goto-char (point-min)) (org-toggle-checkbox 'checkbox) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-cycle-list-bullet combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_cycle_list_bullet_combinations() {
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
       (goto-char (point-min)) (org-cycle-list-bullet) (buffer-string))
     ;; Argument: specific bullet.
     (with-temp-buffer (org-mode) (insert "- item")
       (goto-char (point-min)) (org-cycle-list-bullet "1.") (buffer-string))
     ;; Argument: integer.
     (with-temp-buffer (org-mode) (insert "1. item")
       (goto-char (point-min)) (org-cycle-list-bullet 1) (buffer-string))
     ;; Argument: previous.
     (with-temp-buffer (org-mode) (insert "+ item")
       (goto-char (point-min)) (org-cycle-list-bullet 'previous) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-timer-secs-to-hms combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_timer_secs_to_hms_combinations() {
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

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-timer-fix-incomplete combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_timer_fix_incomplete_combinations() {
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

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-duration-to-minutes combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_duration_to_minutes_combinations() {
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

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-duration-from-minutes combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_duration_from_minutes_combinations() {
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

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-duration-p combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_duration_p_combinations() {
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

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-columns-compile-format combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_columns_compile_format_combinations() {
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

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-columns-uncompile-format combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_columns_uncompile_format_combinations() {
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

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-macro-replace-all combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_macro_replace_all_combinations() {
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
       (goto-char (point-min)) (org-macro-initialize-templates)
       (org-macro-replace-all org-macro-templates) (buffer-string))
     ;; With arguments.
     (with-temp-buffer (org-mode)
       (insert "#+MACRO: macro $1 $2\n{{{macro(some,text)}}}")
       (goto-char (point-min)) (org-macro-initialize-templates)
       (org-macro-replace-all org-macro-templates) (buffer-string))
     ;; Nested macros.
     (with-temp-buffer (org-mode)
       (insert "#+MACRO: in inner\n#+MACRO: out {{{in}}} outer\n{{{out}}}")
       (goto-char (point-min)) (org-macro-initialize-templates)
       (org-macro-replace-all org-macro-templates) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-archive-subtree combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_archive_subtree_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"No file associated to buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-archive)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Top\n** DONE One\n** TODO Two")
      (goto-char (point-min)) (forward-line 1) (org-archive-subtree)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Zetta: org-element with all org-datetree-find-date-create combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn zetta_all_datetree_find_date_create_combinations() {
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
     (with-temp-buffer (org-mode) (insert "* 2012\n")
       (org-datetree-find-date-create '(3 29 2012))
       (org-trim (buffer-string))))))"##,
        expect,
    );
}
