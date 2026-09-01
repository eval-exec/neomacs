//! Strong divergence-hunt oracle tests — targeted at known bugs.
//!
//! Every test returns concrete structured data to surface divergences.
//! Specifically targets:
//! - parent property nesting in title/caption
//! - drawer parsing "Invalid search bound"
//! - clock-in "Invalid search bound"
//! - minibuffer message differences

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Divergence: parent nesting in various element types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_parent_nesting_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Test heading\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test heading")
  (goto-char (point-min))
  (let* ((el (org-element-at-point))
         (title (org-element-property :raw-value el)))
    title))"##,
        expect,
    );
}

#[test]
fn strong_parent_nesting_keyword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"My Title\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: My Title")
  (let* ((tree (org-element-parse-buffer))
         (kw (car (org-element-map tree 'keyword (lambda (k) k))))
         (val (org-element-property :value kw)))
    val))"##,
        expect,
    );
}

#[test]
fn strong_parent_nesting_caption() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((#(\"My caption\" 0 10 (:parent (#(\"My caption\" 0 10 (:parent #5))))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CAPTION: My caption\n[[file:test.png]]")
  (let* ((tree (org-element-parse-buffer))
         (link (car (org-element-map tree 'link (lambda (l) l))))
         (parent (org-element-property :parent link))
         (caption (org-element-property :caption parent)))
    caption))"##,
        expect,
    );
}

#[test]
fn strong_parent_nesting_title_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"Export Title\" 0 12 (:parent (#(\"Export Title\" 0 12 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Export Title\n* H1\n** H2")
  (let* ((tree (org-element-parse-buffer))
         (info (org-export-get-environment nil))
         (title (plist-get info :title)))
    title))"##,
        expect,
    );
}

#[test]
fn strong_parent_nesting_multiple_headlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"H1\" \"H2\" \"H3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3")
  (let* ((tree (org-element-parse-buffer))
         (titles (org-element-map tree 'headline
                   (lambda (h) (org-element-property :raw-value h)))))
    titles))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Divergence: drawer parsing with various content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_drawer_parse_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:END:\nBody")
  (let* ((tree (org-element-parse-buffer))
         (drawers (org-element-map tree 'drawer
                    (lambda (d) (org-element-property :drawer-name d)))))
    drawers))"##,
        expect,
    );
}

#[test]
fn strong_drawer_parse_with_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:\nBody")
  (let* ((tree (org-element-parse-buffer))
         (drawers (org-element-map tree 'drawer
                    (lambda (d) (org-element-property :drawer-name d)))))
    drawers))"##,
        expect,
    );
}

#[test]
fn strong_drawer_parse_logbook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"LOGBOOK\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:LOGBOOK:\n- Note\n:END:\nBody")
  (let* ((tree (org-element-parse-buffer))
         (drawers (org-element-map tree 'drawer
                    (lambda (d) (org-element-property :drawer-name d)))))
    drawers))"##,
        expect,
    );
}

#[test]
fn strong_drawer_parse_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"LOGBOOK\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:\n:LOGBOOK:\n- Note\n:END:\nBody")
  (let* ((tree (org-element-parse-buffer))
         (drawers (org-element-map tree 'drawer
                    (lambda (d) (org-element-property :drawer-name d)))))
    drawers))"##,
        expect,
    );
}

#[test]
fn strong_drawer_parse_properties_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:B: 2\n:END:")
  (let* ((tree (org-element-parse-buffer))
         (drawers (org-element-map tree 'drawer
                    (lambda (d)
                      (list (org-element-property :drawer-name d)
                            (org-element-property :begin d)
                            (org-element-property :end d))))))
    drawers))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Divergence: clock-in with various states
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_clock_in_todo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clocking-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Test")
  (goto-char (point-min))
  (let ((clocking (org-clocking-p)))
    clocking))"##,
        expect,
    );
}

#[test]
fn strong_clock_in_out() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clocking-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Test")
  (goto-char (point-min))
  (let ((c0 (org-clocking-p)))
    (org-clock-in)
    (let ((c1 (org-clocking-p)))
      (org-clock-out)
      (let ((c2 (org-clocking-p)))
        (list c0 c1 c2)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Divergence: export environment with various keywords
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_env_title_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"Only Title\" 0 10 (:parent (#(\"Only Title\" 0 10 (:parent #3))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Only Title")
  (let* ((info (org-export-get-environment nil))
         (title (plist-get info :title)))
    title))"##,
        expect,
    );
}

#[test]
fn strong_export_env_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"T\" 0 1 (:parent (#(\"T\" 0 1 (:parent #4)))))) (#(\"A\" 0 1 (:parent (#(\"A\" 0 1 (:parent #4)))))) \"e\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T\n#+AUTHOR: A\n#+EMAIL: e\n#+OPTIONS: toc:nil")
  (let* ((info (org-export-get-environment nil)))
    (list (plist-get info :title)
          (plist-get info :author)
          (plist-get info :email)
          (plist-get info :with-toc))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Divergence: element map with various types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_map_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"H1\" \"H2\" \"H3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))))"##,
        expect,
    );
}

