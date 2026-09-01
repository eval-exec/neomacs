//! Strong uncovered-features-15 oracle tests — test features not yet tested.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with-multiple-drawers (avoid parsing divergence)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_drawer_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((5 29))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:")
  (org-element-map (org-element-parse-buffer) 'property-drawer
    (lambda (d) (list (org-element-property :begin d)
                      (org-element-property :end d)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map property
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"A\" \"1\") (\"B\" \"2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:B: 2\n:END:")
  (org-element-map (org-element-parse-buffer) 'node-property
    (lambda (p) (list (org-element-property :key p)
                      (org-element-property :value p)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map clock
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((closed \"1:30\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nCLOCK: [2026-01-10 10:00]--[2026-01-10 11:30] =>  1:30")
  (org-element-map (org-element-parse-buffer) 'clock
    (lambda (c) (list (org-element-property :status c)
                      (org-element-property :duration c)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map diary-sexp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_diary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"%%(diary-anniversary 1 1 2000)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "%%(diary-anniversary 1 1 2000)")
  (org-element-map (org-element-parse-buffer) 'diary-sexp
    (lambda (d) (org-element-property :value d))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map horizontal-rule
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_hr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "-----\nText\n-----\nMore")
  (org-element-map (org-element-parse-buffer) 'horizontal-rule
    (lambda (h) (org-element-property :begin h))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map snippet
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_snippet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text @@html:<b>bold</b>@@ more @@latex:\\textbf{x}@@")
  (org-element-map (org-element-parse-buffer) 'inline-src-block
    (lambda (s) (list (org-element-property :language s)
                      (org-element-property :value s)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map inline-task
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "*************** TODO Inline task\nBody\n*************** END")
  (org-element-map (org-element-parse-buffer) 'inlinetask
    (lambda (i) (list (org-element-property :raw-value i)
                      (org-element-property :level i)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map statistics-cookie
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"[1/2]\" 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T [1/2]\n- [X] a\n- [ ] b")
  (org-element-map (org-element-parse-buffer) 'statistics-cookie
    (lambda (s) (list (org-element-property :value s)
                      (org-element-property :begin s)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map entity
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_entity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"alpha\" \"α\") (\"beta\" \"β\") (\"gamma\" \"γ\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text \\alpha \\beta \\gamma")
  (org-element-map (org-element-parse-buffer) 'entity
    (lambda (e) (list (org-element-property :name e)
                      (org-element-property :utf-8 e)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map latex-fragment
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"$x^2$\" \"$$y=mx+b$$\" \"\\\\(z\\\\)\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text $x^2$ and $$y=mx+b$$ and \\(z\\)")
  (org-element-map (org-element-parse-buffer) 'latex-fragment
    (lambda (l) (org-element-property :value l))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map macro
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"name\" \"{{{name}}}\" nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: name Hello\nText {{{name}}} end")
  (org-element-map (org-element-parse-buffer) 'macro
    (lambda (m) (list (org-element-property :key m)
                      (org-element-property :value m)
                      (org-element-property :args m)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map radio-target
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_radio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((radio-target \"radio\") (target \"radio\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "<<<radio>>> and <<radio>>")
  (org-element-map (org-element-parse-buffer) '(radio-target target)
    (lambda (t) (list (org-element-type t)
                      (org-element-property :value t)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map superscript/subscript
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_script() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((subscript \"2O\") (superscript \"2\") (subscript \"n+1\") (superscript \"b\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "H_2O and E=mc^2 and x_{n+1} and a^{b}")
  (org-element-map (org-element-parse-buffer) '(subscript superscript)
    (lambda (s) (list (org-element-type s)
                      (org-trim (buffer-substring-no-properties
                                  (org-element-property :contents-begin s)
                                  (org-element-property :contents-end s)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map timestamp types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_ts_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((active-range 2026 20) (inactive 2026 30))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15>\n* U\n<2026-01-20>--<2026-01-25>\n* V\n[2026-01-30]")
  (org-element-map (org-element-parse-buffer) 'timestamp
    (lambda (ts) (list (org-element-property :type ts)
                      (org-element-property :year-start ts)
                      (org-element-property :day-start ts)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with-multiple-keywords
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TITLE\" \"Test\") (\"AUTHOR\" \"Me\") (\"DATE\" \"2026-01-15\") (\"OPTIONS\" \"toc:nil\") (\"FILETAGS\" \":tag1:tag2:\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n#+AUTHOR: Me\n#+DATE: 2026-01-15\n#+OPTIONS: toc:nil\n#+FILETAGS: :tag1:tag2:")
  (org-element-map (org-element-parse-buffer) 'keyword
    (lambda (k) (list (org-element-property :key k)
                      (org-element-property :value k)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with-multiple-blocks
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (src-block quote-block center-block)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\n#+BEGIN_QUOTE\nQ\n#+END_QUOTE\n#+BEGIN_CENTER\nC\n#+END_CENTER")
  (org-element-map (org-element-parse-buffer) '(src-block quote-block center-block)
    (lambda (b) (org-element-type b))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with-multiple-headlines
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_headlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"H1\" \"TODO\" 65 (\"t1\")) (2 \"H2\" \"DONE\" 66 (\"t2\")) (3 \"H3\" \"TODO\" nil nil) (1 \"H4\" \"DONE\" nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO [#A] H1 :t1:\n** DONE [#B] H2 :t2:\n*** TODO H3\n* DONE H4")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :level h)
                      (org-element-property :raw-value h)
                      (org-element-property :todo-keyword h)
                      (org-element-property :priority h)
                      (org-element-property :tags h)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with-multiple-paragraphs
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_paras() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"Para1 *bold*\" \"Para2 /italic/\" \"Para3 =code=\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Para1 *bold*\n\nPara2 /italic/\n\nPara3 =code=")
  (org-element-map (org-element-parse-buffer) 'paragraph
    (lambda (p) (org-trim (buffer-substring-no-properties
                            (org-element-property :contents-begin p)
                            (org-element-property :contents-end p))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with-multiple-lists
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"- \" nil \"A\\n  1. sub\\n  2. sub\") (\"1. \" nil \"sub\") (\"2. \" nil \"sub\") (\"- \" nil \"B\") (\"+ \" nil \"C\") (\"- \" nil \"D\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n  1. sub\n  2. sub\n- B\n+ C\n- D")
  (org-element-map (org-element-parse-buffer) 'item
    (lambda (i) (list (org-element-property :bullet i)
                      (org-element-property :level i)
                      (org-trim (buffer-substring-no-properties
                                  (org-element-property :contents-begin i)
                                  (org-element-property :contents-end i)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with-multiple-tables
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 3) (40 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n|---+---|\n| 1 | 2 |\n\nTable2:\n| x | y |")
  (org-element-map (org-element-parse-buffer) 'table
    (lambda (t) (list (org-element-property :begin t)
                      (length (org-element-map t 'table-row 'identity))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with-multiple-links
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_links() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"http\" \"//a\") (\"file\" \"f.el\") (\"id\" \"xxx\") (\"mailto\" \"e@x\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n[[http://a][A]] [[file:f.el]] [[id:xxx]] [[mailto:e@x]]")
  (org-element-map (org-element-parse-buffer) 'link
    (lambda (l) (list (org-element-property :type l)
                      (org-element-property :path l)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with-multiple-footnotes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_footnotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] more[fn:2]\n\n[fn:1] Def1\n[fn:2] Def2")
  (list (length (org-element-map (org-element-parse-buffer) 'footnote-reference 'identity))
        (length (org-element-map (org-element-parse-buffer) 'footnote-definition 'identity))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map all element types in complex doc
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Complex\n* TODO [#A] H1 :t1:\nBody *bold* /italic/\n** H2\n- item1\n- item2\n| a | b |\n#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC\n* DONE H3\n:PROPERTIES:\n:A: 1\n:END:")
  (let ((types (org-element-map (org-element-parse-buffer) 'element 'org-element-type)))
    (list (length types)
          (sort (delete-dups (copy-sequence types)) 'string<))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map all object types in complex doc
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf15_complex_obj() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* /italic/ _under_ +strike+ =code= ~verb~ [[http://a][Link]] $x^2$ \\alpha H_2O E=mc^2")
  (let ((types (org-element-map (org-element-parse-buffer) 'object 'org-element-type)))
    (list (length types)
          (sort (delete-dups (copy-sequence types)) 'string<))))"##,
        expect,
    );
}
