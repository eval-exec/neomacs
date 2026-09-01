//! Strong edge-combos-4 oracle tests — edge case combinations.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn ec4_empty_buffer_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function insert-heading)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert-heading)
  (insert "First")
  (list (org-get-heading t t t t) (org-current-level)))"##,
        expect,
    );
}

#[test]
fn ec4_empty_buffer_meta_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* Item\" headline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (org-meta-return)
  (insert "Item")
  (list (buffer-string) (org-element-type (org-element-at-point))))"##,
        expect,
    );
}

#[test]
fn ec4_deep_nesting_promote_demote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((3 \"H3\") (2 \"H3\") (4 \"H3\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\n**** H4")
  (goto-char (point-min))
  (search-forward "H3")
  (let ((before (list (org-current-level) (org-get-heading t t t t))))
    (org-promote)
    (let ((after-p (list (org-current-level) (org-get-heading t t t t))))
      (org-demote)
      (org-demote)
      (list before after-p (list (org-current-level) (org-get-heading t t t t))))))"##,
        expect,
    );
}

#[test]
fn ec4_unicode_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"値\" \"名前\" \"新しい値\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:VAR: 値\n:NAME: 名前\n:END:")
  (goto-char (point-min))
  (list (org-entry-get nil "VAR") (org-entry-get nil "NAME")
        (progn (org-entry-put nil "VAR" "新しい値") (org-entry-get nil "VAR"))))"##,
        expect,
    );
}

#[test]
fn ec4_table_empty_rows() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"a\" \"b\") (\"\" \"\") (\"c\" \"d\")) ((#(\"a\" 0 1 (face org-table)) #(\"b\" 0 1 (face org-table))) (\"X\" \"\") (#(\"c\" 0 1 (face org-table)) #(\"d\" 0 1 (face org-table)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n|   |   |\n| c | d |")
  (goto-char (point-min))
  (let ((d1 (org-table-to-lisp)))
    (org-table-next-row)
    (org-table-put 2 1 "X")
    (list d1 (org-table-to-lisp))))"##,
        expect,
    );
}

#[test]
fn ec4_list_mixed_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((unordered 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- plain\n1. ordered\n+ bullet\n- another")
  (org-element-map (org-element-parse-buffer) 'plain-list
    (lambda (pl)
      (list (org-element-property :type pl)
            (length (org-element-contents pl))))))"##,
        expect,
    );
}

#[test]
fn ec4_footnote_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1\" inline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1: inline def] more")
  (let* ((tree (org-element-parse-buffer))
         (fn (car (org-element-map tree 'footnote-reference (lambda (f) f)))))
    (list (org-element-property :label fn) (org-element-property :type fn))))"##,
        expect,
    );
}

#[test]
fn ec4_timestamp_active_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<2026-01-15> [2026-01-20]")
  (let* ((tree (org-element-parse-buffer))
         (ts (org-element-map tree 'timestamp
               (lambda (t) (list (org-element-property :type t)
                                 (org-element-property :year-start t)
                                 (org-element-property :day-start t))))))
    ts)"##,
        expect,
    );
}

#[test]
fn ec4_drawer_empty_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:END:")
  (let* ((tree (org-element-parse-buffer))
         (drawer (car (org-element-map tree 'drawer (lambda (d) d)))))
    (org-element-property :drawer-name drawer))"##,
        expect,
    );
}

#[test]
fn ec4_link_no_description() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"https\" \"//example.com\" \"https://example.com\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "See [[https://example.com]]")
  (let* ((tree (org-element-parse-buffer))
         (link (car (org-element-map tree 'link (lambda (l) l)))))
    (list (org-element-property :type link)
          (org-element-property :path link)
          (org-element-property :raw-link link))))"##,
        expect,
    );
}

#[test]
fn ec4_planning_deadline_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"<2026-01-20>\" nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task\nDEADLINE: <2026-01-20>")
  (goto-char (point-min))
  (list (org-entry-get nil "DEADLINE") (org-entry-get nil "SCHEDULED") (org-entry-get nil "CLOSED")))"##,
        expect,
    );
}

#[test]
fn ec4_tag_no_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Simple heading")
  (goto-char (point-min))
  (org-get-tags nil t))"##,
        expect,
    );
}

#[test]
fn ec4_priority_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO No priority")
  (goto-char (point-min))
  (org-get-priority (char-after)))"##,
        expect,
    );
}

#[test]
fn ec4_todo_no_keyword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Just a heading")
  (goto-char (point-min))
  (org-get-todo-state))"##,
        expect,
    );
}

