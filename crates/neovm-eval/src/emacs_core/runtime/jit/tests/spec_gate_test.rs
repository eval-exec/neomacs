//! The speculated-call gates read the attention words: the bytecode spec
//! shim's fast path engages when nothing needs attention and steps aside
//! for a quit before it pushes a frame; the subr shims' single gate and the
//! inline half of `subr_spec_armed` agree with the reference protocol in
//! every state of the epoch, the compiler overrides and the force harness.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::eval::{AttentionMask, Context};
use neovm_value::error::FlowResultExt as _;
use neovm_value::intern::intern;
use neovm_value::value::LambdaParams;

/// A lexical bytecode function of NARGS required arguments, rooted for the
/// test's lifetime, hot (tiered up at its first call) when HOT.
fn lambda(nargs: usize, hot: bool, ops: Vec<Op>, constants: Vec<Value>) -> Value {
    lambda_stack(nargs, hot, 16, ops, constants)
}

/// [`lambda`] with an explicit declared `max_stack`. The bytecode-stack
/// charge is that depth, so two copies of one body can disagree on cost.
fn lambda_stack(
    nargs: usize,
    hot: bool,
    max_stack: usize,
    ops: Vec<Op>,
    constants: Vec<Value>,
) -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams {
        // SAFETY: test placeholder id, never resolved (peer seal N8).
        required: (1..=nargs)
            .map(|i| unsafe { SymId::from_raw(i as u32) })
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(max_stack);
    f.seal_hand_assembled_ops();
    if hot {
        f.jit_runtime().set_hot_for_test();
    }
    let v = Value::make_bytecode(f);
    crate::emacs_core::eval::push_scratch_gc_root(v);
    v
}

/// `(lambda (a1 .. aN) (if a1 aN aN))`: two blocks, so a caller speculates
/// the call instead of inlining the body.
fn callee(nargs: usize) -> Value {
    lambda(
        nargs,
        false,
        vec![
            Op::StackRef(nargs as u16 - 1),
            Op::GotoIfNil(4),
            Op::StackRef(0),
            Op::Return,
            Op::StackRef(0),
            Op::Return,
        ],
        vec![],
    )
}

fn shim_fast() -> u64 {
    SPEC_SHIM_FAST_COUNT.load(Ordering::Relaxed)
}

/// (a) With nothing needing attention, the shim's own fast path runs the
/// cached leaf for one-, two- and three-argument calls (the three frame
/// shapes the native push records).
#[test]
fn spec_fast_path_engages_for_one_two_and_three_arguments() {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    // An engagement test: pinned off under the force harness, whose whole
    // point is that no call takes the fast path.
    force_slow_spec_for_test(Some(false));
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    for nargs in 1..=3usize {
        let name = format!("neo-spec-gate-g{nargs}");
        let g = Value::symbol(&name);
        let target = callee(nargs);
        ev.obarray.set_symbol_function_id(intern(&name), target);
        // (lambda (x) (G x x ... x))
        let mut ops = vec![Op::Constant(0)];
        for i in 0..nargs {
            ops.push(Op::StackRef(1 + i as u16));
        }
        ops.push(Op::Call(nargs as u16));
        ops.push(Op::Return);
        let caller = lambda(1, true, ops, vec![g]);
        let arg = Value::make_int(7);
        crate::emacs_core::jit::test_support::prepare_entry(
            &mut ev,
            target.get_bytecode_data().expect("callee bytecode"),
        );
        crate::emacs_core::jit::test_support::prepare_entry(
            &mut ev,
            caller.get_bytecode_data().expect("caller bytecode"),
        );
        // Code is ready; the first original call arms the site, and the rest
        // take the armed fast path.
        assert_eq!(ev.funcall_general_untraced(caller, vec![arg]).unwrap(), arg);
        let before = shim_fast();
        for _ in 0..3 {
            assert_eq!(
                ev.funcall_general_untraced(caller, vec![arg]).unwrap(),
                arg,
                "{nargs} arguments"
            );
        }
        assert!(
            shim_fast() >= before + 3,
            "{nargs} arguments: the armed fast path runs every call"
        );
        assert!(ev.maybe_quit_hot_ok());
    }
}

