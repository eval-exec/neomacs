//! Strong combo-complex-7 oracle tests — deep multi-step workflows.
//!
//! Every test chains multiple operations capturing deep mutable state.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex element map with multiple types → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo7_map_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* /italic/ _under_ +strike+ =code= ~verb~ [[http://a][Link]] $x^2$ \\alpha H_2O")
  (let ((r ''))
    ;; all inline objects
    (push (list :inline (org-element-map (org-element-parse-buffer) '(bold italic underline strike-through code verbatim link latex-fragment entity subscript superscript)
                           (lambda (o) (list (org-element-type o)
                                             (org-trim (buffer-substring-no-properties
                                                         (org-element-property :contents-begin o)
                                                         (org-element-property :contents-end o))))))) r)
    ;; parent chain for subscript
    (search-forward "_2")
    (let* ((obj (org-element-context))
           (chain '()))
      (let ((p obj))
        (while p
          (push (org-element-type p) chain)
          (setq p (org-element-property :parent p))))
      (push (list :sub-chain (nreverse chain)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex heading with planning → verify all fields
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo7_planning_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\nSCHEDULED: <2026-01-15 Wed>\nDEADLINE: <2026-01-20 Mon>\nCLOSED: [2026-01-10 Fri]\nBody")
  (let ((r ''))
    ;; planning
    (push (list :planning (org-element-map (org-element-parse-buffer) 'planning
                            (lambda (p) (list (when (org-element-property :scheduled p) "S")
                                              (when (org-element-property :deadline p) "D")
                                              (when (org-element-property :closed p) "C"))))) r)
    ;; timestamps
    (push (list :timestamps (org-element-map (org-element-parse-buffer) 'timestamp
                              (lambda (ts) (list (org-element-property :type ts)
                                                (org-element-property :year-start ts)
                                                (org-element-property :day-start ts))))) r)
    ;; entry properties
    (goto-char (point-min))
    (push (list :todo (org-entry-get nil "TODO")) r)
    (push (list :sched (org-entry-get nil "SCHEDULED")) r)
    (push (list :dead (org-entry-get nil "DEADLINE")) r)
    (push (list :closed (org-entry-get nil "CLOSED")) r)
    ;; modify schedule
    (org-schedule nil "<2026-01-16>")
    (push (list :after-sched (org-entry-get nil "SCHEDULED")) r)
    ;; modify deadline
    (org-deadline nil "<2026-01-21>")
    (push (list :after-dead (org-entry-get nil "DEADLINE")) r)
    ;; verify buffer
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex block types → verify all present
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo7_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\n#+BEGIN_QUOTE\nQ\n#+END_QUOTE\n#+BEGIN_CENTER\nC\n#+END_CENTER\n#+BEGIN_EXPORT html\n<b>Bold</b>\n#+END_EXPORT\n#+BEGIN_VERSE\nV\n#+END_VERSE\n:MYDRAWER:\nData\n:END:")
  (let ((r ''))
    ;; block types
    (push (list :types (org-element-map (org-element-parse-buffer) '(src-block quote-block center-block export-block verse-block drawer)
                          (lambda (b) (org-element-type b)))) r)
    ;; src value
    (push (list :src (org-element-map (org-element-parse-buffer) 'src-block
                        (lambda (s) (org-element-property :value s)))) r)
    ;; quote content
    (push (list :quote (org-element-map (org-element-parse-buffer) 'quote-block
                          (lambda (q) (org-trim (buffer-substring-no-properties
                                                  (org-element-property :contents-begin q)
                                                  (org-element-property :contents-end q)))))) r)
    ;; export type
    (push (list :export (org-element-map (org-element-parse-buffer) 'export-block
                          (lambda (e) (org-element-property :type e)))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex link types → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo7_links() {
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
// Build doc → complex keyword types → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo7_keywords() {
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
// Build doc → complex entity types → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo7_entities() {
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
// Build doc → complex latex types → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo7_latex() {
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
// Build doc → complex timestamp types → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo7_timestamps() {
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

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex macro types → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo7_macros() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: name Hello\n#+MACRO: greeting Hi $1!\nText {{{name}}} and {{{greeting(World)}}}")
  (let ((r ''))
    ;; macros
    (push (list :macros (org-element-map (org-element-parse-buffer) 'macro
                          (lambda (m) (list (org-element-property :key m)
                                            (org-element-property :value m)
                                            (org-element-property :args m))))) r)
    ;; collect
    (push (list :collected (org-macro--collect-macros)) r)
    ;; replace
    (let ((raw (buffer-string)))
      (org-macro-replace-all org-macro-templates)
      (push (list :before raw :after (buffer-string)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex statistics-cookie → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo7_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T [1/3]\n- [X] a\n- [ ] b\n- [ ] c\n- [X] d")
  (let ((r ''))
    ;; initial stats
    (push (list :init (org-element-map (org-element-parse-buffer) 'statistics-cookie
                        (lambda (s) (org-element-property :value s)))) r)
    ;; update stats
    (goto-char (point-min))
    (org-update-statistics-cookies t)
    (push (list :after-update (org-element-map (org-element-parse-buffer) 'statistics-cookie
                                (lambda (s) (org-element-property :value s)))) r)
    ;; verify buffer
    (push (list :content (buffer-substring-no-properties (line-beginning-position) (line-end-position))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex clock types → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo7_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\nCLOCK: [2026-01-11 14:00]--[2026-01-11 15:00] =>  1:00\n:END:")
  (let ((r ''))
    ;; clock entries
    (push (list :clocks (org-element-map (org-element-parse-buffer) 'clock
                          (lambda (c) (list (org-element-property :status c)
                                            (org-element-property :duration c))))) r)
    ;; clock sum
    (org-clock-sum)
    (push (list :sum org-clock-file-total-minutes) r)
    ;; clock string
    (goto-char (point-min))
    (push (list :string (org-clock-get-clock-string)) r)
    ;; clock timestamps
    (push (list :timestamps (org-clock-get-timestamps)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex footnote types → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo7_footnotes() {
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
