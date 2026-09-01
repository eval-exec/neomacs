//! Strong multi-step workflow oracle tests — real-world editing sequences.
//!
//! Every test captures deep mutable state after multiple operations.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Workflow: create document from scratch
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_create_document_from_scratch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Project Plan\" 0 12 (:parent (#(\"Project Plan\" 0 12 (:parent #4)))))) ((\"Phase 1\" \"TODO\" nil) (\"Design\" \"TODO\" nil) (\"Implementation\" \"TODO\" nil) (\"Phase 2\" \"DONE\" nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  ;; Build document step by step
  (insert "#+TITLE: Project Plan\n#+AUTHOR: Team\n")
  (insert "* TODO Phase 1\n")
  (insert "** TODO Design\nSCHEDULED: <2026-02-01>\n")
  (insert "** TODO Implementation\nDEADLINE: <2026-03-01>\n")
  (insert "* DONE Phase 2\nCLOSED: [2026-01-10]\n")
  ;; Read back all state
  (let* ((tree (org-element-parse-buffer))
         (info (org-export-get-environment nil))
         (headlines (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :raw-value h)
                              (org-element-property :todo-keyword h)
                              (org-element-property :tags h))))))
    (list (plist-get info :title) headlines)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: modify existing document
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_modify_existing_document() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"Original\" \"TODO\") (\"Sub\" \"TODO\")) ((\"Modified\" \"DONE\" (\"done\")) (\"Sub\" \"TODO\" nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Original\nBody text\n** TODO Sub")
  ;; Read initial state
  (let* ((tree1 (org-element-parse-buffer))
         (h1 (org-element-map tree1 'headline
               (lambda (h) (list (org-element-property :raw-value h)
                                 (org-element-property :todo-keyword h))))))
    ;; Modify
    (goto-char (point-min))
    (org-edit-headline "Modified")
    (org-todo 'done)
    (org-set-tags '("done"))
    ;; Read modified state
    (let* ((tree2 (org-element-parse-buffer))
           (h2 (org-element-map tree2 'headline
                 (lambda (h) (list (org-element-property :raw-value h)
                                   (org-element-property :todo-keyword h)
                                   (org-element-property :tags h))))))
      (list h1 h2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: table manipulation sequence
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_table_manipulation_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a table\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| A | 1 |\n| B | 2 |\n| C | 3 |")
  (goto-char (point-min))
  ;; Initial state
  (let ((d1 (org-table-to-lisp)))
    ;; Add formula
    (goto-char (point-max))
    (insert "\n#+TBLFM: $3=$2*10")
    (org-table-recalculate 'all)
    (let ((d2 (org-table-to-lisp)))
      ;; Sort by first column
      (org-table-sort-lines nil ?a)
      (let ((d3 (org-table-to-lisp)))
        ;; Transpose
        (org-table-transpose)
        (let ((d4 (org-table-to-lisp)))
          (list d1 d2 d3 d4))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: checkbox hierarchy management
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_checkbox_hierarchy_management() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"* Task [0%]\" \"* Task [0%]\" \"* Task [100%]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task [%]\n- [ ] parent\n  - [ ] child1\n  - [ ] child2\n  - [ ] child3")
  (goto-char (point-min))
  ;; Initial stats
  (org-update-statistics-cookies t)
  (let ((h0 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
    ;; Check child1
    (forward-line 2)
    (org-toggle-checkbox)
    ;; Check child2
    (forward-line 1)
    (org-toggle-checkbox)
    ;; Update stats
    (org-update-statistics-cookies t)
    (goto-char (point-min))
    (let ((h1 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
      ;; Check child3
      (forward-line 4)
      (org-toggle-checkbox)
      (org-update-statistics-cookies t)
      (goto-char (point-min))
      (let ((h2 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
        (list h0 h1 h2)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: sparse tree filtering
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_sparse_tree_filtering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A :work:\n* DONE B :personal:\n* TODO C :work:\n** TODO D :work:\n** DONE E\n* WAITING F")
  (goto-char (point-min))
  ;; Filter by TODO
  (org-match-sparse-tree nil "TODO")
  (let ((vis1 '()) (hid1 '()))
    (goto-char (point-min))
    (while (not (eobp))
      (let ((h (org-get-heading t t t t)))
        (when h
          (if (get-char-property (point) 'invisible)
              (push h hid1) (push h vis1))))
      (forward-line))
    ;; Show all
    (org-set-startup-visibility 'all)
    ;; Filter by tag
    (org-match-sparse-tree nil "work")
    (let ((vis2 '()) (hid2 '()))
      (goto-char (point-min))
      (while (not (eobp))
        (let ((h (org-get-heading t t t t)))
          (when h
            (if (get-char-property (point) 'invisible)
                (push h hid2) (push h vis2))))
        (forward-line))
      (list (nreverse vis1) (nreverse hid1)
            (nreverse vis2) (nreverse hid2)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: property management
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_property_management() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"CATEGORY\" . \"???\") (\"A\" . \"1\")) ((\"CATEGORY\" . \"???\") (\"C\" . \"3\") (\"B\" . \"2\") (\"A\" . \"1\")) ((\"CATEGORY\" . \"???\") (\"C\" . \"3\") (\"A\" . \"10\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task\n:PROPERTIES:\n:A: 1\n:END:")
  (goto-char (point-min))
  ;; Initial props
  (let ((p1 (org-entry-properties nil 'standard)))
    ;; Add properties
    (org-entry-put nil "B" "2")
    (org-entry-put nil "C" "3")
    (let ((p2 (org-entry-properties nil 'standard)))
      ;; Modify existing
      (org-entry-put nil "A" "10")
      ;; Delete one
      (org-entry-delete nil "B")
      (let ((p3 (org-entry-properties nil 'standard)))
        (list p1 p2 p3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: heading manipulation
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_heading_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"A\" \"B\" \"C\" \"D\") (\"A\" \"C\" \"B\" \"D\") (\"A\" \"C\" \"D\" \"B\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n* B\n* C\n* D")
  ;; Read initial order
  (let ((o1 (org-element-map (org-element-parse-buffer) 'headline
              (lambda (h) (org-element-property :raw-value h)))))
    ;; Move B down
    (goto-char (point-min))
    (forward-line 1)
    (org-move-subtree-down)
    (let ((o2 (org-element-map (org-element-parse-buffer) 'headline
                (lambda (h) (org-element-property :raw-value h)))))
      ;; Move D up
      (goto-char (point-max))
      (beginning-of-line)
      (org-move-subtree-up)
      (let ((o3 (org-element-map (org-element-parse-buffer) 'headline
                  (lambda (h) (org-element-property :raw-value h)))))
        (list o1 o2 o3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: visibility cycling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_visibility_cycling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2a\n*** H3\nBody\n** H2b\n* H1b")
  (goto-char (point-min))
  (let ((states '()))
    ;; Overview
    (org-set-startup-visibility 'overview)
    (push (list :overview
                (get-char-property (search-forward "H2a") 'invisible)
                (progn (forward-line) (get-char-property (point) 'invisible)))
          states)
    ;; Content
    (org-set-startup-visibility 'content)
    (push (list :content
                (get-char-property (search-forward "H2a") 'invisible)
                (progn (forward-line) (get-char-property (point) 'invisible)))
          states)
    ;; All
    (org-set-startup-visibility 'all)
    (push (list :all
                (get-char-property (search-forward "H2a") 'invisible)
                (progn (forward-line) (get-char-property (point) 'invisible)))
          states)
    (nreverse states)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: link management
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_link_management() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading\nBody")
  ;; Store link
  (goto-char (point-min))
  (org-store-link nil)
  (let ((stored (car org-stored-links)))
    ;; Insert at end
    (goto-char (point-max))
    (insert "\n\n")
    (org-insert-link nil stored "click here")
    ;; Parse all links
    (let* ((tree (org-element-parse-buffer))
           (links (org-element-map tree 'link
                    (lambda (l)
                      (list (org-element-property :type l)
                            (org-element-property :path l)
                            (org-element-property :raw-link l))))))
      (list stored links))))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: footnote management
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_footnote_management() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function footnote-add-footnote)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text")
  ;; Add footnote
  (goto-char (point-max))
  (footnote-add-footnote)
  (insert "First footnote")
  ;; Add second
  (goto-char (point-max))
  (footnote-add-footnote)
  (insert "Second footnote")
  ;; Parse
  (let* ((tree (org-element-parse-buffer))
         (refs (org-element-map tree 'footnote-reference
                 (lambda (f) (org-element-property :label f))))
         (defs (org-element-map tree 'footnote-definition
                 (lambda (d) (org-element-property :label d)))))
    (list refs defs)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: macro expansion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_macro_expansion() {
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
// Workflow: dynamic block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_dynamic_block() {
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
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: outline navigation
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_outline_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Project\" \"Task 1\" \"Subtask 1.1\") 4 \"Subsub 1.1.1\" (\"Project\" \"Task 2\") 3 \"Subtask 2.1\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Project\n** Task 1\n*** Subtask 1.1\n**** Subsub 1.1.1\n** Task 2\n*** Subtask 2.1")
  (goto-char (point-min))
  (search-forward "Subsub 1.1.1")
  (let ((path1 (org-get-outline-path))
        (level1 (org-current-level))
        (title1 (org-get-heading t t t t)))
    (search-forward "Subtask 2.1")
    (beginning-of-line)
    (let ((path2 (org-get-outline-path))
          (level2 (org-current-level))
          (title2 (org-get-heading t t t t)))
      (list path1 level1 title1 path2 level2 title2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: element hierarchy
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_element_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"H1\" 2) (2 \"H2a\" 2) (3 \"H3a\" 0) (3 \"H3b\" 0) (2 \"H2b\" 1) (3 \"H3c\" 2) (4 \"H4a\" 0) (4 \"H4b\" 0) (1 \"H1b\" 1) (2 \"H2c\" 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2a\n*** H3a\n*** H3b\n** H2b\n*** H3c\n**** H4a\n**** H4b\n* H1b\n** H2c")
  (let* ((tree (org-element-parse-buffer))
         (structure (org-element-map tree 'headline
                      (lambda (h)
                        (list (org-element-property :level h)
                              (org-element-property :raw-value h)
                              (length (org-element-contents h)))))))
    structure))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: export environment
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_export_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Full Test\" 0 9 (:parent (#(\"Full Test\" 0 9 (:parent #4)))))) (#(\"Author\" 0 6 (:parent (#(\"Author\" 0 6 (:parent #4)))))) \"test@example.com\" (#(\"2026-01-15\" 0 10 (:parent (#(\"2026-01-15\" 0 10 (:parent #4)))))) 2 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Full Test\n#+AUTHOR: Author\n#+EMAIL: test@example.com\n#+DATE: 2026-01-15\n#+OPTIONS: toc:2 num:t")
  (let* ((info (org-export-get-environment nil)))
    (list (plist-get info :title)
          (plist-get info :author)
          (plist-get info :email)
          (plist-get info :date)
          (plist-get info :with-toc)
          (plist-get info :with-numbers))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: timestamp operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_timestamp_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((active-range 2026 15 10 0 2026 15 nil nil nil) (active-range 2026 16 nil nil 2026 20 nil nil nil) (active 2026 25 nil nil 2026 25 cumulate 1 week))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* M\n<2026-01-15 10:00-11:30>\n<2026-01-16>--<2026-01-20>\n<2026-01-25 Wed +1w -3d>")
  (let* ((tree (org-element-parse-buffer))
         (timestamps (org-element-map tree 'timestamp
                       (lambda (ts)
                         (list (org-element-property :type ts)
                               (org-element-property :year-start ts)
                               (org-element-property :day-start ts)
                               (org-element-property :hour-start ts)
                               (org-element-property :minute-start ts)
                               (org-element-property :year-end ts)
                               (org-element-property :day-end ts)
                               (org-element-property :repeater-type ts)
                               (org-element-property :repeater-value ts)
                               (org-element-property :repeater-unit ts))))))
    timestamps))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: drawer operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_drawer_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"LOGBOOK\" 30 53) (\"MYDRAWER\" 53 77))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:\n:LOGBOOK:\n- Note\n:END:\n:MYDRAWER:\n- Data\n:END:\nBody")
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
// Workflow: block operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_block_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((src-block \"emacs-lisp\" \"-n\" \":results value :exports both\") (example-block nil \"-n\" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp -n :results value :exports both\n(+ 1 2)\n#+END_SRC\n\n#+BEGIN_EXAMPLE -n\nExample\n#+END_EXAMPLE")
  (let* ((tree (org-element-parse-buffer))
         (blocks (org-element-map tree '(src-block example-block)
                   (lambda (b)
                     (list (org-element-type b)
                           (org-element-property :language b)
                           (org-element-property :switches b)
                           (org-element-property :parameters b))))))
    blocks))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: inline task
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_inline_task() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-inlinetask)
  (insert "Body\n*************** TODO Inline\n*************** END\nMore")
  (let* ((tree (org-element-parse-buffer))
         (tasks (org-element-map tree 'headline
                  (lambda (h)
                    (when (= (org-element-property :level h) 15)
                      (list (org-element-property :raw-value h)
                            (org-element-property :todo-keyword h)
                            (org-element-property :level h)))))))
    tasks))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: entity and radio
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_entity_radio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\alpha \\\\beta \\\\gamma\\n<<<target>>>\\nSee target here\" \"\\\\alpha \\\\beta \\\\gamma\\n<<<target>>>\\nSee target here\" (\"target\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "\\alpha \\beta \\gamma\n<<<target>>>\nSee target here")
  (let ((before (buffer-string)))
    (org-toggle-pretty-entities)
    (let* ((after (buffer-string))
           (tree (org-element-parse-buffer))
           (targets (org-element-map tree 'radio-target
                      (lambda (rt) (org-element-property :value rt)))))
      (list before after targets))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: refile targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_refile_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"P1\" (nil \"^\\\\(\\\\*+\\\\)\\\\(?: +\\\\(DONE\\\\|TODO\\\\)\\\\)?\\\\(?: +\\\\(\\\\[#\\\\(?:[A-Z]\\\\|[0-9]\\\\|[1-5][0-9]\\\\|6[0-4]\\\\)\\\\]\\\\)\\\\)?\\\\(?: +\\\\(?:COMMENT +\\\\)?\\\\(?:\\\\[[0-9%/]+\\\\] *\\\\)*\\\\(P1\\\\)\\\\(?: *\\\\[[0-9%/]+\\\\]\\\\)*\\\\)\\\\(?:[ \t]+\\\\(:\\\\([[:alnum:]_@#%:]+\\\\):\\\\)\\\\)?[ \t]*$\" 1)) (\"P2\" (nil \"^\\\\(\\\\*+\\\\)\\\\(?: +\\\\(DONE\\\\|TODO\\\\)\\\\)?\\\\(?: +\\\\(\\\\[#\\\\(?:[A-Z]\\\\|[0-9]\\\\|[1-5][0-9]\\\\|6[0-4]\\\\)\\\\]\\\\)\\\\)?\\\\(?: +\\\\(?:COMMENT +\\\\)?\\\\(?:\\\\[[0-9%/]+\\\\] *\\\\)*\\\\(P2\\\\)\\\\(?: *\\\\[[0-9%/]+\\\\]\\\\)*\\\\)\\\\(?:[ \t]+\\\\(:\\\\([[:alnum:]_@#%:]+\\\\):\\\\)\\\\)?[ \t]*$\" 19)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* P1\n** T1\n*** S1\n* P2\n** T2\n*** S2")
  (let ((targets (org-refile-get-targets nil)))
    (mapcar (lambda (t) (list (car t) (cdr t))) targets)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: agenda entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_agenda_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C\n* WAITING D\n* TODO E :work:")
  (org-map-entries
    (lambda ()
      (list (org-get-heading t t t t)
            (org-get-todo-state)
            (org-get-tags nil t)))
    nil 'file))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: colview format
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_colview_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-columns-get-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %3PRIORITY %TAGS %EFFORT\n* TODO [#A] Task :work:\n:PROPERTIES:\n:EFFORT: 2h\n:END:\n* DONE [#B] Task2 :home:\n:PROPERTIES:\n:EFFORT: 30m\n:END:")
  (goto-char (point-min))
  (org-columns-get-format))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: pcomplete
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_pcomplete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "\\agr")
  (length (all-completions "\\ag" (pcomplete-entries))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: clock sum
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_clock_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 210""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:00] =>  1:00\nCLOCK: [2026-01-11 14:00]--[2026-01-11 15:30] =>  1:30\n:END:\n* B\n:LOGBOOK:\nCLOCK: [2026-01-12 09:00]--[2026-01-12 10:00] =>  1:00\n:END:")
  (org-clock-sum))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: element map with options
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_element_map_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"H2a\" \"H3\" \"H2b\") (\"H2a\" \"H3\" \"H2b\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2a\n*** H3\n** H2b")
  (let* ((tree (org-element-parse-buffer))
         (h1 (car (org-element-map tree 'headline (lambda (h) h))))
         (direct (org-element-map (org-element-contents h1) 'headline
                   (lambda (h) (org-element-property :raw-value h))
                   nil nil nil t))
         (recursive (org-element-map (org-element-contents h1) 'headline
                      (lambda (h) (org-element-property :raw-value h)))))
    (list direct recursive)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: multi-buffer parse
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_multi_buffer_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"A1\") (\"B\" \"B1\" \"B2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((r '()))
  (with-temp-buffer
    (org-mode)
    (insert "* A\n** A1\nBodyA")
    (push (org-element-map (org-element-parse-buffer) 'headline
            (lambda (h) (org-element-property :raw-value h))) r))
  (with-temp-buffer
    (org-mode)
    (insert "* B\n** B1\n** B2\nBodyB")
    (push (org-element-map (org-element-parse-buffer) 'headline
            (lambda (h) (org-element-property :raw-value h))) r))
  (nreverse r))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: deferred element chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_deferred_element_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:todo \"TODO\" :pri 65 :tags (\"tag\") :var \"val\" :title \"Orig\") (:todo \"DONE\" :pri 66 :tags (\"newtag\") :var \"newval\" :title \"Changed\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Orig :tag:\n:PROPERTIES:\n:VAR: val\n:END:\nBody")
  (goto-char (point-min))
  (let* ((el (org-element-at-point))
         (p1 (list :todo (org-element-property :todo-keyword el)
                   :pri (org-element-property :priority el)
                   :tags (org-element-property :tags el)
                   :var (org-entry-get nil "VAR")
                   :title (org-element-property :raw-value el))))
    (org-todo 'right)
    (org-priority 'down)
    (org-set-tags '("newtag"))
    (org-entry-put nil "VAR" "newval")
    (org-edit-headline "Changed")
    (let* ((el2 (org-element-at-point))
           (p2 (list :todo (org-element-property :todo-keyword el2)
                     :pri (org-element-property :priority el2)
                     :tags (org-element-property :tags el2)
                     :var (org-entry-get nil "VAR")
                     :title (org-element-property :raw-value el2))))
      (list p1 p2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: narrow and widen
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_narrow_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* H1\\nBody 1\\n** H2\\nSub\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody 1\n** H2\nSub\n* H2b\nBody 2")
  (goto-char (point-min))
  (org-narrow-to-subtree)
  (let ((narrowed (buffer-string)))
    (widen)
    (list narrowed)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: end of subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_end_of_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (31 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2a\n*** H3\nBody\n** H2b\n* H1b")
  (goto-char (point-min))
  (let ((p1 (progn (org-end-of-subtree) (point))))
    (goto-char (point-min))
    (search-forward "H2a")
    (beginning-of-line)
    (let ((p2 (progn (org-end-of-subtree) (point))))
      (list p1 p2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: mark subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_mark_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 1 21)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody\n** H2\nSub\n* H1b")
  (goto-char (point-min))
  (org-mark-subtree)
  (let ((m (mark))
        (p (point)))
    (list (< p m) p m)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: clone subtree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_clone_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clone-subtree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task\n** Sub1\n** Sub2")
  (goto-char (point-min))
  (org-clone-subtree 2)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :level h)
                      (org-element-property :raw-value h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Workflow: sort entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn wf_sort_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Zebra\n* Apple\n* Mango\n* Banana")
  (org-sort-entries nil ?a)
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))))"##,
        expect,
    );
}
