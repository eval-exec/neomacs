//! The native-stack guard of compiled leaves (design `p1-1-direct-native-calls`
//! §3.7, T11): a byte-compiled recursion under an unbounded
//! `max-lisp-eval-depth` signals GNU's `(error "Bytecode stack overflow")`
//! (src/bytecode.c:514-515) instead of overflowing the native stack -- on
//! the 128 MiB test thread (the batch main thread's size), on a 64 MiB
//! thread like the GUI evaluator's, and on a stacker segment -- and the
//! evaluator is whole afterwards.

use super::stack_guard::stack_guards_emitted_for_test;
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::native_stack::{
    JIT_STACK_RED_ZONE, grow_tracking_jit_limit, jit_stack_limit_here,
};
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

/// GNU's answer (31.1, byte-compiled, `max-lisp-eval-depth` at
/// `most-positive-fixnum`): the bytecode stack runs out first.
const OVERFLOW: &str = "(error \"Bytecode stack overflow\")";

/// Define, byte-compile and warm `neovm--sg-deep` past the tier-up
/// threshold, so its self-call runs leaf to leaf.
fn warm_deep(ev: &mut Context) {
    ev.eval_str("(defun neovm--sg-deep (n) (if (= n 0) 0 (1+ (neovm--sg-deep (1- n)))))")
        .expect("defun");
    ev.eval_str("(byte-compile 'neovm--sg-deep)")
        .expect("byte-compile");
    ev.eval_str("(dotimes (_ 40) (neovm--sg-deep 100))")
        .expect("warm-up");
}

/// Recurse far past any native stack under an unbounded depth limit and
/// return the printed outcome, checking that the recursion ran natively and
/// that the unwind restored the evaluator's depth and specpdl.
fn recurse_without_depth_limit(ev: &mut Context) -> String {
    let depth0 = ev.depth;
    let spec0 = ev.specpdl.len();
    let spec_calls = SPEC_CALL_COUNT.load(Ordering::Relaxed);
    let outcome = ev
        .eval_str(
            "(let ((max-lisp-eval-depth most-positive-fixnum))
               (condition-case err (neovm--sg-deep 100000000) (error err)))",
        )
        .expect("evaluates");
    assert!(
        SPEC_CALL_COUNT.load(Ordering::Relaxed) - spec_calls > 1000,
        "the recursion ran through the compiled self-call"
    );
    assert_eq!(ev.depth, depth0, "depth restored");
    assert_eq!(ev.specpdl.len(), spec0, "specpdl restored");
    print_value(&outcome)
}

/// The evaluator still runs compiled code normally after the overflow.
fn still_runs(ev: &mut Context) {
    let v = ev.eval_str("(neovm--sg-deep 500)").expect("runs");
    assert_eq!(print_value(&v), "500");
}

/// T11 on the test thread, whose 128 MiB stack (`RUST_MIN_STACK`) is the
/// batch main thread's (`increase_stack_limit`).
#[test]
fn deep_compiled_recursion_signals_bytecode_stack_overflow() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    assert_ne!(ev.jit_stack_limit, 0, "stacker knows this thread's stack");
    let remaining = stacker::remaining_stack().expect("bounds known");
    assert!(
        remaining > 64 * 1024 * 1024,
        "the test thread has the batch stack: {remaining} bytes left"
    );
    let guards = stack_guards_emitted_for_test();
    warm_deep(&mut ev);
    assert!(
        stack_guards_emitted_for_test() > guards,
        "the recursive leaf carries the entry guard"
    );
    assert_eq!(recurse_without_depth_limit(&mut ev), OVERFLOW);
    still_runs(&mut ev);
}

/// T11 on a 64 MiB thread, the GUI evaluator thread's size
/// (`GUI_EVALUATOR_THREAD_STACK_SIZE`).
#[test]
fn deep_compiled_recursion_signals_on_a_64_mib_thread_like_the_gui_evaluator() {
    crate::test_utils::init_test_tracing();
    let outcome = std::thread::Builder::new()
        .name("gui-evaluator-sized".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let mut ev = crate::test_utils::runtime_startup_context();
            assert_ne!(ev.jit_stack_limit, 0, "stacker knows this thread's stack");
            warm_deep(&mut ev);
            let outcome = recurse_without_depth_limit(&mut ev);
            still_runs(&mut ev);
            outcome
        })
        .expect("spawn")
        .join()
        .expect("no crash");
    assert_eq!(outcome, OVERFLOW);
}

