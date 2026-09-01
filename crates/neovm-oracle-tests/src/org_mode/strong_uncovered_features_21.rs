//! Strong uncovered-features-21 oracle tests — complex multi-step workflows.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-cycle + org-element-parse after cycling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_cycle_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:headlines 4) (:visible \"* H1\\n** H2\\n*** H3\\nBody\\n* H1b\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\nBody\n* H1b")
  (goto-char (point-min))
  (org-overview)
  (let ((r '()))
    (push (list :headlines (length (org-element-map (org-element-parse-buffer) 'headline 'identity))) r)
    (push (list :visible (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// insert headline + set todo + set tags + set property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_full_build() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:headlines ((1 \"Task1\" \"TODO\" (\"work\")) (2 \"Sub1\" \"DONE\" (\"home\")) (1 \"WAITING Task2\" nil nil))) (:planning ((\"sched\" nil))) (:properties ((\"EFFORT\" \"2h\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task1 :work:\n:PROPERTIES:\n:EFFORT: 2h\n:END:\n** DONE Sub1 :home:\n* WAITING Task2\nSCHEDULED: <2026-02-01>")
  (let ((r '()))
    (push (list :headlines (org-element-map (org-element-parse-buffer) 'headline
                              (lambda (h) (list (org-element-property :level h)
                                                (org-element-property :raw-value h)
                                                (org-element-property :todo-keyword h)
                                                (org-element-property :tags h))))) r)
    (push (list :planning (org-element-map (org-element-parse-buffer) 'planning
                            (lambda (p) (list (when (org-element-property :scheduled p) "sched")
                                              (when (org-element-property :deadline p) "dead"))))) r)
    (push (list :properties (org-element-map (org-element-parse-buffer) 'node-property
                              (lambda (p) (list (org-element-property :key p)
                                                (org-element-property :value p))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build doc then reparse after modifications
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_modify_reparse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init (\"A\" \"B\" \"C\")) (:after-move (\"B\" \"A\" \"C\")) (:after-insert (\"B\" \"A\" \"C\" \"D\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n* B\n* C")
  (let ((r '()))
    (push (list :init (org-element-map (org-element-parse-buffer) 'headline
                        (lambda (h) (org-element-property :raw-value h)))) r)
    (goto-char (point-min))
    (org-metadown)
    (push (list :after-move (org-element-map (org-element-parse-buffer) 'headline
                              (lambda (h) (org-element-property :raw-value h)))) r)
    (goto-char (point-max))
    (insert "\n* D")
    (push (list :after-insert (org-element-map (org-element-parse-buffer) 'headline
                                (lambda (h) (org-element-property :raw-value h)))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build list then indent/dedent multiple items
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_list_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init ((nil \"A\") (nil \"B\") (nil \"C\") (nil \"D\"))) (:indented ((nil \"A\\n  - B\\n  - C\") (nil \"B\") (nil \"C\") (nil \"D\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C\n- D")
  (let ((r '()))
    (push (list :init (org-element-map (org-element-parse-buffer) 'item
                        (lambda (i) (list (org-element-property :level i)
                                          (org-trim (buffer-substring-no-properties
                                                      (org-element-property :contents-begin i)
                                                      (org-element-property :contents-end i))))))) r)
    (goto-char (point-min))
    (forward-line 1)
    (org-metaright)
    (forward-line 1)
    (org-metaright)
    (push (list :indented (org-element-map (org-element-parse-buffer) 'item
                            (lambda (i) (list (org-element-property :level i)
                                              (org-trim (buffer-substring-no-properties
                                                          (org-element-property :contents-begin i)
                                                          (org-element-property :contents-end i))))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build table then add row/column
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_table_add() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:rows 4) (:cells 9) (:content #(\"| a | b |   |\\n|---+---+---|\\n| 3 | 4 |   |\\n| 1 | 2 |   |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table rear-nonsticky t display (space :relative-width 1)) 10 11 (face org-table) 11 12 (face org-table display (space :relative-width 1.001)) 12 13 (face org-table) 13 14 (face org-table-row) 14 15 (face org-table) 15 27 (face org-table) 27 28 (face org-table-row) 28 29 (face org-table) 29 30 (face org-table rear-nonsticky t display (space :relative-width 1)) 30 31 (face org-table) 31 32 (face org-table display (space :relative-width 1.001)) 32 33 (face org-table) 33 34 (face org-table rear-nonsticky t display (space :relative-width 1)) 34 35 (face org-table) 35 36 (face org-table display (space :relative-width 1.001)) 36 37 (face org-table) 37 38 (face org-table rear-nonsticky t display (space :relative-width 1)) 38 39 (face org-table) 39 40 (face org-table display (space :relative-width 1.001)) 40 41 (face org-table) 41 42 (face org-table-row) 42 43 (face org-table) 43 44 (face org-table rear-nonsticky t display (space :relative-width 1)) 44 45 (face org-table) 45 46 (face org-table display (space :relative-width 1.001)) 46 47 (face org-table) 47 48 (face org-table rear-nonsticky t display (space :relative-width 1)) 48 49 (face org-table) 49 50 (face org-table display (space :relative-width 1.001)) 50 51 (face org-table) 51 52 (face org-table rear-nonsticky t display (space :relative-width 1)) 52 53 (face org-table) 53 54 (face org-table display (space :relative-width 1.001)) 54 55 (face org-table) 55 56 (face org-table-row))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n|---+---|\n| 1 | 2 |")
  (goto-char (point-max))
  (org-table-insert-row)
  (insert "3 | 4")
  (org-table-align)
  (let ((r '()))
    (push (list :rows (length (org-element-map (org-element-parse-buffer) 'table-row 'identity))) r)
    (push (list :cells (length (org-element-map (org-element-parse-buffer) 'table-cell 'identity))) r)
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build src block then execute
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_src_exec() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC")
  (goto-char (point-min))
  (org-babel-execute-src-block)
  (let ((r '()))
    (push (list :results (org-element-map (org-element-parse-buffer) 'fixed-width
                           (lambda (fw) (org-element-property :value fw)))) r)
    (push (list :content (buffer-string)) r)
    (nreverse r)))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build doc with footnotes then collect
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_footnotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:refs (\"1\" \"2\")) (:defs ((\"1\" \"First def\") (\"2\" \"Second def\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] more[fn:2] end\n\n[fn:1] First def\n[fn:2] Second def")
  (let ((r '()))
    (push (list :refs (org-element-map (org-element-parse-buffer) 'footnote-reference
                        (lambda (f) (org-element-property :label f)))) r)
    (push (list :defs (org-element-map (org-element-parse-buffer) 'footnote-definition
                        (lambda (f) (list (org-element-property :label f)
                                          (org-trim (buffer-substring-no-properties
                                                      (org-element-property :contents-begin f)
                                                      (org-element-property :contents-end f))))))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build doc with links then collect types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"http\" \"//a.com\" \"http://a.com\") (\"file\" \"b.el\" \"file:b.el\") (\"id\" \"xxx\" \"id:xxx\") (\"mailto\" \"d@e.com\" \"mailto:d@e.com\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n[[http://a.com][A]] [[file:b.el][B]] [[id:xxx][C]] [[mailto:d@e.com]]")
  (org-element-map (org-element-parse-buffer) 'link
    (lambda (l) (list (org-element-property :type l)
                      (org-element-property :path l)
                      (org-element-property :raw-link l)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build doc with inline markup then collect all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* /italic/ _under_ +strike+ =code= ~verb~")
  (org-element-map (org-element-parse-buffer) '(bold italic underline strike-through code verbatim)
    (lambda (o) (list (org-element-type o)
                      (org-trim (buffer-substring-no-properties
                                  (org-element-property :contents-begin o)
                                  (org-element-property :contents-end o)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build doc with entities then collect
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_entities() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"alpha\" \"α\" \"\\\\alpha\") (\"beta\" \"β\" \"\\\\beta\") (\"gamma\" \"γ\" \"\\\\gamma\") (\"delta\" \"δ\" \"\\\\delta\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text \\alpha \\beta \\gamma \\delta")
  (org-element-map (org-element-parse-buffer) 'entity
    (lambda (e) (list (org-element-property :name e)
                      (org-element-property :utf-8 e)
                      (org-element-property :latex e)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build doc with latex fragments then collect
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"$x^2$\" \"$$y=mx+b$$\" \"\\\\(z\\\\)\" \"\\\\[w\\\\]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text $x^2$ $$y=mx+b$$ and \\(z\\) \\[w\\]")
  (org-element-map (org-element-parse-buffer) 'latex-fragment
    (lambda (l) (org-element-property :value l))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build doc with timestamps then collect properties
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_timestamps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((active-range 2026 1 25 nil nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15 Wed>\n* U\nDEADLINE: <2026-01-20 Mon +1w>\n* V\n<2026-01-25>--<2026-01-30>")
  (org-element-map (org-element-parse-buffer) 'timestamp
    (lambda (ts) (list (org-element-property :type ts)
                      (org-element-property :year-start ts)
                      (org-element-property :month-start ts)
                      (org-element-property :day-start ts)
                      (org-element-property :repeater-type ts)
                      (org-element-property :repeater-value ts)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build doc with clock entries then collect
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_clocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((closed \"1:30\" (timestamp (:standard-properties [12 nil nil nil 51 1 nil nil nil nil nil nil nil nil nil nil nil nil] :type inactive-range :range-type daterange :raw-value \"[2026-01-10 10:00]--[2026-01-10 11:30]\" :year-start 2026 :month-start 1 :day-start 10 :hour-start 10 :minute-start 0 :year-end 2026 :month-end 1 :day-end 10 :hour-end 11 :minute-end 30))) (closed \"1:00\" (timestamp (:standard-properties [67 nil nil nil 106 1 nil nil nil nil nil nil nil nil nil nil nil nil] :type inactive-range :range-type daterange :raw-value \"[2026-01-11 14:00]--[2026-01-11 15:00]\" :year-start 2026 :month-start 1 :day-start 11 :hour-start 14 :minute-start 0 :year-end 2026 :month-end 1 :day-end 11 :hour-end 15 :minute-end 0))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30\nCLOCK: [2026-01-11 14:00]--[2026-01-11 15:00] =>  1:00")
  (org-element-map (org-element-parse-buffer) 'clock
    (lambda (c) (list (org-element-property :status c)
                      (org-element-property :duration c)
                      (org-element-property :value c)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build complex doc then get full element type distribution
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_distribution() {
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
// build doc with all block types then collect
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (src-block quote-block center-block export-block verse-block)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\n#+BEGIN_QUOTE\nQ\n#+END_QUOTE\n#+BEGIN_CENTER\nC\n#+END_CENTER\n#+BEGIN_EXPORT html\n<b>Bold</b>\n#+END_EXPORT\n#+BEGIN_VERSE\nV\n#+END_VERSE")
  (org-element-map (org-element-parse-buffer) '(src-block quote-block center-block export-block verse-block)
    (lambda (b) (org-element-type b))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build doc with all planning types then collect
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"S\" nil nil) (nil \"D\" nil) (nil nil \"C\") (\"S\" \"D\" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\nSCHEDULED: <2026-01-15>\n* B\nDEADLINE: <2026-01-20>\n* C\nCLOSED: [2026-01-10]\n* D\nSCHEDULED: <2026-01-15> DEADLINE: <2026-01-20>")
  (org-element-map (org-element-parse-buffer) 'planning
    (lambda (p) (list (when (org-element-property :scheduled p) "S")
                      (when (org-element-property :deadline p) "D")
                      (when (org-element-property :closed p) "C")))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build doc with all keyword types then collect
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TITLE\" \"T\") (\"AUTHOR\" \"A\") (\"DATE\" \"D\") (\"OPTIONS\" \"o\") (\"FILETAGS\" \":t:\") (\"STARTUP\" \"overview\") (\"CATEGORY\" \"c\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T\n#+AUTHOR: A\n#+DATE: D\n#+OPTIONS: o\n#+FILETAGS: :t:\n#+STARTUP: overview\n#+CATEGORY: c")
  (org-element-map (org-element-parse-buffer) 'keyword
    (lambda (k) (list (org-element-property :key k)
                      (org-element-property :value k)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build doc then org-element-map with-multiple-type filter
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_multi_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic latex-fragment link)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* /italic/ [[http://a][Link]] $x^2$")
  (sort (delete-dups (org-element-map (org-element-parse-buffer) '(bold italic link latex-fragment)
                        'org-element-type))
        'string<))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build doc then get parent chain for deeply nested object
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_parent_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"bold\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* text")
  (search-forward "bold")
  (let* ((obj (org-element-context))
         (chain '()))
    (let ((p obj))
      (while p
        (push (org-element-type p) chain)
        (setq p (org-element-property :parent p))))
    (nreverse chain)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// build doc then get lineage with types filter
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf21_lineage() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"bold\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* text")
  (search-forward "bold")
  (let* ((obj (org-element-context))
         (lineage (org-element-lineage obj '(headline paragraph bold) t)))
    (mapcar 'org-element-type lineage)))"##,
        expect,
    );
}
