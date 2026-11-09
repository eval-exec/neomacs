//! The native-stack guard of compiled leaves (design `p1-1-direct-native-calls`
//! §3.7, T11): a byte-compiled recursion under an unbounded
//! `max-lisp-eval-depth` signals GNU's `(error "Bytecode stack overflow")`
//! (src/bytecode.c:514-515) instead of overflowing the native stack -- on
//! the 128 MiB test thread (the batch main thread's size), on a 64 MiB
//! thread like the GUI evaluator's, and on a stacker segment -- and the
//! evaluator is whole afterwards.

use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::compile::compile_bytecode_function;
use crate::emacs_core::print::print_value;
use neovm_eval::test_support::authority::{
    TestLeafAbi, body_may_reenter_lisp_for_test, cached_leaf_abi_for_test,
    force_direct_call_for_test, force_observe_knobs_for_test, force_profit_gate_for_test,
    force_register_abi_for_test, grow_tracking_jit_limit_for_test,
    jit_compiled_id_or_assign_for_test, jit_stack_limit_here_for_test, jit_stack_red_zone_for_test,
    leaf_report_entry_rows_for_test, prepare_named_for_test, set_jit_stack_limit_for_test,
    stack_guards_emitted_for_test,
};
use neovm_value::value::LambdaParams;
use neovm_value::value::Value;

/// GNU's answer (31.1, byte-compiled, `max-lisp-eval-depth` at
/// `most-positive-fixnum`): the bytecode stack runs out first.
const OVERFLOW: &str = "(error \"Bytecode stack overflow\")";

/// Define, byte-compile and warm `neovm--sg-deep` past the tier-up
/// threshold, so its self-call runs leaf to leaf.
fn warm_deep(ev: &mut Context) {
    force_observe_knobs_for_test(false, false, true);
    ev.eval_str("(defun neovm--sg-deep (n) (if (= n 0) 0 (1+ (neovm--sg-deep (1- n)))))")
        .expect("defun");
    ev.eval_str("(byte-compile 'neovm--sg-deep)")
        .expect("byte-compile");
    prepare_named_for_test(ev, &["neovm--sg-deep"]);
    ev.eval_str("(dotimes (_ 40) (neovm--sg-deep 100))")
        .expect("warm-up");
}

/// Count this recursive function's native entries through every call path,
/// including direct calls that bypass `neovm_jit_call_spec`. Include retired
/// leaves too, in case the warm-up or the observation replaces its tier.
fn deep_native_entries(ev: &Context) -> u64 {
    let sym = neovm_value::intern::intern("neovm--sg-deep");
    let f = ev.obarray().symbol_function_id(sym).expect("defined");
    let bc = f.get_bytecode_data().expect("byte-code");
    let id = jit_compiled_id_or_assign_for_test(&bc);
    let rows: Vec<_> = leaf_report_entry_rows_for_test()
        .into_iter()
        .filter(|(row_id, _, _)| *row_id == id)
        .collect();
    assert!(
        !rows.is_empty(),
        "the recursive function has a compiled leaf"
    );
    assert!(
        rows.iter().all(|(_, counted, _)| *counted),
        "the recursive leaf counts all native entries"
    );
    rows.iter().map(|(_, _, entries)| entries).sum()
}

/// Recurse far past any native stack under an unbounded depth limit and
/// return the printed outcome, checking that the recursion ran natively and
/// that the unwind restored the evaluator's depth and specpdl.
fn recurse_without_depth_limit(ev: &mut Context) -> String {
    let depth0 = ev.depth_for_test();
    let spec0 = ev.specpdl().len();
    let native_entries = deep_native_entries(ev);
    let outcome = ev
        .eval_str(
            "(let ((max-lisp-eval-depth most-positive-fixnum))
               (condition-case err (neovm--sg-deep 100000000) (error err)))",
        )
        .expect("evaluates");
    assert!(
        deep_native_entries(ev) - native_entries > 1000,
        "the recursion ran through the compiled self-call"
    );
    assert_eq!(ev.depth_for_test(), depth0, "depth restored");
    assert_eq!(ev.specpdl().len(), spec0, "specpdl restored");
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
    assert_ne!(
        ev.jit_stack_limit_for_test(),
        0,
        "stacker knows this thread's stack"
    );
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
            assert_ne!(
                ev.jit_stack_limit_for_test(),
                0,
                "stacker knows this thread's stack"
            );
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
    let own = ev.jit_stack_limit_for_test();
    let (outcome, on_segment) = grow_tracking_jit_limit_for_test(&mut ev, 4 * 1024 * 1024, |ev| {
        let on_segment = ev.jit_stack_limit_for_test();
        (recurse_without_depth_limit(ev), on_segment)
    });
    assert_eq!(outcome, OVERFLOW);
    assert_ne!(on_segment, own, "the segment has its own limit");
    assert_eq!(
        ev.jit_stack_limit_for_test(),
        own,
        "the caller's limit is back"
    );
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        grow_tracking_jit_limit_for_test(&mut ev, 4 * 1024 * 1024, |_| {
            panic!("unwinds through the segment")
        })
    }));
    assert!(panicked.is_err());
    assert_eq!(
        ev.jit_stack_limit_for_test(),
        own,
        "restored after an unwind too"
    );
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
    set_jit_stack_limit_for_test(&mut ev, usize::MAX);
    let v = ev.eval_str("(neovm--sg-deep 1000)").expect("runs");
    assert_eq!(print_value(&v), "1000");
    ev.refresh_jit_stack_limit();
    assert!(ev.jit_stack_limit_for_test() < usize::MAX);
}

