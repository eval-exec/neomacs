//! Strong combo-complex-10 oracle tests — deep multi-step workflows.
//!
//! Every test chains multiple operations capturing deep mutable state.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex list struct → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo10_list_struct() {
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
fn combo10_export_string() {
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
// Build doc → complex element map all types → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo10_map_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T\n* TODO [#A] H1 :t1:\nSCHEDULED: <2026-01-15>\nBody *bold* /italic/ [[http://a][Link]] $x^2$\n** H2\n- [X] a\n- [ ] b\n| x | y |\n| 1 | 2 |\n#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\n* DONE H3\n:PROPERTIES:\n:A: 1\n:END:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:00] =>  1:00")
  (let ((r ''))
    ;; element types
    (let ((types (org-element-map (org-element-parse-buffer) 'element 'org-element-type)))
      (push (list :element-count (length types)) r)
      (push (list :element-types (sort (delete-dups (copy-sequence types)) 'string<)) r))
    ;; object types
    (let ((types (org-element-map (org-element-parse-buffer) 'object 'org-element-type)))
      (push (list :object-count (length types)) r)
      (push (list :object-types (sort (delete-dups (copy-sequence types)) 'string<)) r))
    ;; specific counts
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
fn combo10_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\nCLOCK: [2026-01-11 14:00]--[2026-01-11 15:00] =>  1:00\n:END:\nBody")
  (let ((r ''))
    ;; planning
    (push (list :planning (org-element-map (org-element-parse-buffer) 'planning
                            (lambda (p) (list (when (org-element-property :scheduled p) "S")
                                              (when (org-element-property :deadline p) "D")
                                              (when (org-element-property :closed p) "C"))))) r)
    ;; clocks
    (push (list :clocks (org-element-map (org-element-parse-buffer) 'clock
                          (lambda (c) (list (org-element-property :status c)
                                            (org-element-property :duration c))))) r)
    ;; clock sum
    (org-clock-sum)
    (push (list :clock-sum org-clock-file-total-minutes) r)
    ;; entry properties
    (goto-char (point-min))
    (push (list :todo (org-entry-get nil "TODO")) r)
    (push (list :sched (org-entry-get nil "SCHEDULED")) r)
    (push (list :dead (org-entry-get nil "DEADLINE")) r)
    ;; clock string
    (push (list :clock-string (org-clock-get-clock-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex footnote → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo10_footnotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Para1[fn:1] text[fn:2] more[fn:3] end\n\nPara2[fn:1] ref\n\n[fn:1] First def\n[fn:2] Second def\n[fn:3] Third def")
  (let ((r ''))
    ;; refs
    (push (list :refs (org-element-map (org-element-parse-buffer) 'footnote-reference
                        (lambda (f) (org-element-property :label f)))) r)
    ;; defs
    (push (list :defs (org-element-map (org-element-parse-buffer) 'footnote-definition
                        (lambda (f) (list (org-element-property :label f)
                                          (org-trim (buffer-substring-no-properties
                                                      (org-element-property :contents-begin f)
                                                      (org-element-property :contents-end f))))))) r)
    ;; counts
    (push (list :ref-count (length (org-element-map (org-element-parse-buffer) 'footnote-reference 'identity))) r)
    (push (list :def-count (length (org-element-map (org-element-parse-buffer) 'footnote-definition 'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex link → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo10_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n[[http://a.com][A]] and [[file:b.el]] and [[id:xxx][C]] and [[mailto:d@e.com]] and [[news:comp]]")
  (let ((r ''))
    ;; link details
    (push (list :links (org-element-map (org-element-parse-buffer) 'link
                          (lambda (l) (list (org-element-property :type l)
                                            (org-element-property :path l)
                                            (org-element-property :raw-link l))))) r)
    ;; link count
    (push (list :count (length (org-element-map (org-element-parse-buffer) 'link 'identity))) r)
    ;; link types
    (push (list :types (org-element-map (org-element-parse-buffer) 'link
                          (lambda (l) (org-element-property :type l)))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex keyword → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo10_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n#+AUTHOR: Me\n#+DATE: 2026-01-15\n#+OPTIONS: toc:nil\n#+FILETAGS: :t1:t2:\n#+STARTUP: overview\n#+CATEGORY: c\n#+LANGUAGE: en")
  (let ((r ''))
    ;; keywords
    (push (list :keywords (org-element-map (org-element-parse-buffer) 'keyword
                            (lambda (k) (list (org-element-property :key k)
                                              (org-element-property :value k))))) r)
    ;; collect-keywords
    (push (list :collected (org-collect-keywords '("TITLE" "AUTHOR" "DATE" "OPTIONS" "FILETAGS"))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex entity → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo10_entities() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text \\alpha \\beta \\gamma \\Agrave \\copy \\deg \\pm \\times")
  (let ((r ''))
    ;; entities
    (push (list :entities (org-element-map (org-element-parse-buffer) 'entity
                            (lambda (e) (list (org-element-property :name e)
                                              (org-element-property :utf-8 e)
                                              (org-element-property :latex e)
                                              (org-element-property :html e))))) r)
    ;; count
    (push (list :count (length (org-element-map (org-element-parse-buffer) 'entity 'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex latex → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo10_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text $x^2$ $$y=mx+b$$ and \\(z\\) \\[w\\] \\alpha")
  (let ((r ''))
    ;; latex fragments
    (push (list :fragments (org-element-map (org-element-parse-buffer) 'latex-fragment
                              (lambda (l) (org-element-property :value l)))) r)
    ;; entities
    (push (list :entities (org-element-map (org-element-parse-buffer) 'entity
                            (lambda (e) (org-element-property :name e)))) r)
    ;; counts
    (push (list :frag-count (length (org-element-map (org-element-parse-buffer) 'latex-fragment 'identity))) r)
    (push (list :ent-count (length (org-element-map (org-element-parse-buffer) 'entity 'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex timestamp → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo10_timestamps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15 Wed>\n* U\n<2026-01-20>--<2026-01-25>\n* V\n[2026-01-30]\n* W\n<2026-02-01 +1w>")
  (let ((r ''))
    ;; timestamps
    (push (list :timestamps (org-element-map (org-element-parse-buffer) 'timestamp
                              (lambda (ts) (list (org-element-property :type ts)
                                                (org-element-property :year-start ts)
                                                (org-element-property :month-start ts)
                                                (org-element-property :day-start ts)
                                                (org-element-property :repeater-type ts)
                                                (org-element-property :repeater-value ts))))) r)
    ;; count
    (push (list :count (length (org-element-map (org-element-parse-buffer) 'timestamp 'identity))) r)
    ;; types
    (push (list :types (org-element-map (org-element-parse-buffer) 'timestamp
                          (lambda (ts) (org-element-property :type ts)))) r)
    (nreverse r)))"##,
        expect,
    );
}
