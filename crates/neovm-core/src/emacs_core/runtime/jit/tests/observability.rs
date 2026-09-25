//! Release-build JIT observability: the per-leaf deopt/signal counters
//! (`LeafObs`), counted once at each cold exit.

use super::*;
use crate::emacs_core::jit::cache;
use crate::emacs_core::value::LambdaParams;

fn function(ops: Vec<Op>, constants: Vec<Value>, arity: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity).map(|i| SymId(i as u32 + 1)).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    f.seal_hand_assembled_ops();
    f
}

fn install(ev: &mut Context, name: &str, f: ByteCodeFunction) -> Value {
    let sym = Value::symbol(name);
    ev.obarray
        .set_symbol_function_id(sym.as_symbol_id().unwrap(), Value::make_bytecode(f));
    sym
}

/// The cached leaf row for `id` in the given state.
fn row_for(id: u64, state: cache::LeafState) -> cache::LeafRow {
    cache::leaf_report_rows()
        .0
        .into_iter()
        .find(|r| r.id == id && r.state == state)
        .unwrap_or_else(|| panic!("no {state:?} leaf row for id {id}"))
}

/// A precise deopt is counted once per failing run, keyed by its resume pc.
#[test]
fn jit_obs_precise_deopt_counts_per_leaf_and_pc() {
    // (+ 5 nil): the Add at pc 2 fails its fixnum guard every run.
    let leaf = lower_nullary_leaf(
        &[Op::Constant(0), Op::Nil, Op::Add, Op::Return],
        &[Value::make_int(5)],
    )
    .expect("compiles");
    assert_eq!(leaf.obs.deopt_at.get(), 0);
    assert_eq!(leaf.call_for_test(&[]), None);
    assert_eq!(leaf.call_for_test(&[]), None);
    let snap = leaf.obs.snapshot();
    assert_eq!(snap.deopt_at, 2);
    assert_eq!(snap.deopt_rerun, 0, "the baseline is all-precise");
    assert_eq!(snap.deopt_pcs, vec![(2, 2)], "both at the Add");
    assert_eq!(snap.id, 0, "built outside the cache");
}

/// Through the tier-up seam with a real Context: a leaf compiled on fixnum
/// feedback and fed a float deopts once and is keyed by its compiled_id. (A
/// pure MIR leaf reruns from the start, so the seam answers `None` and the
/// caller interprets; a precise one resumes mid-function.)
#[test]
fn jit_obs_tier_up_seam_deopt_is_counted_on_the_cached_leaf() {
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    // (lambda (x) (+ x 1))
    let f = function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    );
    let f_val = Value::make_bytecode(f.clone());
    let run = |arg: Value| {
        crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[arg]).expect("no signal")
    };
    assert_eq!(
        run(Value::make_int(41)),
        Some(Value::make_int(42).bits()),
        "fixnum: native"
    );
    let id = f.jit_runtime().compiled_id().expect("compiled");
    let before = row_for(id, cache::LeafState::Live).obs;
    assert_eq!(before.id, id, "the cache stamps the leaf with its id");
    assert_eq!(before.deopt_at + before.deopt_rerun, 0);
    if let Some(out) = run(Value::make_float(1.5)) {
        assert_eq!(Value::from_bits(out).as_float(), Some(2.5), "resumed");
    }
    let after = row_for(id, cache::LeafState::Live).obs;
    assert_eq!(after.deopt_at + after.deopt_rerun, 1, "{after:?}");
}

/// The every-guard-fails harness deopts every call, and every one is counted.
#[test]
fn jit_obs_force_deopt_counts_every_call() {
    force_deopt_for_test(true);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    );
    let f_val = Value::make_bytecode(f.clone());
    for i in 0..5 {
        let got = crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(i)])
            .expect("no signal");
        assert!(
            got.is_none_or(|bits| bits == Value::make_int(i + 1).bits()),
            "a precise resume is exact; a rerun falls back to the caller"
        );
    }
    let id = f.jit_runtime().compiled_id().expect("compiled");
    let obs = row_for(id, cache::LeafState::Live).obs;
    assert_eq!(obs.deopt_at + obs.deopt_rerun, 5, "{obs:?}");
}

