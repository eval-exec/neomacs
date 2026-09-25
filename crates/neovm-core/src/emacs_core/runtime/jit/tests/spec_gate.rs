//! The speculated-call gates read the attention words: the bytecode spec
//! shim's fast path engages when nothing needs attention and steps aside
//! for a quit before it pushes a frame; the subr shims' single gate and the
//! inline half of `subr_spec_armed` agree with the reference protocol in
//! every state of the epoch, the compiler overrides and the force harness.

use super::*;
use crate::emacs_core::eval::{AttentionMask, Context};
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::LambdaParams;

/// A lexical bytecode function of NARGS required arguments, rooted for the
/// test's lifetime, hot (tiered up at its first call) when HOT.
fn lambda(nargs: usize, hot: bool, ops: Vec<Op>, constants: Vec<Value>) -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=nargs).map(|i| SymId(i as u32)).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
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
    let mut ev = Context::new();
    for nargs in 1..=3usize {
        let name = format!("neo-spec-gate-g{nargs}");
        let g = Value::symbol(&name);
        ev.obarray
            .set_symbol_function_id(intern(&name), callee(nargs));
        // (lambda (x) (G x x ... x))
        let mut ops = vec![Op::Constant(0)];
        for i in 0..nargs {
            ops.push(Op::StackRef(1 + i as u16));
        }
        ops.push(Op::Call(nargs as u16));
        ops.push(Op::Return);
        let caller = lambda(1, true, ops, vec![g]);
        let arg = Value::make_int(7);
        // The first call compiles the caller and arms the site; the rest
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

/// (b) A quit raised by compiled code is noticed at the next speculated call,
/// before the callee's frame is pushed or the callee runs: the call signals
/// `quit` from the caller's frame, and the fast path is not taken.
#[test]
fn a_quit_set_by_compiled_code_stops_the_next_spec_call_before_its_frame() {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    let mut ev = Context::new();
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
    match result {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
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
    let mut ev = Context::new();
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
        let cur = ctx.obarray.symbol_function_id(SymId(sym as u32));
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
                let mut ev = Context::new();
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
                let got = subr_spec_armed(&ev, sym.0 as i64, expected, &split);
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
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    // (lambda (x) (symbol-name x))
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
    f.constants = vec![Value::symbol("symbol-name")].into();
    f.max_stack = 8;
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
