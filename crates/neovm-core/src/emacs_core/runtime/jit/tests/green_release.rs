//! Release-point revalidation (green-guard lever 1, design rev 2): an
//! inlined definition can change only while this thread has released the
//! Baton, so leaf validity is re-established after EVERY release point —
//! the return of a MayReenter call, a handler landing pad, and the
//! back-edge poll's serviced path — never per inline site. Every test
//! here redefines `logand` (or `+`) from inside the release and expects
//! the NEW definition at the pickup, GNU-exact through the symbol, with
//! resume-AFTER semantics (a completed call is never re-executed).

use crate::emacs_core::bytecode::{ByteCodeFunction, Op, StackDepth};
use crate::emacs_core::error::{EvalResult, signal};
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::jit::{bg, cache, compile};
use crate::emacs_core::subr::SubrSpec;
use crate::emacs_core::value::{LambdaParams, Value};

fn function(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut source = ByteCodeFunction::new(LambdaParams::simple(Vec::new()));
    source.lexical = true;
    source.ops = ops;
    source.constants = constants.into();
    source.max_stack = StackDepth::for_test(16);
    source.seal_hand_assembled_ops();
    source
        .jit_runtime()
        .set_heat_for_test(crate::emacs_core::jit::hot_threshold().saturating_mul(64));
    source
}

/// Bind `logand` to a constant-37 lambda and collect: the epoch moves.
fn redefine_logand(context: &mut Context) -> EvalResult {
    let mut replacement = ByteCodeFunction::new(LambdaParams::simple(vec![
        crate::emacs_core::intern::intern("green-release-a"),
        crate::emacs_core::intern::intern("green-release-b"),
    ]));
    replacement.lexical = true;
    replacement.ops = vec![Op::Constant(0), Op::Return];
    replacement.constants = vec![Value::fixnum(37)].into();
    replacement.max_stack = StackDepth::for_test(16);
    replacement.seal_hand_assembled_ops();
    let replacement = Value::make_bytecode(replacement);
    context.with_obarray_mut(|mut obarray| {
        obarray.set_symbol_function("logand", replacement);
    });
    context.apply(Value::symbol("garbage-collect"), Vec::new())?;
    Ok(Value::symbol("peer-finished"))
}

/// Park inside a call, then signal: the landing pad is the frame's first
/// resume point after the redefinition (the call itself never returns).
fn park_redefine_and_signal(context: &mut Context) -> EvalResult {
    let peer = context.apply(
        Value::symbol("make-thread"),
        vec![Value::symbol("green-release-peer")],
    )?;
    context.apply(Value::symbol("thread-join"), vec![peer])?;
    Err(signal(
        "green-release-boom",
        vec![Value::string("parked, redefined, signaling")],
    ))
}

/// The canonical release-witness body: one MayReenter call whose callee
/// parks (or redefines), then the inline-depended `logand` call.
///
///     (lambda () (green-release-helper) (logand 7 3))
fn witness_caller() -> ByteCodeFunction {
    function(
        vec![
            Op::Constant(0), // 'green-release-helper
            Op::Call(0),     // <- the release point
            Op::Pop,
            Op::Constant(1), // 'logand
            Op::Constant(2), // 7
            Op::Constant(3), // 3
            Op::Call(2),     // <- must deliver the NEW definition post-deopt
            Op::Return,
        ],
        vec![
            Value::symbol("green-release-helper"),
            Value::symbol("logand"),
            Value::fixnum(7),
            Value::fixnum(3),
        ],
    )
}

struct ArithScope {
    profit: compile::ProfitGateTestPolicy,
    jit_off: bool,
    arithmetic: bool,
}

impl Drop for ArithScope {
    fn drop(&mut self) {
        compile::replace_profit_gate_for_test(self.profit);
        crate::emacs_core::jit::force_jit_off_for_test(self.jit_off);
        compile::force_inline_arith_for_test(self.arithmetic);
    }
}

fn arith_scope() -> ArithScope {
    let scope = ArithScope {
        // The witness bodies are one real call plus an intrinsified `logand` —
        // `calls > arith` under the default gate, so they would never tier.
        // green_arith's `exercise` bypasses the gate for the same body shape.
        profit: compile::replace_profit_gate_for_test(compile::ProfitGateTestPolicy::Bypass),
        jit_off: crate::emacs_core::jit::jit_forced_off_for_test(),
        arithmetic: compile::jit_inline_arith_on(),
    };
    crate::emacs_core::jit::force_jit_off_for_test(false);
    compile::force_inline_arith_for_test(true);
    scope
}

