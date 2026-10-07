//! Calls in tail position root no residuals (design
//! `p1-1-direct-native-calls` §3.8.4, P1.0 S1.2): nothing after a call that
//! `Return` follows reads the operand-stack slots below its callee, so the
//! site stores none of them into the GC root window -- in the baseline and
//! the MIR tier alike -- while every other call still does.

use super::lowering::{TAIL_CALLS_UNROOTED, rootwin_counters};
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

fn tail_calls_unrooted() -> usize {
    TAIL_CALLS_UNROOTED.with(std::cell::Cell::get)
}

/// `(lambda (l) (h1 l) (h2 (cdr l)))`, and with `tail` false the same body
/// returning `l` after the second call, so that call is not in tail position.
/// Both calls are generic (the heads are unbound), so the body stays on the
/// baseline tier and hoists its root window (two rooting sites).
fn two_calls(tail: bool) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::Constant(0), // h1          [l h1]
        Op::StackRef(1), // l           [l h1 l]
        Op::Call(1),     // residual l  [l r1]
        Op::Pop,         //             [l]
        Op::Constant(1), // h2          [l h2]
        Op::StackRef(1), // l           [l h2 l]
        Op::Cdr,         //             [l h2 d]
        Op::Call(1),     // residual l  [l r2]
    ];
    if !tail {
        f.ops.push(Op::Pop);
    }
    f.ops.push(Op::Return);
    f.constants = vec![Value::symbol("neovm--tc-h1"), Value::symbol("neovm--tc-h2")].into();
    f.max_stack = 8;
    f
}

#[test]
fn a_tail_call_stores_no_residual_and_other_calls_still_do() {
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    force_tail_unrooted_for_test(Some(true));
    let ev = Context::new();
    let before = tail_calls_unrooted();
    let leaf =
        compile_bytecode_function_with(&two_calls(true), Some(&ev.obarray)).expect("compiles");
    assert_eq!(leaf.tier, super::leaf::LeafTier::Baseline);
    assert_eq!(
        rootwin_counters(),
        (1, 0),
        "only the first call roots `l`; the tail call roots nothing"
    );
    assert_eq!(tail_calls_unrooted(), before + 1);

    let leaf =
        compile_bytecode_function_with(&two_calls(false), Some(&ev.obarray)).expect("compiles");
    assert_eq!(leaf.tier, super::leaf::LeafTier::Baseline);
    assert_eq!(
        rootwin_counters(),
        (1, 1),
        "a call followed by more code keeps `l` rooted (carried from the first site)"
    );
    assert_eq!(tail_calls_unrooted(), before + 1, "no second tail call");

    // `NEOVM_JIT_TAIL_UNROOTED=off`: the tail call roots `l` as before.
    force_tail_unrooted_for_test(Some(false));
    let leaf =
        compile_bytecode_function_with(&two_calls(true), Some(&ev.obarray)).expect("compiles");
    assert_eq!(leaf.tier, super::leaf::LeafTier::Baseline);
    assert_eq!(rootwin_counters(), (1, 1), "the knob restores the rooting");
    force_tail_unrooted_for_test(None);
}

/// listlen-tc's shape through the MIR tier: its self-call in tail position
/// roots nothing, and a collection at the bottom of a deep recursion finds
/// every live value rooted anyway (the list through the frames' arguments).
#[test]
fn a_compiled_tail_recursion_survives_a_collection_at_its_bottom() {
    crate::test_utils::init_test_tracing();
    force_tail_unrooted_for_test(Some(true));
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(
        "(progn
           (defvar neovm--tc-gc nil)
           (defun neovm--tc-len (l n)
             (if (null l)
                 (progn (when neovm--tc-gc (garbage-collect)) n)
               (neovm--tc-len (cdr l) (1+ n))))
           (byte-compile 'neovm--tc-len))",
    )
    .expect("defined");
    let before = tail_calls_unrooted();
    ev.eval_str("(let ((l (make-list 100 'x))) (dotimes (_ 60) (neovm--tc-len l 0)))")
        .expect("warm-up");
    assert!(
        tail_calls_unrooted() > before,
        "the compiled self-call is a tail call without roots"
    );
    let v = ev
        .eval_str(
            "(let ((neovm--tc-gc t)
                   (max-lisp-eval-depth 10000))
               (list (neovm--tc-len (mapcar #'number-to-string (number-sequence 1 3000)) 0)
                     (neovm--tc-len nil 7)))",
        )
        .expect("runs");
    assert_eq!(print_value(&v), "(3000 7)");
}
