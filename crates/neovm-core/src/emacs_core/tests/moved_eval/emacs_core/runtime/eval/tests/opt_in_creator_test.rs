//! E6-P.1: the opt-in creator `neomacs-make-parallel-thread` and its hint
//! `neomacs-thread-parallel-p` (pentry DESIGN §6).
//!
//! Production admission is ON: in Green, an eligible opt-in request is admitted
//! and `neomacs-thread-parallel-p` reports t for it; in Sim, admission is
//! unavailable and the request falls back to an ordinary Baton thread with the
//! hint nil. Both arms are covered — the fallback through the crate's
//! force-off test seam, so flipping the switch either way keeps a meaningful
//! row. A thread a default `make-thread` creates inside an opt-in worker is
//! Baton too. Rows run under the simulated default and under Green.

use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::value::Value;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(neovm_value::error::EvalError::Signal { symbol, data, .. }) => format!(
            "ERR {} {}",
            neovm_value::intern::resolve_sym(symbol),
            print_value(&Value::list(data))
        ),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn each_mode(form: &str, expected: &str) {
    each_mode_split(form, expected, expected);
}

/// Per-mode expectations: Sim cannot admit (no Green world), Green can. A row
/// that must agree in both modes uses `each_mode`.
fn each_mode_split(form: &str, sim_expected: &str, green_expected: &str) {
    crate::test_utils::init_test_tracing();
    // One context at a time. An array would construct both before the loop
    // body, and the second constructor refuses while the first is still active.
    for mode in ["sim", "green"] {
        let mut ev = if mode == "sim" {
            crate::test_utils::new_context()
        } else {
            crate::test_utils::new_green_context()
        };
        let expected = if mode == "sim" {
            sim_expected
        } else {
            green_expected
        };
        assert_eq!(eval(&mut ev, form), expected, "{mode}");
    }
}

/// Run `body` in Green with production admission forced OFF: the fallback arm.
fn green_with_admission_off(body: impl FnOnce(&mut Context)) {
    crate::test_utils::init_test_tracing();
    neovm_eval::test_support::authority::with_parallel_admission_disabled_for_test(|| {
        let mut ev = crate::test_utils::new_green_context();
        body(&mut ev);
    });
}

#[test]
fn the_opt_in_creator_has_make_threads_arity_and_the_hint_takes_one_thread() {
    each_mode(
        "(list (fboundp 'neomacs-make-parallel-thread)
               (func-arity 'neomacs-make-parallel-thread)
               (func-arity 'make-thread)
               (func-arity 'neomacs-thread-parallel-p))",
        "(t (1 . 3) (1 . 3) (1 . 1))",
    );
}

const ORDINARY_THREAD_FORM: &str = "\
(let* ((name (copy-sequence \"e6p1\"))
       (th (neomacs-make-parallel-thread
            (lambda () (list 'result (eq (current-thread) main-thread)))
            name)))
  (list (threadp th) (eq name (thread-name th))
        (and (memq th (all-threads)) t)
        (neomacs-thread-parallel-p th)
        (thread-join th) (thread-join th)
        (thread-live-p th)
        (neomacs-thread-parallel-p main-thread)))";

#[test]
fn an_opt_in_thread_is_an_ordinary_thread_and_the_hint_reports_the_configuration() {
    // Sim cannot admit; Green admits. Everything except the hint must agree.
    each_mode_split(
        ORDINARY_THREAD_FORM,
        "(t t t nil (result nil) (result nil) nil nil)",
        "(t t t t (result nil) (result nil) nil nil)",
    );
}

#[test]
fn a_forced_off_request_falls_back_to_a_baton_thread_with_the_hint_nil() {
    // The fallback arm, kept covered now that production admission is on.
    green_with_admission_off(|ev| {
        assert_eq!(
            eval(ev, ORDINARY_THREAD_FORM),
            "(t t t nil (result nil) (result nil) nil nil)"
        );
    });
}

#[test]
fn the_opt_in_creator_validates_like_make_thread() {
    each_mode(
        "(list (condition-case err (neomacs-make-parallel-thread (lambda () 1) 42)
                 (error err))
               (condition-case err (make-thread (lambda () 1) 42)
                 (error err)))",
        "((wrong-type-argument stringp 42) (wrong-type-argument stringp 42))",
    );
}

#[test]
fn the_hint_refuses_a_non_thread() {
    each_mode(
        "(condition-case err (neomacs-thread-parallel-p 'not-a-thread) (error err))",
        "(wrong-type-argument threadp not-a-thread)",
    );
}

#[test]
fn a_default_thread_made_inside_an_opt_in_worker_is_baton() {
    each_mode(
        "(thread-join
          (neomacs-make-parallel-thread
           (lambda ()
             (let ((child (make-thread (lambda () 'child-result))))
               (list (thread-join child) (neomacs-thread-parallel-p child))))))",
        "(child-result nil)",
    );
}