/// A pure MIR body reruns from the start on a failed guard: counted as a
/// rerun, not as a precise deopt.
#[test]
fn jit_obs_mir_rerun_deopt_counted() {
    let ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
    let m = mir::build_mir(&ops, &[], 2).expect("builds");
    let leaf = lower_mir_pure(&m).expect("lowers");
    assert_eq!(
        leaf.call_for_test(&[Value::string("x"), Value::make_int(2)]),
        None
    );
    assert_eq!(leaf.obs.deopt_rerun.get(), 1);
    assert_eq!(leaf.obs.deopt_at.get(), 0);
}

/// A native-to-native (speculated) call whose callee deopts is counted
/// exactly once on the CALLEE, whichever tier compiled it.
#[test]
fn jit_obs_direct_path_deopt_counted_once() {
    force_profit_gate_for_test(false);
    let mut ev = Context::new();
    // A multi-block callee, so the MIR inliner leaves the call in place:
    // (lambda (n) (if n (1- n) 0))
    let step = install(
        &mut ev,
        "jit-obs-step",
        function(
            vec![
                Op::StackRef(0),
                Op::GotoIfNil(5),
                Op::StackRef(0),
                Op::Sub1,
                Op::Return,
                Op::Constant(0),
                Op::Return,
            ],
            vec![Value::make_int(0)],
            1,
        ),
    );
    // (lambda (x) (jit-obs-step x))
    let f = function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![step],
        1,
    );
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    let ctx = &mut ev as *mut Context as *mut u8;
    for _ in 0..3 {
        assert_eq!(
            leaf.call(ctx, &[Value::make_int(5)]),
            NativeRun::Ok(Value::make_int(4).bits())
        );
    }
    let callee_id = ev
        .obarray
        .symbol_function_id(step.as_symbol_id().unwrap())
        .and_then(|v| v.get_bytecode_data())
        .and_then(|bc| bc.jit_runtime().compiled_id())
        .expect("the speculated call compiled the callee");
    let base = row_for(callee_id, cache::LeafState::Live).obs;
    assert_eq!(base.deopt_at + base.deopt_rerun, 0);
    for _ in 0..2 {
        match leaf.call(ctx, &[Value::make_float(5.0)]) {
            NativeRun::Ok(bits) => assert_eq!(Value::from_bits(bits).as_float(), Some(4.0)),
            other => panic!("expected the resumed result, got {other:?}"),
        }
    }
    let obs = row_for(callee_id, cache::LeafState::Live).obs;
    assert_eq!(
        obs.deopt_at + obs.deopt_rerun,
        2,
        "one count per failing callee run: {obs:?}"
    );
    assert_eq!(leaf.obs.deopt_at.get() + leaf.obs.deopt_rerun.get(), 0);
}

/// A signal leaving native code is counted on the leaf that exited with it:
/// the framed exit (`cold_frame_exit`) and the direct callee exit
/// (`direct_call_cold`).
#[test]
fn jit_obs_signal_exit_counted() {
    force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    // Framed: (car 5) through the call shim.
    let framed = lower_nullary_leaf(
        &[Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        &[Value::symbol("car"), Value::make_int(5)],
    )
    .expect("compiles");
    assert_eq!(framed.call(ctx, &[]), NativeRun::Signal);
    take_pending_flow().expect("wrong-type-argument");
    assert_eq!(framed.obs.signals.get(), 1);

    // Direct: F calls G natively; G signals.
    let g = install(
        &mut ev,
        "jit-obs-signaller",
        function(
            vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
            vec![Value::symbol("car")],
            1,
        ),
    );
    let f = function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![g],
        1,
    );
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    let ctx = &mut ev as *mut Context as *mut u8;
    for _ in 0..2 {
        assert_eq!(leaf.call(ctx, &[Value::make_int(5)]), NativeRun::Signal);
        take_pending_flow().expect("wrong-type-argument");
    }
    assert_eq!(leaf.obs.signals.get(), 2, "the caller exits with it too");
    let g_id = ev
        .obarray
        .symbol_function_id(g.as_symbol_id().unwrap())
        .and_then(|v| v.get_bytecode_data())
        .and_then(|bc| bc.jit_runtime().compiled_id())
        .expect("the speculated call compiled the callee");
    assert_eq!(row_for(g_id, cache::LeafState::Live).obs.signals, 2);
}
