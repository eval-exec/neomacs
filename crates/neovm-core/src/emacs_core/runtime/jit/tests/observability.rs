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
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
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
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
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
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
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
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
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
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
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

/// An epoch move for an UNRELATED symbol makes an armed site re-validate
/// and re-arm (`spec-rearm`); rebinding its own callee is `spec-rebind`.
#[test]
fn jit_obs_spec_revalidation_counts_rearm_and_rebind() {
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    use crate::emacs_core::jit::stats::epoch::{EpochCounters, SpecRevalidation};
    force_profit_gate_for_test(false);
    let mut ev = Context::new();
    let step = install(
        &mut ev,
        "jit-obs-rearm-step",
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
    let f = function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![step],
        1,
    );
    let leaf = compile_bytecode_function_with(&f, Some(&ev.obarray)).expect("compiles");
    let ctx = &mut ev as *mut Context as *mut u8;
    let call = |want: i64| {
        assert_eq!(
            leaf.call(ctx, &[Value::make_int(5)]),
            NativeRun::Ok(Value::make_int(want).bits())
        );
    };
    call(4);
    call(4);
    let base = EpochCounters::snapshot();
    ev.eval_str("(fset 'jit-obs-rearm-unrelated (lambda () 1))")
        .expect("unrelated fset");
    let ctx = &mut ev as *mut Context as *mut u8;
    assert_eq!(
        leaf.call(ctx, &[Value::make_int(5)]),
        NativeRun::Ok(Value::make_int(4).bits())
    );
    let d = EpochCounters::snapshot().since(&base);
    if jit_force_slow_spec() {
        assert!(d.spec_for(SpecRevalidation::Rearmed) >= 1, "{}", d.render());
    } else {
        assert_eq!(d.spec_for(SpecRevalidation::Rearmed), 1, "{}", d.render());
    }
    assert_eq!(d.spec_for(SpecRevalidation::BindingChanged), 0);

    ev.eval_str("(fset 'jit-obs-rearm-step (lambda (n) (+ n 20)))")
        .expect("rebind");
    let ctx = &mut ev as *mut Context as *mut u8;
    assert_eq!(
        leaf.call(ctx, &[Value::make_int(5)]),
        NativeRun::Ok(Value::make_int(25).bits())
    );
    let d = EpochCounters::snapshot().since(&base);
    assert_eq!(
        d.spec_for(SpecRevalidation::BindingChanged),
        1,
        "{}",
        d.render()
    );
}

/// Redefining a callee that a cached leaf inlined evicts that leaf, and the
/// eviction is counted.
#[test]
fn jit_obs_inline_eviction_counted() {
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    use crate::emacs_core::jit::stats::epoch::EpochCounters;
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    // C = (lambda (x) (* x x)); F = (lambda (a) (C a)) inlines C.
    let c_sym = install(
        &mut ev,
        "jit-obs-inline-c",
        function(vec![Op::Dup, Op::Mul, Op::Return], vec![], 1),
    );
    let f = function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![c_sym],
        1,
    );
    let f_val = Value::make_bytecode(f.clone());
    let r = crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(5)]);
    assert!(matches!(r, Ok(Some(b)) if b == Value::make_int(25).bits()));
    let c_id = c_sym.as_symbol_id().unwrap();
    assert_eq!(
        crate::emacs_core::jit::cache::inline_dependent_count_for_test(c_id),
        1,
        "F inlined C"
    );
    let base = EpochCounters::snapshot();
    install(
        &mut ev,
        "jit-obs-inline-c",
        function(vec![Op::Add1, Op::Return], vec![], 1),
    );
    let d = EpochCounters::snapshot().since(&base);
    assert_eq!(d.inline_evicted_leaves, 1, "{}", d.render());
}

fn force_naming(on: bool) {
    crate::emacs_core::jit::stats::force_observe_for_test(
        crate::emacs_core::jit::stats::ObserveOverride {
            naming: on,
            ..Default::default()
        },
    );
}

/// Under naming, a function tiered up through the interpreter's `Bcall` is
/// declared as `lisp:<symbol>#<id>:<tier>`, and the leaf remembers it.
#[test]
fn jit_obs_label_names_symbol_callee() {
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    use crate::emacs_core::jit::stats::perf_map::last_entry_name_for_test;
    force_naming(true);
    let mut ev = Context::new();
    // (defun jit-obs-named (x) (+ x 1)), already hot.
    let named = install(
        &mut ev,
        "jit-obs-named",
        function(
            vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
            vec![Value::make_int(1)],
            1,
        ),
    );
    let callee_bc = ev
        .obarray
        .symbol_function_id(named.as_symbol_id().unwrap())
        .and_then(|v| v.get_bytecode_data())
        .expect("bytecode");
    callee_bc.jit_runtime().set_hot_for_test();
    // The caller runs on the interpreter: (jit-obs-named 5)
    let caller = function(
        vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        vec![named, Value::make_int(5)],
        0,
    );
    let out = Vm::from_context(&mut ev)
        .execute(&caller, vec![])
        .expect("runs");
    assert_eq!(out.bits(), Value::make_int(6).bits());
    let id = callee_bc
        .jit_runtime()
        .compiled_id()
        .expect("the Bcall tier-up compiled the callee");
    let row = row_for(id, cache::LeafState::Live);
    let want = format!("lisp:jit-obs-named#{id}:{}", row.tier.name());
    assert_eq!(row.label.as_deref(), Some(want.as_str()));
    assert_eq!(last_entry_name_for_test(), want, "declared under the label");
}