/// Compile the witness caller, run it through the seam, and return
/// `(result, precise_deopts_during_the_apply)`.
fn run_witness(context: &mut Context) -> (Value, u64) {
    let source = Value::make_bytecode(witness_caller());
    context.with_obarray_mut(|mut obarray| {
        obarray.set_symbol_function("green-release-caller", source)
    });
    let _root = context.retain_gc_roots([source]);
    let source_data = source.get_bytecode_data().unwrap();
    cache::prepare_entry_for_test(context, source_data);
    let before = cache::native_run_counters().2;
    let result = context
        .apply(Value::symbol("green-release-caller"), Vec::new())
        .expect("the witness caller completes");
    let deopts = cache::native_run_counters().2 - before;
    (result, deopts)
}

/// Shared green-mode chassis: `setup` installs the helper/peer functions,
/// the witness runs once natively with the redefinition landing inside the
/// helper's park. The release-point deopt must deliver the NEW definition
/// exactly once.
fn exercise_green_park(setup: &str) {
    let _scope = arith_scope();
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Off);
    let _background = bg::TestModeScope::enter(bg::BgMode::Sync);
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    context.set_lexical_binding(true);
    context.register_subr(SubrSpec::fixed0("green-release-redefine", redefine_logand));
    context
        .eval_str("(setq green-release-done nil)")
        .expect("flag");
    context.eval_str(setup).expect("setup");
    let (result, deopts) = run_witness(&mut context);
    assert_eq!(
        result,
        Value::fixnum(37),
        "the release-point deopt must deliver the new definition"
    );
    assert_eq!(
        deopts, 1,
        "the helper call's return is the one release point that fires"
    );
    context.green_teardown();
    assert_eq!(context.jit_root_stack_top, 0);
    assert_eq!(cache::native_depth(), 0);
}

/// Transitive park: the compiled frame calls a helper, the helper yields
/// the Baton in a loop, and only the peer's redefinition ends the park.
/// The check is at the OUTER call's return — one release point covering
/// every release transitively reachable through the callee.
#[test]
fn release_deopt_after_thread_yield_park() {
    exercise_green_park(
        "(fset 'green-release-peer (lambda ()
              (green-release-redefine)
              (setq green-release-done t)))
         (fset 'green-release-helper (lambda ()
              (make-thread 'green-release-peer)
              (while (null green-release-done)
                (thread-yield))))",
    );
}

/// `sleep-for` parks the green thread; the peer redefines mid-sleep.
#[test]
fn release_deopt_after_sleep_for_park() {
    exercise_green_park(
        "(fset 'green-release-peer (lambda ()
              (sleep-for 0 5)
              (green-release-redefine)))
         (fset 'green-release-helper (lambda ()
              (make-thread 'green-release-peer)
              (sleep-for 0 30)))",
    );
}

/// Contended `mutex-lock` parks (GNU `lisp_mutex_lock` releases on
/// contention): the peer holds the mutex across the redefinition.
#[test]
fn release_deopt_after_contended_mutex_lock_park() {
    exercise_green_park(
        "(setq green-release-mutex (make-mutex))
         (fset 'green-release-peer (lambda ()
              (mutex-lock green-release-mutex)
              (sleep-for 0 20)
              (green-release-redefine)
              (mutex-unlock green-release-mutex)))
         (fset 'green-release-helper (lambda ()
              (make-thread 'green-release-peer)
              (sleep-for 0 3)
              (mutex-lock green-release-mutex)
              (mutex-unlock green-release-mutex)))",
    );
}

/// `accept-process-output` waiting on a timeout parks the thread.
#[test]
fn release_deopt_after_accept_process_output_park() {
    exercise_green_park(
        "(fset 'green-release-peer (lambda ()
              (sleep-for 0 5)
              (green-release-redefine)))
         (fset 'green-release-helper (lambda ()
              (make-thread 'green-release-peer)
              (accept-process-output nil 0 30)))",
    );
}

