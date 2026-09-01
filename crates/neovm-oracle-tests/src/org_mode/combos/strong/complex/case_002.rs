//! Strong combo-complex-2 oracle tests — deep multi-step workflows.
//!
//! Every test chains multiple operations capturing deep mutable state to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Build doc with nested headings → move/indent → parent chain verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo2_nested_move() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init-chain (\"D\" \"C\" \"B\" \"A\" nil)) (:after-left (\"D\" \"B\" \"A\" nil)) (:after-right (\"D\" \"C\" \"B\" \"A\" nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\n*** C\n**** D\n* E")
  (let ((r '()))
    ;; initial parent chain for D
    (goto-char (point-min))
    (search-forward "D")
    (beginning-of-line)
    (let* ((obj (org-element-at-point))
           (chain '()))
      (let ((p obj))
        (while p
          (push (org-element-property :raw-value p) chain)
          (setq p (org-element-property :parent p))))
      (push (list :init-chain (nreverse chain)) r))
    ;; move D up one level (indent left)
    (org-metaleft)
    (let* ((obj (org-element-at-point))
           (chain '()))
      (let ((p obj))
        (while p
          (push (org-element-property :raw-value p) chain)
          (setq p (org-element-property :parent p))))
      (push (list :after-left (nreverse chain)) r))
    ;; move D back right
    (org-metaright)
    (let* ((obj (org-element-at-point))
           (chain '()))
      (let ((p obj))
        (while p
          (push (org-element-property :raw-value p) chain)
          (setq p (org-element-property :parent p))))
      (push (list :after-right (nreverse chain)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → cycle global → cycle local → verify visibility states
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo2_visibility_states() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:overview \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\\nSub\") (:global1 \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\\nSub\") (:global2 \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\\nSub\") (:global3 \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\\nSub\") (:local-children \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\\nSub\") (:local-subtree \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\\n** H2b\\nSub\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n* H1b\n** H2b\nSub")
  (let ((r '()))
    ;; overview
    (org-overview)
    (push (list :overview (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; global cycle 1
    (org-global-cycle nil)
    (push (list :global1 (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; global cycle 2
    (org-global-cycle nil)
    (push (list :global2 (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; global cycle 3
    (org-global-cycle nil)
    (push (list :global3 (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; local cycle on H1
    (goto-char (point-min))
    (org-cycle 'children)
    (push (list :local-children (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; local cycle subtree
    (org-cycle 'subtree)
    (push (list :local-subtree (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc with all object types → parse → verify all present
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo2_all_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* /italic/ _under_ +strike+ =code= ~verb~ [[http://a][Link]] $x^2$ \\alpha H_2O E=mc^2")
  (let ((r '()))
    ;; collect all object types
    (push (list :types (sort (delete-dups (org-element-map (org-element-parse-buffer) 'object 'org-element-type)) 'string<)) r)
    ;; collect all objects with content
    (push (list :objects (org-element-map (org-element-parse-buffer) '(bold italic underline strike-through code verbatim link latex-fragment entity subscript superscript)
                           (lambda (o) (list (org-element-type o)
                                             (org-trim (buffer-substring-no-properties
                                                         (org-element-property :contents-begin o)
                                                         (org-element-property :contents-end o))))))) r)
    ;; parent chain for bold
    (goto-char (point-min))
    (search-forward "bold")
    (let* ((obj (org-element-context))
           (chain '()))
      (let ((p obj))
        (while p
          (push (org-element-type p) chain)
          (setq p (org-element-property :parent p))))
      (push (list :bold-chain (nreverse chain)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → export html/latex/ascii → compare structure
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo2_export_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-string-as)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((src "* H\nBody *bold* /italic/"))
  (let ((html (org-export-string-as src 'html t))
        (latex (org-export-string-as src 'latex t))
        (ascii (org-export-string-as src 'ascii t)))
    (list (list :html-has-h2 (string-match-p "<h2>" html))
          (list :html-has-bold (string-match-p "<b>bold</b>" html))
          (list :html-has-italic (string-match-p "<i>italic</i>" html))
          (list :latex-has-section (string-match-p "\\\\section" latex))
          (list :latex-has-textbf (string-match-p "\\\\textbf" latex))
          (list :latex-has-textit (string-match-p "\\\\textit" latex))
          (list :ascii-has-h (string-match-p "H" ascii))
          (list :ascii-has-bold (string-match-p "bold" ascii)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex property operations → verify state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo2_props_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:after-put (\"1\" \"2\" \"3\")) (:after-update (\"1\" \"22\" \"3\")) (:after-delete (nil \"22\" \"3\")) (:multi (\"v1\" \"v2\" \"v3\")) (:content \"* T\\n:PROPERTIES:\\n:B:        22\\n:C:        3\\n:D:        v1 v2 v3\\n:END:\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T")
  (let ((r '()))
    (goto-char (point-min))
    ;; put multiple properties
    (org-entry-put nil "A" "1")
    (org-entry-put nil "B" "2")
    (org-entry-put nil "C" "3")
    (push (list :after-put (list (org-entry-get nil "A") (org-entry-get nil "B") (org-entry-get nil "C"))) r)
    ;; update B
    (org-entry-put nil "B" "22")
    (push (list :after-update (list (org-entry-get nil "A") (org-entry-get nil "B") (org-entry-get nil "C"))) r)
    ;; delete A
    (org-entry-delete nil "A")
    (push (list :after-delete (list (org-entry-get nil "A") (org-entry-get nil "B") (org-entry-get nil "C"))) r)
    ;; multivalued property
    (org-entry-put-multivalued-property nil "D" "v1" "v2" "v3")
    (push (list :multi (org-entry-get-multivalued-property nil "D")) r)
    ;; verify buffer
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex tag operations → verify state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo2_tags_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init (\"existing\")) (:after-add (\"existing\" \"new\")) (:after-remove (\"new\")) (:after-set (\"a\" \"b\" \"c\")) (:after-toggle (\"b\" \"c\")) (:content \"* T                                                                     :b:c:\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T :existing:")
  (let ((r '()))
    (goto-char (point-min))
    ;; initial tags
    (push (list :init (org-get-tags)) r)
    ;; add tag
    (org-toggle-tag "new")
    (push (list :after-add (org-get-tags)) r)
    ;; remove existing
    (org-toggle-tag "existing")
    (push (list :after-remove (org-get-tags)) r)
    ;; set tags directly
    (org-set-tags '("a" "b" "c"))
    (push (list :after-set (org-get-tags)) r)
    ;; toggle a
    (org-toggle-tag "a")
    (push (list :after-toggle (org-get-tags)) r)
    ;; verify buffer
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex todo operations → verify state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo2_todo_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init nil) (:cycle #(\"TODO\" 0 4 (org-todo-head \"TODO\"))) (:cycle #(\"DONE\" 0 4 (org-todo-head \"TODO\"))) (:cycle nil) (:cycle #(\"TODO\" 0 4 (org-todo-head \"TODO\"))) (:cycle #(\"DONE\" 0 4 (org-todo-head \"TODO\"))) (:cycle nil) (:content #(\"* H\" 0 3 (org-todo-head \"TODO\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (let ((r '()))
    (goto-char (point-min))
    ;; initial
    (push (list :init (org-get-todo-state)) r)
    ;; cycle 6 times
    (dotimes (_ 6)
      (org-todo)
      (push (list :cycle (org-get-todo-state)) r))
    ;; verify buffer
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex priority operations → verify state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo2_priority_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init 66) (:up 65) (:up nil) (:up 67) (:down nil) (:down 65) (:down 66) (:content \"* [#B] T\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* [#B] T")
  (let ((r '()))
    (goto-char (point-min))
    ;; initial
    (push (list :init (org-element-property :priority (org-element-at-point))) r)
    ;; priority up 3 times
    (dotimes (_ 3)
      (org-priority-up)
      (push (list :up (org-element-property :priority (org-element-at-point))) r))
    ;; priority down 3 times
    (dotimes (_ 3)
      (org-priority-down)
      (push (list :down (org-element-property :priority (org-element-at-point))) r))
    ;; verify buffer
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex list operations → verify state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo2_list_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init (\"A\" \"B\" \"C\" \"D\")) (:after-indent ((nil \"A\\n  - B\") (nil \"B\") (nil \"C\") (nil \"D\"))) (:after-move ((nil \"C\") (nil \"A\\n  - B\") (nil \"B\") (nil \"D\"))) (:after-dedent ((nil \"C\") (nil \"A\") (nil \"B\") (nil \"D\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C\n- D")
  (let ((r '()))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'item
                        (lambda (i) (org-trim (buffer-substring-no-properties
                                                (org-element-property :contents-begin i)
                                                (org-element-property :contents-end i)))))) r)
    ;; indent B under A
    (goto-char (point-min))
    (forward-line 1)
    (org-metaright)
    (push (list :after-indent (org-element-map (org-element-parse-buffer) 'item
                                (lambda (i) (list (org-element-property :level i)
                                                  (org-trim (buffer-substring-no-properties
                                                              (org-element-property :contents-begin i)
                                                              (org-element-property :contents-end i))))))) r)
    ;; move C up
    (goto-char (point-min))
    (forward-line 2)
    (org-metaup)
    (push (list :after-move (org-element-map (org-element-parse-buffer) 'item
                              (lambda (i) (list (org-element-property :level i)
                                                (org-trim (buffer-substring-no-properties
                                                            (org-element-property :contents-begin i)
                                                            (org-element-property :contents-end i))))))) r)
    ;; dedent B
    (goto-char (point-min))
    (search-forward "B")
    (beginning-of-line)
    (org-metaleft)
    (push (list :after-dedent (org-element-map (org-element-parse-buffer) 'item
                                (lambda (i) (list (org-element-property :level i)
                                                  (org-trim (buffer-substring-no-properties
                                                              (org-element-property :contents-begin i)
                                                              (org-element-property :contents-end i))))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex table operations → verify state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo2_table_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init \"| a | b |\\n| 1 | 2 |\\n| 3 | 4 |\") (:rows 3) (:after-row #(\"| a | b |   |\\n| 1 | 2 |   |\\n| 5 | 6 |   |\\n| 3 | 4 |   |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table rear-nonsticky t display (space :relative-width 1)) 10 11 (face org-table) 11 12 (face org-table display (space :relative-width 1.001)) 12 13 (face org-table) 13 14 (face org-table-row) 14 15 (face org-table) 15 16 (face org-table rear-nonsticky t display (space :relative-width 1)) 16 17 (face org-table) 17 18 (face org-table display (space :relative-width 1.001)) 18 19 (face org-table) 19 20 (face org-table rear-nonsticky t display (space :relative-width 1)) 20 21 (face org-table) 21 22 (face org-table display (space :relative-width 1.001)) 22 23 (face org-table) 23 24 (face org-table rear-nonsticky t display (space :relative-width 1)) 24 25 (face org-table) 25 26 (face org-table display (space :relative-width 1.001)) 26 27 (face org-table) 27 28 (face org-table-row) 28 29 (face org-table) 29 30 (face org-table rear-nonsticky t display (space :relative-width 1)) 30 31 (face org-table) 31 32 (face org-table display (space :relative-width 1.001)) 32 33 (face org-table) 33 34 (face org-table rear-nonsticky t display (space :relative-width 1)) 34 35 (face org-table) 35 36 (face org-table display (space :relative-width 1.001)) 36 37 (face org-table) 37 38 (face org-table rear-nonsticky t display (space :relative-width 1)) 38 39 (face org-table) 39 40 (face org-table display (space :relative-width 1.001)) 40 41 (face org-table) 41 42 (face org-table-row) 42 43 (face org-table) 43 44 (face org-table rear-nonsticky t display (space :relative-width 1)) 44 45 (face org-table) 45 46 (face org-table display (space :relative-width 1.001)) 46 47 (face org-table) 47 48 (face org-table rear-nonsticky t display (space :relative-width 1)) 48 49 (face org-table) 49 50 (face org-table display (space :relative-width 1.001)) 50 51 (face org-table) 51 52 (face org-table rear-nonsticky t display (space :relative-width 1)) 52 53 (face org-table) 53 54 (face org-table display (space :relative-width 1.001)) 54 55 (face org-table) 55 56 (face org-table-row))) (:after-col #(\"| a |   | b |   |\\n| 1 |   | 2 |   |\\n| 5 |   | 6 |   |\\n| 3 |   | 4 |   |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table rear-nonsticky t display (space :relative-width 1)) 10 11 (face org-table) 11 12 (face org-table display (space :relative-width 1.001)) 12 13 (face org-table) 13 14 (face org-table rear-nonsticky t display (space :relative-width 1)) 14 15 (face org-table) 15 16 (face org-table display (space :relative-width 1.001)) 16 17 (face org-table) 17 18 (face org-table-row) 18 19 (face org-table) 19 20 (face org-table rear-nonsticky t display (space :relative-width 1)) 20 21 (face org-table) 21 22 (face org-table display (space :relative-width 1.001)) 22 23 (face org-table) 23 24 (face org-table rear-nonsticky t display (space :relative-width 1)) 24 25 (face org-table) 25 26 (face org-table display (space :relative-width 1.001)) 26 27 (face org-table) 27 28 (face org-table rear-nonsticky t display (space :relative-width 1)) 28 29 (face org-table) 29 30 (face org-table display (space :relative-width 1.001)) 30 31 (face org-table) 31 32 (face org-table rear-nonsticky t display (space :relative-width 1)) 32 33 (face org-table) 33 34 (face org-table display (space :relative-width 1.001)) 34 35 (face org-table) 35 36 (face org-table-row) 36 37 (face org-table) 37 38 (face org-table rear-nonsticky t display (space :relative-width 1)) 38 39 (face org-table) 39 40 (face org-table display (space :relative-width 1.001)) 40 41 (face org-table) 41 42 (face org-table rear-nonsticky t display (space :relative-width 1)) 42 43 (face org-table) 43 44 (face org-table display (space :relative-width 1.001)) 44 45 (face org-table) 45 46 (face org-table rear-nonsticky t display (space :relative-width 1)) 46 47 (face org-table) 47 48 (face org-table display (space :relative-width 1.001)) 48 49 (face org-table) 49 50 (face org-table rear-nonsticky t display (space :relative-width 1)) 50 51 (face org-table) 51 52 (face org-table display (space :relative-width 1.001)) 52 53 (face org-table) 53 54 (face org-table-row) 54 55 (face org-table) 55 56 (face org-table rear-nonsticky t display (space :relative-width 1)) 56 57 (face org-table) 57 58 (face org-table display (space :relative-width 1.001)) 58 59 (face org-table) 59 60 (face org-table rear-nonsticky t display (space :relative-width 1)) 60 61 (face org-table) 61 62 (face org-table display (space :relative-width 1.001)) 62 63 (face org-table) 63 64 (face org-table rear-nonsticky t display (space :relative-width 1)) 64 65 (face org-table) 65 66 (face org-table display (space :relative-width 1.001)) 66 67 (face org-table) 67 68 (face org-table rear-nonsticky t display (space :relative-width 1)) 68 69 (face org-table) 69 70 (face org-table display (space :relative-width 1.001)) 70 71 (face org-table) 71 72 (face org-table-row))) (:after-sort #(\"| a |   | b |   |\\n| 1 |   | 2 |   |\\n| 5 |   | 6 |   |\\n| 3 |   | 4 |   |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table rear-nonsticky t display (space :relative-width 1)) 10 11 (face org-table) 11 12 (face org-table display (space :relative-width 1.001)) 12 13 (face org-table) 13 14 (face org-table rear-nonsticky t display (space :relative-width 1)) 14 15 (face org-table) 15 16 (face org-table display (space :relative-width 1.001)) 16 17 (face org-table) 17 18 (face org-table-row) 18 19 (face org-table) 19 20 (face org-table rear-nonsticky t display (space :relative-width 1)) 20 21 (face org-table) 21 22 (face org-table display (space :relative-width 1.001)) 22 23 (face org-table) 23 24 (face org-table rear-nonsticky t display (space :relative-width 1)) 24 25 (face org-table) 25 26 (face org-table display (space :relative-width 1.001)) 26 27 (face org-table) 27 28 (face org-table rear-nonsticky t display (space :relative-width 1)) 28 29 (face org-table) 29 30 (face org-table display (space :relative-width 1.001)) 30 31 (face org-table) 31 32 (face org-table rear-nonsticky t display (space :relative-width 1)) 32 33 (face org-table) 33 34 (face org-table display (space :relative-width 1.001)) 34 35 (face org-table) 35 36 (face org-table-row) 36 37 (face org-table) 37 38 (face org-table rear-nonsticky t display (space :relative-width 1)) 38 39 (face org-table) 39 40 (face org-table display (space :relative-width 1.001)) 40 41 (face org-table) 41 42 (face org-table rear-nonsticky t display (space :relative-width 1)) 42 43 (face org-table) 43 44 (face org-table display (space :relative-width 1.001)) 44 45 (face org-table) 45 46 (face org-table rear-nonsticky t display (space :relative-width 1)) 46 47 (face org-table) 47 48 (face org-table display (space :relative-width 1.001)) 48 49 (face org-table) 49 50 (face org-table rear-nonsticky t display (space :relative-width 1)) 50 51 (face org-table) 51 52 (face org-table display (space :relative-width 1.001)) 52 53 (face org-table) 53 54 (face org-table-row) 54 55 (face org-table) 55 56 (face org-table rear-nonsticky t display (space :relative-width 1)) 56 57 (face org-table) 57 58 (face org-table display (space :relative-width 1.001)) 58 59 (face org-table) 59 60 (face org-table rear-nonsticky t display (space :relative-width 1)) 60 61 (face org-table) 61 62 (face org-table display (space :relative-width 1.001)) 62 63 (face org-table) 63 64 (face org-table rear-nonsticky t display (space :relative-width 1)) 64 65 (face org-table) 65 66 (face org-table display (space :relative-width 1.001)) 66 67 (face org-table) 67 68 (face org-table rear-nonsticky t display (space :relative-width 1)) 68 69 (face org-table) 69 70 (face org-table display (space :relative-width 1.001)) 70 71 (face org-table) 71 72 (face org-table-row))) (:after-transpose #(\"| a | 1 | 5 | 3 |\\n|   |   |   |   |\\n| b | 2 | 6 | 4 |\\n|   |   |   |   |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table rear-nonsticky t display (space :relative-width 1)) 10 11 (face org-table) 11 12 (face org-table display (space :relative-width 1.001)) 12 13 (face org-table) 13 14 (face org-table rear-nonsticky t display (space :relative-width 1)) 14 15 (face org-table) 15 16 (face org-table display (space :relative-width 1.001)) 16 17 (face org-table) 17 18 (face org-table-row) 18 19 (face org-table) 19 20 (face org-table rear-nonsticky t display (space :relative-width 1)) 20 21 (face org-table) 21 22 (face org-table display (space :relative-width 1.001)) 22 23 (face org-table) 23 24 (face org-table rear-nonsticky t display (space :relative-width 1)) 24 25 (face org-table) 25 26 (face org-table display (space :relative-width 1.001)) 26 27 (face org-table) 27 28 (face org-table rear-nonsticky t display (space :relative-width 1)) 28 29 (face org-table) 29 30 (face org-table display (space :relative-width 1.001)) 30 31 (face org-table) 31 32 (face org-table rear-nonsticky t display (space :relative-width 1)) 32 33 (face org-table) 33 34 (face org-table display (space :relative-width 1.001)) 34 35 (face org-table) 35 36 (face org-table-row) 36 37 (face org-table) 37 38 (face org-table rear-nonsticky t display (space :relative-width 1)) 38 39 (face org-table) 39 40 (face org-table display (space :relative-width 1.001)) 40 41 (face org-table) 41 42 (face org-table rear-nonsticky t display (space :relative-width 1)) 42 43 (face org-table) 43 44 (face org-table display (space :relative-width 1.001)) 44 45 (face org-table) 45 46 (face org-table rear-nonsticky t display (space :relative-width 1)) 46 47 (face org-table) 47 48 (face org-table display (space :relative-width 1.001)) 48 49 (face org-table) 49 50 (face org-table rear-nonsticky t display (space :relative-width 1)) 50 51 (face org-table) 51 52 (face org-table display (space :relative-width 1.001)) 52 53 (face org-table) 53 54 (face org-table-row) 54 55 (face org-table) 55 56 (face org-table rear-nonsticky t display (space :relative-width 1)) 56 57 (face org-table) 57 58 (face org-table display (space :relative-width 1.001)) 58 59 (face org-table) 59 60 (face org-table rear-nonsticky t display (space :relative-width 1)) 60 61 (face org-table) 61 62 (face org-table display (space :relative-width 1.001)) 62 63 (face org-table) 63 64 (face org-table rear-nonsticky t display (space :relative-width 1)) 64 65 (face org-table) 65 66 (face org-table display (space :relative-width 1.001)) 66 67 (face org-table) 67 68 (face org-table rear-nonsticky t display (space :relative-width 1)) 68 69 (face org-table) 69 70 (face org-table display (space :relative-width 1.001)) 70 71 (face org-table) 71 72 (face org-table-row))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |\n| 3 | 4 |")
  (let ((r '()))
    ;; initial
    (push (list :init (buffer-string)) r)
    (push (list :rows (length (org-element-map (org-element-parse-buffer) 'table-row 'identity))) r)
    ;; add row
    (goto-char (point-max))
    (org-table-insert-row)
    (insert "5 | 6")
    (org-table-align)
    (push (list :after-row (buffer-string)) r)
    ;; add column
    (org-table-insert-column)
    (push (list :after-col (buffer-string)) r)
    ;; sort
    (org-table-sort-lines nil ?a)
    (push (list :after-sort (buffer-string)) r)
    ;; transpose
    (goto-char (point-min))
    (org-table-transpose-table-at-point)
    (push (list :after-transpose (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex src operations → verify state
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo2_src_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((:init \"#+BEGIN_SRC emacs-lisp\\n(+ 1)\\n(+ 2)\\n(+ 3)\\n#+END_SRC\") (:after-demarcate \"#+BEGIN_SRC emacs-lisp\\n  (+ 1)\\n#+END_SRC\\n\\n#+BEGIN_SRC emacs-lisp\\n  (+ 2)\\n  (+ 3)\\n#+END_SRC\\n\") (:blocks ((\"emacs-lisp\" \"  (+ 1)\\n\") (\"emacs-lisp\" \"  (+ 2)\\n  (+ 3)\\n\"))))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n(+ 2)\n(+ 3)\n#+END_SRC")
  (let ((r '()))
    ;; initial
    (push (list :init (buffer-string)) r)
    ;; demarcate block
    (goto-char (point-min))
    (search-forward "(+ 2)")
    (beginning-of-line)
    (org-babel-demarcate-block)
    (push (list :after-demarcate (buffer-string)) r)
    ;; verify src blocks
    (push (list :blocks (org-element-map (org-element-parse-buffer) 'src-block
                          (lambda (s) (list (org-element-property :language s)
                                            (org-element-property :value s))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex heading navigation → verify positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo2_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 2) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\n*** C\n** D\n* E\n** F")
  (let ((r '()))
    ;; forward same level
    (goto-char (point-min))
    (org-forward-heading-same-level 1)
    (push (list :fwd1 (org-element-property :raw-value (org-element-at-point))) r)
    (org-forward-heading-same-level 1)
    (push (list :fwd2 (org-element-property :raw-value (org-element-at-point))) r)
    ;; backward same level
    (org-backward-heading-same-level 1)
    (push (list :back1 (org-element-property :raw-value (org-element-at-point))) r)
    ;; up heading
    (org-up-heading)
    (push (list :up1 (org-element-property :raw-value (org-element-at-point))) r)
    ;; next visible
    (goto-char (point-min))
    (org-next-visible-heading 1)
    (push (list :next1 (org-element-property :raw-value (org-element-at-point))) r)
    (org-next-visible-heading 1)
    (push (list :next2 (org-element-property :raw-value (org-element-at-point))) r)
    ;; previous visible
    (org-previous-visible-heading 1)
    (push (list :prev1 (org-element-property :raw-value (org-element-at-point))) r)
    (nreverse r)))"##,
        expect,
    );
}
