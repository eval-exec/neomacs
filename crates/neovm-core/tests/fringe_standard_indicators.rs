//! Stage 2 integration tests for the standard fringe-indicator system.
//!
//! These verify that with a fully lisp-loaded bootstrap evaluator (loadup.el ->
//! fringe.el), the standard built-in fringe bitmaps are seeded AND
//! `lisp/fringe.el`'s `(boundp 'fringe-bitmaps)`-guarded block installs the GNU
//! default `fringe-indicator-alist` / `fringe-cursor-alist`. The block is gated
//! on `fringe-bitmaps` being bound, which neomacs now binds in Rust (mirroring
//! GNU `syms_of_fringe`'s `Vfringe_bitmaps = Qnil`).
//!
//! NOTE: these tests exercise the full `create_bootstrap_evaluator_cached`
//! lisp-load path. In a tree where that bootstrap is healthy they verify the
//! Stage 2 contract end to end. They will fail to even reach their assertions
//! whenever the loadup bootstrap is broken for unrelated reasons (e.g. an
//! in-progress display-engine refactor) — the failure surfaces in `setup_eval`'s
//! `expect("bootstrap")`, not in the fringe assertions. The bare-`Context`
//! analogues in `builtins/fringe_bitmap_test.rs` cover what can be checked
//! without the full lisp surface.

use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::format_eval_result_with_eval;
use neovm_core::emacs_core::load::{
    apply_runtime_startup_state, create_bootstrap_evaluator_cached,
};

fn setup_eval() -> Context {
    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap");
    apply_runtime_startup_state(&mut eval).expect("startup");
    eval.set_lexical_binding(true);
    eval
}

/// Evaluate FORM and return the printed value, stripping the leading `OK `
/// (panicking if the form signalled an error so failures surface clearly).
fn eval_str(eval: &mut Context, form: &str) -> String {
    let result = eval.eval_str(form);
    let formatted = format_eval_result_with_eval(eval, &result);
    formatted
        .strip_prefix("OK ")
        .unwrap_or_else(|| panic!("form {form:?} did not evaluate OK: {formatted}"))
        .to_string()
}

/// `fringe-bitmaps` is bound (so fringe.el's seeding block runs at all).
#[test]
fn fringe_bitmaps_var_is_bound() {
    let mut eval = setup_eval();
    assert_eq!(eval_str(&mut eval, "(boundp 'fringe-bitmaps)"), "t");
    // fringe.el `push`es every standard symbol onto it.
    assert_eq!(eval_str(&mut eval, "(consp fringe-bitmaps)"), "t");
    assert_eq!(
        eval_str(&mut eval, "(if (memq 'right-arrow fringe-bitmaps) t nil)"),
        "t"
    );
    assert_eq!(
        eval_str(&mut eval, "(if (memq 'empty-line fringe-bitmaps) t nil)"),
        "t"
    );
}

/// `fringe-indicator-alist` has GNU's default structure (fringe.el ~65-77).
#[test]
fn fringe_indicator_alist_has_gnu_defaults() {
    let mut eval = setup_eval();
    assert_eq!(eval_str(&mut eval, "(listp fringe-indicator-alist)"), "t");

    // fringe.el: `(truncation . (left-arrow right-arrow))` reads as a proper
    // 3-element list `(truncation left-arrow right-arrow)`.
    assert_eq!(
        eval_str(&mut eval, "(assq 'truncation fringe-indicator-alist)"),
        "(truncation left-arrow right-arrow)"
    );
    assert_eq!(
        eval_str(&mut eval, "(assq 'continuation fringe-indicator-alist)"),
        "(continuation left-curly-arrow right-curly-arrow)"
    );
    // `(empty-line . empty-line)` is a genuine dotted pair.
    assert_eq!(
        eval_str(&mut eval, "(assq 'empty-line fringe-indicator-alist)"),
        "(empty-line . empty-line)"
    );
    // `(overlay-arrow . right-triangle)` is a genuine dotted pair.
    assert_eq!(
        eval_str(&mut eval, "(assq 'overlay-arrow fringe-indicator-alist)"),
        "(overlay-arrow . right-triangle)"
    );
    // `(unknown . question-mark)` is a genuine dotted pair.
    assert_eq!(
        eval_str(&mut eval, "(assq 'unknown fringe-indicator-alist)"),
        "(unknown . question-mark)"
    );
    // `(top . (top-left-angle top-right-angle))` reads as `(top top-left-angle
    // top-right-angle)`.
    assert_eq!(
        eval_str(&mut eval, "(assq 'top fringe-indicator-alist)"),
        "(top top-left-angle top-right-angle)"
    );
}

/// `fringe-cursor-alist` has GNU's default structure (fringe.el ~79-84).
#[test]
fn fringe_cursor_alist_has_gnu_defaults() {
    let mut eval = setup_eval();
    assert_eq!(eval_str(&mut eval, "(listp fringe-cursor-alist)"), "t");
    assert_eq!(
        eval_str(&mut eval, "(assq 'box fringe-cursor-alist)"),
        "(box . filled-rectangle)"
    );
    assert_eq!(
        eval_str(&mut eval, "(assq 'hollow fringe-cursor-alist)"),
        "(hollow . hollow-rectangle)"
    );
    assert_eq!(
        eval_str(&mut eval, "(assq 'bar fringe-cursor-alist)"),
        "(bar . vertical-bar)"
    );
    assert_eq!(
        eval_str(&mut eval, "(assq 'hbar fringe-cursor-alist)"),
        "(hbar . horizontal-bar)"
    );
    assert_eq!(
        eval_str(&mut eval, "(assq 'hollow-small fringe-cursor-alist)"),
        "(hollow-small . hollow-square)"
    );
}

/// Every physical bitmap symbol referenced by the indicator alist resolves to a
/// registered bitmap index via its `'fringe` property.
#[test]
fn indicator_alist_symbols_resolve_to_bitmap_indices() {
    let mut eval = setup_eval();
    for sym in [
        "left-arrow",
        "right-arrow",
        "left-curly-arrow",
        "right-curly-arrow",
        "right-triangle",
        "up-arrow",
        "down-arrow",
        "top-left-angle",
        "top-right-angle",
        "bottom-left-angle",
        "bottom-right-angle",
        "left-bracket",
        "right-bracket",
        "empty-line",
        "question-mark",
    ] {
        let got = eval_str(&mut eval, &format!("(integerp (get '{sym} 'fringe))"));
        assert_eq!(got, "t", "{sym} should have an integer 'fringe index");
    }
    // Spot-check an exact index matches fringe.c.
    assert_eq!(eval_str(&mut eval, "(get 'right-arrow 'fringe)"), "4");
    assert_eq!(eval_str(&mut eval, "(get 'empty-line 'fringe)"), "24");
}