/// Without a symbol to name it (here: no Context, so no backtrace), a leaf
/// is labelled by its first symbol constants.
#[test]
fn jit_obs_label_falls_back_to_anon() {
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    force_naming(true);
    let f = function(
        vec![Op::Constant(0), Op::Return],
        vec![Value::symbol("jit-obs-anon-const")],
        0,
    );
    let got = crate::emacs_core::jit::try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[])
        .expect("runs");
    assert_eq!(got, Some(Value::symbol("jit-obs-anon-const").bits()));
    let id = f.jit_runtime().compiled_id().expect("compiled");
    let row = row_for(id, cache::LeafState::Live);
    let want = format!("lisp:anon[jit-obs-anon-const]#{id}:{}", row.tier.name());
    assert_eq!(row.label.as_deref(), Some(want.as_str()));
}

/// With naming off (the default), nothing is labelled and the entry is
/// declared under the legacy static name.
#[test]
fn jit_obs_label_absent_without_naming() {
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    use crate::emacs_core::jit::stats::perf_map::last_entry_name_for_test;
    force_naming(false);
    let f = function(
        vec![Op::Constant(0), Op::Return],
        vec![Value::make_int(3)],
        0,
    );
    let got = crate::emacs_core::jit::try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[])
        .expect("runs");
    assert_eq!(got, Some(Value::make_int(3).bits()));
    let id = f.jit_runtime().compiled_id().expect("compiled");
    let row = row_for(id, cache::LeafState::Live);
    assert_eq!(row.label, None);
    let legacy = match row.tier {
        LeafTier::Mir => "__neovm_mir_leaf",
        _ => "__neovm_jit_leaf",
    };
    assert_eq!(last_entry_name_for_test(), legacy);
}

fn force_entry_count(on: bool) {
    crate::emacs_core::jit::stats::force_observe_for_test(
        crate::emacs_core::jit::stats::ObserveOverride {
            entry_count: on,
            ..Default::default()
        },
    );
}

/// The entry counter is emitted only when entry counting is on at compile
/// time: a few more CLIF instructions (load, add, store through a baked
/// address), and none by default.
#[test]
fn jit_obs_entry_counter_absent_by_default() {
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    let ops = [Op::Constant(0), Op::Constant(1), Op::Add, Op::Return];
    let consts = [Value::make_int(40), Value::make_int(2)];
    force_entry_count(false);
    let off = lower_nullary_leaf(&ops, &consts).expect("compiles");
    force_entry_count(true);
    let on = lower_nullary_leaf(&ops, &consts).expect("compiles");
    assert!(!off.obs.entry_counted);
    assert!(on.obs.entry_counted);
    assert!(on.clif_insts > off.clif_insts, "baseline");
    assert_eq!(off.call_for_test(&[]), Some(Value::make_int(42).bits()));
    assert_eq!(on.call_for_test(&[]), Some(Value::make_int(42).bits()));
    assert_eq!(off.obs.entries.get(), 0, "no counter in default code");
    assert_eq!(on.obs.entries.get(), 1);

    let mir_ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
    let m = mir::build_mir(&mir_ops, &[], 2).expect("builds");
    force_entry_count(false);
    let off = lower_mir_pure(&m).expect("lowers");
    force_entry_count(true);
    let on = lower_mir_pure(&m).expect("lowers");
    assert!(on.clif_insts > off.clif_insts, "mir");
    let args = [Value::make_int(1), Value::make_int(2)];
    assert_eq!(on.call_for_test(&args), Some(Value::make_int(3).bits()));
    assert_eq!(off.call_for_test(&args), Some(Value::make_int(3).bits()));
    assert_eq!(on.obs.entries.get(), 1);
    assert_eq!(off.obs.entries.get(), 0);
}

/// With the counter on, every native entry counts — through the tier-up
/// seam, the speculated native-to-native call, and runs that deopt.
#[test]
fn jit_obs_entry_counter_counts_every_path() {
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    force_profit_gate_for_test(false);
    force_entry_count(true);
    let mut ev = Context::new();
    // Seam: (lambda (x) (+ x 1)) through try_run_compiled.
    let f = function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    );
    let f_val = Value::make_bytecode(f.clone());
    let ctx = &mut ev as *mut Context;
    for i in 0..4 {
        crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::make_int(i)])
            .expect("no signal");
    }
    // A run that deopts was still an entry.
    crate::emacs_core::jit::try_run_compiled(ctx, &f, f_val, &[Value::NIL]).expect("no signal");
    let id = f.jit_runtime().compiled_id().expect("compiled");
    let obs = row_for(id, cache::LeafState::Live).obs;
    assert!(obs.entry_counted);
    assert_eq!(obs.entries, 5, "{obs:?}");

    // Native-to-native: F calls a multi-block callee through its spec site.
    let step = install(
        &mut ev,
        "jit-obs-entry-step",
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
    let caller = function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![step],
        1,
    );
    let leaf = compile_bytecode_function_with(&caller, Some(&ev.obarray)).expect("compiles");
    let ctx = &mut ev as *mut Context as *mut u8;
    for _ in 0..3 {
        assert_eq!(
            leaf.call(ctx, &[Value::make_int(5)]),
            NativeRun::Ok(Value::make_int(4).bits())
        );
    }
    assert_eq!(leaf.obs.entries.get(), 3, "the caller, entered directly");
    let step_id = ev
        .obarray
        .symbol_function_id(step.as_symbol_id().unwrap())
        .and_then(|v| v.get_bytecode_data())
        .and_then(|bc| bc.jit_runtime().compiled_id())
        .expect("the speculated call compiled the callee");
    assert_eq!(
        row_for(step_id, cache::LeafState::Live).obs.entries,
        3,
        "the callee, entered from native code"
    );
}