/// The limit is the running segment's low end plus the red zone: the stack
/// left above it is what the thread has, less the red zone.
#[test]
fn the_limit_sits_one_red_zone_above_the_stack_bottom() {
    let limit = jit_stack_limit_here_for_test();
    let marker = 0u8;
    let here = std::hint::black_box(core::ptr::addr_of!(marker)) as usize;
    let remaining = stacker::remaining_stack().expect("bounds known");
    let expected = here - remaining + jit_stack_red_zone_for_test();
    assert!(
        limit.abs_diff(expected) < 4096,
        "limit {limit:#x} vs {expected:#x}"
    );
}

/// Only a body that can re-enter Lisp carries the guard: a call-free body
/// cannot recurse (and may run with a null vmctx).
#[test]
fn only_bodies_that_can_reenter_lisp_are_guarded() {
    // The arglist is a cons. Core's no-Context fixtures allocate only after
    // the shared test init arms the per-thread fallback heap.
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    let function = |ops: Vec<Op>, constants: Vec<Value>| {
        let mut f = ByteCodeFunction::new(LambdaParams {
            // SAFETY: test placeholder id, never resolved (peer seal N8).
            required: vec![unsafe { neovm_value::intern::SymId::from_raw(1) }],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = constants.into();
        f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
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
    assert!(!body_may_reenter_lisp_for_test(&pure.ops));
}

/// T11 under the register leaf ABI (`reg_abi`, S2.1a): a body with no `out`
/// word compares its stack pointer instead, parks `aux` and its argument
/// registers across the measuring call, and signals the same error; a
/// stale limit still measures and runs the leaf.
#[test]
fn the_register_abi_entry_guard_signals_and_measures_like_the_memory_one() {
    crate::test_utils::init_test_tracing();
    force_register_abi_for_test(Some(true));
    let mut ev = crate::test_utils::runtime_startup_context();
    let guards = stack_guards_emitted_for_test();
    warm_deep(&mut ev);
    assert!(stack_guards_emitted_for_test() > guards, "guarded");
    let sym = neovm_value::intern::intern("neovm--sg-deep");
    let f = ev.obarray().symbol_function_id(sym).expect("defined");
    let bc = f.get_bytecode_data().expect("byte-code");
    let id = jit_compiled_id_or_assign_for_test(&bc);
    let abi = cached_leaf_abi_for_test(id).expect("compiled");
    assert_eq!(abi, TestLeafAbi::Register { arity: 1 });
    assert_eq!(recurse_without_depth_limit(&mut ev), OVERFLOW);
    still_runs(&mut ev);
    set_jit_stack_limit_for_test(&mut ev, usize::MAX);
    let v = ev.eval_str("(neovm--sg-deep 1000)").expect("runs");
    assert_eq!(print_value(&v), "1000");
    ev.refresh_jit_stack_limit();
    force_register_abi_for_test(None);
}

/// The memory entry remains covered when an enclosing gate enables direct
/// calls, which otherwise imply the register ABI for this recursive leaf.
#[test]
fn the_memory_abi_entry_guard_signals_with_direct_calls_enabled_in_the_environment() {
    crate::test_utils::init_test_tracing();
    force_direct_call_for_test(Some(false));
    force_register_abi_for_test(Some(false));
    let mut ev = crate::test_utils::runtime_startup_context();
    warm_deep(&mut ev);
    let sym = neovm_value::intern::intern("neovm--sg-deep");
    let f = ev.obarray().symbol_function_id(sym).expect("defined");
    let bc = f.get_bytecode_data().expect("byte-code");
    let id = jit_compiled_id_or_assign_for_test(&bc);
    let abi = cached_leaf_abi_for_test(id).expect("compiled");
    assert_eq!(abi, TestLeafAbi::Memory);
    assert_eq!(recurse_without_depth_limit(&mut ev), OVERFLOW);
    still_runs(&mut ev);
    force_register_abi_for_test(None);
    force_direct_call_for_test(None);
}
