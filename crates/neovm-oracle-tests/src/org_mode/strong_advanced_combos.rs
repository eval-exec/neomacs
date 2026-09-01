//! Strong advanced combo oracle tests — complex multi-step sequences.
//!
//! Every test returns concrete structured data (lists, plists, numbers,
//! strings) to surface real divergences between Neomacs and GNU Emacs.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: todo workflow with state logging
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_todo_workflow_state_logging() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"TODO\" #(\"DONE\" 0 4 (org-todo-head \"TODO\")) nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq org-todo-keywords '((sequence "TODO" "IN-PROGRESS" "DONE"))
        org-log-into-drawer t)
  (insert "BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n* TODO Test task")
  (goto-char (point-min))
  (search-forward "TODO Test")
  (let ((state1 (org-get-todo-state)))
    (org-todo 'right)  ; TODO -> IN-PROGRESS
    (let ((state2 (org-get-todo-state))
          (log1 (org-entry-get nil "LOGBOOK")))
      (org-todo 'right)  ; IN-PROGRESS -> DONE
      (let ((state3 (org-get-todo-state))
            (log2 (org-entry-get nil "LOGBOOK")))
        (list state1 state2 state3 log1 log2)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: nested blocks with results
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_nested_blocks_with_results() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n\n#+RESULTS:\n: 3\n\n#+BEGIN_SRC emacs-lisp\n(* 3 4)\n#+END_SRC\n\n#+RESULTS:\n: 12")
  (let* ((tree (org-element-parse-buffer))
         (blocks (org-element-map tree 'src-block
                   (lambda (b)
                     (list (org-element-property :value b)
                           (org-element-property :post-affiliated b))))
         (results (org-element-map tree 'fixed-width
                    (lambda (f)
                      (org-element-property :value f)))))
    (list blocks results)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: table with column operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_table_column_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"A\" \"B\" \"C\") (\"1\" \"2\" \"3\") (\"4\" \"5\" \"6\")) ((\"\" \"NEW\" #(\"B\" 0 1 (face org-table)) #(\"C\" 0 1 (face org-table))) (\"\" \"X\" #(\"2\" 0 1 (face org-table)) #(\"3\" 0 1 (face org-table))) (\"\" \"Y\" #(\"5\" 0 1 (face org-table)) #(\"6\" 0 1 (face org-table)))) ((#(\"NEW\" 0 3 (face org-table)) #(\"B\" 0 1 (face org-table)) #(\"C\" 0 1 (face org-table))) (#(\"X\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"3\" 0 1 (face org-table))) (#(\"Y\" 0 1 (face org-table)) #(\"5\" 0 1 (face org-table)) #(\"6\" 0 1 (face org-table)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| A | B | C |\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |")
  (goto-char (point-min))
  (let ((d1 (org-table-to-lisp)))
    (org-table-insert-column)
    (org-table-put 1 2 "NEW")
    (org-table-put 2 2 "X")
    (org-table-put 3 2 "Y")
    (let ((d2 (org-table-to-lisp)))
      (org-table-delete-column)
      (let ((d3 (org-table-to-lisp)))
        (list d1 d2 d3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: list with mixed types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_list_mixed_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((unordered (\"- \" \"- \" \"1. \" \"2. \" \"- \" \"+ \")) (ordered (\"1. \" \"2. \")) (descriptive (\"+ \")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- item 1\n- item 2\n  1. numbered 1\n  2. numbered 2\n- item 3\n  + descriptive :: desc")
  (let* ((tree (org-element-parse-buffer))
         (items (org-element-map tree 'plain-list
                  (lambda (pl)
                    (list (org-element-property :type pl)
                          (org-element-map (org-element-contents pl) 'item
                            (lambda (it)
                              (org-element-property :bullet it))))))))
    items))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: footnote with multiple references
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_footnote_multiple_references() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"1\" 5) (\"2\" 16) (\"1\" 26) (\"2\" 38)) ((\"1\" 46) (\"2\" 68)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] more[fn:2] and[fn:1] again[fn:2]\n\n[fn:1] First footnote\n[fn:2] Second footnote")
  (let* ((tree (org-element-parse-buffer))
         (footnotes (org-element-map tree 'footnote-reference
                      (lambda (fn)
                        (list (org-element-property :label fn)
                              (org-element-property :begin fn)))))
         (defs (org-element-map tree 'footnote-definition
                 (lambda (fd)
                   (list (org-element-property :label fd)
                         (org-element-property :begin fd))))))
    (list footnotes defs)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: export with attributes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_with_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"file\" \"image.png\" paragraph (((#(\"My image\" 0 8 (:parent (#(\"My image\" 0 8 (:parent #7)))))))) (\":width 300px :class thumbnail\") nil) (\"file\" \"other.png\" paragraph nil nil (\":width 0.5\\\\textwidth\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CAPTION: My image\n#+ATTR_HTML: :width 300px :class thumbnail\n#+NAME: fig1\n[[file:image.png]]\n\n#+ATTR_LATEX: :width 0.5\\textwidth\n[[file:other.png]]")
  (let* ((tree (org-element-parse-buffer))
         (links (org-element-map tree 'link
                  (lambda (l)
                    (let ((parent (org-element-property :parent l)))
                      (list (org-element-property :type l)
                            (org-element-property :path l)
                            (org-element-type parent)
                            (org-element-property :caption parent)
                            (org-element-property :attr_html parent)
                            (org-element-property :attr_latex parent)))))))
    links))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: clock with total time
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_clock_total_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((timestamp (:standard-properties [27 nil nil nil 66 1 nil nil nil nil nil nil nil nil nil nil nil nil] :type inactive-range :range-type daterange :raw-value \"[2026-01-10 10:00]--[2026-01-10 11:30]\" :year-start 2026 :month-start 1 :day-start 10 :hour-start 10 :minute-start 0 :year-end 2026 :month-end 1 :day-end 10 :hour-end 11 :minute-end 30)) \"1:30\") ((timestamp (:standard-properties [82 nil nil nil 121 1 nil nil nil nil nil nil nil nil nil nil nil nil] :type inactive-range :range-type daterange :raw-value \"[2026-01-11 14:00]--[2026-01-11 15:00]\" :year-start 2026 :month-start 1 :day-start 11 :hour-start 14 :minute-start 0 :year-end 2026 :month-end 1 :day-end 11 :hour-end 15 :minute-end 0)) \"1:00\") ((timestamp (:standard-properties [162 nil nil nil 201 1 nil nil nil nil nil nil nil nil nil nil nil nil] :type inactive-range :range-type daterange :raw-value \"[2026-01-12 09:00]--[2026-01-12 10:00]\" :year-start 2026 :month-start 1 :day-start 12 :hour-start 9 :minute-start 0 :year-end 2026 :month-end 1 :day-end 12 :hour-end 10 :minute-end 0)) \"1:00\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task 1\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\nCLOCK: [2026-01-11 14:00]--[2026-01-11 15:00] =>  1:00\n:END:\n* Task 2\n:LOGBOOK:\nCLOCK: [2026-01-12 09:00]--[2026-01-12 10:00] =>  1:00\n:END:")
  (let* ((tree (org-element-parse-buffer))
         (clocks (org-element-map tree 'clock
                   (lambda (c)
                     (list (org-element-property :value c)
                           (org-element-property :duration c))))))
    clocks))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: link with search options
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_link_with_search_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((\"file\" \"test.org\" \"*heading\" \"file:test.org::*heading\") (\"file\" \"test.org\" \"#custom-id\" \"file:test.org::#custom-id\") (\"file\" \"test.org\" \"123\" \"file:test.org::123\"))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "See [[file:test.org::*heading][heading link]] and [[file:test.org::#custom-id][id link]] and [[file:test.org::123][line link]]")
  (let* ((tree (org-element-parse-buffer))
         (links (org-element-map tree 'link
                  (lambda (l)
                    (list (org-element-property :type l)
                          (org-element-property :path l)
                          (org-element-property :search-option l)
                          (org-element-property :raw-link l))))))
    links))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: property inheritance with columns
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_property_inheritance_with_columns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-columns-get-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %PRIORITY %TAGS %VAR\n* Parent :work:\n:PROPERTIES:\n:VAR: parent-val\n:END:\n** Child 1\n*** Grandchild\n** Child 2 :personal:")
  (goto-char (point-min))
  (search-forward "Grandchild")
  (let ((var (org-entry-get nil "VAR" 'inherit))
        (tags (org-get-tags nil t))
        (fmt (org-columns-get-format)))
    (list var tags fmt)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: sparse tree with date matching
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_sparse_tree_date_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Task 1\" \"Task 2\" \"Task 3\" \"Task 4\") (\"Task 1\" \"Task 2\" \"Task 3\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task 1\nSCHEDULED: <2026-01-15>\n* Task 2\nSCHEDULED: <2026-01-20>\n* Task 3\nSCHEDULED: <2026-02-01>\n* Task 4")
  (goto-char (point-min))
  (org-match-sparse-tree nil "SCHEDULED<=\"<2026-01-31>\"")
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
// Advanced combo: element hierarchy operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_hierarchy_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"H1\" 2) (2 \"H2a\" 2) (3 \"H3a\" 0) (3 \"H3b\" 0) (2 \"H2b\" 0) (1 \"H1b\" 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2a\n*** H3a\n*** H3b\n** H2b\n* H1b")
  (goto-char (point-min))
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
// Advanced combo: edit sequence with undo
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_edit_sequence_complex_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (user-error \"No undo information in this buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Original\nBody")
  (goto-char (point-min))
  (let ((s1 (buffer-string)))
    (org-edit-headline "Changed 1")
    (org-set-tags '("tag1"))
    (let ((s2 (buffer-string)))
      (org-edit-headline "Changed 2")
      (org-priority 'down)
      (let ((s3 (buffer-string)))
        (undo)
        (undo)
        (undo)
        (let ((s4 (buffer-string)))
          (list s1 s2 s3 s4))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: table with row operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_table_row_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"a\" \"b\") (\"c\" \"d\") (\"e\" \"f\")) ((\"NEW1\" \"NEW2\") (#(\"a\" 0 1 (face org-table)) #(\"b\" 0 1 (face org-table))) (#(\"c\" 0 1 (face org-table)) #(\"d\" 0 1 (face org-table))) (#(\"e\" 0 1 (face org-table)) #(\"f\" 0 1 (face org-table)))) ((#(\"NEW1\" 0 4 (face org-table)) #(\"NEW2\" 0 4 (face org-table))) (#(\"a\" 0 1 (face org-table)) #(\"b\" 0 1 (face org-table))) (#(\"e\" 0 1 (face org-table)) #(\"f\" 0 1 (face org-table)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| c | d |\n| e | f |")
  (goto-char (point-min))
  (let ((d1 (org-table-to-lisp)))
    (org-table-insert-row)
    (org-table-put 1 1 "NEW1")
    (org-table-put 1 2 "NEW2")
    (let ((d2 (org-table-to-lisp)))
      (org-table-goto-line 3)
      (org-table-kill-row)
      (let ((d3 (org-table-to-lisp)))
        (list d1 d2 d3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: headline with drawers
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_headline_with_drawers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"TODO\" \"Task\" ((\"LOGBOOK\" 58 114)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task\n:PROPERTIES:\n:VAR: val\n:CUSTOM_ID: id1\n:END:\n:LOGBOOK:\n- State \"DONE\" from \"TODO\" [2026-01-15]\n:END:\nBody text")
  (let* ((tree (org-element-parse-buffer))
         (headline (car (org-element-map tree 'headline (lambda (h) h))))
         (drawers (org-element-map (org-element-contents headline) 'drawer
                    (lambda (d)
                      (list (org-element-property :drawer-name d)
                            (org-element-property :begin d)
                            (org-element-property :end d))))))
    (list (org-element-property :todo-keyword headline)
          (org-element-property :raw-value headline)
          drawers)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: export with custom backend
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_custom_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Test\" 0 4 (:parent (#(\"Test\" 0 4 (:parent #4)))))) nil \"exec@oracle-host\" ((1 \"Heading\") (2 \"Sub\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Test\n* Heading\n** Sub\nBody")
    (let* ((tree (org-element-parse-buffer))
           (info (org-export-get-environment nil))
           (headlines (org-element-map tree 'headline
                        (lambda (h)
                          (list (org-element-property :level h)
                                (org-element-property :raw-value h)))))
           (title (plist-get info :title))
           (author (plist-get info :author))
           (email (plist-get info :email)))
      (list title author email headlines))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: clock table with scope
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_clock_table_with_scope() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Not in a dynamic block\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task A\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:00] =>  1:00\n:END:\n** Sub A1\n:LOGBOOK:\nCLOCK: [2026-01-11 14:00]--[2026-01-11 15:00] =>  1:00\n:END:\n* Task B\n:LOGBOOK:\nCLOCK: [2026-01-12 09:00]--[2026-01-12 10:00] =>  1:00\n:END:")
  (goto-char (point-max))
  (insert "\n#+BEGIN: clocktable :maxlevel 2 :scope file\n#+END:")
  (org-dblock-update)
  (let ((content (buffer-substring-no-properties (point-min) (point-max))))
    content))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: timestamp with repeater
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_timestamp_with_repeater() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((active cumulate 1 week) (active cumulate 1 month))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Weekly meeting\n<2026-01-15 Wed 10:00 +1w>\n\n* Monthly review\n<2026-01-20 Mon +1m>")
  (let* ((tree (org-element-parse-buffer))
         (timestamps (org-element-map tree 'timestamp
                       (lambda (ts)
                         (list (org-element-property :type ts)
                               (org-element-property :repeater-type ts)
                               (org-element-property :repeater-value ts)
                               (org-element-property :repeater-unit ts))))))
    timestamps))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: link with radio targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_link_radio_targets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"target1\" \"target2\") nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<<<target1>>>\n\nSee target1 and target1 here\n\n<<<target2>>>\n\nAnother target2 reference")
  (let* ((tree (org-element-parse-buffer))
         (targets (org-element-map tree 'radio-target
                    (lambda (rt)
                      (org-element-property :value rt))))
         (links (org-element-map tree 'radio-target
                  (lambda (rt)
                    (org-element-property :raw-link rt)))))
    (list targets links)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: table with formula references
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_table_formula_references() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range [nil 0 1 2 4] 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | 1 |\n| b | 2 |\n| c | 3 |\n|---+---|\n| sum | 6 |\n#+TBLFM: @5$2=vsum(@2..@4)")
  (goto-char (point-min))
  (org-table-recalculate 'all)
  (let ((data (org-table-to-lisp))
        (val (org-table-get 5 2)))
    (list data val)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: element with affiliated keywords
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_affiliated_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (paragraph (((#(\"My caption\" 0 10 (:parent (#(\"My caption\" 0 10 (:parent #6)))))))) (\":width 300px\") \"my-fig\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CAPTION: My caption\n#+ATTR_HTML: :width 300px\n#+NAME: my-fig\n[[file:image.png]]")
  (let* ((tree (org-element-parse-buffer))
         (link (car (org-element-map tree 'link (lambda (l) l))))
         (parent (org-element-property :parent link)))
    (list (org-element-type parent)
          (org-element-property :caption parent)
          (org-element-property :attr_html parent)
          (org-element-property :name parent))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: planning with repeaters and delays
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_planning_repeaters_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((cumulate 1 nil nil) (cumulate 1 nil nil) (nil nil cumulate 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task 1\nSCHEDULED: <2026-01-15 Wed +1w>\n* TODO Task 2\nSCHEDULED: <2026-01-20 Mon +1m -3d>\n* TODO Task 3\nDEADLINE: <2026-02-01 Sun +2w>")
  (let* ((tree (org-element-parse-buffer))
         (planning (org-element-map tree 'planning
                     (lambda (p)
                       (let ((sched (org-element-property :scheduled p))
                             (dl (org-element-property :deadline p)))
                         (list (when sched (org-element-property :repeater-type sched))
                               (when sched (org-element-property :repeater-value sched))
                               (when dl (org-element-property :repeater-type dl))
                               (when dl (org-element-property :repeater-value dl))))))))
    planning))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: block with switches
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_block_with_switches() {
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
// Advanced combo: headline with all planning
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_headline_with_all_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((timestamp (:standard-properties [24 nil nil nil 36 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2026-01-15>\" :year-start 2026 :month-start 1 :day-start 15 :hour-start nil :minute-start nil :year-end 2026 :month-end 1 :day-end 15 :hour-end nil :minute-end nil)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\nCLOSED: [2026-01-10]")
  (let* ((tree (org-element-parse-buffer))
         (headline (car (org-element-map tree 'headline (lambda (h) h))))
         (planning (car (org-element-map (org-element-contents headline) 'planning
                         (lambda (p) p)))))
    (list (org-element-property :scheduled planning)
          (org-element-property :deadline planning)
          (org-element-property :closed planning))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: export with options
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_with_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Test\" 0 4 (:parent (#(\"Test\" 0 4 (:parent #4)))))) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n#+OPTIONS: toc:nil num:nil ^:nil\n#+LANGUAGE: en\n* Heading\n** Sub")
  (let* ((tree (org-element-parse-buffer))
         (info (org-export-get-environment nil)))
    (list (plist-get info :title)
          (plist-get info :with-toc)
          (plist-get info :with-numbers))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: list with checkboxes and counters
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_list_checkboxes_counters() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* Task [2/3]\" \"* Task [2/3]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task [/]\n- [X] item 1\n- [ ] item 2\n  - [X] sub 1\n  - [ ] sub 2\n- [X] item 3")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (let ((h1 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
    (forward-line 2)
    (org-toggle-checkbox)
    (forward-line 1)
    (org-toggle-checkbox)
    (org-update-statistics-cookies t)
    (goto-char (point-min))
    (let ((h2 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
      (list h1 h2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: sparse tree with tags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_sparse_tree_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"Task 1\" \"Task 2\" \"Task 3\" \"Task 4\") nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task 1 :work:\n* Task 2 :personal:\n* Task 3 :work:urgent:\n* Task 4")
  (goto-char (point-min))
  (org-match-sparse-tree nil "work")
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
// Advanced combo: element with deferred ops
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_deferred_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:type headline :todo \"TODO\" :priority 65 :tags (\"tag\") :var \"val\") (:type headline :todo \"DONE\" :priority 66 :tags (\"newtag\") :var \"newval\" :title \"Changed\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Test :tag:\n:PROPERTIES:\n:VAR: val\n:END:\nBody")
  (goto-char (point-min))
  (let* ((el (org-element-at-point))
         (props1 (list :type (org-element-type el)
                       :todo (org-element-property :todo-keyword el)
                       :priority (org-element-property :priority el)
                       :tags (org-element-property :tags el)
                       :var (org-entry-get nil "VAR"))))
    ;; Modify all
    (org-todo 'right)
    (org-priority 'down)
    (org-set-tags '("newtag"))
    (org-entry-put nil "VAR" "newval")
    (org-edit-headline "Changed")
    ;; Read back
    (let* ((el2 (org-element-at-point))
           (props2 (list :type (org-element-type el2)
                         :todo (org-element-property :todo-keyword el2)
                         :priority (org-element-property :priority el2)
                         :tags (org-element-property :tags el2)
                         :var (org-entry-get nil "VAR")
                         :title (org-element-property :raw-value el2))))
      (list props1 props2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: table with mixed content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_table_mixed_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| *bold* | /italic/ |\n| =code= | _underlined_ |\n| [[link][desc]] | 123 |")
  (let* ((tree (org-element-parse-buffer))
         (cells (org-element-map tree 'table-cell
                  (lambda (c)
                    (org-element-property :value c)))))
    cells))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: headline with timestamps
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_headline_with_timestamps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((active 2026 1 15 nil nil) (active 2026 1 16 17 0) (active 2026 1 17 nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Meeting on <2026-01-15 Wed>\nSCHEDULED: <2026-01-14 Tue 10:00>\nDEADLINE: <2026-01-16 Thu 17:00>\nBody with <2026-01-17 Fri> date")
  (let* ((tree (org-element-parse-buffer))
         (timestamps (org-element-map tree 'timestamp
                       (lambda (ts)
                         (list (org-element-property :type ts)
                               (org-element-property :year-start ts)
                               (org-element-property :month-start ts)
                               (org-element-property :day-start ts)
                               (org-element-property :hour-start ts)
                               (org-element-property :minute-start ts))))))
    timestamps))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: drawer with visibility
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_drawer_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"VAR\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:VAR: val\n:END:\nBody")
  (goto-char (point-min))
  (org-cycle-hide-drawers 'overview)
  (let ((hidden1 (get-char-property (search-forward "VAR") 'invisible)))
    (org-cycle '(4))
    (let ((hidden2 (get-char-property (search-forward "VAR") 'invisible)))
      (list hidden1 hidden2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: export with custom attributes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_custom_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\":id my-id :class my-class\") (\":options [my-option]\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+ATTR_HTML: :id my-id :class my-class\n#+ATTR_LATEX: :options [my-option]\n#+BEGIN_QUOTE\nQuoted text\n#+END_QUOTE")
  (let* ((tree (org-element-parse-buffer))
         (block (car (org-element-map tree 'quote-block (lambda (b) b)))))
    (list (org-element-property :attr_html block)
          (org-element-property :attr_latex block))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: clock with effort
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_clock_with_effort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clock-sum-current-entry)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task\n:PROPERTIES:\n:EFFORT: 1:00\n:END:\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 10:30] =>  0:30\n:END:")
  (let ((effort (org-entry-get nil "EFFORT"))
        (clocked (org-clock-sum-current-entry)))
    (list effort clocked)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: link with fuzzy search
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_link_fuzzy_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"fuzzy\" \"Target heading\" \"Target heading\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Target heading\nSome text\n\nSee [[Target heading]]")
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
// Advanced combo: table with column formulas
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_table_column_formulas() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"a\" 0 1 (face org-table)) #(\"1\" 0 1 (face org-table)) #(\"10\" 0 2 (face org-table))) (#(\"b\" 0 1 (face org-table)) #(\"2\" 0 1 (face org-table)) #(\"20\" 0 2 (face org-table))) (#(\"c\" 0 1 (face org-table)) #(\"3\" 0 1 (face org-table)) #(\"30\" 0 2 (face org-table))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | 1 |   |\n| b | 2 |   |\n| c | 3 |   |\n#+TBLFM: $3=$2*10")
  (goto-char (point-min))
  (org-table-recalculate 'all)
  (let ((data (org-table-to-lisp)))
    data))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: property with inheritance
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_property_inheritance_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"2\" \"2\" \"2\" nil \"3\" \"3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+PROPERTY: VAR 1\n* Level 1\n:PROPERTIES:\n:VAR: 2\n:END:\n** Level 2\n*** Level 3\n:PROPERTIES:\n:VAR: 3\n:END:")
  (goto-char (point-min))
  (search-forward "Level 3")
  (let ((v3 (org-entry-get nil "VAR" 'inherit))
        (v3nil (org-entry-get nil "VAR" nil)))
    (search-backward "Level 2")
    (let ((v2 (org-entry-get nil "VAR" 'inherit))
          (v2nil (org-entry-get nil "VAR" nil)))
      (search-backward "Level 1")
      (let ((v1 (org-entry-get nil "VAR" 'inherit))
            (v1nil (org-entry-get nil "VAR" nil)))
        (list v1 v1nil v2 v2nil v3 v3nil)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: headline with all elements
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_headline_all_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"TODO\" 65 (\"tag1\" \"tag2\") \"Title\" (section headline))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Title :tag1:tag2:\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\n:PROPERTIES:\n:VAR: val\n:END:\n:LOGBOOK:\n- Note\n:END:\nBody text\n** Sub heading\n- List item\n| table |\n#+BEGIN_SRC\n(+ 1 2)\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (headline (car (org-element-map tree 'headline (lambda (h) h))))
         (children (mapcar 'org-element-type (org-element-contents headline))))
    (list (org-element-property :todo-keyword headline)
          (org-element-property :priority headline)
          (org-element-property :tags headline)
          (org-element-property :raw-value headline)
          children)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: multiple buffers parse
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_multiple_buffers_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"Buffer 1\" \"Sub 1.1\") (\"Buffer 2\" \"Sub 2.1\" \"Sub 2.2\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((results '()))
  (with-temp-buffer
    (org-mode)
    (insert "* Buffer 1\n** Sub 1.1\nBody 1")
    (let ((tree (org-element-parse-buffer)))
      (push (org-element-map tree 'headline
              (lambda (h) (org-element-property :raw-value h)))
            results)))
  (with-temp-buffer
    (org-mode)
    (insert "* Buffer 2\n** Sub 2.1\n** Sub 2.2\nBody 2")
    (let ((tree (org-element-parse-buffer)))
      (push (org-element-map tree 'headline
              (lambda (h) (org-element-property :raw-value h)))
            results)))
  (nreverse results))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Advanced combo: edit with context preservation
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_edit_context_preservation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Original :tag:\n:PROPERTIES:\n:VAR: val\n:END:\nBody line 1\nBody line 2")
  (goto-char (point-min))
  (let ((ctx1 (list (org-get-heading t t t t) (org-get-todo-state)
                    (org-get-priority (char-after)) (org-get-tags nil t)
                    (org-entry-get nil "VAR"))))
    (org-edit-headline "Changed")
    (let ((ctx2 (list (org-get-heading t t t t) (org-get-todo-state)
                      (org-get-priority (char-after)) (org-get-tags nil t)
                      (org-entry-get nil "VAR"))))
      (list ctx1 ctx2))))"##,
        expect,
    );
}
