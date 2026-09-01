//! Strong combo-complex-3 oracle tests — deep multi-step workflows.
//!
//! Every test chains multiple operations capturing deep mutable state.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Build doc → export with options → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo3_export_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-export-string-as)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((src "#+TITLE: My Title\n#+AUTHOR: Me\n#+OPTIONS: toc:nil\n* H1\nBody\n** H2\nSub"))
  (let ((html (org-export-string-as src 'html t)))
    (list (string-match-p "My Title" html)
          (string-match-p "Me" html)
          (string-match-p "Table of Contents" html))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex heading operations → verify element tree
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo3_heading_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error \"Cannot move past superior level or buffer limit\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\n*** C\n** D\n* E\n** F")
  (let ((r '()))
    ;; full tree
    (push (list :tree (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (list (org-element-property :level h)
                                          (org-element-property :raw-value h)
                                          (let ((p (org-element-property :parent h)))
                                            (when p (org-element-property :raw-value p))))))) r)
    ;; move D under E
    (goto-char (point-min))
    (search-forward "D")
    (beginning-of-line)
    (org-metadown)
    (push (list :after-move (org-element-map (org-element-parse-buffer) 'headline
                              (lambda (h) (list (org-element-property :level h)
                                                (org-element-property :raw-value h)
                                                (let ((p (org-element-property :parent h)))
                                                  (when p (org-element-property :raw-value p))))))) r)
    ;; indent F under E
    (goto-char (point-min))
    (search-forward "F")
    (beginning-of-line)
    (org-metaright)
    (push (list :after-indent (org-element-map (org-element-parse-buffer) 'headline
                                (lambda (h) (list (org-element-property :level h)
                                                  (org-element-property :raw-value h)
                                                  (let ((p (org-element-property :parent h)))
                                                    (when p (org-element-property :raw-value p))))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex inline markup → parse + parent chain
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo3_inline_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold /italic/ inside* text =code ~verb~ end")
  (let ((r '()))
    ;; collect all inline objects
    (push (list :objects (org-element-map (org-element-parse-buffer) '(bold italic code verbatim)
                           (lambda (o) (list (org-element-type o)
                                             (org-trim (buffer-substring-no-properties
                                                         (org-element-property :contents-begin o)
                                                         (org-element-property :contents-end o))))))) r)
    ;; parent chain for italic (nested in bold)
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
      (push (list :italic-chain (nreverse chain)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex planning → verify all planning fields
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo3_planning_fields() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:planning ((\"S\" nil nil))) (:timestamps ((active 2026 20) (inactive 2026 10))) (:todo \"TODO\") (:sched \"<2026-01-15 Wed>\") (:dead nil) (:closed nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\nSCHEDULED: <2026-01-15 Wed>\nDEADLINE: <2026-01-20 Mon>\nCLOSED: [2026-01-10 Fri]\nBody")
  (let ((r '()))
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
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex block types → verify all present
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo3_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:types (src-block quote-block center-block export-block verse-block)) (:src-value (\"(+ 1)\\n\")) (:quote-content (\"Q\")) (:export-type (\"HTML\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\n#+BEGIN_QUOTE\nQ\n#+END_QUOTE\n#+BEGIN_CENTER\nC\n#+END_CENTER\n#+BEGIN_EXPORT html\n<b>Bold</b>\n#+END_EXPORT\n#+BEGIN_VERSE\nV\n#+END_VERSE")
  (let ((r '()))
    ;; collect block types
    (push (list :types (org-element-map (org-element-parse-buffer) '(src-block quote-block center-block export-block verse-block)
                          (lambda (b) (org-element-type b)))) r)
    ;; collect block contents
    (push (list :src-value (org-element-map (org-element-parse-buffer) 'src-block
                              (lambda (s) (org-element-property :value s)))) r)
    (push (list :quote-content (org-element-map (org-element-parse-buffer) 'quote-block
                                  (lambda (q) (org-trim (buffer-substring-no-properties
                                                          (org-element-property :contents-begin q)
                                                          (org-element-property :contents-end q)))))) r)
    (push (list :export-type (org-element-map (org-element-parse-buffer) 'export-block
                                (lambda (e) (org-element-property :type e)))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex footnote operations → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo3_footnotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:refs (\"1\" \"2\" \"1\")) (:defs ((\"1\" \"First definition\") (\"2\" \"Second definition\"))) (:ref-count 3) (:def-count 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Para1[fn:1] text[fn:2] end\n\nPara2[fn:1] ref\n\n[fn:1] First definition\n[fn:2] Second definition")
  (let ((r '()))
    ;; footnote refs
    (push (list :refs (org-element-map (org-element-parse-buffer) 'footnote-reference
                        (lambda (f) (org-element-property :label f)))) r)
    ;; footnote defs
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
// Build doc → complex link operations → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo3_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:links ((\"http\" \"//a.com\" \"http://a.com\") (\"file\" \"b.el\" \"file:b.el\") (\"id\" \"xxx\" \"id:xxx\") (\"mailto\" \"d@e.com\" \"mailto:d@e.com\"))) (:count 4) (:types (\"http\" \"file\" \"id\" \"mailto\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n[[http://a.com][A]] and [[file:b.el]] and [[id:xxx][C]] and [[mailto:d@e.com]]")
  (let ((r '()))
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
// Build doc → complex keyword operations → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo3_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:keywords ((\"TITLE\" \"Test\") (\"AUTHOR\" \"Me\") (\"DATE\" \"2026-01-15\") (\"OPTIONS\" \"toc:nil\") (\"FILETAGS\" \":t1:t2:\") (\"STARTUP\" \"overview\") (\"CATEGORY\" \"c\"))) (:collected ((\"TITLE\" \"Test\") (\"AUTHOR\" \"Me\") (\"DATE\" \"2026-01-15\") (\"OPTIONS\" \"toc:nil\") (\"FILETAGS\" \":t1:t2:\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n#+AUTHOR: Me\n#+DATE: 2026-01-15\n#+OPTIONS: toc:nil\n#+FILETAGS: :t1:t2:\n#+STARTUP: overview\n#+CATEGORY: c")
  (let ((r '()))
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
// Build doc → complex entity operations → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo3_entities() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:entities ((\"alpha\" \"α\" \"\\\\alpha\") (\"beta\" \"β\" \"\\\\beta\") (\"gamma\" \"γ\" \"\\\\gamma\") (\"Agrave\" \"À\" \"\\\\`{A}\") (\"copy\" \"©\" \"\\\\textcopyright{}\"))) (:count 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text \\alpha \\beta \\gamma \\Agrave \\copy")
  (let ((r '()))
    ;; entities
    (push (list :entities (org-element-map (org-element-parse-buffer) 'entity
                            (lambda (e) (list (org-element-property :name e)
                                              (org-element-property :utf-8 e)
                                              (org-element-property :latex e))))) r)
    ;; entity count
    (push (list :count (length (org-element-map (org-element-parse-buffer) 'entity 'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex latex operations → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo3_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:fragments (\"$x^2$\" \"$$y=mx+b$$\" \"\\\\(z\\\\)\" \"\\\\[w\\\\]\")) (:count 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text $x^2$ $$y=mx+b$$ and \\(z\\) \\[w\\]")
  (let ((r '()))
    ;; latex fragments
    (push (list :fragments (org-element-map (org-element-parse-buffer) 'latex-fragment
                              (lambda (l) (org-element-property :value l)))) r)
    ;; count
    (push (list :count (length (org-element-map (org-element-parse-buffer) 'latex-fragment 'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex timestamp operations → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo3_timestamps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:timestamps ((active-range 2026 1 20 nil) (inactive 2026 1 30 nil))) (:count 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15 Wed>\n* U\n<2026-01-20>--<2026-01-25>\n* V\n[2026-01-30]")
  (let ((r '()))
    ;; timestamps
    (push (list :timestamps (org-element-map (org-element-parse-buffer) 'timestamp
                              (lambda (ts) (list (org-element-property :type ts)
                                                (org-element-property :year-start ts)
                                                (org-element-property :month-start ts)
                                                (org-element-property :day-start ts)
                                                (org-element-property :repeater-type ts))))) r)
    ;; count
    (push (list :count (length (org-element-map (org-element-parse-buffer) 'timestamp 'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → complex macro operations → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo3_macros() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Undefined Org macro: greeting; aborting\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: greeting Hello $1!\n{{{greeting(World)}}} and {{{greeting(Elisp)}}}")
  (let ((r '()))
    ;; macros
    (push (list :macros (org-element-map (org-element-parse-buffer) 'macro
                          (lambda (m) (list (org-element-property :key m)
                                            (org-element-property :value m)
                                            (org-element-property :args m))))) r)
    ;; collect-macros
    (push (list :collected (org-macro--collect-macros)) r)
    ;; replace and verify
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
fn combo3_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init (\"[1/2]\")) (:after-update (\"[2/3]\")) (:content \"* T [2/3]\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T [1/2]\n- [X] a\n- [ ] b\n- [X] c")
  (let ((r '()))
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
// Build doc → complex clock operations → verify
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo3_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Invalid date: \")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:LOGBOOK:\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\nCLOCK: [2026-01-11 14:00]--[2026-01-11 15:00] =>  1:00\n:END:")
  (let ((r '()))
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
    (nreverse r)))"##,
        expect,
    );
}
