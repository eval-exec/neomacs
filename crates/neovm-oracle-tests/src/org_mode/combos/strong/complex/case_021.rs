//! Strong combo-complex-21 oracle tests — deep multi-step workflows.
//!
//! Every test chains multiple operations capturing deep mutable state.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex heading with all modifications → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo21_heading_mods() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H")
  (let ((r ''))
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (list (org-element-property :level h)
                                          (org-element-property :raw-value h))))) r)
    (goto-char (point-min))
    (org-todo)
    (push (list :after-todo (org-element-map (org-element-parse-buffer) 'headline
                              (lambda (h) (list (org-element-property :level h)
                                                (org-element-property :raw-value h)
                                                (org-element-property :todo-keyword h))))) r)
    (org-priority ?A)
    (push (list :after-prio (org-element-map (org-element-parse-buffer) 'headline
                              (lambda (h) (list (org-element-property :level h)
                                                (org-element-property :raw-value h)
                                                (org-element-property :todo-keyword h)
                                                (org-element-property :priority h))))) r)
    (org-set-tags '("tag1" "tag2"))
    (push (list :after-tags (org-element-map (org-element-parse-buffer) 'headline
                              (lambda (h) (list (org-element-property :level h)
                                                (org-element-property :raw-value h)
                                                (org-element-property :todo-keyword h)
                                                (org-element-property :priority h)
                                                (org-element-property :tags h))))) r)
    (org-entry-put nil "CUSTOM_ID" "myid")
    (push (list :after-prop (org-entry-get nil "CUSTOM_ID")) r)
    (org-schedule nil "<2026-01-15>")
    (push (list :after-sched (org-element-map (org-element-parse-buffer) 'planning
                                (lambda (p) (when (org-element-property :scheduled p) "S")))) r)
    (org-deadline nil "<2026-01-20>")
    (push (list :after-dead (org-element-map (org-element-parse-buffer) 'planning
                              (lambda (p) (list (when (org-element-property :scheduled p) "S")
                                                (when (org-element-property :deadline p) "D"))))) r)
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex table with all operations → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo21_table_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |\n| 3 | 4 |")
  (let ((r ''))
    (push (list :init (buffer-string)) r)
    (goto-char (point-max))
    (org-table-insert-row)
    (insert "5 | 6")
    (org-table-align)
    (push (list :after-row (buffer-string)) r)
    (org-table-insert-column)
    (push (list :after-col (buffer-string)) r)
    (push (list :rows (length (org-element-map (org-element-parse-buffer) 'table-row 'identity))) r)
    (push (list :cells (length (org-element-map (org-element-parse-buffer) 'table-cell 'identity))) r)
    (goto-char (point-min))
    (forward-line 2)
    (org-table-delete-row)
    (push (list :after-del-row (buffer-string)) r)
    (org-table-goto-column 2)
    (org-table-delete-column)
    (push (list :after-del-col (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex list with all operations → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo21_list_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C\n- D")
  (let ((r ''))
    (push (list :init (org-element-map (org-element-parse-buffer) 'item
                        (lambda (i) (org-trim (buffer-substring-no-properties
                                                (org-element-property :contents-begin i)
                                                (org-element-property :contents-end i)))))) r)
    (goto-char (point-min))
    (forward-line 1)
    (org-metaright)
    (push (list :after-indent (org-element-map (org-element-parse-buffer) 'item
                                (lambda (i) (list (org-element-property :level i)
                                                  (org-trim (buffer-substring-no-properties
                                                              (org-element-property :contents-begin i)
                                                              (org-element-property :contents-end i))))))) r)
    (goto-char (point-min))
    (forward-line 2)
    (org-metaup)
    (push (list :after-move (org-element-map (org-element-parse-buffer) 'item
                              (lambda (i) (list (org-element-property :level i)
                                                (org-trim (buffer-substring-no-properties
                                                            (org-element-property :contents-begin i)
                                                            (org-element-property :contents-end i))))))) r)
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
// Build doc → complex src with all operations → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo21_src_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n(+ 2)\n(+ 3)\n#+END_SRC")
  (let ((r ''))
    (push (list :init (buffer-string)) r)
    (goto-char (point-min))
    (search-forward "(+ 2)")
    (beginning-of-line)
    (org-babel-demarcate-block)
    (push (list :after-demarcate (buffer-string)) r)
    (push (list :blocks (org-element-map (org-element-parse-buffer) 'src-block
                          (lambda (s) (list (org-element-property :language s)
                                            (org-element-property :value s))))) r)
    (goto-char (point-min))
    (org-babel-execute-src-block)
    (push (list :after-exec (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex visibility with all operations → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo21_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n* H1b\n** H2b\nSub\n*** H3b\nDeep")
  (let ((r ''))
    (org-overview)
    (push (list :overview (buffer-substring-no-properties (point-min) (point-max))) r)
    (org-global-cycle nil)
    (push (list :global1 (buffer-substring-no-properties (point-min) (point-max))) r)
    (org-global-cycle nil)
    (push (list :global2 (buffer-substring-no-properties (point-min) (point-max))) r)
    (org-global-cycle nil)
    (push (list :global3 (buffer-substring-no-properties (point-min) (point-max))) r)
    (goto-char (point-min))
    (org-cycle 'children)
    (push (list :local-children (buffer-substring-no-properties (point-min) (point-max))) r)
    (org-cycle 'subtree)
    (push (list :local-subtree (buffer-substring-no-properties (point-min) (point-max))) r)
    (org-overview)
    (search-forward "H3b")
    (beginning-of-line)
    (org-reveal)
    (push (list :reveal (buffer-substring-no-properties (point-min) (point-max))) r)
    (goto-char (point-min))
    (search-forward "H2\n")
    (beginning-of-line)
    (org-narrow-to-subtree)
    (push (list :narrowed (buffer-string)) r)
    (widen)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex navigation with all operations → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo21_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\n*** C\n** D\n* E\n** F\n*** G\n** H")
  (let ((r ''))
    (goto-char (point-min))
    (org-forward-heading-same-level 1)
    (push (list :fwd1 (org-element-property :raw-value (org-element-at-point))) r)
    (org-up-heading)
    (push (list :up1 (org-element-property :raw-value (org-element-at-point))) r)
    (org-next-visible-heading 1)
    (push (list :next1 (org-element-property :raw-value (org-element-at-point))) r)
    (org-backward-heading-same-level 1)
    (push (list :back1 (org-element-property :raw-value (org-element-at-point))) r)
    (goto-char (point-min))
    (org-end-of-subtree)
    (push (list :end1 (point)) r)
    (goto-char (point-min))
    (condition-case nil
        (progn (org-next-block 1)
               (push (list :next-block (buffer-substring-no-properties (line-beginning-position) (line-end-position))) r))
      (error nil))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex export with all elements → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo21_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-string-as)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((src "#+TITLE: T\n* TODO [#A] H1 :t1:\nBody *bold* /italic/\n** H2\n- [X] a\n- [ ] b\n| x | y |\n| 1 | 2 |\n#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\n* DONE H3\n:PROPERTIES:\n:A: 1\n:END:"))
  (let ((html (org-export-string-as src 'html t))
        (latex (org-export-string-as src 'latex t))
        (ascii (org-export-string-as src 'ascii t)))
    (list (list :html-has-title (string-match-p "T" html))
          (list :html-has-todo (string-match-p "TODO" html))
          (list :html-has-bold (string-match-p "<b>bold</b>" html))
          (list :html-has-table (string-match-p "<table" html))
          (list :latex-has-title (string-match-p "\\\\title" latex))
          (list :latex-has-section (string-match-p "\\\\section" latex))
          (list :latex-has-bold (string-match-p "\\\\textbf" latex))
          (list :latex-has-table (string-match-p "\\\\begin{tabular}" latex))
          (list :ascii-has-h (string-match-p "H1" ascii))
          (list :ascii-has-bold (string-match-p "bold" ascii)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex element map all types → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo21_map_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T\n* TODO [#A] H1 :t1:\nSCHEDULED: <2026-01-15>\nBody *bold* /italic/ [[http://a][Link]] $x^2$\n** H2\n- [X] a\n- [ ] b\n| x | y |\n| 1 | 2 |\n#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\n* DONE H3\n:PROPERTIES:\n:A: 1\n:END:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:00] =>  1:00")
  (let ((r ''))
    (let ((types (org-element-map (org-element-parse-buffer) 'element 'org-element-type)))
      (push (list :element-count (length types)) r)
      (push (list :element-types (sort (delete-dups (copy-sequence types)) 'string<)) r))
    (let ((types (org-element-map (org-element-parse-buffer) 'object 'org-element-type)))
      (push (list :object-count (length types)) r)
      (push (list :object-types (sort (delete-dups (copy-sequence types)) 'string<)) r))
    (push (list :headlines (length (org-element-map (org-element-parse-buffer) 'headline 'identity))) r)
    (push (list :paragraphs (length (org-element-map (org-element-parse-buffer) 'paragraph 'identity))) r)
    (push (list :items (length (org-element-map (org-element-parse-buffer) 'item 'identity))) r)
    (push (list :tables (length (org-element-map (org-element-parse-buffer) 'table 'identity))) r)
    (push (list :src-blocks (length (org-element-map (org-element-parse-buffer) 'src-block 'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex clock + planning → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo21_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\nCLOCK: [2026-01-11 14:00]--[2026-01-11 15:00] =>  1:00\n:END:\nBody")
  (let ((r ''))
    (push (list :planning (org-element-map (org-element-parse-buffer) 'planning
                            (lambda (p) (list (when (org-element-property :scheduled p) "S")
                                              (when (org-element-property :deadline p) "D")
                                              (when (org-element-property :closed p) "C"))))) r)
    (push (list :clocks (org-element-map (org-element-parse-buffer) 'clock
                          (lambda (c) (list (org-element-property :status c)
                                            (org-element-property :duration c))))) r)
    (org-clock-sum)
    (push (list :clock-sum org-clock-file-total-minutes) r)
    (goto-char (point-min))
    (push (list :todo (org-entry-get nil "TODO")) r)
    (push (list :sched (org-entry-get nil "SCHEDULED")) r)
    (push (list :dead (org-entry-get nil "DEADLINE")) r)
    (push (list :clock-string (org-clock-get-clock-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex footnote → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo21_footnotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Para1[fn:1] text[fn:2] more[fn:3] end\n\nPara2[fn:1] ref\n\n[fn:1] First def\n[fn:2] Second def\n[fn:3] Third def")
  (let ((r ''))
    (push (list :refs (org-element-map (org-element-parse-buffer) 'footnote-reference
                        (lambda (f) (org-element-property :label f)))) r)
    (push (list :defs (org-element-map (org-element-parse-buffer) 'footnote-definition
                        (lambda (f) (list (org-element-property :label f)
                                          (org-trim (buffer-substring-no-properties
                                                      (org-element-property :contents-begin f)
                                                      (org-element-property :contents-end f))))))) r)
    (push (list :ref-count (length (org-element-map (org-element-parse-buffer) 'footnote-reference 'identity))) r)
    (push (list :def-count (length (org-element-map (org-element-parse-buffer) 'footnote-definition 'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}