/// The real two-argument bytecode spec shim sees the same ordered Value words
/// in the unconditional packed producer. Distinct equal-content heap objects prove
/// identity preservation, rather than merely equal Lisp results.
#[test]
fn two_argument_spec_calls_preserve_order_and_heap_identity() {
    // These setters have no raw-override getter. Restore the prior effective
    // scalar settings, including if this fixture is entered under an override.
    struct CompilerPolicy {
        direct: bool,
        inline: bool,
        profit: bool,
        slow_spec: bool,
        tier2: Tier2Knob,
    }
    impl CompilerPolicy {
        #[doc(hidden)]
        pub fn enter() -> Self {
            let prior = Self {
                direct: jit_direct_call_on(),
                inline: crate::emacs_core::jit::inline::jit_inline_on(),
                profit: jit_profit_gate_on(),
                slow_spec: jit_force_slow_spec(),
                tier2: jit_tier2(),
            };
            force_direct_call_for_test(Some(false));
            crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
            force_profit_gate_for_test(false);
            force_slow_spec_for_test(Some(false));
            force_tier2_for_test(Some(Tier2Knob {
                on: false,
                ..prior.tier2
            }));
            prior
        }
    }
    impl Drop for CompilerPolicy {
        fn drop(&mut self) {
            force_direct_call_for_test(Some(self.direct));
            crate::emacs_core::jit::inline::force_inline_for_test(Some(self.inline));
            force_profit_gate_for_test(self.profit);
            force_slow_spec_for_test(Some(self.slow_spec));
            force_tier2_for_test(Some(self.tier2));
        }
    }
    struct FixtureRoots(usize);
    impl Drop for FixtureRoots {
        fn drop(&mut self) {
            crate::emacs_core::eval::restore_scratch_gc_roots(self.0);
        }
    }
    fn assert_arguments(pair: Value, left: Value, right: Value) {
        assert!(pair.is_cons(), "callee returns both arguments as a cons");
        assert_eq!(pair.cons_car().bits(), left.bits(), "first argument word");
        assert_eq!(pair.cons_cdr().bits(), right.bits(), "second argument word");
    }

    crate::test_utils::init_test_tracing();
    let _backend =
        crate::emacs_core::jit::bg::TestModeScope::enter(crate::emacs_core::jit::bg::BgMode::Sync);
    let _compiler = CompilerPolicy::enter();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let _roots = FixtureRoots(crate::emacs_core::eval::save_scratch_gc_roots());
    let left_heap = Value::make_string("same contents, distinct objects");
    crate::emacs_core::eval::push_scratch_gc_root(left_heap);
    let right_heap = Value::make_string("same contents, distinct objects");
    crate::emacs_core::eval::push_scratch_gc_root(right_heap);
    assert_ne!(left_heap.bits(), right_heap.bits());
    let arguments = [
        (Value::make_int(0x12345), Value::make_int(-0x23456)),
        (Value::NIL, Value::t()),
        (left_heap, right_heap),
    ];
    let name = "neo-spec-gate-packed-two-identity";
    let g = Value::symbol(name);
    // Two blocks prevent the trivial-body path. Both branches return (a . b),
    // with each StackRef selecting a different original argument word.
    let target = lambda(
        2,
        false,
        vec![
            Op::StackRef(1),
            Op::GotoIfNil(6),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Cons,
            Op::Return,
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Cons,
            Op::Return,
        ],
        vec![],
    );
    ev.obarray.set_symbol_function_id(intern(name), target);
    crate::emacs_core::jit::test_support::prepare_entry(
        &mut ev,
        target.get_bytecode_data().expect("callee bytecode"),
    );
    assert!(
        target
            .get_bytecode_data()
            .unwrap()
            .jit_runtime()
            .compiled_id()
            .is_some()
    );

    // A fresh site targets the prepared named callee. Direct and inline
    // compilation remain disabled, so it uses the real bytecode spec shim.
    let caller = lambda(
        2,
        true,
        vec![
            Op::Constant(0),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![g],
    );
    crate::emacs_core::jit::test_support::prepare_entry(
        &mut ev,
        caller.get_bytecode_data().expect("caller bytecode"),
    );
    assert!(
        caller
            .get_bytecode_data()
            .unwrap()
            .jit_runtime()
            .compiled_id()
            .is_some()
    );
    // The first call arms this fresh site. Every subsequent pair exercises
    // the warmed CallSpec path; the callee itself contains no nested calls.
    let (left, right) = arguments[0];
    assert_arguments(
        ev.funcall_general_untraced(caller, vec![left, right])
            .unwrap(),
        left,
        right,
    );
    let before = shim_fast();
    for (left, right) in arguments {
        for _ in 0..3 {
            assert_arguments(
                ev.funcall_general_untraced(caller, vec![left, right])
                    .unwrap(),
                left,
                right,
            );
        }
    }
    // Run this focused fixture alone with --test-threads=1: the engagement
    // counter is process-global. The prepared callee contains no nested calls.
    assert_eq!(
        shim_fast(),
        before + 9,
        "all nine warmed CallSpec admissions"
    );
    assert!(ev.maybe_quit_hot_ok());
}

/// (b) A quit raised by compiled code is noticed at the next speculated call,
/// before the callee's frame is pushed or the callee runs: the call signals
/// `quit` from the caller's frame, and the fast path is not taken.
#[test]
fn a_quit_set_by_compiled_code_stops_the_next_spec_call_before_its_frame() {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.eval_str("(setq neo-spec-gate-ran 0)").unwrap();
    // (lambda (x) (setq neo-spec-gate-ran (1+ neo-spec-gate-ran)) (if x x x))
    let g_body = lambda(
        1,
        false,
        vec![
            Op::VarRef(0),
            Op::Add1,
            Op::VarSet(0),
            Op::StackRef(0),
            Op::GotoIfNil(6),
            Op::StackRef(0),
            Op::Return,
            Op::StackRef(0),
            Op::Return,
        ],
        vec![Value::symbol("neo-spec-gate-ran")],
    );
    ev.obarray
        .set_symbol_function_id(intern("neo-spec-gate-q"), g_body);
    // (lambda (q x) (setq quit-flag q) (neo-spec-gate-q x))
    let caller = lambda(
        2,
        true,
        vec![
            Op::StackRef(1),
            Op::VarSet(0),
            Op::Constant(1),
            Op::StackRef(1),
            Op::Call(1),
            Op::Return,
        ],
        vec![Value::symbol("quit-flag"), Value::symbol("neo-spec-gate-q")],
    );
    let x = Value::make_int(3);
    for _ in 0..3 {
        assert_eq!(
            ev.funcall_general_untraced(caller, vec![Value::NIL, x])
                .unwrap(),
            x
        );
    }
    assert_eq!(
        ev.eval_str("neo-spec-gate-ran").unwrap(),
        Value::make_int(3)
    );
    let fast = shim_fast();
    let specpdl = ev.specpdl.len();
    let depth = ev.depth;
    let result = ev.funcall_general_untraced(caller, vec![Value::T, x]);
    match result.kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "quit")
        }
        other => panic!("expected a quit signal, got {other:?}"),
    }
    assert_eq!(
        shim_fast(),
        fast,
        "the quit kept the call off the fast path"
    );
    assert_eq!(
        ev.eval_str("neo-spec-gate-ran").unwrap(),
        Value::make_int(3),
        "the callee never ran"
    );
    assert_eq!(ev.specpdl.len(), specpdl);
    assert_eq!(ev.depth, depth);
    assert!(ev.quit_flag_value().is_nil(), "the quit was processed");
    assert!(ev.maybe_quit_hot_ok());
}

