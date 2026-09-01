//! Strong combo-complex-1 oracle tests — multi-step org workflows capturing deep state.
//!
//! Every test chains multiple operations and captures intermediate state to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Build doc → parse → modify structure → reparse → compare
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_build_modify_reparse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init (\"A\" \"B\" \"C\")) (:after-move (\"B\" \"A\" \"C\")) (:after-insert (\"B\" \"A\" \"C\" \"D\")) (:after-indent ((2 \"B\") (1 \"A\") (1 \"C\") (1 \"D\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n* B\n* C")
  (let ((r '()))
    ;; initial parse
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h)))) r)
    ;; move B before A
    (goto-char (point-min))
    (org-metadown)
    (push (list :after-move (org-element-map (org-element-parse-buffer) 'headline
                              (lambda (h) (org-element-property :raw-value h)))) r)
    ;; add heading at end
    (goto-char (point-max))
    (insert "\n* D")
    (push (list :after-insert (org-element-map (org-element-parse-buffer) 'headline
                                (lambda (h) (org-element-property :raw-value h)))) r)
    ;; indent B under A
    (goto-char (point-min))
    (search-forward "B")
    (beginning-of-line)
    (org-metaright)
    (push (list :after-indent (org-element-map (org-element-parse-buffer) 'headline
                                (lambda (h) (list (org-element-property :level h)
                                                  (org-element-property :raw-value h))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build list → indent/dedent → checkbox toggle → verify counts
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_list_indent_checkbox() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init ((on \"A\") (off \"B\") (off \"C\") (on \"D\"))) (:after-indent ((nil off \"A\\n  - [ ] B\\n  - [ ] C\") (nil off \"B\") (nil off \"C\") (nil on \"D\"))) (:after-toggle ((nil trans) (nil on) (nil off) (nil on))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- [X] A\n- [ ] B\n- [ ] C\n- [X] D")
  (let ((r '()))
    ;; initial state
    (push (list :init (org-element-map (org-element-parse-buffer) 'item
                        (lambda (i) (list (org-element-property :checkbox i)
                                          (org-trim (buffer-substring-no-properties
                                                      (org-element-property :contents-begin i)
                                                      (org-element-property :contents-end i))))))) r)
    ;; indent B and C under A
    (goto-char (point-min))
    (forward-line 1)
    (org-metaright)
    (forward-line)
    (org-metaright)
    (push (list :after-indent (org-element-map (org-element-parse-buffer) 'item
                                (lambda (i) (list (org-element-property :level i)
                                                  (org-element-property :checkbox i)
                                                  (org-trim (buffer-substring-no-properties
                                                              (org-element-property :contents-begin i)
                                                              (org-element-property :contents-end i))))))) r)
    ;; toggle checkbox on B
    (goto-char (point-min))
    (forward-line 1)
    (org-toggle-checkbox)
    (push (list :after-toggle (org-element-map (org-element-parse-buffer) 'item
                                (lambda (i) (list (org-element-property :level i)
                                                  (org-element-property :checkbox i))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build table → add row → add column → eval formula → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_table_build_formula() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a table\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |\n| 3 | 4 |")
  (let ((r '()))
    ;; initial state
    (push (list :init (buffer-string)) r)
    (push (list :rows (length (org-element-map (org-element-parse-buffer) 'table-row 'identity))) r)
    (push (list :cells (length (org-element-map (org-element-parse-buffer) 'table-cell 'identity))) r)
    ;; add column
    (org-table-insert-column)
    (push (list :after-col (buffer-string)) r)
    ;; add formula
    (goto-char (point-max))
    (insert "\n#+TBLFM: $3=$1+$2")
    (org-table-iterate)
    (push (list :after-formula (buffer-string)) r)
    ;; verify cell values
    (goto-char (point-min))
    (forward-line 1)
    (push (list :cell1 (org-table-get "1" "3")) r)
    (forward-line)
    (push (list :cell2 (org-table-get "2" "3")) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc with planning → modify schedule → verify planning state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_planning_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init nil) (:after-sched ((\"S\" nil))) (:after-dead ((\"S\" \"D\"))) (:content \"* TODO T\\nDEADLINE: <2026-01-20 Tue> SCHEDULED: <2026-01-15 Thu>\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T")
  (let ((r '()))
    ;; initial state
    (push (list :init (org-element-map (org-element-parse-buffer) 'planning 'identity)) r)
    ;; add schedule
    (goto-char (point-min))
    (org-schedule nil "<2026-01-15>")
    (push (list :after-sched (org-element-map (org-element-parse-buffer) 'planning
                                (lambda (p) (list (when (org-element-property :scheduled p) "S")
                                                  (when (org-element-property :deadline p) "D"))))) r)
    ;; add deadline
    (org-deadline nil "<2026-01-20>")
    (push (list :after-dead (org-element-map (org-element-parse-buffer) 'planning
                              (lambda (p) (list (when (org-element-property :scheduled p) "S")
                                                (when (org-element-property :deadline p) "D"))))) r)
    ;; verify buffer content
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc with tags → toggle tags → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_tags_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init (\"existing\")) (:after-add (\"existing\" \"new\")) (:after-remove (\"new\")) (:after-toggle nil) (:content \"* T\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T :existing:")
  (let ((r '()))
    ;; initial
    (push (list :init (org-get-tags)) r)
    ;; add tag
    (goto-char (point-min))
    (org-toggle-tag "new")
    (push (list :after-add (org-get-tags)) r)
    ;; remove existing
    (org-toggle-tag "existing")
    (push (list :after-remove (org-get-tags)) r)
    ;; toggle new again
    (org-toggle-tag "new")
    (push (list :after-toggle (org-get-tags)) r)
    ;; verify buffer
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc with properties → put/delete → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_props_crud() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init nil) (:after-put (\"1\" \"2\" \"3\")) (:after-update (\"1\" \"22\" \"3\")) (:after-delete (nil \"22\" \"3\")) (:content \"* T\\n:PROPERTIES:\\n:B:        22\\n:C:        3\\n:END:\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (let ((r '()))
    ;; initial
    (push (list :init (org-entry-get nil "A")) r)
    ;; put
    (goto-char (point-min))
    (org-entry-put nil "A" "1")
    (org-entry-put nil "B" "2")
    (org-entry-put nil "C" "3")
    (push (list :after-put (list (org-entry-get nil "A") (org-entry-get nil "B") (org-entry-get nil "C"))) r)
    ;; update
    (org-entry-put nil "B" "22")
    (push (list :after-update (list (org-entry-get nil "A") (org-entry-get nil "B") (org-entry-get nil "C"))) r)
    ;; delete
    (org-entry-delete nil "A")
    (push (list :after-delete (list (org-entry-get nil "A") (org-entry-get nil "B") (org-entry-get nil "C"))) r)
    ;; verify buffer
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc with todo → cycle → verify state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_todo_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init nil) (:cycle #(\"TODO\" 0 4 (org-todo-head \"TODO\"))) (:cycle #(\"DONE\" 0 4 (org-todo-head \"TODO\"))) (:cycle nil) (:cycle #(\"TODO\" 0 4 (org-todo-head \"TODO\"))) (:content #(\"* TODO H\" 0 8 (org-todo-head \"TODO\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (let ((r '()))
    ;; initial
    (goto-char (point-min))
    (push (list :init (org-get-todo-state)) r)
    ;; cycle 4 times
    (dotimes (_ 4)
      (org-todo)
      (push (list :cycle (org-get-todo-state)) r))
    ;; verify buffer
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc with src block → execute → verify results
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_src_execute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""Evaluate this emacs-lisp code block on your system? (yes or no) OK nil""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC")
  (let ((r '()))
    ;; initial
    (push (list :init (buffer-string)) r)
    ;; execute
    (goto-char (point-min))
    (org-babel-execute-src-block)
    ;; verify
    (push (list :after-exec (buffer-string)) r)
    (push (list :results (org-element-map (org-element-parse-buffer) 'fixed-width
                           (lambda (fw) (org-element-property :value fw)))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc with links → parse → verify link properties
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_links_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:links ((\"http\" \"//a.com\" \"http://a.com\") (\"file\" \"b.el\" \"file:b.el\") (\"id\" \"xxx\" \"id:xxx\") (\"mailto\" \"d@e.com\" \"mailto:d@e.com\"))) (:count 4) (:chain (link paragraph section headline org-data)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n[[http://a.com][A]] [[file:b.el][B]] [[id:xxx][C]] [[mailto:d@e.com]]")
  (let ((r '()))
    ;; parse links
    (push (list :links (org-element-map (org-element-parse-buffer) 'link
                          (lambda (l) (list (org-element-property :type l)
                                            (org-element-property :path l)
                                            (org-element-property :raw-link l))))) r)
    ;; verify counts
    (push (list :count (length (org-element-map (org-element-parse-buffer) 'link 'identity))) r)
    ;; verify parent chain
    (goto-char (point-min))
    (search-forward "A")
    (let* ((obj (org-element-context))
           (chain '()))
      (let ((p obj))
        (while p
          (push (org-element-type p) chain)
          (setq p (org-element-property :parent p))))
      (push (list :chain (nreverse chain)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc with footnotes → verify refs/defs
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_footnotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:refs (\"1\" \"2\")) (:defs ((\"1\" \"First def\") (\"2\" \"Second def\"))) (:ref-count 2) (:def-count 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] more[fn:2] end\n\n[fn:1] First def\n[fn:2] Second def")
  (let ((r '()))
    ;; parse footnotes
    (push (list :refs (org-element-map (org-element-parse-buffer) 'footnote-reference
                        (lambda (f) (org-element-property :label f)))) r)
    (push (list :defs (org-element-map (org-element-parse-buffer) 'footnote-definition
                        (lambda (f) (list (org-element-property :label f)
                                          (org-trim (buffer-substring-no-properties
                                                      (org-element-property :contents-begin f)
                                                      (org-element-property :contents-end f))))))) r)
    ;; verify counts
    (push (list :ref-count (length (org-element-map (org-element-parse-buffer) 'footnote-reference 'identity))) r)
    (push (list :def-count (length (org-element-map (org-element-parse-buffer) 'footnote-definition 'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build complex doc → full element distribution → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_full_distribution() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Complex\n#+FILETAGS: :t1:t2:\n* TODO [#A] H1 :work:\nSCHEDULED: <2026-01-15>\nBody *bold* /italic/\n** H2\n- [X] a\n- [ ] b\n| x | y |\n|---+---|\n| 1 | 2 |\n#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\n* DONE [#B] H2 :home:\n:PROPERTIES:\n:A: 1\n:END:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:00] =>  1:00")
  (let ((types (org-element-map (org-element-parse-buffer) 'element 'org-element-type)))
    (list (length types)
          (sort (delete-dups (copy-sequence types)) 'string<))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → cycle visibility → verify buffer state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_cycle_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:overview \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\") (:content \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\") (:all \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\") (:children \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\") (:subtree \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n* H1b\n** H2b")
  (let ((r '()))
    ;; overview
    (org-overview)
    (push (list :overview (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; content
    (org-content)
    (push (list :content (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; all
    (org-show-all)
    (push (list :all (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; children
    (goto-char (point-min))
    (org-cycle 'children)
    (push (list :children (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; subtree
    (org-cycle 'subtree)
    (push (list :subtree (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → move headings → verify order
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_move_headings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init (\"A\" \"B\" \"C\" \"D\")) (:after-down (\"A\" \"C\" \"B\" \"D\")) (:after-up (\"A\" \"B\" \"C\" \"D\")) (:after-right ((1 \"A\") (2 \"B\") (1 \"C\") (1 \"D\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n* B\n* C\n* D")
  (let ((r '()))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h)))) r)
    ;; move B down
    (goto-char (point-min))
    (forward-line 1)
    (org-metadown)
    (push (list :after-down (org-element-map (org-element-parse-buffer) 'headline
                              (lambda (h) (org-element-property :raw-value h)))) r)
    ;; move C up
    (goto-char (point-min))
    (forward-line 2)
    (org-metaup)
    (push (list :after-up (org-element-map (org-element-parse-buffer) 'headline
                            (lambda (h) (org-element-property :raw-value h)))) r)
    ;; move B right (indent)
    (goto-char (point-min))
    (search-forward "B")
    (beginning-of-line)
    (org-metaright)
    (push (list :after-right (org-element-map (org-element-parse-buffer) 'headline
                                (lambda (h) (list (org-element-property :level h)
                                                  (org-element-property :raw-value h))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → sort headings → verify order
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_sort_headings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Zebra\n* Apple\n* Mango\n* Banana")
  (let ((r '()))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h)))) r)
    ;; sort alphabetically
    (org-sort-entries nil ?a)
    (push (list :after-sort (org-element-map (org-element-parse-buffer) 'headline
                              (lambda (h) (org-element-property :raw-value h)))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → clone subtree → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_clone_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-clone-subtree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n** Sub1\n** Sub2")
  (let ((r '()))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (list (org-element-property :level h)
                                          (org-element-property :raw-value h))))) r)
    ;; clone
    (goto-char (point-min))
    (org-clone-subtree 2)
    (push (list :after-clone (org-element-map (org-element-parse-buffer) 'headline
                                (lambda (h) (list (org-element-property :level h)
                                                  (org-element-property :raw-value h))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → narrow → show context → widen → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_narrow_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:narrowed \"** H2\\n*** H3\\nBody\") (:context \"** H2\\n*** H3\\nBody\") (:widened \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n* H1b\n** H2b")
  (let ((r '()))
    ;; narrow to H2
    (goto-char (point-min))
    (search-forward "H2")
    (beginning-of-line)
    (org-narrow-to-subtree)
    (push (list :narrowed (buffer-string)) r)
    ;; show context
    (org-show-context 'agenda)
    (push (list :context (buffer-string)) r)
    ;; widen
    (widen)
    (push (list :widened (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → toggle heading → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_toggle_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init (\"H1\" \"H2\" \"H3\")) (:after-toggle ((headline \"H1\") (headline \"H3\"))) (:after-restore (\"H1\" \"H2\" \"H3\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2\n* H3")
  (let ((r '()))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h)))) r)
    ;; toggle H2 to plain list
    (goto-char (point-min))
    (forward-line 1)
    (org-toggle-heading)
    (push (list :after-toggle (org-element-map (org-element-parse-buffer) '(headline plain-list item)
                                (lambda (e) (list (org-element-type e)
                                                  (org-element-property :raw-value e))))) r)
    ;; toggle back
    (goto-char (point-min))
    (forward-line 1)
    (org-toggle-heading)
    (push (list :after-restore (org-element-map (org-element-parse-buffer) 'headline
                                  (lambda (h) (org-element-property :raw-value h)))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → insert various elements → verify full parse
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_insert_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:types nil) (:count 0) (:content \"* H\\nBody\\n#+BEGIN_SRC emacs-lisp\\n(+ 1)\\n#+END_SRC\\n#+BEGIN_QUOTE\\nQuoted\\n#+END_QUOTE\\n- item1\\n- item2\\n| a | b |\\n| 1 | 2 |\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody")
  (let ((r '()))
    ;; insert src block
    (goto-char (point-max))
    (insert "\n#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC")
    ;; insert quote
    (insert "\n#+BEGIN_QUOTE\nQuoted\n#+END_QUOTE")
    ;; insert list
    (insert "\n- item1\n- item2")
    ;; insert table
    (insert "\n| a | b |\n| 1 | 2 |")
    ;; verify
    (let ((types (org-element-map (org-element-parse-buffer) 'element 'org-element-type)))
      (push (list :types (sort (delete-dups (copy-sequence types)) 'string<)) r)
      (push (list :count (length types)) r))
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → export string → verify output structure
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_export_verify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-string-as)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((html (org-export-string-as "* H\nBody *bold*" 'html t)))
  (list (string-match-p "<h2>" html)
        (string-match-p "<b>bold</b>" html)
        (string-match-p "</body>" html)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → element-map with predicate → verify filtered results
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo1_map_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:all (\"A\" \"B\" \"C\" \"D\" \"WAITING E\")) (:todo (\"A\" \"B\" \"C\" \"D\" \"WAITING E\")) (:done (\"A\" \"B\" \"C\" \"D\" \"WAITING E\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C\n* DONE D\n* WAITING E")
  (let ((r '()))
    ;; all headlines
    (push (list :all (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h)))) r)
    ;; only TODO
    (push (list :todo (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h))
                        nil nil nil
                        (lambda (h) (string= (org-element-property :todo-keyword h) "TODO")))) r)
    ;; only DONE
    (push (list :done (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h))
                        nil nil nil
                        (lambda (h) (string= (org-element-property :todo-keyword h) "DONE")))) r)
    (nreverse r)))"##,
        expect,
    );
}