#[test]
fn ec4_visibility_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Only heading")
  (goto-char (point-min))
  (let ((hidden (get-char-property (point) 'invisible)))
    (org-cycle)
    (list hidden (get-char-property (point) 'invisible))))"##,
        expect,
    );
}

#[test]
fn ec4_property_empty_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:EMPTY: \n:END:")
  (goto-char (point-min))
  (org-entry-get nil "EMPTY"))"##,
        expect,
    );
}

#[test]
fn ec4_macro_no_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Undefined Org macro: greeting; aborting\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: greeting Hello!\n{{{greeting}}}")
  (let ((raw (buffer-string)))
    (org-macro-replace-all org-macro-templates)
    (list raw (buffer-string))))"##,
        expect,
    );
}

#[test]
fn ec4_dynamic_block_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK #(\"#+BEGIN: clocktable\\n#+CAPTION: Clock summary at [2026-06-15 Mon 12:00]\\n| Headline     | Time   |\\n|--------------+--------|\\n| *Total time* | *0:00* |\\n#+END:\" 71 72 (face org-table) 72 73 (face org-table rear-nonsticky t display (space :relative-width 1)) 73 81 (face org-table) 81 85 (face org-table) 85 86 (face org-table display (space :relative-width 1.001)) 86 87 (face org-table) 87 88 (face org-table rear-nonsticky t display (space :relative-width 1)) 88 92 (face org-table) 92 94 (face org-table) 94 95 (face org-table display (space :relative-width 1.001)) 95 96 (face org-table) 96 97 (face org-table-row) 97 98 (face org-table) 98 122 (face org-table) 122 123 (face org-table-row) 123 124 (face org-table) 124 125 (face org-table rear-nonsticky t display (space :relative-width 1)) 125 137 (org-emphasis t font-lock-multiline t face (bold org-table)) 137 138 (face org-table display (space :relative-width 1.001)) 138 139 (face org-table) 139 140 (face org-table rear-nonsticky t display (space :relative-width 1)) 140 146 (org-emphasis t font-lock-multiline t face (bold org-table)) 146 147 (face org-table display (space :relative-width 1.001)) 147 148 (face org-table) 148 149 (face org-table-row))""##
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN: clocktable\n#+END:")
  (goto-char (point-min))
  (org-dblock-update)
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn ec4_structure_template_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-try-structure-completion)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<s")
  (org-try-structure-completion)
  (let ((s1 (buffer-string)))
    (erase-buffer)
    (insert "<e")
    (org-try-structure-completion)
    (list s1 (buffer-string))))"##,
        expect,
    );
}

#[test]
fn ec4_comment_single_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Comment line\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "# Comment line")
  (org-element-map (org-element-parse-buffer) 'comment
    (lambda (c) (org-element-property :value c))))"##,
        expect,
    );
}

#[test]
fn ec4_fixed_width_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert ": Fixed width")
  (let* ((tree (org-element-parse-buffer))
         (fw (car (org-element-map tree 'fixed-width (lambda (f) f)))))
    (org-element-property :value fw))"##,
        expect,
    );
}

#[test]
fn ec4_export_empty_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (let* ((info (org-export-get-environment nil)))
    (plist-get info :title)))"##,
        expect,
    );
}

#[test]
fn ec4_element_buffer_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (headline 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading\nBody")
  (goto-char (point-min))
  (let* ((el (org-element-at-point)))
    (list (org-element-type el) (org-element-property :begin el))))"##,
        expect,
    );
}

#[test]
fn ec4_element_buffer_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading\nBody")
  (goto-char (point-max))
  (org-element-type (org-element-at-point)))"##,
        expect,
    );
}

#[test]
fn ec4_clock_no_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clocking-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task with no clock")
  (org-clocking-p))"##,
        expect,
    );
}

#[test]
fn ec4_statistics_no_cookies() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* Task\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task\n- [ ] item 1\n- [ ] item 2")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (buffer-substring-no-properties (line-beginning-position) (line-end-position)))"##,
        expect,
    );
}

#[test]
fn ec4_statistics_all_checked() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* Task [3/3]\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task [0/0]\n- [X] item 1\n- [X] item 2\n- [X] item 3")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (buffer-substring-no-properties (line-beginning-position) (line-end-position)))"##,
        expect,
    );
}

#[test]
fn ec4_sparse_tree_no_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"Task 1\" \"Task 2\") nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task 1\n* TODO Task 2")
  (goto-char (point-min))
  (org-match-sparse-tree nil "DONE")
  (let ((v '()) (h '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((hd (org-get-heading t t t t)))
        (when hd
          (if (get-char-property (point) 'invisible) (push hd h) (push hd v))))
      (forward-line))
    (list (nreverse v) (nreverse h))))"##,
        expect,
    );
}

#[test]
fn ec4_table_single_row() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"a\" \"b\" \"c\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c |")
  (org-table-to-lisp))"##,
        expect,
    );
}

#[test]
fn ec4_table_single_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"a\") (\"b\") (\"c\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a |\n| b |\n| c |")
  (org-table-to-lisp))"##,
        expect,
    );
}
