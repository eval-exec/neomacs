//! Strong edge-deep-6 oracle tests — boundary conditions.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Edge: empty buffer operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_empty_buffer_heading() {
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
fn ed6_empty_buffer_meta_return() {
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

// ═══════════════════════════════════════════════════════════════════════
// Edge: deeply nested structure
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_deep_nesting_10() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 \"L10\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* L1\n** L2\n*** L3\n**** L4\n***** L5\n****** L6\n******* L7\n******** L8\n********* L9\n********** L10")
  (goto-char (point-max))
  (list (org-current-level) (org-get-heading t t t t)))"##,
        expect,
    );
}

#[test]
fn ed6_deep_nesting_promote_demote() {
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

// ═══════════════════════════════════════════════════════════════════════
// Edge: special characters
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_special_chars_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Headline with *special* /chars/ =code= :tag:")
  (goto-char (point-min))
  (list (org-get-heading t t t t) (org-get-todo-state) (org-get-priority (char-after)) (org-get-tags nil t)))"##,
        expect,
    );
}

#[test]
fn ed6_special_chars_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"https\" \"//example.com/path?q=1&r=2\" \"https://example.com/path?q=1&r=2\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "See [[https://example.com/path?q=1&r=2][link & more]]")
  (org-element-map (org-element-parse-buffer) 'link
    (lambda (l) (list (org-element-property :type l) (org-element-property :path l) (org-element-property :raw-link l)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: unicode content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_unicode_headlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TODO\" \"任务一\") (\"DONE\" \"任务二\") (nil \"WAITING 任务三\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO 任务一\n* DONE 任务二\n* WAITING 任务三")
  (goto-char (point-min))
  (list (list (org-get-todo-state) (org-get-heading t t t t))
        (progn (forward-line) (list (org-get-todo-state) (org-get-heading t t t t)))
        (progn (forward-line) (list (org-get-todo-state) (org-get-heading t t t t)))))"##,
        expect,
    );
}

#[test]
fn ed6_unicode_properties() {
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

// ═══════════════════════════════════════════════════════════════════════
// Edge: table boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_table_single_cell() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (((\"only\")) 1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| only |")
  (list (org-table-to-lisp) (org-table-current-line) (org-table-current-column)))"##,
        expect,
    );
}