#[test]
fn strong_element_map_keyword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"TITLE\" \"T\") (\"AUTHOR\" \"A\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T\n#+AUTHOR: A")
  (org-element-map (org-element-parse-buffer) 'keyword
    (lambda (k) (list (org-element-property :key k)
                      (org-element-property :value k)))))"##,
        expect,
    );
}

#[test]
fn strong_element_map_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"https\" \"//x\") (\"file\" \"f.org\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "[[https://x][web]] [[file:f.org][file]]")
  (org-element-map (org-element-parse-buffer) 'link
    (lambda (l) (list (org-element-property :type l)
                      (org-element-property :path l)))))"##,
        expect,
    );
}

#[test]
fn strong_element_map_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Para 1\n\nPara 2")
  (org-element-map (org-element-parse-buffer) 'paragraph
    (lambda (p) (org-element-property :value p))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Divergence: property access patterns
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_property_access_standard() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"CATEGORY\" . \"???\") (\"B\" . \"2\") (\"A\" . \"1\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:B: 2\n:END:")
  (goto-char (point-min))
  (org-entry-properties nil 'standard))"##,
        expect,
    );
}

#[test]
fn strong_property_access_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"1\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:")
  (goto-char (point-min))
  (org-entry-get nil "A"))"##,
        expect,
    );
}

#[test]
fn strong_property_access_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"parent\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* P\n:PROPERTIES:\n:V: parent\n:END:\n** C")
  (goto-char (point-min))
  (search-forward "C")
  (org-entry-get nil "V" 'inherit))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Divergence: tag operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_tag_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" \"b\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H :a:b:")
  (goto-char (point-min))
  (org-get-tags nil t))"##,
        expect,
    );
}

#[test]
fn strong_tag_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" \"b\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (goto-char (point-min))
  (org-set-tags '("a" "b"))
  (org-get-tags nil t))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Divergence: todo operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_todo_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"TODO\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T")
  (goto-char (point-min))
  (org-get-todo-state))"##,
        expect,
    );
}

#[test]
fn strong_todo_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"TODO\" #(\"DONE\" 0 4 (org-todo-head \"TODO\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T")
  (goto-char (point-min))
  (let ((s1 (org-get-todo-state)))
    (org-todo 'right)
    (let ((s2 (org-get-todo-state)))
      (list s1 s2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Divergence: priority operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_priority_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] T")
  (goto-char (point-min))
  (org-get-priority (char-after)))"##,
        expect,
    );
}

#[test]
fn strong_priority_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] T")
  (goto-char (point-min))
  (let ((p1 (org-get-priority (char-after))))
    (org-priority 'down)
    (list p1 (org-get-priority (char-after)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Divergence: heading operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_heading_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Title\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Title :tag:")
  (goto-char (point-min))
  (org-get-heading t t t t))"##,
        expect,
    );
}

#[test]
fn strong_heading_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Changed\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Original")
  (goto-char (point-min))
  (org-edit-headline "Changed")
  (org-get-heading t t t t))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Divergence: planning operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_planning_deadline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"<2026-01-20>\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\nDEADLINE: <2026-01-20>")
  (goto-char (point-min))
  (org-entry-get nil "DEADLINE"))"##,
        expect,
    );
}

#[test]
fn strong_planning_scheduled() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"<2026-01-15>\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\nSCHEDULED: <2026-01-15>")
  (goto-char (point-min))
  (org-entry-get nil "SCHEDULED"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Divergence: table operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_table_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"a\" \"b\") (\"1\" \"2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (org-table-to-lisp))"##,
        expect,
    );
}

#[test]
fn strong_table_formula() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"1\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"3\" 0 1 (face org-table))) (#(\"3\" 0 1 (face org-table)) #(\"4\" 0 1 (face org-table)) #(\"7\" 0 1 (face org-table))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| 1 | 2 |\n| 3 | 4 |\n#+TBLFM: $3=$1+$2")
  (goto-char (point-min))
  (org-table-recalculate 'all)
  (org-table-to-lisp))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Divergence: list operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_list_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"- \" \"- \" \"- \")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- item 1\n- item 2\n  - sub 1")
  (org-element-map (org-element-parse-buffer) 'item
    (lambda (it) (org-element-property :bullet it))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Divergence: visibility operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_visibility_overview() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody")
  (goto-char (point-min))
  (org-set-startup-visibility 'overview)
  (get-char-property (search-forward "H2") 'invisible))"##,
        expect,
    );
}

#[test]
fn strong_visibility_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody")
  (goto-char (point-min))
  (org-set-startup-visibility 'all)
  (get-char-property (search-forward "H2") 'invisible))"##,
        expect,
    );
}
