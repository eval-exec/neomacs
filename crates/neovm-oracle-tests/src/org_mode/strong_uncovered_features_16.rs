//! Strong uncovered-features-16 oracle tests — test features not yet tested.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point in various contexts
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:top headline) (:body paragraph) (:h2 headline) (:sub paragraph))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody text\n** H2\nSub\n* H3")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :top (org-element-type (org-element-at-point))) r)
    (search-forward "Body")
    (beginning-of-line)
    (push (list :body (org-element-type (org-element-at-point))) r)
    (search-forward "H2")
    (beginning-of-line)
    (push (list :h2 (org-element-type (org-element-at-point))) r)
    (search-forward "Sub")
    (beginning-of-line)
    (push (list :sub (org-element-type (org-element-at-point))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-context at various positions
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:h headline) (:bold bold) (:italic italic) (:code verbatim))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nPara *bold* /italic/ =code=")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :h (org-element-type (org-element-context))) r)
    (search-forward "bold")
    (push (list :bold (org-element-type (org-element-context))) r)
    (search-forward "italic")
    (push (list :italic (org-element-type (org-element-context))) r)
    (search-forward "code")
    (push (list :code (org-element-type (org-element-context))) r)
    (nreverse r)))"##,
        expect,
    );
}

// �══════════════════════════════════════════════════════════════════════
// org-element-at-point in table
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:table table) (:cell table-cell))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n|---+---|\n| 1 | 2 |")
  (goto-char (point-min))
  (let ((r '()))
    (push (list :table (org-element-type (org-element-at-point))) r)
    (search-forward "a")
    (push (list :cell (org-element-type (org-element-context))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point in list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:list plain-list) (:item item))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "- A\n- B\n- C")
  (goto-char (point-min))
  (let ((r '()))
    (push (list :list (org-element-type (org-element-at-point))) r)
    (search-forward "B")
    (beginning-of-line)
    (push (list :item (org-element-type (org-element-at-point))) r)
    (nreverse r)))"##,
        expect,
    );
}

// �══════════════════════════════════════════════════════════════════════
// org-element-at-point in src-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_src() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:begin src-block) (:inside src-block))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC")
  (goto-char (point-min))
  (let ((r '()))
    (push (list :begin (org-element-type (org-element-at-point))) r)
    (search-forward "(+")
    (beginning-of-line)
    (push (list :inside (org-element-type (org-element-at-point))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point in quote-block
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_quote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK quote-block""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_QUOTE\nQuoted\n#+END_QUOTE")
  (goto-char (point-min))
  (org-element-type (org-element-at-point)))"##,
        expect,
    );
}

// �══════════════════════════════════════════════════════════════════════
// org-element-at-point in comment
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK comment""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "# Comment\nNormal")
  (goto-char (point-min))
  (org-element-type (org-element-at-point)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point in fixed-width
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK fixed-width""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert ": fixed\n: lines\nNormal")
  (goto-char (point-min))
  (org-element-type (org-element-at-point)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point in planning
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_planning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK planning""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\nSCHEDULED: <2026-01-15>\nBody")
  (goto-char (point-min))
  (forward-line)
  (org-element-type (org-element-at-point)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point in property-drawer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_prop_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK node-property""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* T\n:PROPERTIES:\n:A: 1\n:END:")
  (goto-char (point-min))
  (search-forward ":A:")
  (beginning-of-line)
  (org-element-type (org-element-at-point)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with keyword
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_keyword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK keyword""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n* H")
  (goto-char (point-min))
  (org-element-type (org-element-at-point)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with horizontal-rule
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_hr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK horizontal-rule""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "-----\nText")
  (goto-char (point-min))
  (org-element-type (org-element-at-point)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with footnote
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_footnote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ref paragraph) (:def footnote-definition))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text[fn:1]\n\n[fn:1] Def")
  (goto-char (point-min))
  (let ((r '()))
    (push (list :ref (org-element-type (org-element-context))) r)
    (search-forward "Def")
    (beginning-of-line)
    (push (list :def (org-element-type (org-element-at-point))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"Link\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text [[http://a][Link]] end")
  (search-forward "Link")
  (org-element-type (org-element-context)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with bold
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_bold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"bold\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Para *bold* text")
  (search-forward "bold")
  (org-element-type (org-element-context)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with italic
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"italic\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Para /italic/ text")
  (search-forward "italic")
  (org-element-type (org-element-context)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with code
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_code() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"code\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Para =code= text")
  (search-forward "code")
  (org-element-type (org-element-context)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with verbatim
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_verb() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"verb\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Para ~verb~ text")
  (search-forward "verb")
  (org-element-type (org-element-context)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with strike
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_strike() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"strike\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Para +strike+ text")
  (search-forward "strike")
  (org-element-type (org-element-context)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with underline
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_under() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"under\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Para _under_ text")
  (search-forward "under")
  (org-element-type (org-element-context)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with subscript
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_sub() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"_2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "H_2O text")
  (search-forward "_2")
  (org-element-type (org-element-context)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with superscript
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_super() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"^2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "E=mc^2 text")
  (search-forward "^2")
  (org-element-type (org-element-context)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with latex-fragment
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"$x\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text $x^2$ end")
  (search-forward "$x")
  (org-element-type (org-element-context)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with entity
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_entity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"\\\\alpha\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text \\alpha end")
  (search-forward "\\alpha")
  (org-element-type (org-element-context)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with timestamp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_ts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"<2026\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "Text <2026-01-15> end")
  (search-forward "<2026")
  (org-element-type (org-element-context)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-at-point with macro
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf16_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"{{{m}}}\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+MACRO: m Hi\nText {{{m}}} end")
  (search-forward "{{{m}}}")
  (org-element-type (org-element-context)))"##,
        expect,
    );
}