#[test]
fn ed6_table_empty_rows() {
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
fn ed6_table_formula_division() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"3.3333333\" 0 9 (face org-table)) #(\"3.5\" 0 3 (face org-table)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| 10 | 3 |\n| 7 | 2 |\n#+TBLFM: $3=$1/$2")
  (goto-char (point-min))
  (org-table-recalculate 'all)
  (list (org-table-get 1 3) (org-table-get 2 3)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: list boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_list_single_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((unordered 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- single item")
  (org-element-map (org-element-parse-buffer) 'plain-list
    (lambda (pl) (list (org-element-property :type pl) (length (org-element-contents pl))))))"##,
        expect,
    );
}

#[test]
fn ed6_list_deeply_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"- \" nil) (\"- \" nil) (\"- \" nil) (\"- \" nil) (\"- \" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- L1\n  - L2\n    - L3\n      - L4\n        - L5")
  (org-element-map (org-element-parse-buffer) 'item
    (lambda (it) (list (org-element-property :bullet it) (org-element-property :level it)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: footnote boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_footnote_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1\" inline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1: inline definition] more text")
  (let* ((tree (org-element-parse-buffer))
         (fn (car (org-element-map tree 'footnote-reference (lambda (f) f)))))
    (list (org-element-property :label fn) (org-element-property :type fn))))"##,
        expect,
    );
}

#[test]
fn ed6_footnote_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1\" \"1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] ")
  (let* ((tree (org-element-parse-buffer))
         (fn (car (org-element-map tree 'footnote-reference (lambda (f) f))))
         (def (car (org-element-map tree 'footnote-definition (lambda (fd) fd)))))
    (list (org-element-property :label fn) (org-element-property :label def))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: timestamp boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_timestamp_active_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (active 2026 15)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<2026-01-15>")
  (let* ((tree (org-element-parse-buffer))
         (ts (car (org-element-map tree 'timestamp (lambda (t) t)))))
    (list (org-element-property :type ts) (org-element-property :year-start ts) (org-element-property :day-start ts))))"##,
        expect,
    );
}

#[test]
fn ed6_timestamp_inactive_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (inactive 14 30)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "[2026-01-15 14:30]")
  (let* ((tree (org-element-parse-buffer))
         (ts (car (org-element-map tree 'timestamp (lambda (t) t)))))
    (list (org-element-property :type ts) (org-element-property :hour-start ts) (org-element-property :minute-start ts))))"##,
        expect,
    );
}

#[test]
fn ed6_timestamp_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (active-range 2026 15 2026 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<2026-01-15>--<2026-01-20>")
  (let* ((tree (org-element-parse-buffer))
         (ts (car (org-element-map tree 'timestamp (lambda (t) t)))))
    (list (org-element-property :type ts) (org-element-property :year-start ts) (org-element-property :day-start ts) (org-element-property :year-end ts) (org-element-property :day-end ts))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: block boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_block_empty_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"emacs-lisp\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (block (car (org-element-map tree 'src-block (lambda (b) b)))))
    (list (org-element-property :language block) (org-element-property :value block))))"##,
        expect,
    );
}

#[test]
fn ed6_block_multiple_languages() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"python\" \"print('hello')\\n\") (\"emacs-lisp\" \"(message \\\"hi\\\")\\n\") (\"shell\" \"echo test\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC python\nprint('hello')\n#+END_SRC\n\n#+BEGIN_SRC emacs-lisp\n(message \"hi\")\n#+END_SRC\n\n#+BEGIN_SRC shell\necho test\n#+END_SRC")
  (org-element-map (org-element-parse-buffer) 'src-block
    (lambda (b) (list (org-element-property :language b) (org-element-property :value b)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: drawer boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_drawer_empty_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:END:")
  (let* ((tree (org-element-parse-buffer))
         (drawer (car (org-element-map tree 'drawer (lambda (d) d)))))
    (org-element-property :drawer-name drawer))"##,
        expect,
    );
}

#[test]
fn ed6_drawer_multiple_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 \"1\" \"5\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:A: 1\n:B: 2\n:C: 3\n:D: 4\n:E: 5\n:END:")
  (goto-char (point-min))
  (let ((props (org-entry-properties nil 'standard)))
    (list (length props) (alist-get "A" props nil nil 'equal) (alist-get "E" props nil nil 'equal))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: link boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_link_no_description() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"https\" \"//example.com\" \"https://example.com\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "See [[https://example.com]]")
  (let* ((tree (org-element-parse-buffer))
         (link (car (org-element-map tree 'link (lambda (l) l)))))
    (list (org-element-property :type link) (org-element-property :path link) (org-element-property :raw-link link))))"##,
        expect,
    );
}

#[test]
fn ed6_link_angle_brackets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"https\" \"//example.com\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<https://example.com>")
  (let* ((tree (org-element-parse-buffer))
         (link (car (org-element-map tree 'link (lambda (l) l)))))
    (list (org-element-property :type link) (org-element-property :path link))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: planning boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_planning_deadline_only() {
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
fn ed6_planning_closed_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil \"[2026-01-15]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* DONE Task\nCLOSED: [2026-01-15]")
  (goto-char (point-min))
  (list (org-entry-get nil "DEADLINE") (org-entry-get nil "SCHEDULED") (org-entry-get nil "CLOSED")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: tag boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_tag_no_tags() {
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
fn ed6_tag_multiple_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"tag1\" \"tag2\" \"tag3\") (\"new1\" \"new2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading :tag1:tag2:tag3:")
  (goto-char (point-min))
  (let ((tags (org-get-tags nil t)))
    (org-set-tags '("new1" "new2"))
    (list tags (org-get-tags nil t))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: priority boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_priority_default() {
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
fn ed6_priority_all_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] High\n* TODO [#B] Medium\n* TODO [#C] Low")
  (goto-char (point-min))
  (list (org-get-priority (char-after))
        (progn (forward-line) (org-get-priority (char-after)))
        (progn (forward-line) (org-get-priority (char-after)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: todo boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_todo_no_keyword() {
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
fn ed6_todo_custom_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"TODO\" #(\"DONE\" 0 4 (org-todo-head \"TODO\")) nil #(\"TODO\" 0 4 (org-todo-head \"TODO\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq org-todo-keywords '((sequence "TODO" "IN-PROGRESS" "REVIEW" "DONE")))
  (insert "* TODO Task")
  (goto-char (point-min))
  (let ((s1 (org-get-todo-state)))
    (org-todo 'right)
    (let ((s2 (org-get-todo-state)))
      (org-todo 'right)
      (let ((s3 (org-get-todo-state)))
        (org-todo 'right)
        (list s1 s2 s3 (org-get-todo-state))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: visibility boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_visibility_single_heading() {
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

// ═══════════════════════════════════════════════════════════════════════
// Edge: property boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_property_empty_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:EMPTY: \n:END:")
  (goto-char (point-min))
  (org-entry-get nil "EMPTY"))"##,
        expect,
    );
}

#[test]
fn ed6_property_overwrite() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"old\" \"new\" \"final\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:VAR: old\n:END:")
  (goto-char (point-min))
  (let ((v1 (org-entry-get nil "VAR")))
    (org-entry-put nil "VAR" "new")
    (let ((v2 (org-entry-get nil "VAR")))
      (org-entry-put nil "VAR" "final")
      (list v1 v2 (org-entry-get nil "VAR")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: macro boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_macro_no_args() {
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
fn ed6_macro_multiple_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Undefined Org macro: greet; aborting\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: greet Hello $1 and $2!\n{{{greet(Alice, Bob)}}}")
  (let ((raw (buffer-string)))
    (org-macro-replace-all org-macro-templates)
    (list raw (buffer-string))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: dynamic block boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_dynamic_block_empty() {
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

// ═══════════════════════════════════════════════════════════════════════
// Edge: structure template boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_structure_template_various() {
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

// ═══════════════════════════════════════════════════════════════════════
// Edge: comment boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_comment_single_line() {
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
fn ed6_comment_multiple_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Comment 1\\nComment 2\\nComment 3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "# Comment 1\n# Comment 2\n# Comment 3")
  (org-element-map (org-element-parse-buffer) 'comment
    (lambda (c) (org-element-property :value c))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: fixed-width boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_fixed_width_single() {
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
fn ed6_fixed_width_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Line 1\\nLine 2\\nLine 3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert ": Line 1\n: Line 2\n: Line 3")
  (org-element-map (org-element-parse-buffer) 'fixed-width
    (lambda (f) (org-element-property :value f))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: export boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_export_empty_buffer() {
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
fn ed6_export_title_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"My Title\" 0 8 (:parent (#(\"My Title\" 0 8 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: My Title")
  (let* ((info (org-export-get-environment nil)))
    (plist-get info :title)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: element boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_element_buffer_start() {
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
fn ed6_element_buffer_end() {
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

// ═══════════════════════════════════════════════════════════════════════
// Edge: clock boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_clock_no_clock() {
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

// ═══════════════════════════════════════════════════════════════════════
// Edge: statistics boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_statistics_no_cookies() {
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
fn ed6_statistics_all_checked() {
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

// ═══════════════════════════════════════════════════════════════════════
// Edge: sparse tree boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_sparse_tree_no_match() {
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

// ═══════════════════════════════════════════════════════════════════════
// Edge: table transpose boundary conditions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ed6_table_single_row() {
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
fn ed6_table_single_column() {
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
