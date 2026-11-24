//! E6-P.1: the opt-in creator `neomacs-make-parallel-thread` and its hint
//! `neomacs-thread-parallel-p` (pentry DESIGN §6).
//!
//! Until a production N-mutator admission exists, every opt-in request falls
//! back to an ordinary Baton thread: the same object, the same thread APIs and
//! errors as `make-thread`, and the hint nil. A thread a default `make-thread`
//! creates inside an opt-in worker is Baton too. Each row runs under the
//! simulated default and under Green.

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
    crate::test_utils::init_test_tracing();
    // One context at a time. An array would construct both before the loop
    // body, and the second constructor refuses while the first is still active.
    for mode in ["sim", "green"] {
        let mut ev = if mode == "sim" {
            crate::test_utils::new_context()
        } else {
            crate::test_utils::new_green_context()
        };
        assert_eq!(eval(&mut ev, form), expected, "{mode}");
    }
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

#[test]
fn an_opt_in_thread_is_an_ordinary_thread_with_the_hint_nil() {
    each_mode(
        "(let* ((name (copy-sequence \"e6p1\"))
                (th (neomacs-make-parallel-thread
                     (lambda () (list 'result (eq (current-thread) main-thread)))
                     name)))
           (list (threadp th) (eq name (thread-name th))
                 (and (memq th (all-threads)) t)
                 (neomacs-thread-parallel-p th)
                 (thread-join th) (thread-join th)
                 (thread-live-p th)
                 (neomacs-thread-parallel-p main-thread)))",
        "(t t t nil (result nil) (result nil) nil nil)",
    );
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
