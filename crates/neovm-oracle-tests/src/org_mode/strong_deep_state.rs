//! Strong deep-state oracle tests — capture multiple state changes.
//!
//! Every test returns concrete structured data (lists, plists, numbers,
//! strings) to surface real divergences between Neomacs and GNU Emacs.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Deep state: headline editing with context
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_headline_edit_with_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Test headline :tag1:tag2:\nBody text\n** Sub heading\nSub body")
  (goto-char (point-min))
  (let ((title (org-get-heading t t t t))
        (todo (org-get-todo-state))
        (priority (org-get-priority (char-after)))
        (tags (org-get-tags nil t))
        (level (org-current-level))
        (body (progn (forward-line) (buffer-substring-no-properties
                                      (line-beginning-position)
                                      (line-end-position)))))
    (list title todo priority tags level body)))"##,
        expect,
    );
}

#[test]
fn strong_headline_edit_cycle_todo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"TODO\" #(\"DONE\" 0 4 (org-todo-head \"TODO\")) nil #(\"TODO\" 0 4 (org-todo-head \"TODO\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Test\n* DONE Test2\n* WAITING Test3")
  (goto-char (point-min))
  (let ((states '()))
    (dotimes (_ 3)
      (push (org-get-todo-state) states)
      (org-todo 'right))
    (push (org-get-todo-state) states)
    (nreverse states)))"##,
        expect,
    );
}

#[test]
fn strong_headline_edit_set_tags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"tag1\" \"tag2\") (\"newtag\") (\"newtag\" \"extratag\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test headline")
  (goto-char (point-min))
  (org-set-tags '("tag1" "tag2"))
  (let ((tags1 (org-get-tags nil t)))
    (org-set-tags '("newtag"))
    (let ((tags2 (org-get-tags nil t)))
      (org-toggle-tag "extratag" 'on)
      (let ((tags3 (org-get-tags nil t)))
        (list tags1 tags2 tags3)))))"##,
        expect,
    );
}

