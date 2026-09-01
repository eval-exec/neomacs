//! Strong combo-complex-5 oracle tests — deep multi-step workflows.
//!
//! Every test chains multiple operations capturing deep mutable state.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex heading clone → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo5_clone() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n** Sub1\n** Sub2")
  (let ((r ''))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (list (org-element-property :level h)
                                          (org-element-property :raw-value h))))) r)
    ;; clone 2 times
    (goto-char (point-min))
    (org-clone-subtree 2)
    (push (list :after-clone (org-element-map (org-element-parse-buffer) 'headline
                                (lambda (h) (list (org-element-property :level h)
                                                  (org-element-property :raw-value h))))) r)
    ;; verify buffer
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex sort → verify order
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo5_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO C\n* DONE A\n* TODO B\n* DONE D")
  (let ((r ''))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (list (org-element-property :raw-value h)
                                          (org-element-property :todo-keyword h))))) r)
    ;; sort by todo
    (org-sort-entries nil ?o)
    (push (list :after-todo-sort (org-element-map (org-element-parse-buffer) 'headline
                                    (lambda (h) (list (org-element-property :raw-value h)
                                                      (org-element-property :todo-keyword h))))) r)
    ;; sort alphabetically
    (org-sort-entries nil ?a)
    (push (list :after-alpha-sort (org-element-map (org-element-parse-buffer) 'headline
                                    (lambda (h) (org-element-property :raw-value h)))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex toggle heading/item → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo5_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n* H2\n* H3")
  (let ((r ''))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h)))) r)
    ;; toggle H2 to list
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
    ;; toggle H3 to item
    (goto-char (point-min))
    (forward-line 2)
    (org-toggle-heading)
    (push (list :after-toggle2 (org-element-map (org-element-parse-buffer) '(headline plain-list item)
                                  (lambda (e) (list (org-element-type e)
                                                    (org-element-property :raw-value e))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex move subtree → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo5_move_subtree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** A1\n* B\n** B1\n* C\n** C1")
  (let ((r ''))
    ;; initial
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (list (org-element-property :level h)
                                          (org-element-property :raw-value h))))) r)
    ;; move A down
    (goto-char (point-min))
    (org-metadown)
    (push (list :after-down (org-element-map (org-element-parse-buffer) 'headline
                              (lambda (h) (list (org-element-property :level h)
                                                (org-element-property :raw-value h))))) r)
    ;; move C up
    (goto-char (point-min))
    (search-forward "C\n")
    (beginning-of-line)
    (org-metaup)
    (push (list :after-up (org-element-map (org-element-parse-buffer) 'headline
                            (lambda (h) (list (org-element-property :level h)
                                              (org-element-property :raw-value h))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex narrow + show → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo5_narrow_show() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n* H1b\n** H2b\nSub")
  (let ((r ''))
    ;; narrow to H2
    (goto-char (point-min))
    (search-forward "H2\n")
    (beginning-of-line)
    (org-narrow-to-subtree)
    (push (list :narrowed (buffer-string)) r)
    ;; show context
    (org-show-context 'agenda)
    (push (list :context (buffer-string)) r)
    ;; widen
    (widen)
    (push (list :widened (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; overview
    (org-overview)
    (push (list :overview (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; reveal at H3
    (goto-char (point-min))
    (search-forward "H3")
    (beginning-of-line)
    (org-reveal)
    (push (list :reveal (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex list struct → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo5_list_struct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  - B\n  - C\n- D\n  - E\n    - F")
  (let ((r ''))
    ;; list struct
    (let ((struct (org-list-struct)))
      (push (list :struct struct) r)
      ;; prevs
      (push (list :prevs (org-list-prevs-alist struct)) r)
      ;; parents
      (push (list :parents (org-list-parents-alist struct)) r))
    ;; indent B
    (goto-char (point-min))
    (forward-line 1)
    (org-metaright)
    (push (list :after-indent (org-element-map (org-element-parse-buffer) 'item
                                (lambda (i) (list (org-element-property :level i)
                                                  (org-trim (buffer-substring-no-properties
                                                              (org-element-property :contents-begin i)
                                                              (org-element-property :contents-end i))))))) r)
    ;; dedent B
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
// Build doc → complex export string → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo5_export_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-string-as)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((src "#+TITLE: T\n* H1\nBody *bold* /italic/\n** H2\n- a\n- b\n| x | y |\n| 1 | 2 |"))
  (let ((html (org-export-string-as src 'html t))
        (latex (org-export-string-as src 'latex t))
        (ascii (org-export-string-as src 'ascii t)))
    (list (list :html-has-title (string-match-p "T" html))
          (list :html-has-h2 (string-match-p "<h2>" html))
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
// Build doc → complex element map with predicate → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo5_map_pred() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C\n* DONE D\n* WAITING E")
  (let ((r ''))
    ;; all
    (push (list :all (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h)))) r)
    ;; TODO only
    (push (list :todo (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h))
                        nil nil nil
                        (lambda (h) (string= (org-element-property :todo-keyword h) "TODO")))) r)
    ;; DONE only
    (push (list :done (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h))
                        nil nil nil
                        (lambda (h) (string= (org-element-property :todo-keyword h) "DONE")))) r)
    ;; first match
    (push (list :first (org-element-map (org-element-parse-buffer) 'headline
                          (lambda (h) (org-element-property :raw-value h))
                          nil 'first-match)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex parent chain → verify lineage
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo5_lineage() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"italic\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold /italic/ inside* text")
  (search-forward "italic")
  (let* ((obj (org-element-context))
         (chain '()))
    (let ((p obj))
      (while p
        (push (list (org-element-type p)
                    (when (org-element-property :contents-begin p)
                      (org-trim (buffer-substring-no-properties
                                  (org-element-property :contents-begin p)
                                  (org-element-property :contents-end p)))))
              chain)
        (setq p (org-element-property :parent p))))
    (list :chain (nreverse chain))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex element properties → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo5_element_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:begin 1 :end 19 :post-blank 0 :contents-begin 5 :contents-end 19 :level 1 :raw-value \"H\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody\n** H2\nSub")
  (goto-char (point-min))
  (let ((h1 (org-element-at-point)))
    (list :begin (org-element-property :begin h1)
          :end (org-element-property :end h1)
          :post-blank (org-element-property :post-blank h1)
          :contents-begin (org-element-property :contents-begin h1)
          :contents-end (org-element-property :contents-end h1)
          :level (org-element-property :level h1)
          :raw-value (org-element-property :raw-value h1))))"##,
        expect,
    );
}
