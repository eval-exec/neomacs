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

/// Run `f` on `arg` natively (a fresh compile, whose jump table is answered
/// inline unless `NEOVM_JIT_INLINE_SWITCH=off`) and through the
/// interpreter, and require the same answer (printed: a consed answer is a
/// fresh list on each side). Returns the printed answer.
fn native_matches_interpreter(ev: &mut Context, f: Value, arg: Value, what: &str) -> String {
    let data = f.get_bytecode_data().expect("byte-code function");
    let before = super::switch_dispatch::inline_switch_sites_for_test();
    let leaf = compile_bytecode_function_with(data, Some(&ev.obarray)).expect("compiles");
    assert_eq!(
        super::switch_dispatch::inline_switch_sites_for_test() > before,
        jit_inline_switch_on(),
        "{what}: the jump table is answered inline exactly when the knob is on"
    );
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

/// The elb-pcase kernel as the real byte compiler emits it: its `equal` jump
/// table holds GNU byte offsets, so MIR must build it against the offset
/// map, as the baseline does. With the map the builder reaches the Switch
/// and names it (no tier change: the op is still unmodelled); read as
/// instruction indices, the same offsets failed as a stack-model error.
#[test]
fn elb_pcase_mir_build_resolves_the_jump_table_through_the_offset_map() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let f = byte_compile(&mut ev, ELB_PCASE);
    let data = f.get_bytecode_data().expect("byte-code function");
    let ops = data.executable_ops();
    let map = data.executable_gnu_byte_offset_map();
    assert!(map.is_some(), "real bytecode carries its offset map");
    let arity = data.params.required.len();
    assert!(matches!(
        mir::build_mir(ops, &data.constants, map, arity),
        Err(CompileError::UnsupportedOp("mir-unmodelled-control:Switch"))
    ));
    let unmapped = mir::build_mir(ops, &data.constants, None, arity);
    assert!(
        matches!(
            unmapped,
            Err(CompileError::BadOperand
                | CompileError::StackUnderflow
                | CompileError::UnsupportedOp(
                    "inconsistent stack depth" | "mir-unreachable-block"
                ))
        ),
        "byte offsets read as indices: {unmapped:?}"
    );
    // Through the tier-up: still a baseline leaf, whose verdict now names
    // the Switch.
    crate::emacs_core::jit::stats::force_observe_for_test(
        crate::emacs_core::jit::stats::ObserveOverride {
            stats: true,
            naming: false,
            entry_count: false,
        },
    );
    let leaf = compile_bytecode_function_with(data, Some(&ev.obarray)).expect("compiles");
    assert_eq!(leaf.tier().name(), "baseline");
    assert_eq!(
        leaf.obs.mir_verdict.as_deref(),
        Some("build:UnsupportedOp(\"mir-unmodelled-control:Switch\")")
    );
}
