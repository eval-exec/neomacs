//! Strong org-mode oracle tests — complex multi-step editing operations.
//!
//! These tests perform sequences of editing operations and compare
//! the final buffer content, point position, or computed values.
//! Multi-step tests are the strongest way to catch implementation
//! divergences because any difference in any intermediate step
//! propagates to the final result.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: insert heading then promote then add body
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_insert_promote_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H1\\n** H2\\n* New heading\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2")
      (goto-char (point-max))
      (org-insert-heading)
      (insert "New heading")
      (org-promote)
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: insert then edit headline then add tags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_edit_headline_add_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"* New                                                             :tag1:tag2:\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Old")
      (goto-char (point-min))
      (org-edit-headline "New")
      (org-set-tags '("tag1" "tag2"))
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: set property then get it
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_set_then_get_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"myval\" \"* H\\n:PROPERTIES:\\n:MYPROP:   myval\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H")
      (goto-char (point-min))
      (org-entry-put (point) "MYPROP" "myval")
      (list (org-entry-get (point) "MYPROP")
            (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: set deadline then schedule then get planning
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_deadline_then_schedule() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"* H\\nSCHEDULED: <2024-01-14 Sun> DEADLINE: <2024-01-15 Mon>\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-adapt-indentation nil))
    (with-temp-buffer (org-mode)
      (insert "* H")
      (goto-char (point-min))
      (org-deadline nil "<2024-01-15 Mon>")
      (org-schedule nil "<2024-01-14 Sun>")
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: toggle checkbox then check buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_toggle_checkbox_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- item1\\n- item2\\n- item3\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "- item1\n- item2\n- item3")
      (goto-char (point-min))
      (org-toggle-checkbox)
      (forward-line 1)
      (org-toggle-checkbox)
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: navigate and read properties at each position
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_navigate_and_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"[#A] H1\" \"TODO\" (\"tag1\")) (\"[#B] H2\" \"DONE\" (\"tag2\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* TODO [#A] H1 :tag1:\nBody1\n* DONE [#B] H2 :tag2:\nBody2")
      (goto-char (point-min))
      (let ((r1 (list (org-get-heading t t nil t)
                      (org-entry-get (point) "TODO")
                      (org-get-tags-at))))
        (org-next-visible-heading 1)
        (let ((r2 (list (org-get-heading t t nil t)
                        (org-entry-get (point) "TODO")
                        (org-get-tags-at))))
          (list r1 r2))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: clock in then clock out then get duration
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_clock_in_out_duration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (closed \"0:00\" \"* Task\\n:LOGBOOK:\\nCLOCK: [2026-06-15 Mon 12:00]--[2026-06-15 Mon 12:00] =>  0:00\\n:END:\\nBody\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn
  (require 'org)
  (require 'org-clock)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Task\nBody")
      (goto-char (point-min))
      (org-clock-in)
      (org-clock-out)
      (let* ((tree (org-element-parse-buffer))
             (clock (car (org-element-map tree 'clock #'identity))))
        (list (org-element-property :status clock)
              (org-element-property :duration clock)
              (buffer-string))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: archive then check remaining
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_archive_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"No file associated to buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-archive)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* Keep\n* Archive Me\nBody\n* Also Keep")
      (goto-char (point-min))
      (forward-line 1)
      (org-archive-subtree)
      (list (buffer-string)
            (org-element-map (org-element-parse-buffer) 'headline
              (lambda (h) (substring-no-properties (org-element-property :raw-value h))))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: sparse tree then check visible
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_sparse_tree_then_check() {
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
          (lambda (h) (let ((title (org-element-property :raw-value h)))
                   (when (org-element-property :begin h) (push title visible)))))
        (nreverse visible)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: fill then check buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_fill_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | b |\\n| c | d |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table-row) 10 11 (face org-table) 11 12 (face org-table rear-nonsticky t display (space :relative-width 1)) 12 13 (face org-table) 13 14 (face org-table display (space :relative-width 1.001)) 14 15 (face org-table) 15 16 (face org-table rear-nonsticky t display (space :relative-width 1)) 16 17 (face org-table) 17 18 (face org-table display (space :relative-width 1.001)) 18 19 (face org-table) 19 20 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "|a|b|\n|c|d|")
      (goto-char (point-min))
      (org-fill-element)
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: table formula then check result
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_table_formula_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a #+TBLFM line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| 10 | 20 |   |\n| 30 | 40 |   |\n|    |    |   |\n#+TBLFM: @1$3=$1+$2::@2$3=$1+$2::@3$1=vsum(@1$1..@2$1)::@3$2=vsum(@1$2..@2$2)::@3$3=vsum(@1$3..@2$3)")
      (goto-char (point-min))
      (org-table-calc-current-TBLFM)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: table transpose then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_table_transpose_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | 1 |\\n| b | 2 |\\n| c | 3 |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table-row) 10 11 (face org-table) 11 12 (face org-table rear-nonsticky t display (space :relative-width 1)) 12 13 (face org-table) 13 14 (face org-table display (space :relative-width 1.001)) 14 15 (face org-table) 15 16 (face org-table rear-nonsticky t display (space :relative-width 1)) 16 17 (face org-table) 17 18 (face org-table display (space :relative-width 1.001)) 18 19 (face org-table) 19 20 (face org-table-row) 20 21 (face org-table) 21 22 (face org-table rear-nonsticky t display (space :relative-width 1)) 22 23 (face org-table) 23 24 (face org-table display (space :relative-width 1.001)) 24 25 (face org-table) 25 26 (face org-table rear-nonsticky t display (space :relative-width 1)) 26 27 (face org-table) 27 28 (face org-table display (space :relative-width 1.001)) 28 29 (face org-table) 29 30 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| a | b | c |\n| 1 | 2 | 3 |")
      (goto-char (point-min))
      (org-table-transpose-table-at-point)
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: sort table then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_table_sort_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Format specifier doesn’t match argument type\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "| c |\n| a |\n| b |")
      (goto-char (point-min))
      (org-table-sort-lines ?a 'string)
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: macro expansion then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_macro_expand_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-macro)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+MACRO: greet Hello\n#+MACRO: name World\n{{{greet}}} {{{name}}}!")
      (goto-char (point-min))
      (org-macro-initialize-templates)
      (org-macro-replace-all org-macro-templates)
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: cycle todo then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_todo_cycle_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 13 60)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-todo-keywords '((sequence "TODO" "DONE"))))
    (with-temp-buffer (org-mode)
      (insert "* H")
      (goto-char (point-min))
      (org-todo 'todo)
      (let ((after-todo (buffer-string)))
        (org-todo 'done)
        (let ((after-done (buffer-string)))
          (org-todo nil)
          (list after-todo after-done (buffer-string))))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: sort entries then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_sort_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\n* abc\\n* def\\n* xyz\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\n* def\n* xyz\n* abc")
      (goto-char (point-min))
      (org-sort-entries nil ?a)
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: move subtree then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_move_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-move-subtree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* A\nBody\n* B\nBody\n* C\nBody")
      (goto-char (point-min))
      (org-move-subtree 1)
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: promote/demote subtree then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_promote_demote_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"** H1\\n*** S1\\n*** S2\" \"* H1\\n** S1\\n** S2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** S1\n** S2")
      (goto-char (point-min))
      (org-demote-subtree)
      (let ((after-demote (buffer-string)))
        (org-promote-subtree)
        (list after-demote (buffer-string))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: cycle list bullet then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_cycle_bullet_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 52)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-plain-list-ordered-item-terminator t))
    (with-temp-buffer (org-mode)
      (insert "- item")
      (goto-char (point-min))
      (org-cycle-list-bullet)
      (let ((after1 (buffer-string)))
        (org-cycle-list-bullet)
        (let ((after2 (buffer-string)))
          (org-cycle-list-bullet)
          (list after1 after2 (buffer-string))))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: fold/unfold then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_fold_unfold_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-fold)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert ":drawer:\ncontents\n:end:")
      (goto-char (point-min))
      (org-fold-show-all)
      (org-fold-hide-drawer-toggle)
      (let ((hidden (get-char-property (line-end-position) 'invisible)))
        (org-fold-hide-drawer-toggle 'off)
        (list hidden (get-char-property (line-end-position) 'invisible))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: fold/unfold block then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_fold_unfold_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-fold)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_CENTER\ncontents\n#+END_CENTER")
      (goto-char (point-min))
      (org-fold-hide-block-toggle)
      (let ((hidden (get-char-property (line-end-position) 'invisible)))
        (org-fold-hide-block-toggle 'off)
        (list hidden (get-char-property (line-end-position) 'invisible))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: indent then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_indent_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2 \"* H\\n  A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H\nA")
      (goto-char (point-max))
      (let ((org-adapt-indentation t))
        (org-indent-line)
        (let ((indent (org-get-indentation)))
          (org-indent-line)
          (list indent (org-get-indentation) (buffer-string)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: return key then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_return_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Para\\n graph\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Para graph")
      (goto-char (+ 4 (point-min)))
      (org-return)
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: kill-line then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_kill_line_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\n123\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "abc\n123")
      (goto-char (point-min))
      (org-kill-line)
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: footnote new then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_footnote_new_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Text[fn:1]\\n\\n[fn:1] \\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-footnote-auto-label t) (org-footnote-section nil))
    (with-temp-buffer (org-mode)
      (insert "Text")
      (goto-char (point-max))
      (org-footnote-new)
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: footnote delete then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_footnote_delete_then_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Don’t know which footnote to remove\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-footnote-section nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:1]\n\n[fn:1] Def")
      (goto-char (point-min))
      (search-forward "[fn:1]")
      (org-footnote-delete)
      (org-trim (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: timer operations then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_timer_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3690 130 30)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-timer)
  (list
   (org-timer-hms-to-secs (org-timer-secs-to-hms 3690))
   (org-timer-hms-to-secs (org-timer-secs-to-hms 130))
   (org-timer-hms-to-secs (org-timer-secs-to-hms 30))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: duration roundtrip
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_duration_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (61.0 80.5 130.0 1502.0 150.0 0.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-duration)
  (list
   (org-duration-to-minutes "1:01")
   (org-duration-to-minutes "1:20:30")
   (org-duration-to-minutes "2h 10min")
   (org-duration-to-minutes "1d 1:02")
   (org-duration-to-minutes "2.5h")
   (org-duration-to-minutes "")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: colview format roundtrip
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_colview_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"%ITEM\" \"%ITEM %TODO\" \"%10ITEM\" \"%ITEM{+}\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-colview)
  (list
   (org-columns-uncompile-format (org-columns-compile-format "%ITEM"))
   (org-columns-uncompile-format (org-columns-compile-format "%ITEM %TODO"))
   (org-columns-uncompile-format (org-columns-compile-format "%10ITEM"))
   (org-columns-uncompile-format (org-columns-compile-format "%ITEM{+}"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: protocol parse then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_protocol_parse_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"abc\" \"def\") (\"abc\" \"def\") (\"abc\" \"def\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-protocol)
  (list
   (let ((d (org-protocol-parse-parameters '(:url "abc" :title "def") nil)))
     (list (plist-get d :url) (plist-get d :title)))
   (let ((d (org-protocol-parse-parameters "url=abc&title=def" t)))
     (list (plist-get d :url) (plist-get d :title)))
   (let ((d (org-protocol-parse-parameters "abc/def" nil '(:url :title))))
     (list (plist-get d :url) (plist-get d :title)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: capture template then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_capture_template_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"success!\\n\" \"2026\\n\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-capture)
  (let ((org-store-link-plist nil))
    (list
     (org-capture-fill-template "%(concat \"success\" \"!\")")
     (org-capture-fill-template "%<%Y>")
     (org-capture-fill-template "%i" "hello"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: clock table data then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_clock_table_roundtrip() {
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
// Multi-step: refile targets then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_refile_targets_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" \"B\" \"C\" \"D\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (let ((org-mode-hook nil)
        (org-refile-targets '((nil :maxlevel . 2))))
    (with-temp-buffer (org-mode)
      (insert "* A\n** B\n* C\n** D")
      (goto-char (point-min))
      (mapcar (lambda (r) (car r)) (org-refile-get-targets)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: pcomplete then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_pcomplete_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-pcomplete)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "\\alp")
       (goto-char (point-max)) (pcomplete) (buffer-string))
     (with-temp-buffer (org-mode) (insert "\\frac1")
       (goto-char (point-max)) (pcomplete) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: num mode overlays then check
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_num_mode_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"1 \" 0 2 (face org-level-1)) #(\"1.1 \" 0 4 (face org-level-2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-num)
  (let ((org-mode-hook nil) (org-num-max-level 2))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n*** H3")
      (goto-char (point-min))
      (org-num-mode 1)
      (sort (mapcar (lambda (o) (overlay-get o 'after-string))
                    (overlays-in (point-min) (point-max)))
            #'string-lessp))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: cut and paste subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_cut_paste_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"* B\\nBody B\\n* C\\nBody C\\n* A\\nBody A\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* A\nBody A\n* B\nBody B\n* C\nBody C")
      (goto-char (point-min))
      (org-cut-subtree)
      (goto-char (point-max))
      (org-paste-subtree 1)
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: clone subtree with time shift
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_clone_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"* H1\\n<2015-06-21>\\n* H1\\n<2015-06-23 Tue>\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n<2015-06-21>")
      (goto-char (point-min))
      (org-clone-subtree-with-time-shift 1 "+2d")
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: insert-todo-heading-respect-content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_insert_todo_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* H1\\n Body\\n* TODO \\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n Body")
      (org-insert-todo-heading-respect-content)
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: timer change times
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_timer_change_times() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\\n1:31:15\\n4:00:55\" \"\\n-1:30:25\\n0:59:15\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-timer)
  (list
   (with-temp-buffer (org-mode)
     (insert "\n0:00:25\n2:30:05")
     (org-timer-change-times-in-region (point-min) (point-max) "1:30:50")
     (buffer-string))
   (with-temp-buffer (org-mode)
     (insert "\n0:00:25\n2:30:05")
     (org-timer-change-times-in-region (point-min) (point-max) "-1:30:50")
     (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: set then delete property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_set_delete_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H\\n:PROPERTIES:\\n:MYPROP:   myval\\n:END:\\n\" \"* H\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H")
      (goto-char (point-min))
      (org-entry-put (point) "MYPROP" "myval")
      (let ((result1 (buffer-string)))
        (org-delete-property "MYPROP")
        (list result1 (buffer-string))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: deadline and schedule combined
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_deadline_schedule_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"* H\\nSCHEDULED: <2024-01-14 Sun> DEADLINE: <2024-01-15 Mon>\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-adapt-indentation nil))
    (with-temp-buffer (org-mode)
      (insert "* H")
      (goto-char (point-min))
      (org-deadline nil "<2024-01-15 Mon>")
      (org-schedule nil "<2024-01-14 Sun>")
      (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: todo cycle through states
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_todo_cycle_through() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 13 44)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-todo-keywords '((sequence "TODO" "DONE"))))
    (with-temp-buffer (org-mode)
      (insert "* H")
      (goto-char (point-min))
      (org-todo 'todo)
      (let ((s1 (buffer-string)))
        (org-todo 'done)
        (let ((s2 (buffer-string)))
          (org-todo nil)
          (list s1 s2 (buffer-string))))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: sort entries various types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_sort_entries_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\n* abc\\n* def\\n* xyz\\n\" \"\\n* 1\\n* 2\\n* 10\\n\" \"\\n* [#A] h2\\n* [#B] h3\\n* [#C] h1\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "\n* def\n* xyz\n* abc")
       (goto-char (point-min)) (org-sort-entries nil ?a) (buffer-string))
     (with-temp-buffer (org-mode) (insert "\n* 10\n* 1\n* 2")
       (goto-char (point-min)) (org-sort-entries nil ?n) (buffer-string))
     (with-temp-buffer (org-mode) (insert "\n* [#C] h1\n* [#A] h2\n* [#B] h3")
       (goto-char (point-min)) (org-sort-entries nil ?p) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: move subtree up and down
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_move_subtree_up_down() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-move-subtree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* A\nBody\n* B\nBody\n* C\nBody")
       (goto-char (point-min)) (org-move-subtree 1) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* A\nBody\n* B\nBody\n* C\nBody")
       (goto-char (point-min)) (forward-line 2) (org-move-subtree -1)
       (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: promote/demote subtree roundtrip
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_promote_demote_subtree_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"** H1\\n*** S1\\n*** S2\" \"* H1\\n** S1\\n** S2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** S1\n** S2")
      (goto-char (point-min))
      (org-demote-subtree)
      (let ((after-demote (buffer-string)))
        (org-promote-subtree)
        (list after-demote (buffer-string))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: cycle list bullet various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_cycle_list_bullet_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 44)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-plain-list-ordered-item-terminator t))
    (with-temp-buffer (org-mode)
      (insert "- item")
      (goto-char (point-min))
      (org-cycle-list-bullet)
      (let ((s1 (buffer-string)))
        (org-cycle-list-bullet)
        (let ((s2 (buffer-string)))
          (org-cycle-list-bullet)
          (list s1 s2 (buffer-string))))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: macro replace all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_macro_replace_all_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-macro)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode)
       (insert "#+MACRO: A B\n1 {{{A}}} 3")
       (goto-char (point-min)) (org-macro-initialize-templates)
       (org-macro-replace-all org-macro-templates) (buffer-string))
     (with-temp-buffer (org-mode)
       (insert "#+MACRO: m $1 $2\n{{{m(a,b)}}}")
       (goto-char (point-min)) (org-macro-initialize-templates)
       (org-macro-replace-all org-macro-templates) (buffer-string))
     (with-temp-buffer (org-mode)
       (insert "#+MACRO: in inner\n#+MACRO: out {{{in}}} outer\n{{{out}}}")
       (goto-char (point-min)) (org-macro-initialize-templates)
       (org-macro-replace-all org-macro-templates) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: footnote new and delete cycle
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_footnote_new_delete_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Text[fn:1]\\n\\n[fn:1] \\n\" \"Text\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-footnote-auto-label t) (org-footnote-section nil))
    (with-temp-buffer (org-mode)
      (insert "Text")
      (goto-char (point-max))
      (org-footnote-new)
      (let ((after-new (buffer-string)))
        (goto-char (point-min))
        (search-forward "[fn:")
        (backward-char 4)
        (org-footnote-delete)
        (list after-new (org-trim (buffer-string)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: fill element various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_fill_element_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"| a |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table-row)) \"A B\" \"- A B\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "|a|")
       (goto-char (point-min)) (org-fill-element) (buffer-string))
     (with-temp-buffer (org-mode) (insert "A\nB")
       (goto-char (point-max)) (let ((fill-column 20)) (org-fill-element)) (buffer-string))
     (with-temp-buffer (org-mode) (insert "- A\n  B")
       (goto-char (point-min)) (let ((fill-column 20)) (org-fill-element)) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: fold drawer toggle cycle
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_fold_drawer_toggle_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-fold)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode) (insert ":drawer:\ncontents\n:end:")
      (goto-char (point-min))
      (org-fold-show-all)
      (org-fold-hide-drawer-toggle)
      (let ((h (get-char-property (line-end-position) 'invisible)))
        (org-fold-hide-drawer-toggle 'off)
        (list h (get-char-property (line-end-position) 'invisible))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: fold block toggle cycle
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_fold_block_toggle_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-fold)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+BEGIN_CENTER\ncontents\n#+END_CENTER")
      (goto-char (point-min))
      (org-fold-hide-block-toggle)
      (let ((h (get-char-property (line-end-position) 'invisible)))
        (org-fold-hide-block-toggle 'off)
        (list h (get-char-property (line-end-position) 'invisible))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: indent line various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_indent_line_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 2 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-indent-line) (org-get-indentation))
     (with-temp-buffer (org-mode) (insert "* H\nA")
       (goto-char (point-max)) (let ((org-adapt-indentation t)) (org-indent-line)) (org-get-indentation))
     (with-temp-buffer (org-mode) (insert "* H\nA")
       (goto-char (point-max)) (let ((org-adapt-indentation nil)) (org-indent-line)) (org-get-indentation)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: return various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_return_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Para\\n graph\" \"  Para\\n  graph\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "Para graph")
       (goto-char (+ 4 (point-min))) (org-return) (buffer-string))
     (with-temp-buffer (org-mode) (insert "  Para graph")
       (goto-char (+ 6 (point-min))) (org-return t) (buffer-string))
     (with-temp-buffer (org-mode) (insert "| a |\n| b |")
       (goto-char (point-min)) (forward-char 2) (org-return) (looking-at "b")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: meta-return various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_meta_return_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* a\" \"- \\n- a\" #(\"|   |\\n| a |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table-row) 6 7 (face org-table) 7 8 (face org-table rear-nonsticky t display (space :relative-width 1)) 8 9 (face org-table) 9 10 (face org-table display (space :relative-width 1.001)) 10 11 (face org-table) 11 12 (face org-table-row)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "a")
       (goto-char (point-min)) (org-meta-return) (buffer-string))
     (with-temp-buffer (org-mode) (insert "- a")
       (goto-char (point-min)) (org-meta-return) (buffer-string))
     (with-temp-buffer (org-mode) (insert "| a |")
       (goto-char (point-min)) (forward-char 2) (org-meta-return) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: kill-line various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_kill_line_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" \"ab\" \"\\n123\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "abc")
       (goto-char (point-min)) (org-kill-line) (buffer-string))
     (with-temp-buffer (org-mode) (insert "abc")
       (goto-char (+ 2 (point-min))) (org-kill-line) (buffer-string))
     (with-temp-buffer (org-mode) (insert "abc\n123")
       (goto-char (point-min)) (org-kill-line) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: edit-headline various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_edit_headline_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"* B\" \"* TODO B\" \"* [#A] B\" \"* B :tag:\" \"* A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* A")
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* TODO A")
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* [#A] A")
       (goto-char (point-min)) (org-edit-headline "B") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* A :tag:")
       (goto-char (point-min)) (let ((org-tags-column 4)) (org-edit-headline "B")) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* ")
       (goto-char (point-min)) (org-edit-headline "A") (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: insert-heading various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_insert_heading_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* \" \"* \\n* H\" \"** H\\nP\\n** \" \"\\n* \\n\\n* H1\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (org-insert-heading) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-insert-heading) (buffer-string))
     (with-temp-buffer (org-mode) (insert "** H\nP")
       (goto-char (point-max)) (org-insert-heading) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H1")
       (goto-char (point-min))
       (let ((org-blank-before-new-entry '((heading . t)))) (org-insert-heading))
       (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: toggle-tag various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_toggle_tag_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H                                                                    :test:\" \"* H\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-toggle-tag "test") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H :test:")
       (goto-char (point-min)) (org-toggle-tag "test") (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: set-tags various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_set_tags_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H                                                                    :tag1:\" \"* H                                                                     :new:\" \"* H                                                                     :a:b:\" \"* H\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-set-tags '("tag1")) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H :old:")
       (goto-char (point-min)) (org-set-tags '("new")) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-set-tags '("a" "b")) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H :tag:")
       (goto-char (point-min)) (org-set-tags nil) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: entry-get various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_entry_get_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1\" \"1\" \"1 2\" \"1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "A"))
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "a"))
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A+: 2\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-get (point) "A"))
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:\n** H2")
       (goto-char (point-max)) (org-entry-get (point) "A" t)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: entry-put various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_entry_put_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H\\n:PROPERTIES:\\n:A:        1\\n:END:\\n\" \"* H\\n:PROPERTIES:\\n:A:        2\\n:END:\" #(\"* TODO H\" 0 8 (org-todo-head \"TODO\")) #(\"* H\" 0 3 (org-todo-head nil)) \"* [#A] H\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-entry-put (point) "A" "1") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:A: 1\n:END:")
       (goto-char (point-min)) (org-entry-put (point) "A" "2") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-entry-put (point) "TODO" "TODO") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* TODO H")
       (goto-char (point-min)) (org-entry-put (point) "TODO" nil) (buffer-string))
     (with-temp-buffer (org-mode) (insert "* [#B] H")
       (goto-char (point-min)) (org-entry-put (point) "PRIORITY" "A") (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: delete-property various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_delete_property_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\" \":PROPERTIES:\\n:T1: t\\n:END:\" \"* H\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:TEST: t\n:END:")
       (goto-char (point-min)) (org-delete-property "TEST") (buffer-string))
     (with-temp-buffer (org-mode) (insert ":PROPERTIES:\n:T1: t\n:T2: t\n:END:")
       (goto-char (point-min)) (org-delete-property "T2") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H\n:PROPERTIES:\n:TEST: t\n:END:")
       (goto-char (point-min)) (org-delete-property "TEST") (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: set-property various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_set_property_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\":PROPERTIES:\\n:TEST: t\\n:END:\\n\" \"* H\\n:PROPERTIES:\\n:TEST: t\\n:END:\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode)
       (let ((org-property-format "%s %s")) (org-set-property "TEST" "t"))
       (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min))
       (let ((org-adapt-indentation nil) (org-property-format "%s %s"))
         (org-set-property "TEST" "t"))
       (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: deadline various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_deadline_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H\\nDEADLINE: <2012-03-29 Thu>\" \"* H\\nDEADLINE: <2014-03-04 Tue>\" \"* H\\nDEADLINE: <2012-03-29 Thu +2y>\" \"* H\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-adapt-indentation nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-deadline nil "<2012-03-29>") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H\nDEADLINE: <2012-03-29>")
       (goto-char (point-min)) (org-deadline nil "<2014-03-04>") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-deadline nil "<2012-03-29 +2y>") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H\nDEADLINE: <2012-03-29>")
       (goto-char (point-min)) (org-deadline '(4)) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: schedule various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_schedule_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"* H\\nSCHEDULED: <2012-03-29 Thu>\" \"* H\\nSCHEDULED: <2014-03-04 Tue>\" \"* H\\nSCHEDULED: <2012-03-29 Thu +2y>\" \"* H\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-adapt-indentation nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-schedule nil "<2012-03-29>") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2012-03-29>")
       (goto-char (point-min)) (org-schedule nil "<2014-03-04>") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-schedule nil "<2012-03-29 +2y>") (buffer-string))
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2012-03-29>")
       (goto-char (point-min)) (org-schedule '(4)) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: get-repeat various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_get_repeat_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"+1w\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2023-10-13 Fri +1w>")
       (goto-char (point-min)) (forward-line 1) (org-get-repeat))
     (with-temp-buffer (org-mode) (insert "* H\nSCHEDULED: <2023-10-13 Fri>")
       (goto-char (point-min)) (forward-line 1) (org-get-repeat)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: timestamp-has-time-p various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_timestamp_has_time_p_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "<2023-10-13 Fri 14:30>")
       (goto-char (point-min)) (org-at-timestamp-p 'lax) (org-timestamp-has-time-p))
     (with-temp-buffer (org-mode) (insert "<2023-10-13 Fri>")
       (goto-char (point-min)) (org-at-timestamp-p 'lax) (org-timestamp-has-time-p)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: at-timestamp-p various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_at_timestamp_p_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bracket bracket nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "<2023-10-13 Fri>")
       (goto-char (point-min)) (org-at-timestamp-p 'lax))
     (with-temp-buffer (org-mode) (insert "[2023-10-13 Fri]")
       (goto-char (point-min)) (org-at-timestamp-p 'lax))
     (with-temp-buffer (org-mode) (insert "Not a timestamp")
       (goto-char (point-min)) (org-at-timestamp-p 'lax)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: get-category various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_get_category_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Work\" \"???\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "#+CATEGORY: Work\n* H")
       (goto-char (point-min)) (org-get-category))
     (with-temp-buffer (org-mode) (insert "* H")
       (goto-char (point-min)) (org-get-category)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: clock-get-table-data
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_clock_get_table_data() {
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
// Multi-step: refile-get-targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_refile_get_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" \"B\" \"C\" \"D\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-refile)
  (let ((org-mode-hook nil)
        (org-refile-targets '((nil :maxlevel . 2))))
    (with-temp-buffer (org-mode)
      (insert "* A\n** B\n* C\n** D")
      (goto-char (point-min))
      (mapcar (lambda (r) (car r)) (org-refile-get-targets)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: match-sparse-tree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_match_sparse_tree() {
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
          (lambda (h) (let ((title (org-element-property :raw-value h)))
                   (when (org-element-property :begin h) (push title visible)))))
        (nreverse visible)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: map-entries various matchers
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_map_entries_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 11) (1) (6) (11) (1) (1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* Level 1\n** Level 2")
       (goto-char (point-min)) (org-map-entries #'point))
     (with-temp-buffer (org-mode) (insert "* Level 1\n** Level 2")
       (goto-char (point-min)) (let (org-odd-levels-only) (org-map-entries #'point "LEVEL=1")))
     (with-temp-buffer (org-mode) (insert "* H1\n* TODO H2\n* DONE H3")
       (goto-char (point-min)) (org-map-entries #'point "TODO=\"TODO\""))
     (with-temp-buffer (org-mode) (insert "* H1 :no:\n* H2 :yes:")
       (goto-char (point-min)) (org-map-entries #'point "yes"))
     (with-temp-buffer (org-mode) (insert "* [#A] H1\n* [#B] H2")
       (goto-char (point-min)) (org-map-entries #'point "PRIORITY=\"A\""))
     (with-temp-buffer (org-mode)
       (insert "* H1\n:PROPERTIES:\n:TEST: 1\n:END:\n* H2\n:PROPERTIES:\n:TEST: 2\n:END:")
       (goto-char (point-min)) (org-map-entries #'point "TEST=1")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: entry-blocked-p various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_entry_blocked_p_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (org-enforce-todo-dependencies t)
        (org-blocker-hook '(org-block-todo-from-children-or-siblings-or-parent)))
    (list
     (with-temp-buffer (org-mode) (insert "* TODO Blocked\n** DONE one\n** TODO two")
       (goto-char (point-min)) (org-entry-blocked-p))
     (with-temp-buffer (org-mode) (insert "* TODO Blocked\n** DONE one\n** DONE two")
       (goto-char (point-min)) (org-entry-blocked-p))
     (with-temp-buffer (org-mode) (insert "* Blocked\n** TODO one")
       (goto-char (point-min)) (org-entry-blocked-p))
     (with-temp-buffer (org-mode) (insert "* DONE Blocked\n** TODO one")
       (goto-char (point-min)) (org-entry-blocked-p)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: find-olp various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_find_olp_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer> #<marker in no buffer>)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "\n* Headline\n** COMMENT headline2\n** TODO headline3\n*** [#A] headline4 :tags:\n** [#A]headline5\n** [0%] headline6\n** headline7 [100%]\n** headline8 [1/5] :some:more:tags:\n* Test")
      (goto-char (point-min))
      (list
       (org-find-olp '("Headline") t)
       (org-find-olp '("Headline" "headline2") t)
       (org-find-olp '("Headline" "headline3") t)
       (org-find-olp '("Headline" "headline3" "headline4") t)
       (org-find-olp '("Headline" "headline6") t)
       (org-find-olp '("Headline" "headline7") t)
       (org-find-olp '("Headline" "headline8") t)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: timer roundtrip various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_timer_roundtrip_various() {
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
   (org-timer-hms-to-secs (org-timer-secs-to-hms 30))
   (org-timer-hms-to-secs (org-timer-secs-to-hms 130))
   (org-timer-hms-to-secs (org-timer-secs-to-hms 3690))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: duration conversions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_duration_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (61.0 80.5 130.0 1502.0 150.0 0.0 \"1:00\" \"1:01:30\" 0 0 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-duration)
  (list
   (org-duration-to-minutes "1:01")
   (org-duration-to-minutes "1:20:30")
   (org-duration-to-minutes "2h 10min")
   (org-duration-to-minutes "1d 1:02")
   (org-duration-to-minutes "2.5h")
   (org-duration-to-minutes "")
   (let ((org-duration-format 'h:mm)) (org-duration-from-minutes 60))
   (let ((org-duration-format 'h:mm:ss)) (org-duration-from-minutes 61.5))
   (org-duration-p "3:12")
   (org-duration-p "3d 3h 4min")
   (org-duration-p "3::12")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: colview format roundtrip
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_colview_format_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"%ITEM\" \"%ITEM %TODO\" \"%10ITEM\" \"%ITEM{+}\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-colview)
  (list
   (org-columns-uncompile-format (org-columns-compile-format "%ITEM"))
   (org-columns-uncompile-format (org-columns-compile-format "%ITEM %TODO"))
   (org-columns-uncompile-format (org-columns-compile-format "%10ITEM"))
   (org-columns-uncompile-format (org-columns-compile-format "%ITEM{+}"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: protocol parse roundtrip (second test)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_protocol_parse_roundtrip_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"abc\" \"def\") (\"abc\" \"def\") (\"abc\" \"def\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-protocol)
  (list
   (let ((d (org-protocol-parse-parameters '(:url "abc" :title "def") nil)))
     (list (plist-get d :url) (plist-get d :title)))
   (let ((d (org-protocol-parse-parameters "url=abc&title=def" t)))
     (list (plist-get d :url) (plist-get d :title)))
   (let ((d (org-protocol-parse-parameters "abc/def" nil '(:url :title))))
     (list (plist-get d :url) (plist-get d :title)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: capture template expansion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_capture_template_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"success!\\n\" \"2026\\n\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-capture)
  (let ((org-store-link-plist nil))
    (list
     (org-capture-fill-template "%(concat \"success\" \"!\")")
     (org-capture-fill-template "%<%Y>")
     (org-capture-fill-template "%i" "hello"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: pcomplete entity
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_pcomplete_entity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Defining as dynamic an already lexical var\" org-mode-hook)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-pcomplete)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "\\alp")
       (goto-char (point-max)) (pcomplete) (buffer-string))
     (with-temp-buffer (org-mode) (insert "\\frac1")
       (goto-char (point-max)) (pcomplete) (buffer-string)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: num mode overlays
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_num_mode_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"1 \" 0 2 (face org-level-1)) #(\"1.1 \" 0 4 (face org-level-2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-num)
  (let ((org-mode-hook nil) (org-num-max-level 2))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n*** H3")
      (goto-char (point-min))
      (org-num-mode 1)
      (sort (mapcar (lambda (o) (overlay-get o 'after-string))
                    (overlays-in (point-min) (point-max)))
            #'string-lessp))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: outline path various
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_outline_path_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil (\"H\") (\"H\") (\"H\") #(\"one/two/three\" 0 3 (face org-level-1) 4 7 (face org-level-2) 8 13 (face org-level-3)) \"\" \">>\" #(\">>|one|two|three\" 3 6 (face org-level-1) 7 10 (face org-level-2) 11 16 (face org-level-3)) #(\"one/two/..\" 0 3 (face org-level-1) 4 7 (face org-level-2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode) (insert "* H") (goto-char (point-min)) (org-get-outline-path))
     (with-temp-buffer (org-mode) (insert "* H\n** S") (goto-char (point-max)) (org-get-outline-path))
     (with-temp-buffer (org-mode) (insert "* H\n** S\nText") (goto-char (point-max)) (org-get-outline-path))
     (with-temp-buffer (org-mode) (insert "* H") (goto-char (point-min)) (org-get-outline-path t))
     (org-format-outline-path (list "one" "two" "three"))
     (org-format-outline-path '())
     (org-format-outline-path '() nil ">>")
     (org-format-outline-path (list "one" "two" "three") nil ">>" "|")
     (org-format-outline-path (list "one" "two" "three" "four") 10))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: export headline numbers
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_headline_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((1) 1) ((1 1) 2) ((1 1 1) 3) ((1 2) 2) ((2) 1) ((2 1) 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+OPTIONS: num:t H:3\n* Ch1\n** S1\n*** SS1\n** S2\n* Ch2\n** S3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (mapcar (lambda (h) (list (org-export-get-headline-number h info)
                            (org-export-get-relative-level h info)))
                (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: export footnote numbers
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_footnote_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "Text[fn:1] more[fn:2]\n\n[fn:1] Def 1\n[fn:2] Def 2")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (list
         (mapcar (lambda (ref) (org-export-get-footnote-number ref info))
                 (org-element-map tree 'footnote-reference #'identity))
         (mapcar (lambda (ref) (org-export-footnote-first-reference-p ref info))
                 (org-element-map tree 'footnote-reference #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: export tags and categories
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_tags_categories() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+CATEGORY: work\n* H1 :tag1:\n** H2 :tag2:\n* H3")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment)))))
        (list
         (mapcar (lambda (h) (org-export-get-tags h info))
                 (org-element-map tree 'headline #'identity))
         (mapcar (lambda (h) (org-export-get-category h info))
                 (org-element-map tree 'headline #'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: export sibling detection
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_sibling_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H1\n** H2\n** H3\n** H4\n* H5")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (hls (org-element-map tree 'headline #'identity)))
        (list (mapcar #'org-export-first-sibling-p hls)
              (mapcar #'org-export-last-sibling-p hls))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: export filter chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_filter_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"210\" \"20\" \"0\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   (org-export-filter-apply-functions
    (list (lambda (v &rest _) (concat "1" v))
          (lambda (v &rest _) (concat "2" v)))
    "0" nil)
   (org-export-filter-apply-functions
    (list #'ignore (lambda (v &rest _) (concat "2" v)))
    "0" nil)
   (org-export-filter-apply-functions (list #'ignore) "0" nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: export backend chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_backend_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((parent) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let (org-export-registered-backends)
    (org-export-define-backend 'parent
      '((headline . (lambda (h c i) (format "P: %s" (org-element-property :raw-value h))))
        (section . (lambda (s c i) c))
        (paragraph . (lambda (p c i) c))
        (plain-text . (lambda (t i) t))))
    (org-export-define-derived-backend 'child 'parent
      :translate-alist '((headline . (lambda (h c i) (format "C: %s" (org-element-property :raw-value h))))))
    (list
     (org-export-derived-backend-p 'child 'parent)
     (org-export-derived-backend-p 'child 'child))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: export read-attribute
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_read_attribute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:a \"1\" :b \"2\") nil (:a nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     (org-export-read-attribute
      :attr_html
      (with-temp-buffer (org-mode) (insert "#+ATTR_HTML: :a 1 :b 2\nP")
        (goto-char (point-min)) (org-element-at-point)))
     (org-export-read-attribute
      :attr_html
      (with-temp-buffer (org-mode) (insert "P")
        (goto-char (point-min)) (org-element-at-point)))
     (org-export-read-attribute
      :attr_html
      (with-temp-buffer (org-mode) (insert "#+ATTR_HTML: :a nil\nP")
        (goto-char (point-min)) (org-element-at-point))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: export caption
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_caption() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"My caption\" 0 10 (:parent (#(\"My caption\" 0 10 (:parent #4)))))) ((#(\"long caption\" 0 12 (:parent (#(\"long caption\" 0 12 (:parent #5)))))) (#(\"short\" 0 5 (:parent (#(\"short\" 0 5 (:parent #5))))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (list
     (with-temp-buffer (org-mode)
       (insert "#+CAPTION: My caption\n| a | b |")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (table (car (org-element-map tree 'table #'identity))))
         (org-export-get-caption table)))
     (with-temp-buffer (org-mode)
       (insert "#+CAPTION[short]: long caption\n| a | b |")
       (goto-char (point-min))
       (let* ((tree (org-element-parse-buffer))
              (table (car (org-element-map tree 'table #'identity))))
         (list (org-export-get-caption table)
               (org-export-get-caption table t)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: export optional title
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_optional_title() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-get-optional-title)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "#+TITLE: Doc Title\n* H\nBody")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (info (org-combine-plists
                    (org-export--get-export-attributes)
                    (org-export-get-environment)
                    (org-export--collect-tree-properties tree (org-export-get-environment))))
             (hl (car (org-element-map tree 'headline #'identity))))
        (org-export-get-optional-title hl info)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: export node property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_node_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"myid\" \"2h\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (let ((org-mode-hook nil))
    (with-temp-buffer (org-mode)
      (insert "* H\n:PROPERTIES:\n:CUSTOM_ID: myid\n:EFFORT: 2h\n:END:")
      (goto-char (point-min))
      (let* ((tree (org-element-parse-buffer))
             (hl (car (org-element-map tree 'headline #'identity))))
        (list (org-export-get-node-property :CUSTOM_ID hl)
              (org-export-get-node-property :EFFORT hl))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: element type API
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_type_api() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (plain-text nil nil dummy dummy nil anonymous anonymous nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   (org-element-type "string")
   (org-element-type nil)
   (org-element-type 1)
   (org-element-type '(dummy))
   (org-element-type '(dummy nil 'foo))
   (org-element-type '((dummy)))
   (org-element-type '((dummy)) t)
   (org-element-type '("string") t)
   (org-element-type '(1 2) t)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: element type-p API
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_type_p_api() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (foo) (foo bar) nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   (org-element-type-p '(foo) 'foo)
   (org-element-type-p '(foo) '(foo))
   (org-element-type-p '(foo) '(foo bar))
   (org-element-type-p '(foo) 'bar)
   (org-element-type-p '(foo) '(bar baz))
   (org-element-type-p "string" 'plain-text)
   (org-element-type-p '((foo)) 'anonymous)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: element class API
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_class_api() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (element object element object object element element object object object)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   (org-element-class '(paragraph nil) nil)
   (org-element-class '(target nil) nil)
   (org-element-class '(org-data nil) nil)
   (org-element-class "text" nil)
   (org-element-class '("secondary " "string") nil)
   (org-element-class '(foo nil) nil)
   (org-element-class '(foo nil) '(center-block nil))
   (org-element-class '(foo nil) '(bold nil))
   (org-element-class '(foo nil) '(paragraph nil))
   (org-element-class '(foo nil) '("secondary"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: element property inherited
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_property_inherited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3 (1 2 3) (\"p\") (\"c\") (\"gc\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((gc (org-element-create 'gc '(:shared 3 :own-gc "gc")))
         (c (org-element-create 'c '(:shared 2 :own-c "c") gc))
         (p (org-element-create 'p '(:shared 1 :own-p "p") c)))
    (list
     (org-element-property-inherited :shared gc)
     (org-element-property-inherited :shared gc 'with-self)
     (org-element-property-inherited :shared gc 'with-self 'accumulate)
     (org-element-property-inherited :own-p gc 'with-self 'accumulate)
     (org-element-property-inherited :own-c gc 'with-self 'accumulate)
     (org-element-property-inherited :own-gc gc 'with-self 'accumulate))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: element operations chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_operations_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* \\nP1.\\n* \\nP2.\\n\" \"* \\nP1.\\n\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (let* ((doc (org-element-create 'org-data nil))
         (h1 (org-element-create 'headline '(:level 1 :raw-value "A")
              (org-element-create 'section nil (org-element-create 'paragraph nil "P1.\n"))))
         (h2 (org-element-create 'headline '(:level 1 :raw-value "B")
              (org-element-create 'section nil (org-element-create 'paragraph nil "P2.\n")))))
    (org-element-adopt doc h1 h2)
    (let ((after-adopt (substring-no-properties (org-element-interpret-data doc))))
      (org-element-extract h2)
      (list after-adopt
            (substring-no-properties (org-element-interpret-data doc))
            (org-element-property :parent h2)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: deferred chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_deferred_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((bar nil) bar (bar bar) (1 1) (1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   (let ((el (org-element-create 'd
              `(:deferred ,(org-element-deferred-create t
                            (lambda (el) (org-element-put-property el :foo 'bar) nil))))))
     (list (org-element-property :foo el) (org-element-property :foo2 el)))
   (let ((el (org-element-create 'd `(:foo ,(org-element-deferred-create nil (lambda (_) 'bar))))))
     (org-element-property :foo el))
   (let ((el (org-element-create 'd `(:foo ,(org-element-deferred-create t (lambda (_) 'bar))))))
     (list (org-element-property :foo el) (org-element-property-raw :foo el)))
   (let ((el (org-element-create 'd `( :foo 1 :bar ,(org-element-deferred-create-alias :foo)))))
     (list (org-element-property :foo el) (org-element-property :bar el)))
   (let ((el (org-element-create 'd `(:foo ,(org-element-deferred-create-list
                              (list 1 2 (org-element-deferred-create nil (lambda (_) 3))))))))
     (org-element-property :foo el))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-step: parse-and-interpret round-trips
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_parse_interpret_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"*text*\\n\" 1 5 (:parent (bold (:standard-properties [1 nil 2 6 7 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 7 7 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 7 7 0 nil first-section nil nil nil 1 7 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 7 7 0 nil org-data nil nil nil 3 7 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"text\" 0 4 (:parent #3))))) #(\"/text/\\n\" 1 5 (:parent (italic (:standard-properties [1 nil 2 6 7 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 7 7 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 7 7 0 nil first-section nil nil nil 1 7 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 7 7 0 nil org-data nil nil nil 3 7 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"text\" 0 4 (:parent #3))))) \"~text~\\n\" \"=text=\\n\" #(\"_text_\\n\" 1 5 (:parent (underline (:standard-properties [1 nil 2 6 7 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 7 7 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 7 7 0 nil first-section nil nil nil 1 7 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 7 7 0 nil org-data nil nil nil 3 7 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"text\" 0 4 (:parent #3))))) #(\"+target+\\n\" 1 7 (:parent (strike-through (:standard-properties [1 nil 2 8 9 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 9 9 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 9 9 0 nil first-section nil nil nil 1 9 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 9 9 0 nil org-data nil nil nil 3 9 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"target\" 0 6 (:parent #3))))) #(\"a_b\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"a\" 0 1 (:parent #3)) (subscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :use-brackets-p nil) #(\"b\" 0 1 (:parent #4))))) 2 3 (:parent (subscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"a\" 0 1 (:parent #6)) #3)] :use-brackets-p nil) #(\"b\" 0 1 (:parent #3))))) #(\"a_{b}\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"a\" 0 1 (:parent #3)) (subscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :use-brackets-p t) #(\"b\" 0 1 (:parent #4))))) 3 4 (:parent (subscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"a\" 0 1 (:parent #6)) #3)] :use-brackets-p t) #(\"b\" 0 1 (:parent #3))))) #(\"a^b\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"a\" 0 1 (:parent #3)) (superscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :use-brackets-p nil) #(\"b\" 0 1 (:parent #4))))) 2 3 (:parent (superscript (:standard-properties [2 nil 3 4 4 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 4 4 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 4 4 0 nil first-section nil nil nil 1 4 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 4 4 0 nil org-data nil nil nil 3 4 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"a\" 0 1 (:parent #6)) #3)] :use-brackets-p nil) #(\"b\" 0 1 (:parent #3))))) #(\"a^{b}\\n\" 0 1 (:parent (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"a\" 0 1 (:parent #3)) (superscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :use-brackets-p t) #(\"b\" 0 1 (:parent #4))))) 3 4 (:parent (superscript (:standard-properties [2 nil 4 5 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 6 6 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 6 6 0 nil first-section nil nil nil 1 6 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 6 6 0 nil org-data nil nil nil 3 6 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"a\" 0 1 (:parent #6)) #3)] :use-brackets-p t) #(\"b\" 0 1 (:parent #3))))) #(\"\\\\alpha text\\n\" 7 11 (:parent (paragraph (:standard-properties [1 1 1 12 12 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 12 12 0 nil first-section nil nil nil 1 12 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 12 12 0 nil org-data nil nil nil 3 12 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) (entity (:standard-properties [1 nil nil nil 8 1 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :name \"alpha\" :latex \"\\\\alpha\" :latex-math-p t :html \"&alpha;\" :ascii \"alpha\" :latin1 \"alpha\" :utf-8 \"α\" :use-brackets-p nil)) #(\"text\" 0 4 (:parent #3))))) #(\"\\\\alpha{}text\\n\" 8 12 (:parent (paragraph (:standard-properties [1 1 1 13 13 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 13 13 0 nil first-section nil nil nil 1 13 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 13 13 0 nil org-data nil nil nil 3 13 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) (entity (:standard-properties [1 nil nil nil 9 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :name \"alpha\" :latex \"\\\\alpha\" :latex-math-p t :html \"&alpha;\" :ascii \"alpha\" :latin1 \"alpha\" :utf-8 \"α\" :use-brackets-p t)) #(\"text\" 0 4 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "*text*") (funcall f "/text/") (funcall f "~text~")
     (funcall f "=text=") (funcall f "_text_") (funcall f "+target+")
     (funcall f "a_b") (funcall f "a_{b}") (funcall f "a^b") (funcall f "a^{b}")
     (funcall f "\\alpha text") (funcall f "\\alpha{}text"))))"##,
        expect,
    );
}

#[test]
fn strong_link_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"[[https://orgmode.org]]\\n\" #(\"[[https://orgmode.org][Org mode]]\\n\" 23 31 (:parent (link (:standard-properties [1 nil 24 32 34 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 34 34 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 34 34 0 nil first-section nil nil nil 1 34 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 34 34 0 nil org-data nil nil nil 3 34 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)] :type \"https\" :type-explicit-p t :path \"//orgmode.org\" :format bracket :raw-link \"https://orgmode.org\" :application nil :search-option nil) #(\"Org mode\" 0 8 (:parent #3))))) \"[[file:todo.org::*task]]\\n\" \"[[id:aaaa]]\\n\" \"[[#id]]\\n\" \"https://orgmode.org\\n\" \"<https://orgmode.org>\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "[[https://orgmode.org]]")
     (funcall f "[[https://orgmode.org][Org mode]]")
     (funcall f "[[file:todo.org::*task]]")
     (funcall f "[[id:aaaa]]")
     (funcall f "[[#id]]")
     (funcall f "https://orgmode.org")
     (funcall f "<https://orgmode.org>"))))"##,
        expect,
    );
}

#[test]
fn strong_footnote_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"Text[fn:1]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 11 11 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 11 11 0 nil first-section nil nil nil 1 11 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 11 11 0 nil org-data nil nil nil 3 11 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Text\" 0 4 (:parent #3)) (footnote-reference (:standard-properties [5 nil nil nil 11 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :label \"1\" :type standard))))) #(\"Text[fn:label]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 15 15 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 15 15 0 nil first-section nil nil nil 1 15 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 15 15 0 nil org-data nil nil nil 3 15 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Text\" 0 4 (:parent #3)) (footnote-reference (:standard-properties [5 nil nil nil 15 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :label \"label\" :type standard))))) #(\"Text[fn:label:def]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 19 19 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 19 19 0 nil first-section nil nil nil 1 19 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 19 19 0 nil org-data nil nil nil 3 19 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Text\" 0 4 (:parent #3)) (footnote-reference (:standard-properties [5 nil 15 18 19 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :label \"label\" :type inline) #(\"def\" 0 3 (:parent #4))))) 14 17 (:parent (footnote-reference (:standard-properties [5 nil 15 18 19 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 19 19 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 19 19 0 nil first-section nil nil nil 1 19 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 19 19 0 nil org-data nil nil nil 3 19 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"Text\" 0 4 (:parent #6)) #3)] :label \"label\" :type inline) #(\"def\" 0 3 (:parent #3))))) #(\"Text[fn::def]\\n\" 0 4 (:parent (paragraph (:standard-properties [1 1 1 14 14 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 14 14 0 nil first-section nil nil nil 1 14 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 14 14 0 nil org-data nil nil nil 3 14 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Text\" 0 4 (:parent #3)) (footnote-reference (:standard-properties [5 nil 10 13 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #3] :label nil :type inline) #(\"def\" 0 3 (:parent #4))))) 9 12 (:parent (footnote-reference (:standard-properties [5 nil 10 13 14 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 14 14 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 14 14 0 nil first-section nil nil nil 1 14 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 14 14 0 nil org-data nil nil nil 3 14 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #(\"Text\" 0 4 (:parent #6)) #3)] :label nil :type inline) #(\"def\" 0 3 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "Text[fn:1]") (funcall f "Text[fn:label]")
     (funcall f "Text[fn:label:def]") (funcall f "Text[fn::def]"))))"##,
        expect,
    );
}

#[test]
fn strong_block_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (#(\"#+begin_center\\nText\\n#+end_center\\n\" 15 20 (:parent (paragraph (:standard-properties [16 16 16 21 21 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (center-block (:standard-properties [1 1 16 21 33 0 nil top-comment nil nil nil 16 21 nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 33 33 0 nil first-section nil nil nil 1 33 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 33 33 0 nil org-data nil nil nil 3 33 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"Text\\n\" 0 5 (:parent #3))))) #(\"#+begin_quote\\nText\\n#+end_quote\\n\" 14 19 (:parent (paragraph (:standard-properties [15 15 15 20 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (quote-block (:standard-properties [1 1 15 20 31 0 nil top-comment nil nil nil 15 20 nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 31 31 0 nil first-section nil nil nil 1 31 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 31 31 0 nil org-data nil nil nil 3 31 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)]) #(\"Text\\n\" 0 5 (:parent #3))))) \"#+begin_example\\nTest\\n#+end_example\\n\" \"#+begin_export HTML\\n<p>Text</p>\\n#+end_export\\n\" #(\"#+begin_verse\\nTest\\n#+end_verse\\n\" 14 19 (:parent (verse-block (:standard-properties [1 1 15 20 31 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 31 31 0 nil first-section nil nil nil 1 31 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 31 31 0 nil org-data nil nil nil 3 31 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #6)]) #3)]) #(\"Test\\n\" 0 5 (:parent #3))))))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil) (org-src-preserve-indentation t)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "#+BEGIN_CENTER\nText\n#+END_CENTER")
     (funcall f "#+BEGIN_QUOTE\nText\n#+END_QUOTE")
     (funcall f "#+BEGIN_EXAMPLE\nTest\n#+END_EXAMPLE")
     (funcall f "#+BEGIN_EXPORT HTML\n<p>Text</p>\n#+END_EXPORT")
     (funcall f "#+BEGIN_VERSE\nTest\n#+END_VERSE"))))"##,
        expect,
    );
}

#[test]
fn strong_inline_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"call_test()\\n\" \"call_test(x=2)\\n\" \"src_emacs-lisp{(+ 1 1)}\\n\" \"@@backend:contents@@\\n\" \"\\\\command{}\\n\" \"$x$\\n\" \"$$x+y$$\\n\" \"\\\\(x+y\\\\)\\n\" \"\\\\[x+y\\\\]\\n\" \"[0/1]\\n\" \"[66%]\\n\" \"<<target>>\\n\" #(\"<<<some text>>>\\n\" 3 12 (:parent (radio-target (:standard-properties [1 nil 4 13 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 16 16 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 16 16 0 nil first-section nil nil nil 1 16 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 16 16 0 nil org-data nil nil nil 3 16 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #9)]) #6)]) #3)] :value \"some text\") #(\"some text\" 0 9 (:parent #3))))) \"{{{test}}}\\n\" \"{{{test(arg1,arg2)}}}\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "call_test()") (funcall f "call_test(x=2)")
     (funcall f "src_emacs-lisp{(+ 1 1)}") (funcall f "@@backend:contents@@")
     (funcall f "\\command{}") (funcall f "$x$") (funcall f "$$x+y$$")
     (funcall f "\\(x+y\\)") (funcall f "\\[x+y\\]")
     (funcall f "[0/1]") (funcall f "[66%]")
     (funcall f "<<target>>") (funcall f "<<<some text>>>")
     (funcall f "{{{test}}}") (funcall f "{{{test(arg1,arg2)}}}"))))"##,
        expect,
    );
}

#[test]
fn strong_table_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"| a | b |\\n| c | d |\\n\" 2 3 (:parent (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11))) (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"d\" 0 1 (:parent #11)))))] :type standard) #3 (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"b\" 0 1 (:parent #7))))]) #(\"a\" 0 1 (:parent #3)))) 6 7 (:parent (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11))) (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"d\" 0 1 (:parent #11)))))] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"a\" 0 1 (:parent #7))) #3)]) #(\"b\" 0 1 (:parent #3)))) 12 13 (:parent (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11)))) #6)] :type standard) #3 (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"d\" 0 1 (:parent #7))))]) #(\"c\" 0 1 (:parent #3)))) 16 17 (:parent (table-cell (:standard-properties [16 nil 17 18 20 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [11 11 12 20 20 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 20 20 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 20 20 0 nil first-section nil nil nil 1 20 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 20 20 0 nil org-data nil nil nil 3 20 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11)))) #6)] :type standard) (table-cell (:standard-properties [12 nil 13 14 16 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"c\" 0 1 (:parent #7))) #3)]) #(\"d\" 0 1 (:parent #3))))) #(\"| a | b |\\n|---+---|\\n| c | d |\\n\" 2 3 (:parent (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 30 30 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 30 30 0 nil first-section nil nil nil 1 30 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 30 30 0 nil org-data nil nil nil 3 30 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [11 11 nil nil 21 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type rule)) (table-row (:standard-properties [21 21 22 30 30 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [22 nil 23 24 26 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11))) (table-cell (:standard-properties [26 nil 27 28 30 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"d\" 0 1 (:parent #11)))))] :type standard) #3 (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"b\" 0 1 (:parent #7))))]) #(\"a\" 0 1 (:parent #3)))) 6 7 (:parent (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 30 30 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 30 30 0 nil first-section nil nil nil 1 30 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 30 30 0 nil org-data nil nil nil 3 30 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) #6 (table-row (:standard-properties [11 11 nil nil 21 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type rule)) (table-row (:standard-properties [21 21 22 30 30 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [22 nil 23 24 26 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"c\" 0 1 (:parent #11))) (table-cell (:standard-properties [26 nil 27 28 30 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"d\" 0 1 (:parent #11)))))] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"a\" 0 1 (:parent #7))) #3)]) #(\"b\" 0 1 (:parent #3)))) 22 23 (:parent (table-cell (:standard-properties [22 nil 23 24 26 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [21 21 22 30 30 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 30 30 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 30 30 0 nil first-section nil nil nil 1 30 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 30 30 0 nil org-data nil nil nil 3 30 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11)))) (table-row (:standard-properties [11 11 nil nil 21 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type rule)) #6)] :type standard) #3 (table-cell (:standard-properties [26 nil 27 28 30 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"d\" 0 1 (:parent #7))))]) #(\"c\" 0 1 (:parent #3)))) 26 27 (:parent (table-cell (:standard-properties [26 nil 27 28 30 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil (table-row (:standard-properties [21 21 22 30 30 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil (table (:standard-properties [1 1 1 30 30 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 30 30 0 nil first-section nil nil nil 1 30 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 30 30 0 nil org-data nil nil nil 3 30 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)] :type org :tblfm nil :value nil) (table-row (:standard-properties [1 1 2 10 11 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type standard) (table-cell (:standard-properties [2 nil 3 4 6 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"a\" 0 1 (:parent #11))) (table-cell (:standard-properties [6 nil 7 8 10 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #10]) #(\"b\" 0 1 (:parent #11)))) (table-row (:standard-properties [11 11 nil nil 21 0 nil table-row nil nil nil nil nil nil #<killed buffer> nil nil #9] :type rule)) #6)] :type standard) (table-cell (:standard-properties [22 nil 23 24 26 0 nil nil nil nil nil nil nil nil #<killed buffer> nil nil #6]) #(\"c\" 0 1 (:parent #7))) #3)]) #(\"d\" 0 1 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "| a | b |\n| c | d |")
     (funcall f "| a | b |\n|---+---|\n| c | d |"))))"##,
        expect,
    );
}

#[test]
fn strong_timestamp_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (string-match "<2012-03-29 .* 16:40>" (funcall f "<2012-03-29 thu. 16:40>"))
     (string-match "\\[2012-03-29 .* 16:40\\]" (funcall f "[2012-03-29 thu. 16:40]"))
     (string-match "<2012-03-29 .* 16:40-16:41>" (funcall f "<2012-03-29 thu. 16:40-16:41>"))
     (string-match "<2012-03-29 .* \\+1y>" (funcall f "<2012-03-29 thu. +1y>"))
     (equal "<%%(diary-float t 4 2)>\n" (funcall f "<%%(diary-float t 4 2)>"))))"##,
        expect,
    );
}

#[test]
fn strong_keyword_comment_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"#+keyword: value\\n\" \"# Comment\\n\" \"#+begin_comment\\nTest\\n#+end_comment\\n\" \": Test\\n\" \"-----\\n\" \"\\\\begin{equation}\\n1+1=2\\n\\\\end{equation}\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "#+KEYWORD: value") (funcall f "# Comment")
     (funcall f "#+BEGIN_COMMENT\nTest\n#+END_COMMENT")
     (funcall f ": Test") (funcall f "-------")
     (funcall f "\\begin{equation}\n1+1=2\n\\end{equation}"))))"##,
        expect,
    );
}

#[test]
fn strong_citation_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"[cite:@key]\\n\" \"[cite/style:@key]\\n\" #(\"[cite:pre @key]\\n\" 6 10 (:parent (citation-reference (:standard-properties [7 nil nil nil 15 0 (:prefix :suffix) nil nil nil nil nil nil nil #<killed buffer> nil nil (citation (:standard-properties [1 nil 7 15 16 0 (:prefix :suffix) nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 16 16 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 16 16 0 nil first-section nil nil nil 1 16 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 16 16 0 nil org-data nil nil nil 3 16 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)]) #6)] :style nil) #3)] :key \"key\" :prefix (#(\"pre \" 0 4 (:parent #3))))))) #(\"[cite:@key post]\\n\" 10 15 (:parent (citation-reference (:standard-properties [7 nil nil nil 16 0 (:prefix :suffix) nil nil nil nil nil nil nil #<killed buffer> nil nil (citation (:standard-properties [1 nil 7 16 17 0 (:prefix :suffix) nil nil nil nil nil nil nil #<killed buffer> nil nil (paragraph (:standard-properties [1 1 1 17 17 0 nil top-comment nil nil nil nil nil nil #<killed buffer> nil nil (section (:standard-properties [1 1 1 17 17 0 nil first-section nil nil nil 1 17 nil #<killed buffer> nil nil (org-data (:standard-properties [1 1 1 17 17 0 nil org-data nil nil nil 3 17 nil #<killed buffer> nil nil nil] :pre-blank 0 :path nil :CATEGORY nil) #12)]) #9)]) #6)] :style nil) #3)] :key \"key\" :suffix (#(\" post\" 0 5 (:parent #3))))))) \"[cite:@a;@b;@c]\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'oc)
  (let ((org-mode-hook nil)
        (f (lambda (text)
             (with-temp-buffer (org-mode) (insert text)
               (org-element-interpret-data (org-element-parse-buffer))))))
    (list
     (funcall f "[cite:@key]") (funcall f "[cite/style:@key]")
     (funcall f "[cite:pre @key]") (funcall f "[cite:@key post]")
     (funcall f "[cite:@a;@b;@c]"))))"##,
        expect,
    );
}