/// T11 on a stacker segment: the probes that switch segments (here the
/// shared helper they all use) point the limit at the new segment, so the
/// compiled recursion is stopped there, not at the bottom of a stack it is
/// not running on; the caller's limit comes back afterwards, after a panic
/// too.
#[test]
fn the_guard_follows_a_stacker_segment_and_restores_the_callers_limit() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    warm_deep(&mut ev);
    let own = ev.jit_stack_limit;
    let (outcome, on_segment) = grow_tracking_jit_limit(
        &mut ev,
        Context::jit_stack_limit_mut,
        4 * 1024 * 1024,
        |ev| {
            let on_segment = ev.jit_stack_limit;
            (recurse_without_depth_limit(ev), on_segment)
        },
    );
    assert_eq!(outcome, OVERFLOW);
    assert_ne!(on_segment, own, "the segment has its own limit");
    assert_eq!(ev.jit_stack_limit, own, "the caller's limit is back");
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        grow_tracking_jit_limit(
            &mut ev,
            Context::jit_stack_limit_mut,
            4 * 1024 * 1024,
            |_| panic!("unwinds through the segment"),
        )
    }));
    assert!(panicked.is_err());
    assert_eq!(ev.jit_stack_limit, own, "restored after an unwind too");
    still_runs(&mut ev);
}

/// A limit that names another stack (here: above every address) sends each
/// guarded entry to the measuring slow path, which lets the leaf run: never
/// a false signal.
#[test]
fn a_stale_limit_measures_the_real_stack_and_runs_the_leaf() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    warm_deep(&mut ev);
    ev.jit_stack_limit = usize::MAX;
    let v = ev.eval_str("(neovm--sg-deep 1000)").expect("runs");
    assert_eq!(print_value(&v), "1000");
    ev.refresh_jit_stack_limit();
    assert!(ev.jit_stack_limit < usize::MAX);
}

/// The limit is the running segment's low end plus the red zone: the stack
/// left above it is what the thread has, less the red zone.
#[test]
fn the_limit_sits_one_red_zone_above_the_stack_bottom() {
    let limit = jit_stack_limit_here();
    let marker = 0u8;
    let here = std::hint::black_box(core::ptr::addr_of!(marker)) as usize;
    let remaining = stacker::remaining_stack().expect("bounds known");
    let expected = here - remaining + JIT_STACK_RED_ZONE;
    assert!(
        limit.abs_diff(expected) < 4096,
        "limit {limit:#x} vs {expected:#x}"
    );
}

/// Only a body that can re-enter Lisp carries the guard: a call-free body
/// cannot recurse (and may run with a null vmctx).
#[test]
fn only_bodies_that_can_reenter_lisp_are_guarded() {
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let function = |ops: Vec<Op>, constants: Vec<Value>| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![crate::emacs_core::intern::SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = constants.into();
        f.max_stack = 8;
        f
    };
    // (lambda (x) (1+ x))
    let pure = function(vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]);
    // (lambda (x) (neovm--sg-callee x))
    let calls = function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![Value::symbol("neovm--sg-callee")],
    );
    // (lambda (x) (setq neovm--sg-var x)): a watcher can run Lisp.
    let sets = function(
        vec![Op::StackRef(0), Op::Dup, Op::VarSet(0), Op::Return],
        vec![Value::symbol("neovm--sg-var")],
    );
    for (what, f, guarded) in [
        ("pure", &pure, false),
        ("call", &calls, true),
        ("setq", &sets, true),
    ] {
        let before = stack_guards_emitted_for_test();
        let _leaf = compile_bytecode_function(f).expect("compiles");
        assert_eq!(
            stack_guards_emitted_for_test() > before,
            guarded,
            "{what}: guarded exactly when the body can re-enter Lisp"
        );
    }
    assert!(!super::stack_guard::body_may_reenter_lisp(&pure.ops));
}
