//! Strong uncovered-features-24 oracle tests — complex multi-step workflows.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results table
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_table_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "'((1 2) (3 4))" '((:results . "table")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results list
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_list_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "'(1 2 3)" '((:results . "list")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results vector
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_vector_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "[1 2 3]" '((:results . "vector")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results scalar
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_scalar_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "scalar")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results verbatim
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_verbatim_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "\"hello\"" '((:results . "verbatim")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results file-link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_file_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "\"/tmp/test.txt\"" '((:results . "file-link")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results graphics
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_graphics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "\"/tmp/img.png\"" '((:results . "graphics")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results replace
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "replace")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results append
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_append() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "append")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results prepend
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_prepend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "prepend")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results link
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "\"/tmp/test.txt\"" '((:results . "link")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results file
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "\"/tmp/test.txt\"" '((:results . "file")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :results org
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_org() {
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
fn uf24_html() {
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
fn uf24_latex() {
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
fn uf24_code() {
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
fn uf24_pp() {
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
fn uf24_drawer() {
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
fn uf24_wrap() {
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
fn uf24_prologue() {
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
fn uf24_eval_never() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(+ 1 2)" '((:results . "value") (:eval . "never")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :cache yes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "(random)" '((:results . "value") (:cache . "yes")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :hlines yes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_hlines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "'((1 2) :hline (3 4))" '((:results . "value") (:hlines . "yes")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :colnames yes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_colnames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "'((\"a\" \"b\") (1 2))" '((:results . "value") (:colnames . "yes")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-babel-execute:emacs-lisp with :rownames yes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf24_rownames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-babel-execute:emacs-lisp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-babel-execute:emacs-lisp "'((\"a\" 1) (\"b\" 2))" '((:results . "value") (:rownames . "yes")))"##,
        expect,
    );
}
