//! Strong uncovered-features-23 oracle tests — complex multi-step workflows.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp direct
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf23_elisp_direct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "value")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with output
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf23_elisp_output() {
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
fn uf23_elisp_var() {
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
fn uf23_elisp_list() {
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
fn uf23_elisp_table() {
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
fn uf23_elisp_multi() {
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
fn uf23_elisp_both() {
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
fn uf23_elisp_silent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "silent")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results file
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf23_elisp_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "\"/tmp/test.txt\"" '((:results . "file")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results raw
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf23_elisp_raw() {
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
fn uf23_elisp_org() {
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
fn uf23_elisp_html() {
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
fn uf23_elisp_latex() {
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
fn uf23_elisp_code() {
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
fn uf23_elisp_pp() {
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
fn uf23_elisp_drawer() {
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
fn uf23_elisp_wrap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "value") (:wrap . "example")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :post
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf23_elisp_post() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+BEGIN_SRC emacs-lisp :results value :post data\n(+ 1 2)\n#+END_SRC\n#+NAME: data\n#+BEGIN_SRC emacs-lisp\n(* 10 20)\n#+END_SRC")
  (goto-char (point-min))
  (org-babel-execute-src-block)
  (buffer-string))"##,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :prologue/:epilogue
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf23_elisp_prologue() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ x 2)" '((:results . "value") (:prologue . "(setq x 10)") (:epilogue . "(message \"done\")")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :eval query
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf23_elisp_eval() {
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
fn uf23_elisp_cache() {
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
fn uf23_elisp_hlines() {
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
fn uf23_elisp_colnames() {
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
fn uf23_elisp_rownames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "'((\"a\" 1) (\"b\" 2))" '((:results . "value") (:rownames . "yes")))"##,
        expect,
    );
}