#[test]
fn strong_headline_edit_set_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] Test\n* TODO [#B] Test2\n* TODO Test3")
  (goto-char (point-min))
  (let ((p1 (org-get-priority (char-after))))
    (org-priority 'down)
    (let ((p2 (org-get-priority (char-after))))
      (org-priority 'up)
      (let ((p3 (org-get-priority (char-after))))
        (list p1 p2 p3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: property operations with inheritance
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_property_inheritance_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"2\" \"2\" \"2\" nil \"2\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+PROPERTY: VAR 1\n* Level 1\n:PROPERTIES:\n:VAR: 2\n:END:\n** Level 2\n*** Level 3")
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

#[test]
fn strong_property_set_get_delete_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:VAR: old\n:END:")
  (goto-char (point-min))
  (let ((v1 (org-entry-get nil "VAR")))
    (org-entry-put nil "VAR" "new")
    (let ((v2 (org-entry-get nil "VAR")))
      (org-entry-delete nil "VAR")
      (let ((v3 (org-entry-get nil "VAR")))
        (org-entry-put nil "VAR" "final")
        (let ((v4 (org-entry-get nil "VAR")))
          (list v1 v2 v3 v4)))))"##,
        expect,
    );
}

#[test]
fn strong_property_block_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"1\" \"2\" \"1\" nil \"1\" \"4\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:A: 1\n:B: 2\n:C: 3\n:END:")
  (goto-char (point-min))
  (let ((p1 (org-entry-properties nil 'standard)))
    (org-entry-delete nil "B")
    (let ((p2 (org-entry-properties nil 'standard)))
      (org-entry-put nil "D" "4")
      (let ((p3 (org-entry-properties nil 'standard)))
        (list (alist-get "A" p1 nil nil 'equal)
              (alist-get "B" p1 nil nil 'equal)
              (alist-get "A" p2 nil nil 'equal)
              (alist-get "B" p2 nil nil 'equal)
              (alist-get "A" p3 nil nil 'equal)
              (alist-get "D" p3 nil nil 'equal))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: planning timestamp operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_planning_deadline_schedule_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Test task")
  (goto-char (point-min))
  (org-deadline nil "2026-01-15")
  (org-schedule nil "2026-01-10")
  (let ((dl (org-entry-get nil "DEADLINE"))
        (sc (org-entry-get nil "SCHEDULED"))
        (has-time (org-entry-get nil "TIMESTAMP_IA")))
    (org-deadline nil "2026-02-01")
    (let ((dl2 (org-entry-get nil "DEADLINE")))
      (org-schedule nil nil)  ; remove scheduled
      (let ((sc2 (org-entry-get nil "SCHEDULED")))
        (list dl sc dl2 sc2)))))"##,
    );
}

#[test]
fn strong_planning_timestamp_active_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"Test\" nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n<2026-01-15>\n[2026-01-20]\n<2026-01-25 Sun>")
  (goto-char (point-min))
  (let ((ts1 (org-element-at-point)))
    (forward-line)
    (let ((ts2 (org-element-at-point)))
      (forward-line)
      (let ((ts3 (org-element-at-point)))
        (list (org-element-property :type ts1)
              (org-element-property :raw-value ts1)
              (org-element-property :type ts2)
              (org-element-property :raw-value ts2)
              (org-element-property :type ts3)
              (org-element-property :raw-value ts3))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: table formula and structure
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_table_formula_set_eval_recalc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"3\" 0 1 (face org-table)) #(\"7\" 0 1 (face org-table)) #(\"12\" 0 2 (face org-table)) #(\"7\" 0 1 (face org-table)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| 1 | 2 |\n| 3 | 4 |\n|---|\n#+TBLFM: $3=$1+$2")
  (goto-char (point-min))
  (org-table-recalculate 'all)
  (let ((row1 (org-table-get 1 3))
        (row2 (org-table-get 2 3)))
    (org-table-put 1 1 "10")
    (org-table-recalculate 'all)
    (let ((row1b (org-table-get 1 3))
          (row2b (org-table-get 2 3)))
      (list row1 row2 row1b row2b))))"##,
        expect,
    );
}

#[test]
fn strong_table_insert_delete_rows() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((#(\"a\" 0 1 (face org-table)) #(\"b\" 0 1 (face org-table))) (\"NEW\" \"\") (#(\"c\" 0 1 (face org-table)) #(\"d\" 0 1 (face org-table))) (#(\"e\" 0 1 (face org-table)) #(\"f\" 0 1 (face org-table)))) ((#(\"a\" 0 1 (face org-table)) #(\"b\" 0 1 (face org-table))) (#(\"NEW\" 0 3 (face org-table)) \"\") (#(\"e\" 0 1 (face org-table)) #(\"f\" 0 1 (face org-table)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| c | d |\n| e | f |")
  (goto-char (point-min))
  (org-table-next-row)
  (org-table-insert-row)
  (org-table-put 2 1 "NEW")
  (let ((data1 (org-table-to-lisp)))
    (org-table-goto-line 3)
    (org-table-kill-row)
    (let ((data2 (org-table-to-lisp)))
      (list data1 data2))))"##,
        expect,
    );
}

#[test]
fn strong_table_move_columns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not in table data field\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c | d |")
  (goto-char (point-min))
  (let ((d1 (org-table-to-lisp)))
    (org-table-move-column-right)
    (let ((d2 (org-table-to-lisp)))
      (org-table-move-column-right)
      (let ((d3 (org-table-to-lisp)))
        (org-table-move-column-left)
        (let ((d4 (org-table-to-lisp)))
          (list d1 d2 d3 d4))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: list operations with checkbox
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_list_checkbox_toggle_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil nil \"- [-] item 1\\n  - [ ] sub 1\\n  - [X] sub 2\\n- [ ] item 2\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [ ] item 1\n  - [ ] sub 1\n  - [ ] sub 2\n- [ ] item 2")
  (goto-char (point-min))
  (org-toggle-checkbox)
  (let ((cb1 (org-at-item-checkbox-p))
          (stat1 (org-get-at-bol 'org-checkbox-stat)))
    (forward-line 2)
    (org-toggle-checkbox)
    (let ((stat2 (org-get-at-bol 'org-checkbox-stat)))
      (org-update-statistics-cookies t)
      (let ((stats (buffer-substring-no-properties (point-min) (point-max))))
        (list stat1 stat2 stats)))))"##,
        expect,
    );
}

#[test]
fn strong_list_indent_outdent_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error \"At first item: use S-M-<left/right> to move the whole list\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- item 1\n- item 2\n  - sub 1\n- item 3")
  (goto-char (point-min))
  (end-of-line)
  (org-metaright)
  (let ((struct1 (org-list-struct)))
    (forward-line)
    (org-metaright)
    (let ((struct2 (org-list-struct)))
      (forward-line 2)
      (org-metaup)
      (let ((struct3 (org-list-struct)))
        (list struct1 struct2 struct3)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: footnote operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_footnote_create_edit_delete_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function footnote-at-reference-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text with footnote[fn:1]\n\n[fn:1] Original footnote")
  (goto-char (point-min))
  (search-forward "[fn:1]")
  (let ((ref1 (footnote-at-reference-p))
        (def1 (progn (forward-line 2) (footnote-at-definition-p))))
    (goto-char (point-min))
    (search-forward "Original")
    (replace-match "Edited")
    (let ((def2 (buffer-substring-no-properties
                  (line-beginning-position) (line-end-position))))
      (footnote-add-footnote)
      (let ((count (count-matches "\\[fn:")))
        (list ref1 def1 def2 count)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: sparse tree and agenda
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_sparse_tree_todo_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil (\"Task 1\" \"Task 2\" \"Task 3\" \"WAITING Task 4\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task 1\n* DONE Task 2\n* TODO Task 3\n* WAITING Task 4")
  (goto-char (point-min))
  (let ((all (org-map-entries (lambda () (org-get-heading t t t t)) nil 'file)))
    (org-match-sparse-tree nil "TODO={DONE}")
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
      (list all (nreverse visible) (nreverse hidden)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: clock operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_clock_in_out_duration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Test task")
  (goto-char (point-min))
  (org-clock-in)
  (let ((clocking (org-clocking-p))
        (clock-h (org-entry-get nil "CLOCK")))
    (org-clock-out)
    (let ((clocking2 (org-clocking-p))
          (clock-h2 (org-entry-get nil "CLOCK")))
      (list clocking clock-h clocking2 clock-h2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: link operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_link_create_open_store() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading\nSome text")
  (goto-char (point-min))
  (org-store-link nil)
  (let ((stored (car org-stored-links)))
    (goto-char (point-max))
    (org-insert-link nil stored "custom desc")
    (let ((link-text (buffer-substring-no-properties (point-min) (point-max))))
      (list stored link-text))))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: export preprocessing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_export_collect_headlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"Test\" 0 4 (:parent (#(\"Test\" 0 4 (:parent #4)))))) ((1 \"H1\" nil) (2 \"H2a\" nil) (2 \"H2b\" nil) (1 \"H3\" nil) (2 \"H3a\" nil) (3 \"H3a1\" nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Test\n* H1\n** H2a\n** H2b\n* H3\n** H3a\n*** H3a1")
    (let* ((info (org-export-get-environment nil))
           (tree (org-element-parse-buffer))
           (headlines (org-element-map tree 'headline
                        (lambda (h)
                          (list (org-element-property :level h)
                                (org-element-property :raw-value h)
                                (org-element-property :parent-type
                                  (org-element-property :parent h)))))))
      (list (plist-get info :title) headlines))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: element parsing with all properties
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_element_headline_all_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:todo \"TODO\" :priority 66 :tags (\"tag1\" \"tag2\") :title \"Title\" :level 1 :scheduled (timestamp (:standard-properties [42 nil nil nil 54 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2026-01-15>\" :year-start 2026 :month-start 1 :day-start 15 :hour-start nil :minute-start nil :year-end 2026 :month-end 1 :day-end 15 :hour-end nil :minute-end nil)) :deadline nil :begin 1 :end 116 :contents-begin 31 :contents-end 116)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#B] Title :tag1:tag2:\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\n:PROPERTIES:\n:VAR: val\n:END:\nBody text")
  (goto-char (point-min))
  (let* ((el (org-element-at-point))
         (props (list :todo (org-element-property :todo-keyword el)
                      :priority (org-element-property :priority el)
                      :tags (org-element-property :tags el)
                      :title (org-element-property :raw-value el)
                      :level (org-element-property :level el)
                      :scheduled (org-element-property :scheduled el)
                      :deadline (org-element-property :deadline el)
                      :begin (org-element-property :begin el)
                      :end (org-element-property :end el)
                      :contents-begin (org-element-property :contents-begin el)
                      :contents-end (org-element-property :contents-end el))))
    props))"##,
        expect,
    );
}

#[test]
fn strong_element_paragraph_with_markup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((bold nil 9 16) (italic nil 20 29) (underline nil 33 46) (verbatim \"code\" 50 57) (code \"verbatim\" 61 71))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "This is *bold* and /italic/ and _underlined_ and =code= and ~verbatim~")
  (goto-char (point-min))
  (let* ((tree (org-element-parse-buffer))
         (inlines (org-element-map tree '(bold italic underline code verbatim)
                    (lambda (el)
                      (list (org-element-type el)
                            (org-element-property :value el)
                            (org-element-property :begin el)
                            (org-element-property :end el))))))
    inlines))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: macro expansion with arguments
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_macro_expand_with_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Undefined Org macro: greet; aborting\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: greet Hello $1 $2!\n{{{greet(World, 42)}}}\n{{{greet(Foo, bar)}}}")
  (let ((raw (buffer-string)))
    (org-macro-replace-all org-macro-templates)
    (let ((expanded (buffer-substring-no-properties (point-min) (point-max)))
          (templates org-macro-templates))
      (list raw expanded templates))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: drawer operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_drawer_insert_toggle_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil org-fold-outline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Test\n:PROPERTIES:\n:VAR: val\n:END:\nBody\n* Another")
  (goto-char (point-min))
  (let ((vis1 (org-at-property-p)))
    (org-cycle-hide-drawers 'overview)
    (let ((hidden1 (get-char-property (line-end-position) 'invisible)))
      (org-cycle '(4))
      (let ((hidden2 (get-char-property (line-end-position) 'invisible)))
        (list vis1 hidden1 hidden2)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: babel source block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_babel_named_block_reference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"my-block\" \"emacs-lisp\" \"(+ 1 2)\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+NAME: my-block\n#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n\nSee {{{my-block}}}")
  (goto-char (point-min))
  (let* ((el (org-element-at-point))
         (name (org-element-property :name el))
         (lang (org-element-property :language el))
         (value (org-element-property :value el)))
    (list name lang value)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: dynamic block operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_dynamic_block_clocktable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK \"#+BEGIN: clocktable :maxlevel 2\\n#+CAPTION: Clock summary at [2026-06-15 Mon 12:00]\\n| Headline     | Time   |\\n|--------------+--------|\\n| *Total time* | *0:00* |\\n#+END:\"""##
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN: clocktable :maxlevel 2\n#+END:")
  (goto-char (point-min))
  (org-dblock-update)
  (let ((content (buffer-substring-no-properties (point-min) (point-max))))
    content))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: structure template expansion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_structure_template_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-try-structure-completion)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<s")
  (org-try-structure-completion)
  (let ((content (buffer-substring-no-properties (point-min) (point-max))))
    content))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: comment and planning toggle
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_comment_toggle_headline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"* TODO COMMENT Task\" \"* DONE COMMENT Task2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task\n* DONE Task2\n* WAITING Task3")
  (goto-char (point-min))
  (org-toggle-comment)
  (let ((h1 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
    (forward-line)
    (org-toggle-comment)
    (let ((h2 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
      (list h1 h2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: refile targets
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_refile_get_targets() {
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
// Deep state: agenda commands
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_agenda_todo_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-agenda)
  (with-temp-buffer
    (org-mode)
    (insert "* TODO Task 1\n* DONE Task 2\n* TODO Task 3\n* WAITING Task 4")
    (let* ((files (list (buffer-file-name)))
           (org-agenda-files files)
           (todo-entries (org-map-entries
                          (lambda ()
                            (list (org-get-heading t t t t)
                                  (org-get-todo-state)
                                  (org-entry-get nil "PRIORITY")))
                          nil 'file)))
      todo-entries)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: timer and duration
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_timer_start_stop_lap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-timer-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-timer)
  (org-timer-start)
  (sleep-for 0.1)
  (org-timer-item)
  (let ((content (buffer-substring-no-properties (point-min) (point-max)))
        (running (org-timer-p)))
    (list running content)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: column view format
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_colview_format_dynamics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-columns-get-format)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+COLUMNS: %25ITEM %TODO %3PRIORITY %TAGS\n* TODO [#A] Test :tag:")
  (goto-char (point-min))
  (let ((fmt (org-columns-get-format)))
    (list (nth 0 fmt) (nth 1 fmt) (nth 2 fmt))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: completion (pcomplete)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_pcomplete_entity_completion() {
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
// Deep state: multi-file parse consistency
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_parse_consistency_multiple_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"Buffer 1\" \"Sub 1\") (\"Buffer 2\" \"Sub 2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((results '()))
  (with-temp-buffer
    (org-mode)
    (insert "* Buffer 1\n** Sub 1\nBody 1")
    (let ((tree1 (org-element-parse-buffer)))
      (push (org-element-map tree1 'headline
              (lambda (h) (org-element-property :raw-value h)))
            results)))
  (with-temp-buffer
    (org-mode)
    (insert "* Buffer 2\n** Sub 2\nBody 2")
    (let ((tree2 (org-element-parse-buffer)))
      (push (org-element-map tree2 'headline
              (lambda (h) (org-element-property :raw-value h)))
            results)))
  (nreverse results))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: complex nested structure
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_complex_nested_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:title nil :todo-count 3 :scheduled 1 :clocks 1 :blocks 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Complex Test\n#+AUTHOR: Test\n* TODO [#A] Project :work:\n:PROPERTIES:\n:VAR: test\n:END:\n** TODO Sub-task 1\nSCHEDULED: <2026-01-15>\n- [ ] checkbox 1\n- [ ] checkbox 2\n** DONE Sub-task 2\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:00] =>  1:00\n:END:\n#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC")
  (let* ((tree (org-element-parse-buffer))
         (data (list :title (org-element-property :value
                               (car (org-element-map tree 'keyword
                                      (lambda (k) (equal (org-element-property :key k) "TITLE")))))
                     :todo-count (length (org-element-map tree 'headline
                                         (lambda (h) (org-element-property :todo-keyword h))))
                     :scheduled (length (org-element-map tree 'planning
                                         (lambda (p) (org-element-property :scheduled p))))
                     :clocks (length (org-element-map tree 'clock
                                      (lambda (c) c)))
                     :blocks (length (org-element-map tree 'src-block
                                       (lambda (b) b))))))
    data))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: edit sequence with undo
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_edit_sequence_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (user-error \"No undo information in this buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Original\nBody")
  (goto-char (point-min))
  (let ((s1 (buffer-string)))
    (org-edit-headline "Changed")
    (let ((s2 (buffer-string)))
      (org-toggle-tag "newtag" 'on)
      (let ((s3 (buffer-string)))
        (undo)
        (let ((s4 (buffer-string)))
          (undo)
          (let ((s5 (buffer-string)))
            (list s1 s2 s3 s4 s5)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: buffer-wide operations
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_buffer_wide_sort_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO C task\n* DONE A task\n* TODO B task\n* WAITING D task")
  (let ((before (org-map-entries (lambda () (org-get-heading t t t t)) nil 'file)))
    (org-sort-entries nil ?a)
    (let ((after (org-map-entries (lambda () (org-get-heading t t t t)) nil 'file)))
      (list before after))))"##,
        expect,
    );
}

#[test]
fn strong_buffer_wide_set_tags_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"global\") (\"new\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task 1\n* Task 2\n* Task 3")
  (goto-char (point-min))
  (org-set-tags '("global"))
  (let ((tags1 (org-get-tags nil t)))
    (org-set-tags '("new"))
    (let ((tags2 (org-get-tags nil t)))
      (list tags1 tags2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: statistics cookies
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_statistics_cookies_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"* TODO task [1/3]\" \"* TODO task [2/3]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO task [/]\n- [ ] item 1\n- [ ] item 2\n- [X] item 3")
  (goto-char (point-min))
  (org-update-statistics-cookies t)
  (let ((h (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
    (forward-line 2)
    (org-toggle-checkbox)
    (org-update-statistics-cookies t)
    (goto-char (point-min))
    (let ((h2 (buffer-substring-no-properties (line-beginning-position) (line-end-position))))
      (list h h2))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Deep state: visibility cycling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn strong_visibility_cycling_all_states() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 0) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n** H2b\n* H1b")
  (goto-char (point-min))
  (let ((states '()))
    ;; overview
    (org-set-startup-visibility 'overview)
    (push (list :overview
                (get-char-property (point) 'invisible)
                (progn (search-forward "H2") (get-char-property (point) 'invisible))
                (progn (search-forward "H3") (get-char-property (point) 'invisible)))
          states)
    ;; content
    (org-set-startup-visibility 'content)
    (goto-char (point-min))
    (push (list :content
                (get-char-property (point) 'invisible)
                (progn (search-forward "H2") (get-char-property (point) 'invisible))
                (progn (search-forward "H3") (get-char-property (point) 'invisible)))
          states)
    ;; all
    (org-set-startup-visibility 'all)
    (goto-char (point-min))
    (push (list :all
                (get-char-property (point) 'invisible)
                (progn (search-forward "H2") (get-char-property (point) 'invisible))
                (progn (search-forward "H3") (get-char-property (point) 'invisible)))
          states)
    (nreverse states)))"##,
        expect,
    );
}
