//! Ported upstream ERT tests from org-mode's test-org-table.el (9.7.11).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ── Reference conversion: an format ──────────────────────────────────

#[test]
fn upstream_org_table_convert_refs_to_an_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"A2\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (org-table-convert-refs-to-an "@2$1"))"##,
        expect,
    );
}

#[test]
fn upstream_org_table_convert_refs_to_an_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"A1 = $0\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (org-table-convert-refs-to-an "@1$1 = $0"))"##,
        expect,
    );
}

#[test]
fn upstream_org_table_convert_refs_to_an_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"C& = remote(FOO, @@#B&)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (org-table-convert-refs-to-an "$3 = remote(FOO, @@#$2)"))"##,
        expect,
    );
}

// ── Reference conversion: rc format ──────────────────────────────────

#[test]
fn upstream_org_table_convert_refs_to_rc_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"@2$1\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (org-table-convert-refs-to-rc "A2"))"##,
        expect,
    );
}

#[test]
fn upstream_org_table_convert_refs_to_rc_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"@1$1 = $0\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (org-table-convert-refs-to-rc "A1 = $0"))"##,
        expect,
    );
}

#[test]
fn upstream_org_table_convert_refs_to_rc_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"$3 = remote(FOO, @@#$2)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-table)
  (org-table-convert-refs-to-rc "C& = remote(FOO, @@#B&)"))"##,
        expect,
    );
}

// ── Table formulas ───────────────────────────────────────────────────

#[test]
fn upstream_org_table_simple_formula_no_grouping() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a #+TBLFM line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "
| 2 |
| 4 |
| 8 |
|   |
")
      (goto-char (point-min))
      (org-table-calc-current-TBLFM)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

#[test]
fn upstream_org_table_formula_with_title_row() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a #+TBLFM line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "
| foo |
|-----|
|   2 |
|   4 |
|   8 |
|     |
#+TBLFM: @>$1 = vsum(@I..@>>)
")
      (goto-char (point-min))
      (org-table-calc-current-TBLFM)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-at-table-p ────────────────────────────────────────────

#[test]
fn upstream_org_table_at_table_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; On table.
     (with-temp-buffer
       (org-mode)
       (insert "| a | b |")
       (goto-char (point-min))
       (org-at-table-p))
     ;; Not on table.
     (with-temp-buffer
       (org-mode)
       (insert "Not a table")
       (goto-char (point-min))
       (org-at-table-p))
     ;; On separator.
     (with-temp-buffer
       (org-mode)
       (insert "| a |\n|---|\n| b |")
       (goto-char (point-min))
       (forward-line 1)
       (org-at-table-p)))))"##,
        expect,
    );
}

// ── Table: org-table-align ───────────────────────────────────────────

#[test]
fn upstream_org_table_align() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a | b |\\n| c | d |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "|a|b|\n|c|d|")
      (goto-char (point-min))
      (org-table-align)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-table-insert-column ───────────────────────────────────

#[test]
fn upstream_org_table_insert_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"|   | a | b |\\n|   | c | d |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| a | b |\n| c | d |")
      (goto-char (point-min))
      (org-table-insert-column)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-table-delete-column ───────────────────────────────────

#[test]
fn upstream_org_table_delete_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| b | c |\\n| e | f |\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| a | b | c |\n| d | e | f |")
      (goto-char (point-min))
      (forward-char 4)
      (org-table-delete-column)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-table-insert-row ──────────────────────────────────────

#[test]
fn upstream_org_table_insert_row() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"|   |   |\\n| a | b |\\n| c | d |\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| a | b |\n| c | d |")
      (goto-char (point-min))
      (org-table-insert-row)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-table-kill-row ────────────────────────────────────────

#[test]
fn upstream_org_table_kill_row() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a | b |\\n| e | f |\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| a | b |\n| c | d |\n| e | f |")
      (goto-char (point-min))
      (forward-line 1)
      (org-table-kill-row)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-table-move-column ─────────────────────────────────────

#[test]
fn upstream_org_table_move_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"| b | a | c |\" \"| b | a | c |\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Move right.
     (with-temp-buffer
       (org-mode)
       (insert "| a | b | c |")
       (goto-char (point-min))
       (forward-char 2)
       (org-table-move-column-right)
       (buffer-substring-no-properties (point-min) (point-max)))
     ;; Move left.
     (with-temp-buffer
       (org-mode)
       (insert "| a | b | c |")
       (goto-char (point-min))
       (forward-char 6)
       (org-table-move-column-left)
       (buffer-substring-no-properties (point-min) (point-max))))))"##,
        expect,
    );
}

// ── Table: org-table-move-row ────────────────────────────────────────

#[test]
fn upstream_org_table_move_row() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"| b |\\n| a |\\n| c |\" \"| a |\\n| c |\\n| b |\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; Move down.
     (with-temp-buffer
       (org-mode)
       (insert "| a |\n| b |\n| c |")
       (goto-char (point-min))
       (org-table-move-row-down)
       (buffer-substring-no-properties (point-min) (point-max)))
     ;; Move up.
     (with-temp-buffer
       (org-mode)
       (insert "| a |\n| b |\n| c |")
       (goto-char (point-min))
       (forward-line 2)
       (org-table-move-row-up)
       (buffer-substring-no-properties (point-min) (point-max))))))"##,
        expect,
    );
}

