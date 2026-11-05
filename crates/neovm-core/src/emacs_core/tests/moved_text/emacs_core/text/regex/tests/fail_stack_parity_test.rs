//! GNU parity of the backtracker's fail stack (P3.3 section 12, D5-D7): the
//! cycle check of a non-greedy loop over a nullable body, and the "Stack
//! overflow in regexp matcher" that the Pike fallback used to mask.
//!
//! Every expected value below is GNU 31.1's `(string-match RE S)` followed by
//! `(match-data t)` (probe: `tmp/probes/nasty.el` in the U0.5 worktree).

use neovm_text::emacs_core::regex_emacs::*;

#[test]
fn nullable_nongreedy_loop_answers_nil_from_lisp() {
    let mut ev = Context::new();
    let caught = ev
        .eval_str(
            "(list (condition-case err (string-match \"\\\\(a\\\\|\\\\)+?x\" \"aaaay\") \
                     (error err)) \
                   (string-match \"\\\\(?:a?\\\\)*?b\" \"aaaac\"))",
        )
        .expect("condition-case evaluates");
    assert_eq!(crate::emacs_core::print::print_value(&caught), "(nil nil)");
}

#[test]
fn pike_fallback_overflow_signals_the_gnu_error_from_lisp() {
    let mut ev = Context::new();
    let caught = ev
        .eval_str(
            "(condition-case err \
                 (string-match \"x\\\\(?:a\\\\|b\\\\)*c\" \
                               (concat \"x\" (apply #'concat (make-list 100000 \"ab\")))) \
               (error err))",
        )
        .expect("condition-case evaluates");
    assert_eq!(
        crate::emacs_core::print::print_value(&caught),
        "(error \"Stack overflow in regexp matcher\")"
    );
}
