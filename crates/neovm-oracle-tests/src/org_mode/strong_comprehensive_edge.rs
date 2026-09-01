//! Strong comprehensive edge-case oracle tests — full coverage.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Comprehensive edge: all element types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_element_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (3 . 8) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading\n:PROPERTIES:\n:VAR: val\n:END:\nBody text\n- List item\n| table |\n#+BEGIN_SRC\n(+ 1 2)\n#+END_SRC\n# comment\n: fixed-width\n#+TITLE: Test")
  (let* ((tree (org-element-parse-buffer))
         (types (org-element-map tree (lambda (el) (org-element-type el)))))
    types))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Comprehensive edge: all planning types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_planning_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((timestamp (:standard-properties [24 nil nil nil 36 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2026-01-15>\" :year-start 2026 :month-start 1 :day-start 15 :hour-start nil :minute-start nil :year-end 2026 :month-end 1 :day-end 15 :hour-end nil :minute-end nil)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\nCLOSED: [2026-01-10]")
  (let* ((tree (org-element-parse-buffer))
         (planning (car (org-element-map tree 'planning
                         (lambda (p) p)))))
    (list (org-element-property :scheduled planning)
          (org-element-property :deadline planning)
          (org-element-property :closed planning))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Comprehensive edge: all block types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_block_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((src-block \"(+ 1 2)\\n\") (example-block \"Example\\n\") (quote-block nil) (center-block nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n\n#+BEGIN_EXAMPLE\nExample\n#+END_EXAMPLE\n\n#+BEGIN_QUOTE\nQuote\n#+END_QUOTE\n\n#+BEGIN_CENTER\nCenter\n#+END_CENTER")
  (let* ((tree (org-element-parse-buffer))
         (blocks (org-element-map tree '(src-block example-block quote-block center-block)
                   (lambda (b)
                     (list (org-element-type b)
                           (org-element-property :value b))))))
    blocks))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Comprehensive edge: all link types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_link_types() {
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
// Comprehensive edge: all timestamp types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_timestamp_types() {
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
// Comprehensive edge: all list types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_list_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((unordered 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- plain\n1. ordered\n+ bullet\n- another plain\n2. ordered 2")
  (let* ((tree (org-element-parse-buffer))
         (lists (org-element-map tree 'plain-list
                  (lambda (pl)
                    (list (org-element-property :type pl)
                          (length (org-element-contents pl)))))))
    lists))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Comprehensive edge: all drawer types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_drawer_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"LOGBOOK\" 33 56) (\"CUSTOM\" 56 78))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:A: 1\n:END:\n:LOGBOOK:\n- Note\n:END:\n:CUSTOM:\n- Data\n:END:\nBody")
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
// Comprehensive edge: all keyword types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_keyword_types() {
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
// Comprehensive edge: all comment types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_comment_types() {
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
// Comprehensive edge: all fixed-width types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_fixed_width_types() {
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
// Comprehensive edge: all affiliated keywords
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_affiliated_keywords() {
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
// Comprehensive edge: all property types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_property_types() {
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
// Comprehensive edge: all tag types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_tag_types() {
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
// Comprehensive edge: all priority types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_priority_types() {
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
// Comprehensive edge: all todo states
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_todo_states() {
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
// Comprehensive edge: all visibility states
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_visibility_states() {
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
// Comprehensive edge: all macro types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_macro_types() {
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
// Comprehensive edge: all dynamic block types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_dynamic_block_types() {
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
// Comprehensive edge: all structure template types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_structure_template_types() {
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
// Comprehensive edge: all entity types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_entity_types() {
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
// Comprehensive edge: all radio target types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_radio_target_types() {
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
// Comprehensive edge: all statistics types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_statistics_types() {
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
// Comprehensive edge: all sparse tree types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_sparse_tree_types() {
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
// Comprehensive edge: all table formula types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_table_formula_types() {
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

// ═══════════════════════════════════════════════════════════════════════
// Comprehensive edge: all outline path types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_outline_path_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Project\" \"Task 1\" \"Subtask 1.1\") 4 \"Subsub 1.1.1\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Project\n** Task 1\n*** Subtask 1.1\n**** Subsub 1.1.1\n** Task 2")
  (goto-char (point-min))
  (search-forward "Subsub 1.1.1")
  (let ((path (org-get-outline-path))
        (level (org-current-level))
        (title (org-get-heading t t t t)))
    (list path level title)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Comprehensive edge: all refile target types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_refile_target_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Project A\" \"Project B\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Project A\n** Task 1\n** Task 2\n* Project B\n** Task 3")
  (let ((targets (org-refile-get-targets nil)))
    (mapcar (lambda (t) (car t)) targets)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Comprehensive edge: all agenda todo types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_agenda_todo_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task 1\n* DONE Task 2\n* TODO Task 3\n* WAITING Task 4")
  (let ((entries (org-map-entries
                  (lambda ()
                    (list (org-get-heading t t t t)
                          (org-get-todo-state)
                          (org-entry-get nil "PRIORITY")))
                  nil 'file)))
    entries))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Comprehensive edge: all colview format types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_colview_format_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-columns-get-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %3PRIORITY %TAGS %VAR\n* TODO [#A] Test :tag:")
  (goto-char (point-min))
  (let ((fmt (org-columns-get-format)))
    fmt))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Comprehensive edge: all pcomplete entity types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_pcomplete_entity_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "\\agr")
  (let ((completions (all-completions "\\ag" (pcomplete-entries))))
    (length completions)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Comprehensive edge: all parse consistency types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_all_parse_consistency_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"Buffer 1\" \"Sub 1\") (\"Buffer 2\" \"Sub 2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((results '()))
  (with-temp-buffer
    (org-mode)
    (insert "* Buffer 1\n** Sub 1\nBody 1")
    (let ((tree (org-element-parse-buffer)))
      (push (org-element-map tree 'headline
              (lambda (h) (org-element-property :raw-value h)))
            results)))
  (with-temp-buffer
    (org-mode)
    (insert "* Buffer 2\n** Sub 2\nBody 2")
    (let ((tree (org-element-parse-buffer)))
      (push (org-element-map tree 'headline
              (lambda (h) (org-element-property :raw-value h)))
            results)))
  (nreverse results))"##,
        expect,
    );
}