/// The check does not fire spuriously: an UNCHANGED epoch passes every
/// release point natively (the intrinsic result stands, no deopt). The helper
/// only parks — no `fset` anywhere in the run, so the global epoch the release
/// check compares cannot move at all.
#[test]
fn release_check_passes_when_epoch_is_unchanged() {
    let _scope = arith_scope();
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Off);
    let _background = bg::TestModeScope::enter(bg::BgMode::Sync);
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    context.set_lexical_binding(true);
    context
        .eval_str(
            "(fset 'green-release-helper (lambda ()
              (sleep-for 0 5)))",
        )
        .expect("setup");
    let (result, deopts) = run_witness(&mut context);
    assert_eq!(
        result,
        Value::fixnum(3),
        "an unchanged epoch keeps the baked intrinsic result"
    );
    assert_eq!(deopts, 0, "no revalidation may fire without an epoch move");
    context.green_teardown();
    assert_eq!(context.jit_root_stack_top, 0);
}

/// Same-thread redefinition from inside the called helper — the sim-mode
/// rule is the same rule (one check, no mode branch): the helper's `fset`
/// retires the epoch, its return is the release point, and the interpreter
/// pickup resolves `logand` through the symbol.
#[test]
fn sim_self_fset_delivered_at_release_point() {
    let _scope = arith_scope();
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Off);
    let _background = bg::TestModeScope::enter(bg::BgMode::Sync);
    let mut context = Context::new_with_threading(ThreadingMode::Sim);
    context.set_lexical_binding(true);
    context
        .eval_str(
            "(fset 'green-release-helper (lambda ()
              (fset 'logand (lambda (&rest _args) 37))
              (garbage-collect)))",
        )
        .expect("helper");
    let (result, deopts) = run_witness(&mut context);
    assert_eq!(
        result,
        Value::fixnum(37),
        "the self-fset must be delivered by the release-point deopt"
    );
    assert_eq!(deopts, 1, "the helper call's return deopts exactly once");
    assert_eq!(context.jit_root_stack_top, 0);
    assert_eq!(cache::native_depth(), 0);
}

/// Opcode arithmetic is NOT a function-cell dependent (GNU byte-compiles
/// `(+ 1 2)` to `Badd`; `fset '+` cannot change it). Two pins:
///
/// 1. Mid-run: the helper redefines `+` while the caller is inside the
///    helper call. The redefinition moves the GLOBAL function epoch, but
///    this caller leaf has NO inline dependency — the helper call bakes
///    nothing and `Op::Add` records nothing — so `release_epoch` is None
///    and no release check was ever emitted: the epoch move deopts nobody.
///    The result pin is the real control: the compiled `Badd` arm computes
///    3; `+`'s new cell is consulted by no one, and `+` enters no dep set.
/// 2. Call-free: a leaf that is only `1 + 2` likewise records no dependency
///    — the opcode arm alone runs and nothing deopts, however far the epoch
///    has moved before entry.
#[test]
fn plus_opcode_is_not_a_function_cell_dependent() {
    let _scope = arith_scope();
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Off);
    let _background = bg::TestModeScope::enter(bg::BgMode::Sync);
    let mut context = Context::new_with_threading(ThreadingMode::Sim);
    context.set_lexical_binding(true);
    context
        .eval_str(
            "(fset 'green-release-helper (lambda ()
              (fset '+ (lambda (&rest _args) 99))
              (garbage-collect)))",
        )
        .expect("helper");
    let source = Value::make_bytecode(function(
        vec![
            Op::Constant(0), // 'green-release-helper
            Op::Call(0),     // <- the release point (the leaf's only dep)
            Op::Pop,
            Op::Constant(1), // 1
            Op::Constant(2), // 2
            Op::Add,         // <- the opcode; fset '+ cannot touch it
            Op::Return,
        ],
        vec![
            Value::symbol("green-release-helper"),
            Value::fixnum(1),
            Value::fixnum(2),
        ],
    ));
    context.with_obarray_mut(|mut obarray| {
        obarray.set_symbol_function("green-release-plus-caller", source)
    });
    let _root = context.retain_gc_roots([source]);
    let source_data = source.get_bytecode_data().unwrap();
    cache::prepare_entry_for_test(&mut context, source_data);
    let before = cache::native_run_counters().2;
    let result = context
        .apply(Value::symbol("green-release-plus-caller"), Vec::new())
        .expect("the plus caller completes");
    assert_eq!(result, Value::fixnum(3), "the opcode's own arithmetic wins");
    assert_eq!(
        cache::native_run_counters().2 - before,
        0,
        "the helper's fset moved the GLOBAL epoch, but a leaf with no inline \
         deps emits no release check: there is nothing to deopt"
    );
    assert_eq!(
        cache::inline_dependent_count_for_test(crate::emacs_core::intern::intern("+")),
        0,
        "an opcode never enters any inline-dep set"
    );

    // 2. Call-free: no Op::Call anywhere in the body => no inline deps, no
    //    emission (release_epoch is None), and a pre-entry fset of `+`
    //    cannot matter to the compiled opcode arm.
    context
        .eval_str("(fset '+ (lambda (&rest _args) 99))")
        .expect("fset");
    let bare = Value::make_bytecode(function(
        vec![
            Op::Constant(0), // 1
            Op::Constant(1), // 2
            Op::Add,
            Op::Return,
        ],
        vec![Value::fixnum(1), Value::fixnum(2)],
    ));
    context.with_obarray_mut(|mut obarray| {
        obarray.set_symbol_function("green-release-plus-bare", bare)
    });
    let _bare_root = context.retain_gc_roots([bare]);
    let bare_data = bare.get_bytecode_data().unwrap();
    cache::prepare_entry_for_test(&mut context, bare_data);
    let before_bare = cache::native_run_counters().2;
    let result = context
        .apply(Value::symbol("green-release-plus-bare"), Vec::new())
        .expect("the bare plus caller completes");
    assert_eq!(result, Value::fixnum(3), "the compiled Badd arm computes 3");
    assert_eq!(
        cache::native_run_counters().2,
        before_bare,
        "a dependency-free leaf emits no release check: nothing deopts"
    );
    assert_eq!(
        cache::inline_dependent_count_for_test(crate::emacs_core::intern::intern("+")),
        0,
        "`+` is still nobody's inline dependency"
    );
    assert_eq!(context.jit_root_stack_top, 0);
}

