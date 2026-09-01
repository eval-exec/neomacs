//! Eta-strict combo tests for org-mode complex interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex table formulas
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_table_formula_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a #+TBLFM line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| 2 |\n| 4 |\n| 8 |\n|   |\n#+TBLFM: @>$1=vsum(@<..@>>)")
      (goto-char (point-min))
      (org-table-calc-current-TBLFM)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_formula_multiply() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a #+TBLFM line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| 3 | 4 |   |\n#+TBLFM: $3=$1*$2")
      (goto-char (point-min))
      (org-table-calc-current-TBLFM)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_formula_column_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a #+TBLFM line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| 1 | 2 |\n| 3 | 4 |\n|   |   |\n#+TBLFM: @3$1=vsum(@1$1..@2$1)::@3$2=vsum(@1$2..@2$2)")
      (goto-char (point-min))
      (org-table-calc-current-TBLFM)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_formula_with_title_row() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a #+TBLFM line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| foo |\n|-----|\n|   2 |\n|   4 |\n|   8 |\n|     |\n#+TBLFM: @>$1=vsum(@I..@>>)")
      (goto-char (point-min))
      (org-table-calc-current-TBLFM)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_formula_remote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a #+TBLFM line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+NAME: mytable\n| 1 | 2 |\n| 3 | 4 |\n\n|   |   |\n#+TBLFM: $1=remote(mytable,@2$1)::$2=remote(mytable,@2$2)")
      (goto-char (point-min))
      (org-table-calc-current-TBLFM)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex table operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_table_align() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a | b |\\n| c | d |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "|a|b|\n|c|d|")
      (goto-char (point-min)) (org-table-align)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_insert_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"|   | a | b |\\n|   | c | d |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a | b |\n| c | d |")
      (goto-char (point-min)) (org-table-insert-column)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_delete_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| b | c |\\n| e | f |\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a | b | c |\n| d | e | f |")
      (goto-char (point-min)) (forward-char 4) (org-table-delete-column)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_insert_row() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"|   |   |\\n| a | b |\\n| c | d |\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a | b |\n| c | d |")
      (goto-char (point-min)) (org-table-insert-row)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_kill_row() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a | b |\\n| e | f |\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a | b |\n| c | d |\n| e | f |")
      (goto-char (point-min)) (forward-line 1) (org-table-kill-row)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_move_column_right() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| b | a | c |\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a | b | c |")
      (goto-char (point-min)) (forward-char 2) (org-table-move-column-right)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_move_column_left() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| b | a | c |\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a | b | c |")
      (goto-char (point-min)) (forward-char 6) (org-table-move-column-left)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_move_row_down() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| b |\\n| a |\\n| c |\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a |\n| b |\n| c |")
      (goto-char (point-min)) (org-table-move-row-down)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_move_row_up() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a |\\n| c |\\n| b |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a |\n| b |\n| c |")
      (goto-char (point-min)) (forward-line 2) (org-table-move-row-up)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Format specifier doesn’t match argument type\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| c |\n| a |\n| b |")
      (goto-char (point-min)) (org-table-sort-lines ?a 'string)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a | c | e |\\n| b | d | f |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a | b |\n| c | d |\n| e | f |")
      (goto-char (point-min)) (org-table-transpose-table-at-point)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_convert_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a | b | c |\\n| d | e | f |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "a\tb\tc\nd\te\tf")
      (goto-char (point-min))
      (org-table-convert-region (point-min) (point-max))
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"|   |   |   |\\n|---+---+---|\\n|   |   |   |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (org-table-create "3x2")
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn eta_table_get_field() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\" \"d\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| a | b |\n| c | d |")
      (goto-char (point-min))
      (list (org-table-get 1 1) (org-table-get 1 2)
            (org-table-get 2 1) (org-table-get 2 2)))))"##,
        expect,
    );
}

#[test]
fn eta_table_blank_field() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not in table data field\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert "| value |")
      (goto-char (point-min)) (org-table-blank-field)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex table references
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_table_convert_refs_to_an() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A2\" \"A1 = $0\" \"C& = remote(FOO, @@#B&)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (list
   (org-table-convert-refs-to-an "@2$1")
   (org-table-convert-refs-to-an "@1$1 = $0")
   (org-table-convert-refs-to-an "$3 = remote(FOO, @@#$2)")))"##,
        expect,
    );
}

