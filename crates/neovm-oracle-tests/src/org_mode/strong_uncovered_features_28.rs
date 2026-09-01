//! Strong uncovered-features-28 oracle tests — org-timer, org-learn, org-macros.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-timer-start/timer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_timer_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"- 0:00:00 :: * T\\n:LOGBOOK:\\n:END:\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:LOGBOOK:\n:END:")
  (goto-char (point-min))
  (condition-case nil
      (org-timer-start)
    (error nil))
  (org-timer-item)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timer-set-timer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_timer_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (condition-case nil
      (org-timer-set-timer 5)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timer-pause-or-continue
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_timer_pause() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (condition-case nil
      (progn (org-timer-start) (org-timer-pause-or-continue))
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timer-stop
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_timer_stop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (goto-char (point-min))
  (condition-case nil
      (progn (org-timer-start) (org-timer-stop))
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-macro-replace-all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Undefined Org macro: greeting; aborting\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: greeting Hello $1!\n{{{greeting(World)}}} and {{{greeting(Elisp)}}}")
  (let ((raw (buffer-string)))
    (org-macro-replace-all org-macro-templates)
    (list raw (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-macro-accumulate-arguments
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_macro_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-macro-accumulate-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-macro-accumulate-arguments "{{{macro(a,b,c)}}}" 0)"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-macro-expand-macro
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_macro_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-macro-expand-macro)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: greeting Hello $1!\n{{{greeting(World)}}}")
  (let ((org-macro-templates (org-macro--collect-macros)))
    (org-macro-expand-macro "{{{greeting(World)}}}" org-macro-templates)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-macro--collect-macros
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_macro_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"b\" . \"2\") (\"a\" . \"1\") (\"author\") (\"email\") (\"title\") (\"date\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: a 1\n#+MACRO: b 2\n{{{a}}} {{{b}}}")
  (org-macro--collect-macros))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-learn
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_learn() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* T\\nSCHEDULED: <2026-01-15 +1d>\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15 +1d>")
  (goto-char (point-min))
  (condition-case nil
      (org-learn nil 5)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-learn-get-entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_learn_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15 +1d>\n* U\nSCHEDULED: <2026-01-20 +1w>")
  (condition-case nil
      (org-learn-get-entries)
    (error nil)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-duration-to-minutes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_duration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-duration-to-minutes)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-duration-to-minutes "1:30")
        (org-duration-to-minutes "2h30min")
        (org-duration-to-minutes "1d 2h")
        (org-duration-to-minutes "90min"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-duration-from-minutes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_duration_from() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-duration-from-minutes)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-duration-from-minutes 90)
        (org-duration-from-minutes 150)
        (org-duration-from-minutes 1500))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-duration-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_duration_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-duration-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-duration-p "1:30")
        (org-duration-p "2h30min")
        (org-duration-p "invalid")
        (org-duration-p "90min"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-cache-active-p
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_cache_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-cache-active-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody")
  (org-element-cache-active-p))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-cache-flush
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_cache_flush() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-cache-flush)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody")
  (org-element-cache-flush (point-min))
  (let ((s (org-element-cache-status)))
    (list (plist-get s :size))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-cache-sync
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_cache_sync() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-cache-sync)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody")
  (org-element-cache-sync)
  (let ((s (org-element-cache-status)))
    (list (plist-get s :size))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-blank-field
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_table_blank() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a | b |\\n|   | 2 |\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (goto-char (point-min))
  (forward-line 1)
  (forward-char 2)
  (org-table-blank-field)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-insert-row/column
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_table_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"|   | a | b |\\n|   |   |   |\\n|   | 1 | 2 |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table rear-nonsticky t display (space :relative-width 1)) 10 11 (face org-table) 11 12 (face org-table display (space :relative-width 1.001)) 12 13 (face org-table) 13 14 (face org-table-row) 14 15 (face org-table) 15 16 (face org-table rear-nonsticky t display (space :relative-width 1)) 16 17 (face org-table) 17 18 (face org-table display (space :relative-width 1.001)) 18 19 (face org-table) 19 20 (face org-table rear-nonsticky t display (space :relative-width 1)) 20 21 (face org-table) 21 22 (face org-table display (space :relative-width 1.001)) 22 23 (face org-table) 23 24 (face org-table rear-nonsticky t display (space :relative-width 1)) 24 25 (face org-table) 25 26 (face org-table display (space :relative-width 1.001)) 26 27 (face org-table) 27 28 (face org-table-row) 28 29 (face org-table) 29 30 (face org-table rear-nonsticky t display (space :relative-width 1)) 30 31 (face org-table) 31 32 (face org-table display (space :relative-width 1.001)) 32 33 (face org-table) 33 34 (face org-table rear-nonsticky t display (space :relative-width 1)) 34 35 (face org-table) 35 36 (face org-table display (space :relative-width 1.001)) 36 37 (face org-table) 37 38 (face org-table rear-nonsticky t display (space :relative-width 1)) 38 39 (face org-table) 39 40 (face org-table display (space :relative-width 1.001)) 40 41 (face org-table) 41 42 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (goto-char (point-min))
  (forward-line 1)
  (org-table-insert-row)
  (org-table-insert-column)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-delete-row/column
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_table_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-table-delete-row)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c |\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |")
  (goto-char (point-min))
  (forward-line 1)
  (org-table-delete-row)
  (org-table-goto-column 2)
  (org-table-delete-column)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-move-row/column
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_table_move() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | c | b |\\n| 4 | 6 | 5 |\\n| 1 | 3 | 2 |\\n\" 0 5 (face org-table) 5 8 (face org-table) 8 9 (face org-table) 9 12 (face org-table) 12 13 (face org-table) 13 14 (face org-table-row) 14 19 (face org-table) 19 22 (face org-table) 22 23 (face org-table) 23 26 (face org-table) 26 27 (face org-table) 27 28 (face org-table-row) 28 33 (face org-table) 33 36 (face org-table) 36 37 (face org-table) 37 40 (face org-table) 40 41 (face org-table) 41 42 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c |\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |")
  (goto-char (point-min))
  (forward-line 1)
  (org-table-move-row-down)
  (org-table-goto-column 2)
  (org-table-move-column-right)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-sort-lines
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_table_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| name | val |\\n|---+---|\\n| c | 3 |\\n| a | 1 |\\n| b | 2 |\" 0 14 (face org-table) 14 15 (face org-table-row) 15 24 (face org-table) 24 25 (face org-table-row) 25 34 (face org-table) 34 35 (face org-table-row) 35 44 (face org-table) 44 45 (face org-table-row) 45 54 (face org-table))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| name | val |\n|---+---|\n| c | 3 |\n| a | 1 |\n| b | 2 |")
  (goto-char (point-min))
  (org-table-sort-lines nil ?a)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-transpose-table-at-point
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_table_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | 1 | 3 |\\n| b | 2 | 4 |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table rear-nonsticky t display (space :relative-width 1)) 10 11 (face org-table) 11 12 (face org-table display (space :relative-width 1.001)) 12 13 (face org-table) 13 14 (face org-table-row) 14 15 (face org-table) 15 16 (face org-table rear-nonsticky t display (space :relative-width 1)) 16 17 (face org-table) 17 18 (face org-table display (space :relative-width 1.001)) 18 19 (face org-table) 19 20 (face org-table rear-nonsticky t display (space :relative-width 1)) 20 21 (face org-table) 21 22 (face org-table display (space :relative-width 1.001)) 22 23 (face org-table) 23 24 (face org-table rear-nonsticky t display (space :relative-width 1)) 24 25 (face org-table) 25 26 (face org-table display (space :relative-width 1.001)) 26 27 (face org-table) 27 28 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |\n| 3 | 4 |")
  (goto-char (point-min))
  (org-table-transpose-table-at-point)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-toggle-formula-debugger
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_table_debug() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a | b |\\n| 1 | 2 |\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (org-table-toggle-formula-debugger)
  (org-table-toggle-formula-debugger)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-edit-field
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf28_table_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK \"#\\n# Edit field @2$0 and finish with C-c C-c\\n#\\n\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (goto-char (point-min))
  (forward-line 1)
  (org-table-edit-field nil)
  (buffer-string))"##,
        expect,
    );
}