/// A handler landing pad is a release point in its own right: the signal
/// fires after the redefinition (the park happened inside the signalling
/// call, which never returns), so the LANDING revalidation deopts at the
/// handler entry and the interpreter handler resolves `logand` through
/// the symbol.
///
///     (lambda ()
///       (condition-case nil
///           (green-release-park-signal)
///         (error (logand 7 3))))
#[test]
fn release_deopt_at_condition_case_landing() {
    let _scope = arith_scope();
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Off);
    let _background = bg::TestModeScope::enter(bg::BgMode::Sync);
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    context.set_lexical_binding(true);
    context.register_subr(SubrSpec::fixed0(
        "green-release-park-signal",
        park_redefine_and_signal,
    ));
    context.register_subr(SubrSpec::fixed0("green-release-redefine", redefine_logand));
    context
        .eval_str("(fset 'green-release-peer (lambda () (green-release-redefine)))")
        .expect("peer");
    let mut ops = vec![
        Op::PushConditionCase(0), // patched below
        Op::Constant(0),          // 'green-release-park-signal
        Op::Call(0),              // parks (peer redefines), then signals
        Op::Pop,
        Op::PopHandler,
        Op::Constant(1), // 'logand   (dead on the signal path)
        Op::Constant(2),
        Op::Constant(3),
        Op::Call(2),
        Op::Return,
    ];
    let handler = ops.len();
    ops[0] = Op::PushConditionCase(handler as u32);
    ops.extend([
        Op::Pop,         // the error value
        Op::Constant(1), // 'logand
        Op::Constant(2), // 7
        Op::Constant(3), // 3
        Op::Call(2),     // <- the NEW definition, post-landing-deopt
        Op::Return,
    ]);
    let f = function(
        ops,
        vec![
            Value::symbol("green-release-park-signal"),
            Value::symbol("logand"),
            Value::fixnum(7),
            Value::fixnum(3),
        ],
    );
    let source = Value::make_bytecode(f);
    context.with_obarray_mut(|mut obarray| {
        obarray.set_symbol_function("green-release-caller", source)
    });
    let _root = context.retain_gc_roots([source]);
    let source_data = source.get_bytecode_data().unwrap();
    cache::prepare_entry_for_test(&mut context, source_data);
    let before = cache::native_run_counters().2;
    let result = context
        .apply(Value::symbol("green-release-caller"), Vec::new())
        .expect("the landing witness completes");
    assert_eq!(
        result,
        Value::fixnum(37),
        "the landing pad revalidation must deliver the new definition"
    );
    assert_eq!(
        cache::native_run_counters().2 - before,
        1,
        "the landing is the one release point that fires"
    );
    context.green_teardown();
    assert_eq!(context.jit_root_stack_top, 0);
    assert_eq!(cache::native_depth(), 0);
}
