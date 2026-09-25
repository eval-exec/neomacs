//! `Op::Switch` lowered natively on bodies the real byte compiler emits
//! (`byte-compile-cond-jump-table`, lisp/emacs-lisp/bytecomp.el): the answers
//! must be the interpreter's for every shape of dispatch value.

use super::*;
use crate::emacs_core::bytecode::Vm;
use crate::emacs_core::eval::Context;

/// Byte-compile `lambda_src` lexically in a bootstrapped context and return
/// the function value (rooted for the rest of the test).
fn byte_compile(ev: &mut Context, lambda_src: &str) -> Value {
    let f = ev
        .eval_str(&format!(
            "(progn (require 'cl-lib) (eval '(byte-compile {lambda_src}) t))"
        ))
        .expect("byte-compiles");
    crate::emacs_core::eval::push_scratch_gc_root(f);
    assert!(
        f.get_bytecode_data().is_some(),
        "byte-compile made a byte-code function"
    );
    f
}

/// Run `f` on `arg` natively (a fresh compile) and through the interpreter,
/// and require the same answer (printed: a consed answer is a fresh list on
/// each side). Returns the printed answer.
fn native_matches_interpreter(ev: &mut Context, f: Value, arg: Value, what: &str) -> String {
    let data = f.get_bytecode_data().expect("byte-code function");
    let leaf = compile_bytecode_function_with(data, Some(&ev.obarray)).expect("compiles");
    let interp = Vm::from_context(ev)
        .execute(data, vec![arg])
        .expect("interprets");
    let interp = crate::emacs_core::print::print_value(&interp);
    let NativeRun::Ok(bits) = leaf.call(ev as *mut Context as *mut u8, &[arg]) else {
        panic!("{what}: expected a native answer");
    };
    let native = crate::emacs_core::print::print_value(&Value::from_bits(bits));
    assert_eq!(native, interp, "{what}");
    interp
}

/// elb-pcase's body (elisp-benchmarks' `elb-pcase.el`): an `equal` jump
/// table keyed by the conses `(a b)` and `(a)`, inside a `cl-loop`.
const ELB_PCASE: &str = "(lambda (l)
  (cl-loop for x in l
           counting (pcase x
                      (`(a b) 1)
                      (`(a) 2)
                      (_ (1+ (* x x))))))";

/// The same dispatch with a total fallback, so any value can be fed to it.
const PCASE_COLLECT: &str = "(lambda (l)
  (let (acc)
    (dolist (x l)
      (push (pcase x (`(a b) 1) (`(a) 2) ('a 3) (7 4) (_ 0)) acc))
    acc))";

#[test]
fn elb_pcase_body_dispatches_natively_like_the_interpreter() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let f = byte_compile(&mut ev, ELB_PCASE);
    let input = ev
        .eval_str("(let (l) (dotimes (i 30) (push (pcase (% i 3) (0 (list 'a 'b)) (1 (list 'a)) (_ i)) l)) l)")
        .expect("input list");
    crate::emacs_core::eval::push_scratch_gc_root(input);
    let answer = native_matches_interpreter(&mut ev, f, input, "elb-pcase");
    assert_eq!(answer, "30", "every element counts");
}

#[test]
fn pcase_dispatch_answers_every_value_shape_like_the_interpreter() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let f = byte_compile(&mut ev, PCASE_COLLECT);
    let input = ev
        .eval_str(
            "(list (list 'a 'b) (list 'a) 'a 7 (list 'a 'b 'c) (cons 'a 'b) (list 'b) nil t
                   (list (list 'a) 'b) (list 'a (list 'b)) \"a\" 7.0 (expt 2 70) (vector 'a)
                   (let ((c (list 'a 'b))) (setcdr (cdr c) c) c)
                   (let ((c (list 'a))) (setcdr c c) c)
                   (list 'a 'b nil) -7 8 (make-symbol \"a\"))",
        )
        .expect("input list");
    crate::emacs_core::eval::push_scratch_gc_root(input);
    let answer = native_matches_interpreter(&mut ev, f, input, "value shapes");
    assert_eq!(answer, "(0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 4 3 2 1)");
}