#[test]
fn eta_table_convert_refs_to_rc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"@2$1\" \"@1$1 = $0\" \"$3 = remote(FOO, @@#$2)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (list
   (org-table-convert-refs-to-rc "A2")
   (org-table-convert-refs-to-rc "A1 = $0")
   (org-table-convert-refs-to-rc "C& = remote(FOO, @@#B&)")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex list operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_list_struct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- item1\n- item2\n  - sub1\n  - sub2\n- item3")
      (goto-char (point-min))
      (length (org-list-struct)))))"##,
        expect,
    );
}

#[test]
fn eta_toggle_checkbox() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"- item\" \"- [ ] item\")""#]];
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
       (goto-char (point-min)) (org-toggle-checkbox) (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn eta_cycle_list_bullet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"  + item\" \"1. item\" \"- item\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-plain-list-ordered-item-terminator t))
    (list
     ;; Cycle from dash.
     (with-temp-buffer (org-mode) (insert "  - item")
       (goto-char (point-min)) (org-cycle-list-bullet) (buffer-string))
     ;; Argument: specific bullet.
     (with-temp-buffer (org-mode) (insert "- item")
       (goto-char (point-min)) (org-cycle-list-bullet "1.") (buffer-string))
     ;; Argument: previous.
     (with-temp-buffer (org-mode) (insert "+ item")
       (goto-char (point-min)) (org-cycle-list-bullet 'previous) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex timer operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_timer_secs_to_hms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"0:00:30\" \"0:02:10\" \"1:01:30\" \"-1:01:30\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-timer)
  (list
   (org-timer-secs-to-hms 30)
   (org-timer-secs-to-hms 130)
   (org-timer-secs-to-hms 3690)
   (org-timer-secs-to-hms -3690)))"##,
        expect,
    );
}

#[test]
fn eta_timer_hms_to_secs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (30 130 3690)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-timer)
  (list
   (org-timer-hms-to-secs "0:00:30")
   (org-timer-hms-to-secs "0:02:10")
   (org-timer-hms-to-secs "1:01:30")))"##,
        expect,
    );
}

#[test]
fn eta_timer_fix_incomplete() {
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
// Eta: org-element with complex duration operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_duration_to_minutes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (61.0 80.5 130.0 1502.0 150.0 2.0 0.0)""#]];
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
   (org-duration-to-minutes "")))"##,
        expect,
    );
}

#[test]
fn eta_duration_from_minutes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"1:00\" \"1:01:30\" \"1:01\" \"1h\" \"1h 0min\" \"0h 50min\")""#
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
   (let ((org-duration-format '(("h" . t) ("min" . t)))) (org-duration-from-minutes 50))))"##,
        expect,
    );
}

#[test]
fn eta_duration_p() {
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
// Eta: org-element with complex column view
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_columns_compile_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"ITEM\" \"ITEM\" nil nil nil)) ((\"ITEM\" \"ITEM\" nil nil nil) (\"TODO\" \"TODO\" nil nil nil)) ((\"ITEM\" \"ITEM\" 10 nil nil)) ((\"ITEM\" \"some title\" nil nil nil)) ((\"ITEM\" \"ITEM\" nil \"+\" nil)) ((\"ITEM\" \"ITEM\" nil \"+\" \"%.1f\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-colview)
  (list
   (org-columns-compile-format "%ITEM")
   (org-columns-compile-format "%ITEM %TODO")
   (org-columns-compile-format "%10ITEM")
   (org-columns-compile-format "%ITEM(some title)")
   (org-columns-compile-format "%ITEM{+}")
   (org-columns-compile-format "%ITEM{+;%.1f}")))"##,
        expect,
    );
}