// ── Table: org-table-sort-lines ──────────────────────────────────────

#[test]
fn upstream_org_table_sort_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Format specifier doesn’t match argument type\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| c |\n| a |\n| b |")
      (goto-char (point-min))
      (org-table-sort-lines ?a 'string)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-table-transpose ───────────────────────────────────────

#[test]
fn upstream_org_table_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a | c | e |\\n| b | d | f |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| a | b |\n| c | d |\n| e | f |")
      (goto-char (point-min))
      (org-table-transpose-table-at-point)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-table-convert ─────────────────────────────────────────

#[test]
fn upstream_org_table_convert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"| a | b | c |\\n| d | e | f |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "a\tb\tc\nd\te\tf")
      (goto-char (point-min))
      (org-table-convert-region (point-min) (point-max))
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-table-create ──────────────────────────────────────────

#[test]
fn upstream_org_table_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"|   |   |   |\\n|---+---+---|\\n|   |   |   |\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (org-table-create "3x2")
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-table-get ─────────────────────────────────────────────

#[test]
fn upstream_org_table_get_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-table-get-rect)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| a | b |\n| c | d |")
      (goto-char (point-min))
      (list
       ;; Get field.
       (org-table-get 1 1)
       (org-table-get 1 2)
       (org-table-get 2 1)
       ;; Get range.
       (org-table-get-rect (list 1 1 2 2))))))"##,
        expect,
    );
}

// ── Table: org-table-eval-formula ────────────────────────────────────

#[test]
fn upstream_org_table_eval_formula() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a #+TBLFM line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| 2 | 4 |\n| 3 | 6 |\n|   |   |")
      (goto-char (point-min))
      (org-table-calc-current-TBLFM)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-table-recalculate ─────────────────────────────────────

#[test]
fn upstream_org_table_recalculate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"| 1 | 2 | 3 |\\n| 3 | 4 |   |\\n#+TBLFM: $3=$1+$2\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| 1 | 2 |\n| 3 | 4 |\n#+TBLFM: $3=$1+$2")
      (goto-char (point-min))
      (org-table-recalculate)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-table-blank-field ─────────────────────────────────────

#[test]
fn upstream_org_table_blank_field() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not in table data field\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| value |")
      (goto-char (point-min))
      (org-table-blank-field)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-table-copy-down ───────────────────────────────────────

#[test]
fn upstream_org_table_copy_down() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not in table data field\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| val |\n|     |\n|     |")
      (goto-char (point-min))
      (org-table-copy-down 1)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-table-toggle-formula-debugger ─────────────────────────

#[test]
fn upstream_org_table_toggle_formula_debugger() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Formula debugging has been turned on\" \"Formula debugging has been turned off\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| 1 |")
      (goto-char (point-min))
      (list
       (org-table-toggle-formula-debugger)
       (org-table-toggle-formula-debugger)))))"##,
        expect,
    );
}

// ── Table: org-table-wrap-region ─────────────────────────────────────

#[test]
fn upstream_org_table_wrap_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "some text")
      (goto-char (point-min))
      (org-table-wrap-region)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}

// ── Table: org-at-TBLFM-p ───────────────────────────────────────────

#[test]
fn upstream_org_table_at_tblfm_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (list
     ;; At TBLFM line.
     (with-temp-buffer
       (org-mode)
       (insert "| a |\n#+TBLFM: $1=$1")
       (goto-char (point-max))
       (org-at-TBLFM-p))
     ;; Not at TBLFM.
     (with-temp-buffer
       (org-mode)
       (insert "| a |")
       (goto-char (point-min))
       (org-at-TBLFM-p)))))"##,
        expect,
    );
}

// ── Table: org-table-TBLFM-begin ─────────────────────────────────────

#[test]
fn upstream_org_table_tblfm_begin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| a |\n#+TBLFM: $1=$1\n#+TBLFM: $1=$2")
      (goto-char (point-max))
      (org-table-TBLFM-begin))))"##,
        expect,
    );
}

// ── Table: org-table-get-remote-options ──────────────────────────────

#[test]
fn upstream_org_table_get_remote_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-table-get-remote-options)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "#+NAME: mytable\n| a | b |\n| 1 | 2 |")
      (goto-char (point-min))
      (org-table-get-remote-options "mytable"))))"##,
        expect,
    );
}

// ── Table: field formula parsing ─────────────────────────────────────

#[test]
fn upstream_org_table_field_formula() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"| 10 | 20 | 30 |\\n| 30 | 40 |    |\\n#+TBLFM: $3=$1+$2\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (let ((org-mode-hook nil))
    (with-temp-buffer
      (org-mode)
      (insert "| 10 | 20 |\n| 30 | 40 |\n#+TBLFM: $3=$1+$2")
      (goto-char (point-min))
      (org-table-recalculate)
      (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect,
    );
}
