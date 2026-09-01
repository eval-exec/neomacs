//! Strong uncovered-features-14 oracle tests — test features not yet tested.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with first-match
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_map_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"A\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n* B\n* C\n* D")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))
    nil 'first-match))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with no-recursion
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_map_no_rec() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"H1\" \"H2\" \"H3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))
    nil nil nil nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map over objects
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_map_obj() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* /italic/ _under_ +strike+ =code= ~verbatim~")
  (org-element-map (org-element-parse-buffer) '(bold italic underline strike-through code verbatim)
    (lambda (o) (list (org-element-type o)
                      (org-trim (buffer-substring-no-properties
                                  (org-element-property :contents-begin o)
                                  (org-element-property :contents-end o)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map over links
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_map_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"http\" \"//a\") (\"file\" \"f.el\") (\"id\" \"xxx\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n[[http://a][A]] and [[file:f.el]] and [[id:xxx]]")
  (org-element-map (org-element-parse-buffer) 'link
    (lambda (l) (list (org-element-property :type l)
                      (org-element-property :path l)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map over all elements recursively
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_map_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara\n- i1\n- i2\n| a |")
  (sort (delete-dups (org-element-map (org-element-parse-buffer) 'element
                        'org-element-type))
        'string<))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point in narrowed buffer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK paragraph""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nBody 1\n* H2\nBody 2\n* H3\nBody 3")
  (goto-char (point-min))
  (search-forward "Body 2")
  (beginning-of-line)
  (narrow-to-region (line-beginning-position) (line-end-position))
  (let ((type (org-element-type (org-element-at-point))))
    (widen)
    type))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map counts per type
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3 2 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\nPara\n** H2\n- a\n- b\n| x |\n* H3")
  (list (length (org-element-map (org-element-parse-buffer) 'headline 'identity))
        (length (org-element-map (org-element-parse-buffer) 'paragraph 'identity))
        (length (org-element-map (org-element-parse-buffer) 'item 'identity))
        (length (org-element-map (org-element-parse-buffer) 'table 'identity))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map nested structure
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 \"H1\" nil) (2 \"H2\" \"H1\") (3 \"H3\" \"H2\") (4 \"H4\" \"H3\") (5 \"H5\" \"H4\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1\n** H2\n*** H3\n**** H4\n***** H5")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (list (org-element-property :level h)
                      (org-element-property :raw-value h)
                      (let ((parent (org-element-property :parent h)))
                        (when parent (org-element-property :raw-value parent)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map planning
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((timestamp (:standard-properties [21 nil nil nil 33 0 nil nil nil nil nil nil nil nil nil nil nil nil] :type active :range-type nil :raw-value \"<2026-01-15>\" :year-start 2026 :month-start 1 :day-start 15 :hour-start nil :minute-start nil :year-end 2026 :month-end 1 :day-end 15 :hour-end nil :minute-end nil)) nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO T\nSCHEDULED: <2026-01-15>\nDEADLINE: <2026-01-20>\nCLOSED: [2026-01-10]")
  (org-element-map (org-element-parse-buffer) 'planning
    (lambda (p) (list (org-element-property :scheduled p)
                      (org-element-property :deadline p)
                      (org-element-property :closed p)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map drawer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:\n:LOGBOOK:\n:END:\n:MYDRAWER:\nData\n:END:")
  (org-element-map (org-element-parse-buffer) 'drawer
    (lambda (d) (list (org-element-property :drawer-name d)
                      (org-trim (buffer-substring-no-properties
                                  (org-element-property :contents-begin d)
                                  (org-element-property :contents-end d)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map fixed-width
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments buffer-substring-no-properties 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Fixed:\n: line1\n: line2\n: line3")
  (org-element-map (org-element-parse-buffer) 'fixed-width
    (lambda (fw) (org-trim (buffer-substring-no-properties
                              (org-element-property :value fw))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map comment
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments buffer-substring-no-properties 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "# line1\n# line2\nNormal\n# line3")
  (org-element-map (org-element-parse-buffer) 'comment
    (lambda (c) (org-trim (buffer-substring-no-properties
                            (org-element-property :value c))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map verse-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_verse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Line 1\\nLine 2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_VERSE\nLine 1\nLine 2\n#+END_VERSE")
  (org-element-map (org-element-parse-buffer) 'verse-block
    (lambda (v) (org-trim (buffer-substring-no-properties
                            (org-element-property :contents-begin v)
                            (org-element-property :contents-end v))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map center-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_center() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Centered\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_CENTER\nCentered\n#+END_CENTER")
  (org-element-map (org-element-parse-buffer) 'center-block
    (lambda (c) (org-trim (buffer-substring-no-properties
                            (org-element-property :contents-begin c)
                            (org-element-property :contents-end c))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map quote-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_quote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Quoted text\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_QUOTE\nQuoted text\n#+END_QUOTE")
  (org-element-map (org-element-parse-buffer) 'quote-block
    (lambda (q) (org-trim (buffer-substring-no-properties
                            (org-element-property :contents-begin q)
                            (org-element-property :contents-end q))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map export-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"HTML\" \"<b>Bold</b>\\n\") (\"LATEX\" \"\\\\textbf{Bold}\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_EXPORT html\n<b>Bold</b>\n#+END_EXPORT\n#+BEGIN_EXPORT latex\n\\textbf{Bold}\n#+END_EXPORT")
  (org-element-map (org-element-parse-buffer) 'export-block
    (lambda (e) (list (org-element-property :type e)
                      (org-element-property :value e)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map src-block with results
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_src_res() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (src-block fixed-width)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n#+RESULTS:\n: 3")
  (let ((r '()))
    (org-element-map (org-element-parse-buffer) '(src-block fixed-width)
      (lambda (e) (push (org-element-type e) r)))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with info
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody *bold* /italic/")
  (org-element-map (org-element-parse-buffer) 'object
    (lambda (o) (list (org-element-type o)))
    nil nil nil nil nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map skip headline content
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_skip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H1 *bold*\n** H2 /italic/")
  (org-element-map (org-element-parse-buffer) 'object
    (lambda (o) (list (org-element-type o)
                      (org-trim (buffer-substring-no-properties
                                  (org-element-property :begin o)
                                  (org-element-property :end o)))))
    nil nil 'headline))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map table-row types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_row_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (standard rule standard standard)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n|---+---|\n| 1 | 2 |\n| 3 | 4 |")
  (org-element-map (org-element-parse-buffer) 'table-row
    (lambda (r) (org-element-property :type r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map table-cell contents
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_cells() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" \"*b*\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | *b* |")
  (org-element-map (org-element-parse-buffer) 'table-cell
    (lambda (c) (org-trim (buffer-substring-no-properties
                            (org-element-property :contents-begin c)
                            (org-element-property :contents-end c))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map keyword
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_keyword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TITLE\" \"Test\") (\"AUTHOR\" \"Me\") (\"OPTIONS\" \"toc:nil\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n#+AUTHOR: Me\n#+OPTIONS: toc:nil")
  (org-element-map (org-element-parse-buffer) 'keyword
    (lambda (k) (list (org-element-property :key k)
                      (org-element-property :value k)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with-multiple-types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_multi_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (item paragraph src-block table)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody\n- a\n- b\n| x |\n#+BEGIN_SRC emacs-lisp\n(+ 1)\n#+END_SRC")
  (sort (delete-dups (org-element-map (org-element-parse-buffer) '(paragraph item table src-block)
                        'org-element-type))
        'string<))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with-multiple-objects
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_multi_obj() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (bold italic underline strike-through verbatim code link)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* /italic/ _under_ +strike+ =code= ~verb~ [[http://a][Link]]")
  (org-element-map (org-element-parse-buffer) '(bold italic underline strike-through code verbatim link)
    (lambda (o) (org-element-type o))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map footnote-reference
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_foot() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"1\" \"2\") (\"1\" \"2\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1] more[fn:2]\n\n[fn:1] Def1\n[fn:2] Def2")
  (list (org-element-map (org-element-parse-buffer) 'footnote-reference
          (lambda (f) (org-element-property :label f)))
        (org-element-map (org-element-parse-buffer) 'footnote-definition
          (lambda (f) (org-element-property :label f)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map with predicate
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_pred() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" \"B\" \"C\" \"D\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO A\n* DONE B\n* TODO C\n* DONE D")
  (org-element-map (org-element-parse-buffer) 'headline
    (lambda (h) (org-element-property :raw-value h))
    nil nil nil
    (lambda (h) (string= (org-element-property :todo-keyword h) "DONE"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map nested objects
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_nested_obj() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((bold \"bold /italic/ inside\") (italic \"italic\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold /italic/ inside* text")
  (org-element-map (org-element-parse-buffer) '(bold italic)
    (lambda (o) (list (org-element-type o)
                      (org-trim (buffer-substring-no-properties
                                  (org-element-property :contents-begin o)
                                  (org-element-property :contents-end o)))))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-map over full document
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf14_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T\n* H1\nBody\n** H2\n- a\n- b\n| x |\n* H3\n:PROPERTIES:\n:A: 1\n:END:")
  (let ((types (org-element-map (org-element-parse-buffer) 'element 'org-element-type)))
    (list (length types)
          (sort (delete-dups (copy-sequence types)) 'string<))))"##,
        expect,
    );
}