#[test]
fn eta_columns_uncompile_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"%ITEM\" \"%ITEM %TODO\" \"%10ITEM\" \"%ITEM(some title)\" \"%ITEM{+}\" \"%ITEM{+;%.1f}\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-colview)
  (list
   (org-columns-uncompile-format '(("ITEM" "ITEM" nil nil nil)))
   (org-columns-uncompile-format '(("ITEM" "ITEM" nil nil nil) ("TODO" "TODO" nil nil nil)))
   (org-columns-uncompile-format '(("ITEM" "ITEM" 10 nil nil)))
   (org-columns-uncompile-format '(("ITEM" "some title" nil nil nil)))
   (org-columns-uncompile-format '(("ITEM" "ITEM" nil "+" nil)))
   (org-columns-uncompile-format '(("ITEM" "ITEM" nil "+" "%.1f")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex macro operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_macro_replace_all() {
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
// Eta: org-element with complex footnote operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_footnote_new() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Text[fn:1]\\n\\n[fn:1] \\n\" \"Text[fn::]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-footnote-auto-label t)
        (org-footnote-section nil))
    (list
     ;; Create footnote.
     (with-temp-buffer (org-mode) (insert "Text")
       (goto-char (point-max)) (org-footnote-new) (buffer-string))
     ;; Anonymous.
     (with-temp-buffer (org-mode) (insert "Text")
       (goto-char (point-max))
       (let ((org-footnote-auto-label 'anonymous))
         (org-footnote-new)) (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn eta_footnote_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Don’t know which footnote to remove\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-footnote-section nil))
    (list
     ;; Delete regular.
     (with-temp-buffer (org-mode)
       (insert "Text[fn:1]\n\n[fn:1] Def")
       (goto-char (point-min)) (search-forward "[fn:1]")
       (org-footnote-delete) (org-trim (buffer-string)))
     ;; Delete anonymous.
     (with-temp-buffer (org-mode)
       (insert "Para[fn::def]")
       (goto-char (point-min)) (search-forward "[fn::")
       (org-footnote-delete) (org-trim (buffer-string))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex archive operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_archive_subtree() {
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
// Eta: org-element with complex datetree operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_datetree_find_date_create() {
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

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex protocol operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_protocol_parse_parameters() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"abc\" \"def\") (\"abc\" \"def\") (\"abc\" \"def\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-protocol)
  (list
   ;; Plist.
   (let ((data (org-protocol-parse-parameters '(:url "abc" :title "def") nil)))
     (list (plist-get data :url) (plist-get data :title)))
   ;; New-style.
   (let ((data (org-protocol-parse-parameters "url=abc&title=def" t)))
     (list (plist-get data :url) (plist-get data :title)))
   ;; Old-style.
   (let ((data (org-protocol-parse-parameters "abc/def" nil '(:url :title))))
     (list (plist-get data :url) (plist-get data :title)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex pcomplete operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_pcomplete_entity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-pcomplete)
  (let ((org-mode-hook nil))
    (list
     ;; Complete alpha.
     (with-temp-buffer (org-mode) (insert "\\alp")
       (goto-char (point-max)) (pcomplete) (buffer-string))
     ;; Complete frac12.
     (with-temp-buffer (org-mode) (insert "\\frac1")
       (goto-char (point-max)) (pcomplete) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex fold operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_fold_hide_drawer_toggle() {
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
       (goto-char (point-min)) (org-fold-show-all)
       (org-fold-hide-drawer-toggle)
       (get-char-property (line-end-position) 'invisible))
     ;; Show drawer.
     (with-temp-buffer (org-mode) (insert ":drawer:\ncontents\n:end:")
       (goto-char (point-min))
       (org-fold-hide-drawer-toggle)
       (org-fold-hide-drawer-toggle 'off)
       (get-char-property (line-end-position) 'invisible)))))"##,
        expect,
    );
}

#[test]
fn eta_fold_hide_block_toggle() {
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
       (get-char-property (line-end-position) 'invisible)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex num operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_num_max_level() {
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

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex capture operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_capture_fill_template() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"success!\\n\" \"2026\\n\" \"<2026-06-15 Mon>\\n\" \"[2026-06-15 Mon]\\n\" \"\" \"%i\\n\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org-capture)
  (let ((org-store-link-plist nil))
    (list
     ;; %(sexp).
     (org-capture-fill-template "%(concat \"success\" \"!\")")
     ;; %<...>.
     (org-capture-fill-template "%<%Y>")
     ;; %t.
     (org-capture-fill-template "%t")
     ;; %u.
     (org-capture-fill-template "%u")
     ;; %i.
     (org-capture-fill-template "%i" "success!")
     ;; %-escaping.
     (org-capture-fill-template "\\%i" "success!"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex clock operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_clock_table_data() {
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
      (car (org-clock-get-table-data (current-buffer) '(:maxlevel 2))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex refile operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_refile_get_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" \"B\" \"C\" \"D\" \"E\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (let ((org-mode-hook nil)
        (org-refile-targets '((nil :maxlevel . 3))))
    (with-temp-buffer (org-mode)
      (insert "* A\n** B\n*** C\n* D\n** E")
      (goto-char (point-min))
      (mapcar (lambda (r) (car r)) (org-refile-get-targets)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex sparse tree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_match_sparse_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" \"B\" \"C\" \"D\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO A\n* DONE B\n* TODO C\n* DONE D")
      (goto-char (point-min))
      (org-match-sparse-tree nil "TODO")
      (let ((visible nil))
        (org-element-map (org-element-parse-buffer) 'headline
          (lambda (h)
            (let ((title (org-element-property :raw-value h)))
              (when (org-element-property :begin h) (push title visible)))))
        (nreverse visible)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex tag operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_toggle_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H                                                                    :test:\" \"* H\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Toggle on.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-toggle-tag "test") (buffer-string))
     ;; Toggle off.
     (with-temp-buffer (org-mode) (insert "* H :test:")
       (goto-char (point-min)) (org-toggle-tag "test") (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn eta_set_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H                                                                    :tag1:\" \"* H                                                                     :new:\" \"* H                                                                     :a:b:\" \"* H\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Set tag.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-set-tags '("tag1")) (buffer-string))
     ;; Replace.
     (with-temp-buffer (org-mode) (insert "* H :old:")
       (goto-char (point-min)) (org-set-tags '("new")) (buffer-string))
     ;; Multiple.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-set-tags '("a" "b")) (buffer-string))
     ;; Remove.
     (with-temp-buffer (org-mode) (insert "* H :tag:")
       (goto-char (point-min)) (org-set-tags nil) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex todo operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_todo_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"* TODO H\" 0 8 (org-todo-head \"TODO\")) #(\"* DONE H\" 0 8 (org-todo-head \"TODO\")) #(\"* H\" 0 3 (org-todo-head \"TODO\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-todo-keywords '((sequence "TODO" "DONE"))))
    (list
     ;; Cycle to TODO.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-todo 'todo) (buffer-string))
     ;; Cycle to DONE.
     (with-temp-buffer (org-mode) (insert "* TODO H")
       (goto-char (point-min)) (org-todo 'done) (buffer-string))
     ;; Cycle DONE -> empty.
     (with-temp-buffer (org-mode) (insert "* DONE H")
       (goto-char (point-min)) (org-todo nil) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex property operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_entry_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1\" \"1\" \"1 2\" nil \"1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Regular.
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "A"))
     ;; Ignore case.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "a"))
     ;; Extended.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A+: 2\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "A"))
     ;; nil value.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: nil\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "A"))
     ;; Inheritance.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:\n** H2")
       (goto-char (point-max)) (org-entry-get (point) "A" t)))))"##,
        expect,
    );
}

#[test]
fn eta_entry_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"* TODO H\" 0 8 (org-todo-head \"TODO\")) #(\"* H\" 0 3 (org-todo-head nil)) \"* [#A] H\" \"* H\\n:PROPERTIES:\\n:A:        2\\n:END:\" \"* H\\n:PROPERTIES:\\n:A:        1\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Set TODO.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-entry-put (point) "TODO" "TODO") (buffer-string))
     ;; Remove TODO.
     (with-temp-buffer (org-mode) (insert "* TODO H")
       (goto-char (point-min)) (org-entry-put (point) "TODO" nil) (buffer-string))
     ;; Set priority.
     (with-temp-buffer (org-mode) (insert "* [#B] H")
       (goto-char (point-min)) (org-entry-put (point) "PRIORITY" "A") (buffer-string))
     ;; Set property.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-put (point) "A" "2") (buffer-string))
     ;; Set without drawer.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-entry-put (point) "A" "1") (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn eta_delete_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\" \":PROPERTIES:\\n:T1: t\\n:END:\" \"* H\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Delete from drawer.
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:TEST: t\n:END:")
       (goto-char (point-min)) (org-delete-property "TEST") (buffer-string))
     ;; Delete one of two.
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:T1: t\n:T2: t\n:END:")
       (goto-char (point-min)) (org-delete-property "T2") (buffer-string))
     ;; Delete from headline.
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:TEST: t\n:END:")
       (goto-char (point-min)) (org-delete-property "TEST") (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn eta_set_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\":PROPERTIES:\\n:TEST: t\\n:END:\\n\" \"* H\\n:PROPERTIES:\\n:TEST: t\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Empty buffer.
     (with-temp-buffer (org-mode)
       (let ((org-property-format "%s %s")) (org-set-property "TEST" "t"))
       (buffer-string))
     ;; Headline.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min))
       (let ((org-adapt-indentation nil) (org-property-format "%s %s"))
         (org-set-property "TEST" "t"))
       (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex planning operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_deadline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H\\nDEADLINE: <2012-03-29 Thu>\" \"* H\\nDEADLINE: <2014-03-04 Tue>\" \"* H\\nDEADLINE: <2012-03-29 Thu +2y>\" \"* H\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-adapt-indentation nil))
    (list
     ;; Insert.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-deadline nil "<2012-03-29>") (buffer-string))
     ;; Replace.
     (with-temp-buffer (org-mode) (insert "* H\nDEADLINE: <2012-03-29>")
       (goto-char (point-min)) (org-deadline nil "<2014-03-04>") (buffer-string))
     ;; With repeater.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-deadline nil "<2012-03-29 +2y>") (buffer-string))
     ;; Remove.
     (with-temp-buffer (org-mode) (insert "* H\nDEADLINE: <2012-03-29>")
       (goto-char (point-min)) (org-deadline '(4)) (buffer-string)))))"##,
        expect,
    );
}

#[test]
fn eta_schedule() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H\\nSCHEDULED: <2012-03-29 Thu>\" \"* H\\nSCHEDULED: <2014-03-04 Tue>\" \"* H\\nSCHEDULED: <2012-03-29 Thu +2y>\" \"* H\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-adapt-indentation nil))
    (list
     ;; Insert.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-schedule nil "<2012-03-29>") (buffer-string))
     ;; Replace.
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2012-03-29>")
       (goto-char (point-min)) (org-schedule nil "<2014-03-04>") (buffer-string))
     ;; With repeater.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-schedule nil "<2012-03-29 +2y>") (buffer-string))
     ;; Remove.
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2012-03-29>")
       (goto-char (point-min)) (org-schedule '(4)) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex repeat/timestamp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_get_repeat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"+1w\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; With repeater.
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2023-10-13 Fri +1w>")
       (goto-char (point-min)) (forward-line 1) (org-get-repeat))
     ;; No repeater.
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2023-10-13 Fri>")
       (goto-char (point-min)) (forward-line 1) (org-get-repeat)))))"##,
        expect,
    );
}

#[test]
fn eta_timestamp_has_time_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; With time.
     (with-temp-buffer (org-mode) (insert "<2023-10-13 Fri 14:30>")
       (goto-char (point-min)) (org-at-timestamp-p 'lax) (org-timestamp-has-time-p))
     ;; Without time.
     (with-temp-buffer (org-mode) (insert "<2023-10-13 Fri>")
       (goto-char (point-min)) (org-at-timestamp-p 'lax) (org-timestamp-has-time-p)))))"##,
        expect,
    );
}

#[test]
fn eta_at_timestamp_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bracket bracket nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Active.
     (with-temp-buffer (org-mode) (insert "<2023-10-13 Fri>")
       (goto-char (point-min)) (org-at-timestamp-p 'lax))
     ;; Inactive.
     (with-temp-buffer (org-mode) (insert "[2023-10-13 Fri]")
       (goto-char (point-min)) (org-at-timestamp-p 'lax))
     ;; Not at timestamp.
     (with-temp-buffer (org-mode) (insert "Not a timestamp")
       (goto-char (point-min)) (org-at-timestamp-p 'lax)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Eta: org-element with complex category
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn eta_get_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Work\" \"???\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; From keyword.
     (with-temp-buffer (org-mode) (insert "#+CATEGORY: Work\n* H")
       (goto-char (point-min)) (org-get-category))
     ;; Default.
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-get-category)))))"##,
        expect,
    );
}