/// The force harness is a bit of the attention word, derived from the knob:
/// on, it leaves the subr arming mask and the spec-call mask unclear; off,
/// both are clear again.
#[test]
fn force_slow_spec_bit_matches_knob() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let knob = jit_force_slow_spec();
    assert_eq!(
        !ev.attention_word_clear(AttentionMask::SUBR_ARMING),
        knob,
        "the word follows the process knob"
    );
    for on in [true, false] {
        force_slow_spec_for_test(Some(on));
        ev.refresh_attention_for_test();
        let (stored, derived) = ev.attention_words_for_test();
        assert_eq!(stored, derived);
        assert_eq!(!ev.attention_word_clear(AttentionMask::SUBR_ARMING), on);
        assert_eq!(!ev.attention_clear(AttentionMask::SPEC_CALL), on);
        assert!(
            ev.attention_clear(AttentionMask::QUIT),
            "the harness never sends the quit poll to its slow path"
        );
    }
    force_slow_spec_for_test(None);
    ev.refresh_attention_for_test();
}

/// `subr_spec_armed` before its split, verbatim: the reference its inline
/// half must agree with.
fn subr_spec_armed_before_the_split(
    ctx: &Context,
    sym: i64,
    expected: i64,
    slot: &SpecSlot,
) -> bool {
    let slot_epoch = slot.epoch.load(Ordering::Relaxed);
    if slot_epoch == SPEC_EPOCH_DISARMED {
        return false;
    }
    if ctx.compiler_function_overrides_active() {
        return false;
    }
    let epoch = ctx.obarray.function_epoch();
    (!jit_force_slow_spec() && slot_epoch == epoch) || {
        // SAFETY: test placeholder id, never resolved (peer seal N8).
        let cur = ctx
            .obarray
            .symbol_function_id(unsafe { SymId::from_raw(sym as u32) });
        if cur.is_some_and(|v| v.bits() as i64 == expected) {
            slot.epoch.store(epoch, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum SlotState {
    Disarmed,
    Fresh,
    StaleSameBinding,
    StaleChangedBinding,
}

/// The split `subr_spec_armed` answers exactly as the reference in every
/// combination of slot state, compiler overrides and force harness, and
/// leaves the slot's epoch exactly where the reference leaves it.
#[test]
fn subr_spec_armed_hot_split_matches_reference() {
    crate::test_utils::init_test_tracing();
    for state in [
        SlotState::Disarmed,
        SlotState::Fresh,
        SlotState::StaleSameBinding,
        SlotState::StaleChangedBinding,
    ] {
        for overrides in [false, true] {
            for force in [false, true] {
                let mut ev = Context::new(crate::test_utils::unit_runtime());
                let sym = intern("neo-spec-gate-subr");
                let car = ev.obarray.symbol_function_id(intern("car")).expect("car");
                let cdr = ev.obarray.symbol_function_id(intern("cdr")).expect("cdr");
                ev.obarray.set_symbol_function_id(sym, car);
                if overrides {
                    ev.eval_str("(setq internal--compiler-function-overrides '((zzz . car)))")
                        .unwrap();
                }
                force_slow_spec_for_test(Some(force));
                ev.refresh_attention_for_test();
                let armed_at = ev.obarray.function_epoch();
                match state {
                    SlotState::StaleSameBinding => {
                        ev.obarray
                            .set_symbol_function_id(intern("neo-spec-gate-other"), cdr);
                    }
                    SlotState::StaleChangedBinding => {
                        ev.obarray.set_symbol_function_id(sym, cdr);
                    }
                    SlotState::Disarmed | SlotState::Fresh => {}
                }
                let slot_epoch = match state {
                    SlotState::Disarmed => SPEC_EPOCH_DISARMED,
                    _ => armed_at,
                };
                let split = SpecSlot::at_epoch(slot_epoch);
                let reference = SpecSlot::at_epoch(slot_epoch);
                let expected = car.bits() as i64;
                let got = subr_spec_armed(&mut ev, sym.0 as i64, expected, &split);
                let want =
                    subr_spec_armed_before_the_split(&ev, sym.0 as i64, expected, &reference);
                let case = format!("{state:?} overrides={overrides} force={force}");
                assert_eq!(got, want, "{case}");
                assert_eq!(
                    split.epoch.load(Ordering::Relaxed),
                    reference.epoch.load(Ordering::Relaxed),
                    "{case}: the slot is re-armed exactly as the reference re-arms it"
                );
                force_slow_spec_for_test(None);
                ev.refresh_attention_for_test();
            }
        }
    }
}

/// (d) A subr spec site under active compiler overrides takes the generic
/// call, whose resolution the overrides shadow; with them gone the single
/// gate's fast path is back.
#[cfg(debug_assertions)]
#[test]
fn a_subr_spec_site_under_compiler_overrides_takes_the_generic_call() {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let ctx = &mut ev as *mut Context as *mut u8;
    // (lambda (x) (symbol-name x))
    let mut f = ByteCodeFunction::new(LambdaParams {
        // SAFETY: test placeholder id, never resolved (peer seal N8).
        required: vec![unsafe { SymId::from_raw(1) }],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
    f.constants = vec![Value::symbol("symbol-name")].into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    f.seal_hand_assembled_ops();
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    let arg = Value::symbol("zzz");
    let run = || match leaf.call(ctx, &[arg]) {
        NativeRun::Ok(bits) => crate::emacs_core::print::print_value(&Value::from_bits(bits)),
        other => format!("{other:?}"),
    };
    let counts = || {
        (
            SUBR_SPEC_FAST_COUNT.load(Ordering::Relaxed),
            SUBR_SPEC_GENERIC_COUNT.load(Ordering::Relaxed),
        )
    };
    let (fast0, generic0) = counts();
    assert_eq!(run(), "\"zzz\"");
    let (fast1, generic1) = counts();
    assert_eq!((fast1 - fast0, generic1 - generic0), (1, 0), "armed: fast");

    ev.eval_str("(setq internal--compiler-function-overrides '((zzz . car)))")
        .unwrap();
    assert_eq!(run(), "\"zzz\"", "the generic call answers the same");
    let (fast2, generic2) = counts();
    assert_eq!(
        (fast2 - fast1, generic2 - generic1),
        (0, 1),
        "overrides active: generic"
    );

    ev.eval_str("(setq internal--compiler-function-overrides nil)")
        .unwrap();
    assert_eq!(run(), "\"zzz\"");
    let (fast3, generic3) = counts();
    assert_eq!(
        (fast3 - fast2, generic3 - generic2),
        (1, 0),
        "overrides gone: the site re-arms and is fast again"
    );
}

/// (c) `debug-on-next-call` set by compiled code enters the debugger at the
/// next speculated call, with the callee's frame flagged for the exit
/// debugger, and the site's fast path is not taken for it.
///
/// GNU 31.1, `emacs -Q --batch`, both functions byte-compiled:
/// `(list (neo-c nil 3) (neo-c t 3) (reverse log) debug-on-next-call)`
/// => `(3 nil ((lambda) (exit 3)) nil)`.
#[test]
fn debug_on_next_call_set_by_compiled_code_enters_the_debugger_at_the_next_spec_call() {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    force_slow_spec_for_test(Some(false));
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.eval_str(
        "(progn (defvar neo-debug-log nil)
                (setq debugger (lambda (&rest args) (setq neo-debug-log (cons args neo-debug-log)) nil)))",
    )
    .unwrap();
    ev.obarray
        .set_symbol_function_id(intern("neo-spec-gate-dbg"), callee(1));
    // (lambda (d x) (setq debug-on-next-call d) (neo-spec-gate-dbg x))
    let caller = lambda(
        2,
        true,
        vec![
            Op::StackRef(1),
            Op::VarSet(0),
            Op::Constant(1),
            Op::StackRef(1),
            Op::Call(1),
            Op::Return,
        ],
        vec![
            Value::symbol("debug-on-next-call"),
            Value::symbol("neo-spec-gate-dbg"),
        ],
    );
    let x = Value::make_int(3);
    for _ in 0..3 {
        assert_eq!(
            ev.funcall_general_untraced(caller, vec![Value::NIL, x])
                .unwrap(),
            x
        );
    }
    let fast = shim_fast();
    let debugged = ev
        .funcall_general_untraced(caller, vec![Value::T, x])
        .unwrap();
    assert_eq!(
        debugged,
        Value::NIL,
        "the exit debugger's value replaces the call's"
    );
    assert_eq!(shim_fast(), fast, "the armed call left the fast path");
    assert_eq!(
        crate::emacs_core::print::print_value(
            &ev.eval_str("(list (reverse neo-debug-log) debug-on-next-call)")
                .unwrap()
        ),
        "(((lambda) (exit 3)) nil)"
    );
    assert!(!ev.debug_on_next_call_is_armed());
}

/// Direct calls and inlining bypass the spec shim. These rows are about
/// that shim, so both stay off for the test and the previous knobs return
/// when the guard drops.
struct SpecShimPolicy {
    direct: bool,
    inline_on: bool,
    profit: bool,
    slow: bool,
}

impl SpecShimPolicy {
    fn enter(slow: bool) -> Self {
        let prior = Self {
            direct: jit_direct_call_on(),
            inline_on: crate::emacs_core::jit::inline::jit_inline_on(),
            profit: jit_profit_gate_on(),
            slow: jit_force_slow_spec(),
        };
        force_direct_call_for_test(Some(false));
        crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
        force_profit_gate_for_test(false);
        force_slow_spec_for_test(Some(slow));
        prior
    }
}

impl Drop for SpecShimPolicy {
    fn drop(&mut self) {
        force_direct_call_for_test(Some(self.direct));
        crate::emacs_core::jit::inline::force_inline_for_test(Some(self.inline_on));
        force_profit_gate_for_test(self.profit);
        force_slow_spec_for_test(Some(self.slow));
    }
}

fn declared_frame_bytes(fun: Value) -> usize {
    crate::emacs_core::bytecode::vm::bytecode_frame_bytes(
        fun.get_bytecode_data().expect("bytecode").max_stack.get(),
    )
}

/// Park the bytecode-stack position so `caller`'s own frame still fits and
/// `callee`'s frame does not. One less than this value lets the callee fit:
/// GNU's compare is `position > limit` (src/bytecode.c:514-515).
fn position_rejecting_callee(caller: Value, callee: Value) -> usize {
    let limit = crate::emacs_core::bytecode::vm::BC_STACK_POSITION_LIMIT;
    let caller_bytes = declared_frame_bytes(caller);
    let callee_bytes = declared_frame_bytes(callee);
    let cost = caller_bytes + callee_bytes;
    assert!(callee_bytes >= 1 && cost <= limit);
    let parked = limit - cost + 1;
    let after_caller = crate::emacs_core::bytecode::vm::charge_frame_of_bytes(parked, caller_bytes)
        .expect("the caller frame fits at the parked position");
    assert!(
        crate::emacs_core::bytecode::vm::charge_frame_of_bytes(after_caller, callee_bytes)
            .is_none(),
        "the callee frame must not fit"
    );
    parked
}

fn signal_text(flow: &neovm_value::error::Flow) -> String {
    let Some(sig) = flow.as_signal() else {
        return format!("not-a-signal {flow:?}");
    };
    let data = sig
        .data
        .iter()
        .map(crate::emacs_core::print::print_value)
        .collect::<Vec<_>>()
        .join(" ");
    format!("({} {data})", sig.symbol_name())
}

fn id_body(nargs: usize) -> Vec<Op> {
    vec![
        Op::StackRef(nargs as u16 - 1),
        Op::GotoIfNil(4),
        Op::StackRef(0),
        Op::Return,
        Op::StackRef(0),
        Op::Return,
    ]
}

/// `(lambda (x) (NAME x))`, hot, so the call site arms the spec shim.
fn calling(name: &str, max_stack: usize) -> Value {
    lambda_stack(
        1,
        true,
        max_stack,
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![Value::symbol(name)],
    )
}

fn bind_prepared(ev: &mut Context, name: &str, target: Value, caller: Value) {
    ev.obarray
        .set_symbol_function_id(neovm_value::intern::intern(name), target);
    crate::emacs_core::jit::test_support::prepare_entry(
        ev,
        target.get_bytecode_data().expect("callee bytecode"),
    );
    crate::emacs_core::jit::test_support::prepare_entry(
        ev,
        caller.get_bytecode_data().expect("caller bytecode"),
    );
}

const OVERFLOW: &str = "(error \"Bytecode stack overflow\")";

/// LATENT. Expiry: the eval-batch landing. The `neovm-eval` lib-test harness
/// does not compile at `7a7d474cd8`, so this row does not execute on the
/// landing SHA; it runs on the `ea12cc96` scratch. The GNU-observable half is
/// `oracle_eieio_redefined_max_stack_refuses_at_the_new_size`.
///
/// (a) A callee redefined to a larger `max_stack` refuses the next call at
/// the new size. The parked position still accepts the old frame, so a stale
/// stamp would let the call return.
#[test]
fn redefined_max_stack_refuses_at_the_new_size() {
    crate::test_utils::init_test_tracing();
    let _policy = SpecShimPolicy::enter(false);
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let name = "neo-eieio-redef";
    let small = lambda_stack(1, false, 2, id_body(1), vec![]);
    let large = lambda_stack(1, false, 48, id_body(1), vec![]);
    let caller = calling(name, 4);
    bind_prepared(&mut ev, name, small, caller);
    crate::emacs_core::jit::test_support::prepare_entry(
        &mut ev,
        large.get_bytecode_data().expect("large bytecode"),
    );
    let arg = Value::make_int(7);
    let before = shim_fast();
    for _ in 0..4 {
        assert_eq!(ev.funcall_general_untraced(caller, vec![arg]).unwrap(), arg);
    }
    assert!(
        shim_fast() >= before + 3,
        "the small callee armed the spec shim"
    );
    let parked = position_rejecting_callee(caller, large);
    let after_caller = crate::emacs_core::bytecode::vm::charge_frame_of_bytes(
        parked,
        declared_frame_bytes(caller),
    )
    .unwrap();
    assert!(
        crate::emacs_core::bytecode::vm::charge_frame_of_bytes(
            after_caller,
            declared_frame_bytes(small),
        )
        .is_some(),
        "the old frame still fits where the new one does not"
    );
    ev.bytecode_stack_position = parked;
    assert_eq!(
        ev.funcall_general_untraced(caller, vec![arg]).unwrap(),
        arg,
        "the armed old stamp accepts this position"
    );
    ev.obarray
        .set_symbol_function_id(neovm_value::intern::intern(name), large);
    ev.bytecode_stack_position = parked;
    let refused = ev
        .funcall_general_untraced(caller, vec![arg])
        .expect_err("the new max_stack does not fit");
    assert_eq!(signal_text(&refused), OVERFLOW);
    assert_eq!(
        ev.bytecode_stack_position, parked,
        "the refused frame restored the caller's position"
    );
    ev.bytecode_stack_position = 0;
    assert_eq!(
        ev.funcall_general_untraced(caller, vec![arg]).unwrap(),
        arg,
        "the redefined callee runs when its new frame fits"
    );
}

/// LATENT. Expiry: the eval-batch landing. Needs `bytecode_stack_position`
/// and the fast-path census, which Lisp cannot see. The GNU-observable half
/// is `oracle_eieio_armed_byte_limit_refuses_with_gnu_overflow_data`.
///
/// (b) An armed shim whose stamped frame does not fit declines before the
/// fast-path census and the slow half raises GNU's bytecode-stack error
/// (src/bytecode.c:514-515). One byte under that position still takes the
/// fast path.
#[test]
fn armed_shim_byte_limit_refuses_with_gnu_overflow_data() {
    crate::test_utils::init_test_tracing();
    let _policy = SpecShimPolicy::enter(false);
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let name = "neo-eieio-limit";
    let target = callee(1);
    let caller = calling(name, 4);
    bind_prepared(&mut ev, name, target, caller);
    let arg = Value::make_int(7);
    let before = shim_fast();
    for _ in 0..4 {
        assert_eq!(ev.funcall_general_untraced(caller, vec![arg]).unwrap(), arg);
    }
    assert!(shim_fast() >= before + 3, "the shim is armed");
    let rejecting = position_rejecting_callee(caller, target);
    ev.bytecode_stack_position = rejecting - 1;
    let fitted = shim_fast();
    assert_eq!(
        ev.funcall_general_untraced(caller, vec![arg]).unwrap(),
        arg,
        "one byte under the limit still runs"
    );
    assert!(
        shim_fast() > fitted,
        "the fitting call took the armed fast path"
    );
    ev.bytecode_stack_position = rejecting;
    let armed = shim_fast();
    let refused = ev
        .funcall_general_untraced(caller, vec![arg])
        .expect_err("the stamped frame does not fit");
    assert_eq!(signal_text(&refused), OVERFLOW);
    assert_eq!(
        shim_fast(),
        armed,
        "a declined charge is not a fast-path run"
    );
    assert_eq!(ev.bytecode_stack_position, rejecting);
    ev.bytecode_stack_position = 0;
    assert_eq!(ev.funcall_general_untraced(caller, vec![arg]).unwrap(), arg);
}

/// LATENT. Expiry: the eval-batch landing. Compares the armed spec path with
/// `force_slow_spec`, which Lisp cannot select. The GNU-observable half is
/// `oracle_eieio_debug_on_error_backtrace_matches_gnu`.
///
/// (c) `debug-on-error` sees the same backtrace on the armed spec shim as on
/// the slow spec path. Distinct from `debug-on-next-call`, which leaves the
/// fast path before the call.
#[test]
fn debug_on_error_backtrace_matches_the_slow_spec_path() {
    crate::test_utils::init_test_tracing();
    let fast = debug_on_error_backtrace(false);
    let slow = debug_on_error_backtrace(true);
    assert!(
        fast.0.contains("neo-eieio-dbg"),
        "the callee frame is on the armed backtrace: {}",
        fast.0
    );
    assert_eq!(
        fast.0, slow.0,
        "debug-on-error backtrace changed on the shim"
    );
    assert!(fast.1 >= 1, "the erroring call took the armed fast path");
    assert_eq!(slow.1, 0, "the slow path never counts a shim fast run");
}

fn debug_on_error_backtrace(slow: bool) -> (String, u64) {
    let _policy = SpecShimPolicy::enter(slow);
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.eval_str(
        "(progn
           (defvar neo-eieio-arm nil)
           (defvar neo-eieio-log nil)
           (setq debugger
                 (lambda (&rest args)
                   (let (frames)
                     (mapbacktrace
                      (lambda (_evald func args _flags)
                        (unless (memq func '(mapbacktrace debugger))
                          (push (list (if (symbolp func) func (type-of func)) args)
                                frames))))
                     (setq neo-eieio-log (list args (nreverse frames))))
                   nil)))",
    )
    .unwrap();
    let name = "neo-eieio-dbg";
    // Two blocks, so the caller speculates instead of inlining. Nil returns
    // the argument; non-nil is `(signal 'error '("shim-boom"))`. `signal` is
    // the subr, so this does not depend on the Lisp `error` definition.
    let target = lambda(
        1,
        false,
        vec![
            Op::VarRef(0),
            Op::GotoIfNil(7),
            Op::Constant(1),
            Op::Constant(2),
            Op::Constant(3),
            Op::Call(2),
            Op::Return,
            Op::StackRef(0),
            Op::Return,
        ],
        vec![
            Value::symbol("neo-eieio-arm"),
            Value::symbol("signal"),
            Value::symbol("error"),
            Value::cons(Value::string("shim-boom"), Value::NIL),
        ],
    );
    let caller = calling(name, 16);
    bind_prepared(&mut ev, name, target, caller);
    let arg = Value::make_int(3);
    let warmed = shim_fast();
    for _ in 0..4 {
        assert_eq!(
            ev.funcall_general_untraced(caller, vec![arg]).unwrap(),
            arg,
            "warmup returns the argument while the flag is nil"
        );
    }
    if slow {
        assert_eq!(shim_fast(), warmed, "slow spec does not arm the fast path");
    } else {
        assert!(shim_fast() >= warmed + 3, "warmup armed the spec shim");
    }
    ev.eval_str("(setq neo-eieio-arm t debug-on-error t)")
        .unwrap();
    let before = shim_fast();
    let refused = ev
        .funcall_general_untraced(caller, vec![arg])
        .expect_err("the callee signals");
    assert_eq!(signal_text(&refused), "(error \"shim-boom\")");
    let delta = shim_fast() - before;
    let logged = ev.eval_str("neo-eieio-log").unwrap();
    (crate::emacs_core::print::print_value(&logged), delta)
}
