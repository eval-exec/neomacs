//! Strong uncovered-features-41 oracle tests — org-table, org-timer, org-duration.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-table-create
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"|   |   |   |\\n|---+---+---|\\n|   |   |   |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table rear-nonsticky t display (space :relative-width 1)) 10 11 (face org-table) 11 12 (face org-table display (space :relative-width 1.001)) 12 13 (face org-table) 13 14 (face org-table-row) 14 15 (face org-table) 15 27 (face org-table) 27 28 (face org-table-row) 28 29 (face org-table) 29 30 (face org-table rear-nonsticky t display (space :relative-width 1)) 30 31 (face org-table) 31 32 (face org-table display (space :relative-width 1.001)) 32 33 (face org-table) 33 34 (face org-table rear-nonsticky t display (space :relative-width 1)) 34 35 (face org-table) 35 36 (face org-table display (space :relative-width 1.001)) 36 37 (face org-table) 37 38 (face org-table rear-nonsticky t display (space :relative-width 1)) 38 39 (face org-table) 39 40 (face org-table display (space :relative-width 1.001)) 40 41 (face org-table) 41 42 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (org-table-create "3x2")
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-create-with-table-width
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_create_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (void-function org-table-create-with-table-width)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (org-table-create-with-table-width 3)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-justify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_justify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-table-justify)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (goto-char (point-min))
  (forward-line 1)
  (org-table-justify)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-align
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_align() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | b |\\n| 1 | 2 |\" 0 9 (face org-table) 9 10 (face org-table-row) 10 19 (face org-table))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (org-table-align)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-insert-row
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_insert_row() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | b |\\n|   |   |\\n| 1 | 2 |\" 0 9 (face org-table) 9 10 (face org-table-row) 10 19 (face org-table) 19 20 (face org-table-row) 20 29 (face org-table))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (goto-char (point-min))
  (forward-line 1)
  (org-table-insert-row)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-insert-column
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_insert_col() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | b |   |\\n| 1 | 2 |   |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table rear-nonsticky t display (space :relative-width 1)) 10 11 (face org-table) 11 12 (face org-table display (space :relative-width 1.001)) 12 13 (face org-table) 13 14 (face org-table-row) 14 15 (face org-table) 15 16 (face org-table rear-nonsticky t display (space :relative-width 1)) 16 17 (face org-table) 17 18 (face org-table display (space :relative-width 1.001)) 18 19 (face org-table) 19 20 (face org-table rear-nonsticky t display (space :relative-width 1)) 20 21 (face org-table) 21 22 (face org-table display (space :relative-width 1.001)) 22 23 (face org-table) 23 24 (face org-table rear-nonsticky t display (space :relative-width 1)) 24 25 (face org-table) 25 26 (face org-table display (space :relative-width 1.001)) 26 27 (face org-table) 27 28 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (org-table-insert-column)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-delete-row
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_delete_row() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-table-delete-row)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |\n| 3 | 4 |")
  (goto-char (point-min))
  (forward-line 1)
  (org-table-delete-row)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-delete-column
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_delete_col() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | c |\\n| 1 | 3 |\" 0 9 (face org-table) 9 10 (face org-table-row) 10 19 (face org-table))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c |\n| 1 | 2 | 3 |")
  (org-table-goto-column 2)
  (org-table-delete-column)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-move-row
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_move_row() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| 1 | 2 |\\n| a | b |\\n| 3 | 4 |\" 0 9 (face org-table) 9 10 (face org-table-row) 10 19 (face org-table) 19 20 (face org-table-row) 20 29 (face org-table))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |\n| 3 | 4 |")
  (goto-char (point-min))
  (forward-line 1)
  (org-table-move-row 'down)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-move-column
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_move_col() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| b | a | c |\\n| 2 | 1 | 3 |\" 0 13 (face org-table) 13 14 (face org-table-row) 14 27 (face org-table))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c |\n| 1 | 2 | 3 |")
  (org-table-goto-column 2)
  (org-table-move-column 'right)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-sort-lines
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not in table data field\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| name | val |\n|---+---|\n| c | 3 |\n| a | 1 |\n| b | 2 |")
  (org-table-sort-lines nil ?a)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-transpose-table-at-point
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_transpose() {
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
// org-table-blank-field
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_blank() {
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
// org-table-toggle-formula-debugger
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_debug() {
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
fn uf41_table_edit() {
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

// ═══════════════════════════════════════════════════════════════════════
// org-table-next-field
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_next() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c |\n| 1 | 2 | 3 |")
  (goto-char (point-min))
  (forward-line 1)
  (let ((r '()))
    (push (org-table-current-column) r)
    (org-table-next-field)
    (push (org-table-current-column) r)
    (org-table-next-field)
    (push (org-table-current-column) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-previous-field
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_prev() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c |\n| 1 | 2 | 3 |")
  (goto-char (point-min))
  (forward-line 1)
  (org-table-next-field)
  (let ((r '()))
    (push (org-table-current-column) r)
    (org-table-previous-field)
    (push (org-table-current-column) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-copy-down
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_copy_down() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | b |\\n| 1 | 2 |\\n| 2 |   |\\n\" 0 9 (face org-table) 9 10 (face org-table-row) 10 19 (face org-table) 19 20 (face org-table-row) 20 21 (face org-table) 21 22 (face org-table rear-nonsticky t display (space :relative-width 1)) 22 23 (face org-table) 23 24 (face org-table display (space :relative-width 1.001)) 24 25 (face org-table) 25 26 (face org-table rear-nonsticky t display (space :relative-width 1)) 26 27 (face org-table) 27 28 (face org-table display (space :relative-width 1.001)) 28 29 (face org-table) 29 30 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |\n|   |   |")
  (goto-char (point-min))
  (forward-line 1)
  (forward-char 2)
  (org-table-copy-down 1)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-wrap-region
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_table_wrap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not in table data field\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| long text here | 2 |")
  (goto-char (point-min))
  (forward-line 1)
  (org-table-wrap-region 10)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timer-start
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_timer_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (condition-case nil
      (org-timer-start)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-timer-set-timer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_timer_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (condition-case nil
      (org-timer-set-timer 5)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-duration-to-minutes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf41_duration() {
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
fn uf41_duration_from() {
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
fn uf41_duration_p() {
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
