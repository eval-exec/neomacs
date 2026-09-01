//! Strong advanced edge-case oracle tests — unusual but valid inputs.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Edge: maximum nesting depth
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_max_nesting_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 \"L10\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* L1\n** L2\n*** L3\n**** L4\n***** L5\n****** L6\n******* L7\n******** L8\n********* L9\n********** L10")
  (goto-char (point-max))
  (let ((level (org-current-level))
        (title (org-get-heading t t t t)))
    (list level title)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: empty lines between elements
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_empty_lines_between_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"Heading 1\" \"Heading 2\") nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading 1\n\n\nBody text\n\n\n* Heading 2\n\n\nMore body")
  (let* ((tree (org-element-parse-buffer))
         (headlines (org-element-map tree 'headline
                      (lambda (h) (org-element-property :raw-value h))))
         (paragraphs (org-element-map tree 'paragraph
                       (lambda (p) (org-element-property :value p)))))
    (list headlines paragraphs)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: mixed line endings
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_mixed_content_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (3 . 8) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading\n: drawer:\n: value:\n- list item\n| table |\n#+BEGIN_SRC\n(+ 1 2)\n#+END_SRC\n# comment\n: fixed-width")
  (let* ((tree (org-element-parse-buffer))
         (types (org-element-map tree (lambda (el) (org-element-type el)))))
    types))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: special characters in various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_special_chars_everywhere() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Headline with *bold* and /italic/\" (\"tag1\" \"tag2\") \"value with spaces\" (\"link\") (\"1\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Headline with *bold* and /italic/ :tag1:tag2:\n:PROPERTIES:\n:VAR: value with spaces\n:END:\nBody with [[link][desc]] and footnote[fn:1].\n\n[fn:1] Footnote with *markup*.")
  (let* ((tree (org-element-parse-buffer))
         (headline (car (org-element-map tree 'headline (lambda (h) h))))
         (links (org-element-map tree 'link
                  (lambda (l) (org-element-property :path l))))
         (footnotes (org-element-map tree 'footnote-reference
                      (lambda (fn) (org-element-property :label fn)))))
    (list (org-element-property :raw-value headline)
          (org-element-property :tags headline)
          (org-entry-get nil "VAR")
          links footnotes)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: unicode in various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_unicode_everywhere() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (((\"任务标题\" (\"标签\")) (\"子标题\" nil)) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO 任务标题 :标签:\n:PROPERTIES:\n:VAR: 值\n:END:\n正文内容\n** 子标题\n- 列表项\n| 表格 |")
  (let* ((tree (org-element-parse-buffer))
         (headlines (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :raw-value h)
                              (org-element-property :tags h)))))
         (var (org-entry-get nil "VAR")))
    (list headlines var)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: very long content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_very_long_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"TODO\" 170)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO This is a very long headline with many words and characters that goes on and on and on and on and on and on and on and on and on and on and on and on and on and on and on")
  (goto-char (point-min))
  (let ((title (org-get-heading t t t t))
        (todo (org-get-todo-state)))
    (list todo (length title))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: table with many columns
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_table_many_columns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| A | B | C | D | E | F | G | H | I | J |\n| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |")
  (let* ((tree (org-element-parse-buffer))
         (cells (org-element-map tree 'table-cell
                  (lambda (c) (org-element-property :value c)))))
    (list (length cells) cells)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: list with many items
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_list_many_items() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (10 (\"- \" \"- \" \"- \" \"- \" \"- \" \"- \" \"- \" \"- \" \"- \" \"- \"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- Item 1\n- Item 2\n- Item 3\n- Item 4\n- Item 5\n- Item 6\n- Item 7\n- Item 8\n- Item 9\n- Item 10")
  (let* ((tree (org-element-parse-buffer))
         (items (org-element-map tree 'item
                  (lambda (it) (org-element-property :bullet it)))))
    (list (length items) items)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: multiple blocks of same type
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_multiple_blocks_same_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"emacs-lisp\" \"(+ 1 2)\\n\") (\"python\" \"print('hello')\\n\") (\"shell\" \"echo test\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n\n#+BEGIN_SRC python\nprint('hello')\n#+END_SRC\n\n#+BEGIN_SRC shell\necho test\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (blocks (org-element-map tree 'src-block
                   (lambda (b)
                     (list (org-element-property :language b)
                           (org-element-property :value b))))))
    blocks))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: multiple drawers
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_multiple_drawers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"LOGBOOK\" 33 58) (\"CUSTOM\" 58 87))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:A: 1\n:END:\n:LOGBOOK:\n- Note 1\n:END:\n:CUSTOM:\n- Custom data\n:END:\nBody")
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
// Edge: timestamps with various formats
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_timestamps_various_formats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((active 2026 15 nil nil) (active 2026 15 nil nil) (active 2026 15 10 0) (active 2026 15 10 0) (inactive 2026 15 nil nil) (inactive 2026 15 nil nil) (inactive 2026 15 10 0) (active-range 2026 15 nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<2026-01-15>\n<2026-01-15 Wed>\n<2026-01-15 10:00>\n<2026-01-15 Wed 10:00>\n[2026-01-15]\n[2026-01-15 Wed]\n[2026-01-15 10:00]\n<2026-01-15>--<2026-01-20>")
  (let* ((tree (org-element-parse-buffer))
         (timestamps (org-element-map tree 'timestamp
                       (lambda (ts)
                         (list (org-element-property :type ts)
                               (org-element-property :year-start ts)
                               (org-element-property :day-start ts)
                               (org-element-property :hour-start ts)
                               (org-element-property :minute-start ts))))))
    timestamps))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: links with various types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_links_various_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"https\" \"//example.com\" \"https://example.com\") (\"file\" \"test.org\" \"file:test.org\") (\"id\" \"abc123\" \"id:abc123\") (\"elisp\" \"(message \\\"hi\\\")\" \"elisp:(message \\\"hi\\\")\") (\"mailto\" \"test@example.com\" \"mailto:test@example.com\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "See [[https://example.com][web]] and [[file:test.org][file]] and [[id:abc123][id]] and [[elisp:(message \"hi\")][elisp]] and [[mailto:test@example.com][email]]")
  (let* ((tree (org-element-parse-buffer))
         (links (org-element-map tree 'link
                  (lambda (l)
                    (list (org-element-property :type l)
                          (org-element-property :path l)
                          (org-element-property :raw-link l))))))
    links))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: keywords with various keys
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_keywords_various_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TITLE\" \"Test\") (\"AUTHOR\" \"Author\") (\"EMAIL\" \"test@example.com\") (\"DATE\" \"2026-01-15\") (\"DESCRIPTION\" \"Desc\") (\"KEYWORDS\" \"kw1 kw2\") (\"LANGUAGE\" \"en\") (\"SELECT_TAGS\" \"export\") (\"EXCLUDE_TAGS\" \"noexport\") (\"OPTIONS\" \"toc:nil num:nil\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n#+AUTHOR: Author\n#+EMAIL: test@example.com\n#+DATE: 2026-01-15\n#+DESCRIPTION: Desc\n#+KEYWORDS: kw1 kw2\n#+LANGUAGE: en\n#+SELECT_TAGS: export\n#+EXCLUDE_TAGS: noexport\n#+OPTIONS: toc:nil num:nil")
  (let* ((tree (org-element-parse-buffer))
         (keywords (org-element-map tree 'keyword
                     (lambda (k)
                       (list (org-element-property :key k)
                             (org-element-property :value k))))))
    keywords))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: comments with various content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_comments_various_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Simple comment\\nComment with *markup*\\nComment with [[link]]\\nComment with special chars: <>&\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "# Simple comment\n# Comment with *markup*\n# Comment with [[link]]\n# Comment with special chars: <>&\"")
  (let* ((tree (org-element-parse-buffer))
         (comments (org-element-map tree 'comment
                     (lambda (c) (org-element-property :value c)))))
    comments))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: fixed-width with various content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_fixed_width_various_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Simple\\nWith *markup*\\nWith [[link]]\\nWith special: <>&\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert ": Simple\n: With *markup*\n: With [[link]]\n: With special: <>&\"")
  (let* ((tree (org-element-parse-buffer))
         (fw (org-element-map tree 'fixed-width
               (lambda (f) (org-element-property :value f)))))
    fw))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: affiliated keywords with various content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_affiliated_keywords_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (paragraph (((#(\"My caption\" 0 10 (:parent (#(\"My caption\" 0 10 (:parent #6)))))))) (\":width 300px :class thumbnail\") (\":width 0.5\\\\textwidth\") \"my-fig\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CAPTION: My caption\n#+ATTR_HTML: :width 300px :class thumbnail\n#+NAME: my-fig\n#+ATTR_LATEX: :width 0.5\\textwidth\n[[file:image.png]]")
  (let* ((tree (org-element-parse-buffer))
         (link (car (org-element-map tree 'link (lambda (l) l))))
         (parent (org-element-property :parent link)))
    (list (org-element-type parent)
          (org-element-property :caption parent)
          (org-element-property :attr_html parent)
          (org-element-property :attr_latex parent)
          (org-element-property :name parent))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: blocks with various languages
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_blocks_various_languages() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"emacs-lisp\" \"(+ 1 2)\\n\") (\"python\" \"print('hello')\\n\") (\"shell\" \"echo test\\n\") (\"C\" \"int main() { return 0; }\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n\n#+BEGIN_SRC python\nprint('hello')\n#+END_SRC\n\n#+BEGIN_SRC shell\necho test\n#+END_SRC\n\n#+BEGIN_SRC C\nint main() { return 0; }\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (blocks (org-element-map tree 'src-block
                   (lambda (b)
                     (list (org-element-property :language b)
                           (org-element-property :value b))))))
    blocks))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: planning with all combinations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_planning_all_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((timestamp (:standard-properties [26 nil nil nil 38 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2026-01-15>\" :year-start 2026 :month-start 1 :day-start 15 :hour-start nil :minute-start nil :year-end 2026 :month-end 1 :day-end 15 :hour-end nil :minute-end nil)) nil nil) (nil (timestamp (:standard-properties [63 nil nil nil 75 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2026-01-20>\" :year-start 2026 :month-start 1 :day-start 20 :hour-start nil :minute-start nil :year-end 2026 :month-end 1 :day-end 20 :hour-end nil :minute-end nil)) nil) ((timestamp (:standard-properties [101 nil nil nil 113 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2026-01-15>\" :year-start 2026 :month-start 1 :day-start 15 :hour-start nil :minute-start nil :year-end 2026 :month-end 1 :day-end 15 :hour-end nil :minute-end nil)) nil nil) (nil nil (timestamp (:standard-properties [159 nil nil nil 171 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type inactive :range-type nil :raw-value \"[2026-01-10]\" :year-start 2026 :month-start 1 :day-start 10 :hour-start nil :minute-start nil :year-end 2026 :month-end 1 :day-end 10 :hour-end nil :minute-end nil))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task 1\nSCHEDULED: <2026-01-15>\n* TODO Task 2\nDEADLINE: <2026-01-20>\n* TODO Task 3\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\n* DONE Task 4\nCLOSED: [2026-01-10]")
  (let* ((tree (org-element-parse-buffer))
         (planning (org-element-map tree 'planning
                     (lambda (p)
                       (list (org-element-property :scheduled p)
                             (org-element-property :deadline p)
                             (org-element-property :closed p))))))
    planning))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: properties with various values
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_properties_various_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\" \"value with spaces\" \"a:b:c\" \"<>&\\\"\" \"12345\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:EMPTY: \n:SPACE: value with spaces\n:COLON: a:b:c\n:SPECIAL: <>&\"\n:NUMERIC: 12345\n:END:")
  (goto-char (point-min))
  (let ((props (org-entry-properties nil 'standard)))
    (list (alist-get "EMPTY" props nil nil 'equal)
          (alist-get "SPACE" props nil nil 'equal)
          (alist-get "COLON" props nil nil 'equal)
          (alist-get "SPECIAL" props nil nil 'equal)
          (alist-get "NUMERIC" props nil nil 'equal))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: tags with various names
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_tags_various_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"tag1\" \"tag2\" \"tag3\") (\"new1\" \"new2\" \"new3\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading :tag1:tag2:tag3:")
  (goto-char (point-min))
  (let ((tags (org-get-tags nil t)))
    (org-set-tags '("new1" "new2" "new3"))
    (let ((tags2 (org-get-tags nil t)))
      (list tags tags2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: priorities with various levels
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_priorities_various_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] High\n* TODO [#B] Medium\n* TODO [#C] Low\n* TODO No priority")
  (goto-char (point-min))
  (let ((priorities '()))
    (dotimes (_ 4)
      (push (org-get-priority (char-after)) priorities)
      (forward-line))
    (nreverse priorities)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: todo states with various keywords
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_todo_states_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"TODO\" #(\"DONE\" 0 4 (org-todo-head \"TODO\")) nil #(\"TODO\" 0 4 (org-todo-head \"TODO\")) #(\"DONE\" 0 4 (org-todo-head \"TODO\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq org-todo-keywords '((sequence "TODO" "IN-PROGRESS" "REVIEW" "DONE")))
  (insert "* TODO Task")
  (goto-char (point-min))
  (let ((states '()))
    (dotimes (_ 4)
      (push (org-get-todo-state) states)
      (org-todo 'right))
    (push (org-get-todo-state) states)
    (nreverse states)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: visibility with various states
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_visibility_various_states() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody")
  (goto-char (point-min))
  (let ((states '()))
    ;; overview
    (org-set-startup-visibility 'overview)
    (push (get-char-property (search-forward "H2") 'invisible) states)
    ;; content
    (org-set-startup-visibility 'content)
    (push (get-char-property (search-forward "H2") 'invisible) states)
    ;; all
    (org-set-startup-visibility 'all)
    (push (get-char-property (search-forward "H2") 'invisible) states)
    (nreverse states)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: macros with various arguments
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_macros_various_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Undefined Org macro: greet; aborting\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: greet Hello $1 and $2!\n{{{greet(Alice, Bob)}}}\n{{{greet(World, 42)}}}")
  (let ((raw (buffer-string)))
    (org-macro-replace-all org-macro-templates)
    (let ((expanded (buffer-string)))
      (list raw expanded))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: dynamic blocks with various parameters
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_dynamic_blocks_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK #(\"#+BEGIN: clocktable :maxlevel 2\\n#+CAPTION: Clock summary at [2026-06-15 Mon 12:00]\\n| Headline     | Time   |\\n|--------------+--------|\\n| *Total time* | *0:00* |\\n#+END:\" 83 84 (face org-table) 84 85 (face org-table rear-nonsticky t display (space :relative-width 1)) 85 93 (face org-table) 93 97 (face org-table) 97 98 (face org-table display (space :relative-width 1.001)) 98 99 (face org-table) 99 100 (face org-table rear-nonsticky t display (space :relative-width 1)) 100 104 (face org-table) 104 106 (face org-table) 106 107 (face org-table display (space :relative-width 1.001)) 107 108 (face org-table) 108 109 (face org-table-row) 109 110 (face org-table) 110 134 (face org-table) 134 135 (face org-table-row) 135 136 (face org-table) 136 137 (face org-table rear-nonsticky t display (space :relative-width 1)) 137 149 (org-emphasis t font-lock-multiline t face (bold org-table)) 149 150 (face org-table display (space :relative-width 1.001)) 150 151 (face org-table) 151 152 (face org-table rear-nonsticky t display (space :relative-width 1)) 152 158 (org-emphasis t font-lock-multiline t face (bold org-table)) 158 159 (face org-table display (space :relative-width 1.001)) 159 160 (face org-table) 160 161 (face org-table-row))""##
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN: clocktable :maxlevel 2\n#+END:")
  (goto-char (point-min))
  (org-dblock-update)
  (let ((content (buffer-string)))
    content))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: structure templates with various types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_structure_templates_various() {
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
    (let ((s2 (buffer-string)))
      (erase-buffer)
      (insert "<q")
      (org-try-structure-completion)
      (let ((s3 (buffer-string)))
        (list s1 s2 s3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: entities with various names
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_entities_various_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\alpha \\\\beta \\\\gamma \\\\delta \\\\epsilon\" \"\\\\alpha \\\\beta \\\\gamma \\\\delta \\\\epsilon\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "\\alpha \\beta \\gamma \\delta \\epsilon")
  (let ((before (buffer-string)))
    (org-toggle-pretty-entities)
    (let ((after (buffer-string)))
      (list before after))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: radio targets with various names
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_radio_targets_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"target1\" \"target2\" \"target3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<<<target1>>>\n<<<target2>>>\n<<<target3>>>")
  (let* ((tree (org-element-parse-buffer))
         (targets (org-element-map tree 'radio-target
                    (lambda (rt) (org-element-property :value rt)))))
    targets))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: statistics with various formats
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_statistics_various_formats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"* Task [66%]\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task [%]\n- [X] item 1\n- [ ] item 2\n- [X] item 3")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (let ((h (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
    h))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: sparse tree with various match strings
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_sparse_tree_various_matches() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Task 1\" \"Task 2\" \"Task 3\" \"WAITING Task 4\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task 1 :work:\n* DONE Task 2 :personal:\n* TODO Task 3 :work:\n* WAITING Task 4")
  (goto-char (point-min))
  (org-match-sparse-tree nil "TODO")
  (let ((visible '())
        (hidden '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((h (org-get-heading t t t t)))
        (when h
          (if (get-char-property (point) 'invisible)
              (push h hidden)
            (push h visible))))
      (forward-line))
    (list (nreverse visible) (nreverse hidden))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Edge: table formulas with various operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_table_formulas_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"1\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"3\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table))) (#(\"3\" 0 1 (face org-table)) #(\"4\" 0 1 (face org-table)) #(\"7\" 0 1 (face org-table)) #(\"12\" 0 2 (face org-table))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| 1 | 2 |\n| 3 | 4 |\n#+TBLFM: $3=$1+$2::$4=$1*$2")
  (goto-char (point-min))
  (org-table-recalculate 'all)
  (let ((data (org-table-to-lisp)))
    data))"##,
        expect,
    );
}
