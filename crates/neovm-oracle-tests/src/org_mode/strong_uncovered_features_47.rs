//! Strong uncovered-features-47 oracle tests — org-babel complex, org-src edit.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with various :results
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "value")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results output
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(princ \"hello\")" '((:results . "output")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with var
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ x y)" '((:results . "value") (:var . "x=10") (:var . "y=20")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with list result
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "'(1 2 3)" '((:results . "value")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with table result
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "'((1 2) (3 4))" '((:results . "value")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with multiple statements
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(setq x 10)\n(setq y 20)\n(+ x y)" '((:results . "value")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results both
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_both() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(princ \"out\")\n(+ 1)" '((:results . "both")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results silent
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_silent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "silent")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results raw
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_raw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "raw")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results org
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_org() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "org")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results html
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_html() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "\"<b>bold</b>\"" '((:results . "html")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results latex
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "\"\\\\textbf{bold}\"" '((:results . "latex")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results code
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_code() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "code")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results pp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_pp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "'(1 2 3)" '((:results . "pp")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results drawer
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "drawer")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :wrap
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_wrap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "value") (:wrap . "example")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :prologue/:epilogue
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_prologue() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ x 2)" '((:results . "value") (:prologue . "(setq x 10)") (:epilogue . "(message \"done\")")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :eval never
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "value") (:eval . "never")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :cache
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(random)" '((:results . "value") (:cache . "yes")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :hlines
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_hlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "'((1 2) :hline (3 4))" '((:results . "value") (:hlines . "yes")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :colnames
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_colnames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "'((\"a\" \"b\") (1 2))" '((:results . "value") (:colnames . "yes")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :rownames
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf47_babel_rownames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "'((\"a\" 1) (\"b\" 2))" '((:results . "value") (:rownames . "yes")))"##,
        expect,
    );
}
