use super::*;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::value::{LambdaParams, Value};

fn nullary_fn(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    f
}

/// A body the JIT rejects is remembered on its runtime: after the one
/// rejected compile the tier dispatcher answers `Interpret` for it without
/// probing this cache again, until the cache forgets its verdicts.
#[test]
fn rejected_body_is_remembered_on_its_runtime() {
    use crate::emacs_core::jit::Plan;
    // A body with no `Return` is rejected (`CompileError::NoReturn`) at
    // the pipeline's first scan; the verdict's kind is irrelevant here,
    // only that the cache records `NotCompilable` for it.
    let f = nullary_fn(vec![Op::Nil], vec![]);
    let rt = f.jit_runtime();
    let len = f.executable_ops().len();
    rt.set_hot_for_test();
    assert!(
        matches!(rt.dispatch_sized(len), Plan::Compiled),
        "hot and unjudged: the dispatcher consults the cache"
    );
    assert_eq!(
        try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
        None,
        "the rejected body falls back to the interpreter"
    );
    assert!(!is_compiled_for_test(
        rt.compiled_id().expect("the probe assigned an id")
    ));
    assert!(
        matches!(rt.dispatch_sized(len), Plan::Interpret),
        "the verdict is remembered: no more seam trips"
    );
    clear();
    assert!(
        matches!(rt.dispatch_sized(len), Plan::Compiled),
        "the cache forgot its verdicts, so the dispatcher probes again"
    );
}

/// A straight-line body compiles with the fast allocator, and re-tiers to
/// the full one — in place, still running — once its heat proves it hot.
#[test]
fn fast_leaf_retiers_to_full_once_hot() {
    if forced_regalloc().is_some() {
        return; // the A/B knob overrides the policy
    }
    let Some(retier_at) = crate::emacs_core::jit::retier_heat() else {
        return; // NEOVM_JIT_RETIER_FACTOR=0
    };
    let c = Value::make_int(7);
    let f = nullary_fn(vec![Op::Constant(0), Op::Return], vec![c]);
    let run = || try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap();
    assert_eq!(run(), Some(c.bits()));
    let id = f.jit_runtime().compiled_id().expect("compiled once");
    assert_eq!(compiled_regalloc_for_test(id), Some(RegallocChoice::Fast));
    assert_eq!(run(), Some(c.bits()));
    assert_eq!(
        compiled_regalloc_for_test(id),
        Some(RegallocChoice::Fast),
        "cold: no re-tier"
    );
    f.jit_runtime().set_heat_for_test(retier_at);
    assert_eq!(run(), Some(c.bits()));
    assert_eq!(
        compiled_regalloc_for_test(id),
        Some(RegallocChoice::Full),
        "hot: rebuilt full"
    );
    assert_eq!(run(), Some(c.bits()));
    assert_eq!(
        compiled_regalloc_for_test(id),
        Some(RegallocChoice::Full),
        "and stays full"
    );
}

/// A body the profitability gate refuses is DEFERRED, not vetoed, when the
/// factor is set: the dispatcher interprets it without probing until its
/// heat reaches the deferral point, then the compile runs with the gate
/// bypassed and the body runs native. With the factor at 0 it is refused
/// for good, as before.
#[test]
fn unprofitable_body_is_deferred_then_compiled_once_hot_enough() {
    use crate::emacs_core::jit::{Plan, force_profit_defer_for_test, hot_threshold};
    if !super::super::compile::jit_profit_gate_on() {
        return; // NEOVM_JIT_PROFIT=off: nothing is ever unprofitable
    }
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx = &mut ev as *mut crate::emacs_core::eval::Context;
    // (lambda () (length "abc")): one builtin call, no arithmetic → the gate refuses it.
    let body = || {
        nullary_fn(
            vec![
                Op::Constant(0),
                Op::CallBuiltinSym(crate::emacs_core::intern::intern("length"), 1),
                Op::Return,
            ],
            vec![Value::string("abc")],
        )
    };
    // Factor 0: the veto — NotCompilable, remembered on the runtime.
    force_profit_defer_for_test(Some(0));
    let f = body();
    let rt = f.jit_runtime();
    rt.set_hot_for_test();
    assert_eq!(try_run_compiled(ctx, &f, Value::NIL, &[]).unwrap(), None);
    let id = rt.compiled_id().expect("id assigned");
    assert_eq!(cache_entry_kind_for_test(id), "not-compilable");
    assert!(matches!(rt.dispatch_sized(3), Plan::Interpret));
    // Factor 4: deferred to 4× the threshold, interpreted meanwhile.
    force_profit_defer_for_test(Some(4));
    let f = body();
    let rt = f.jit_runtime();
    rt.set_hot_for_test();
    assert_eq!(try_run_compiled(ctx, &f, Value::NIL, &[]).unwrap(), None);
    let id = rt.compiled_id().expect("id assigned");
    assert_eq!(cache_entry_kind_for_test(id), "deferred");
    assert!(
        matches!(rt.dispatch_sized(3), Plan::Interpret),
        "deferred: no probe"
    );
    assert_eq!(
        try_run_compiled(ctx, &f, Value::NIL, &[]).unwrap(),
        None,
        "a probe anyway: still deferred"
    );
    assert_eq!(cache_entry_kind_for_test(id), "deferred");
    // The deferral runs out: the compile runs with the gate bypassed and
    // the body is cached as compiled. (Its native run's builtin dispatch is
    // not this test's claim — the minimal harness cannot serve `length`
    // through the generic shim path — so the run's outcome is not asserted.)
    rt.set_heat_for_test(hot_threshold().saturating_mul(4));
    assert!(matches!(rt.dispatch_sized(3), Plan::Compiled));
    let _ = try_run_compiled(ctx, &f, Value::NIL, &[]);
    assert_eq!(cache_entry_kind_for_test(id), "compiled");
    // Call-heavy: the fast allocator, and no re-tier however hot.
    if forced_regalloc().is_none() && std::env::var_os("NEOVM_JIT_REGALLOC_CALLHEAVY").is_none() {
        use crate::emacs_core::jit::compile::lowering::RegallocChoice;
        assert_eq!(compiled_regalloc_for_test(id), Some(RegallocChoice::Fast));
        if let Some(retier_at) = crate::emacs_core::jit::retier_heat() {
            rt.set_heat_for_test(retier_at);
            let _ = try_run_compiled(ctx, &f, Value::NIL, &[]);
            assert_eq!(
                compiled_regalloc_for_test(id),
                Some(RegallocChoice::Fast),
                "call-heavy bodies are never re-tiered"
            );
        }
    }
    force_profit_defer_for_test(None);
}

/// A baseline leaf that inlined a bit-op records `logand` as an inline
/// dep with no epoch (LEVEL-B). Redefining `logand` must evict it through
/// the precise path without tripping the disjointness assertion, which
/// used to test the epoch instead of the dep list.
#[test]
fn level_b_inlined_bit_op_leaf_is_evicted_on_redefinition_without_panicking() {
    if !super::super::compile::jit_inline_arith_on() {
        return; // NEOVM_JIT_INLINE_ARITH=off
    }
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let ctx = &mut ev as *mut crate::emacs_core::eval::Context;
    let logand = crate::emacs_core::intern::intern("logand");
    // (lambda () (logand 6 3)) → 2, with the bit-op inlined as a native op.
    let f = nullary_fn(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![
            Value::from_sym_id(logand),
            Value::make_int(6),
            Value::make_int(3),
        ],
    );
    // The minimal harness defines no `logand` subr, so the native run may
    // signal void-function through the generic path; the compile — and the
    // dep it registered — is what this test is about.
    let _ = try_run_compiled(ctx, &f, Value::NIL, &[]);
    let id = f.jit_runtime().compiled_id().expect("compiled");
    assert_eq!(cache_entry_kind_for_test(id), "compiled");
    let inlined = INLINE_DEPS.with(|m| m.borrow().get(&logand).is_some_and(|s| s.contains(&id)));
    if !inlined {
        return; // this build did not inline the bit-op; nothing to evict
    }
    evict_inline_dependents(logand);
    assert_eq!(cache_entry_kind_for_test(id), "none", "evicted precisely");
}

#[test]
fn runs_compilable_nullary_leaf() {
    let c = Value::make_int(42);
    let f = nullary_fn(vec![Op::Constant(0), Op::Return], vec![c]);
    // First call compiles + caches; result is the constant's bits.
    assert_eq!(
        try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
        Some(c.bits())
    );
    // Second call hits the cache; same result.
    assert_eq!(
        try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
        Some(c.bits())
    );
}

#[test]
fn returns_none_for_noncompilable_body() {
    // Switch is unsupported -> NotCompilable -> None (interpreter fallback).
    let f = nullary_fn(
        vec![Op::Nil, Op::Nil, Op::Switch, Op::Nil, Op::Return],
        vec![],
    );
    assert_eq!(
        try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
        None
    );
    assert_eq!(
        try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
        None
    );
}

#[test]
fn deopt_returns_none() {
    // MOST_POSITIVE + 1 overflows fixnum range -> native deopts -> None.
    let f = nullary_fn(
        vec![Op::Constant(0), Op::Constant(1), Op::Add, Op::Return],
        vec![
            Value::make_int(Value::MOST_POSITIVE_FIXNUM),
            Value::make_int(1),
        ],
    );
    assert_eq!(
        try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
        None
    );
}

#[test]
fn metering_records_one_compile_per_cache_miss() {
    stats::reset_compile_stats();
    let c = Value::make_int(7);
    let f = nullary_fn(vec![Op::Constant(0), Op::Return], vec![c]);
    assert_eq!(
        try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
        Some(c.bits())
    );
    let s = stats::compile_stats_snapshot();
    assert_eq!(s.total_compiles, 1);
    assert_eq!(s.compiled_ok, 1);
    assert_eq!(s.not_compilable, 0);
    assert_eq!(s.not_profitable, 0);
    assert_eq!(s.aot_loads, 0);
    assert!(s.total_us > 0, "a real compile takes measurable time");
    assert_eq!(s.max_us, s.total_us);
    assert_eq!(s.max_fn_len, 2, "ops.len() of the worst (only) compile");
    assert_eq!(s.histogram_us.iter().sum::<u64>(), 1);
    assert_eq!(s.histogram_us[stats::bucket_index(s.max_us)], 1);
    // Second call is a cache hit: no new compile is recorded.
    assert_eq!(
        try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
        Some(c.bits())
    );
    assert_eq!(stats::compile_stats_snapshot().total_compiles, 1);
}

#[test]
fn metering_aggregates_across_compiles() {
    stats::reset_compile_stats();
    for i in 0..64 {
        let c = Value::make_int(i);
        let f = nullary_fn(vec![Op::Constant(0), Op::Return], vec![c]);
        assert_eq!(
            try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
            Some(c.bits())
        );
    }
    let s = stats::compile_stats_snapshot();
    assert_eq!(s.total_compiles, 64);
    assert_eq!(s.compiled_ok, 64);
    assert_eq!(s.histogram_us.iter().sum::<u64>(), 64);
    assert!(s.max_us <= s.total_us);
    assert_eq!(s.max_fn_len, 2);
}

#[test]
fn metering_counts_noncompilable_outcome() {
    stats::reset_compile_stats();
    let f = nullary_fn(
        vec![Op::Nil, Op::Nil, Op::Switch, Op::Nil, Op::Return],
        vec![],
    );
    assert_eq!(
        try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[]).unwrap(),
        None
    );
    let s = stats::compile_stats_snapshot();
    assert_eq!(s.total_compiles, 1);
    assert_eq!(s.compiled_ok, 0);
    assert_eq!(s.not_compilable, 1);
}

#[test]
fn assigns_stable_unique_ids() {
    let f1 = nullary_fn(vec![Op::Nil, Op::Return], vec![]);
    let f2 = nullary_fn(vec![Op::Nil, Op::Return], vec![]);
    let a = f1.jit_runtime().compiled_id_or_assign();
    let a_again = f1.jit_runtime().compiled_id_or_assign();
    let b = f2.jit_runtime().compiled_id_or_assign();
    assert_eq!(a, a_again, "id is stable per function");
    assert_ne!(a, b, "distinct functions get distinct ids");
    assert_ne!(a, 0, "0 is reserved for unassigned");
}

#[test]
fn runs_with_args_and_rejects_arity_mismatch() {
    // (lambda (a b) (+ a b)), lexical so params are on the stack.
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![
            crate::emacs_core::intern::SymId(1),
            crate::emacs_core::intern::SymId(2),
        ],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::StackRef(1), Op::StackRef(1), Op::Add, Op::Return];
    f.max_stack = 16;
    // Correct arity -> native result.
    assert_eq!(
        try_run_compiled(
            std::ptr::null_mut(),
            &f,
            Value::NIL,
            &[Value::make_int(40), Value::make_int(2)]
        )
        .unwrap(),
        Some(Value::make_int(42).bits())
    );
    // Wrong arity -> None (interpreter will signal wrong-number-of-arguments).
    assert_eq!(
        try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[Value::make_int(40)]).unwrap(),
        None
    );
}

fn unary_add_one() -> ByteCodeFunction {
    // (lambda (x) (+ x 1))
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return];
    f.constants = vec![Value::make_int(1)].into();
    f.max_stack = 16;
    f
}

fn deopts(obs: &LeafObsSnapshot) -> u64 {
    obs.deopt_at + obs.deopt_rerun
}

/// A leaf replaced by its full-allocator rebuild keeps its counters as a
/// `retired` row; the new leaf starts from zero.
#[test]
fn jit_obs_retired_leaf_counts_survive_retier() {
    if forced_regalloc().is_some() {
        return;
    }
    let Some(retier_at) = crate::emacs_core::jit::retier_heat() else {
        return;
    };
    let f = unary_add_one();
    let run = |arg: Value| try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[arg]).unwrap();
    assert_eq!(run(Value::make_int(1)), Some(Value::make_int(2).bits()));
    assert_eq!(run(Value::NIL), None, "a nil operand deopts");
    let id = f.jit_runtime().compiled_id().expect("compiled");
    assert_eq!(compiled_regalloc_for_test(id), Some(RegallocChoice::Fast));
    f.jit_runtime().set_heat_for_test(retier_at);
    assert_eq!(run(Value::make_int(1)), Some(Value::make_int(2).bits()));
    assert_eq!(compiled_regalloc_for_test(id), Some(RegallocChoice::Full));
    let (rows, _) = leaf_report_rows();
    let mine: Vec<&LeafRow> = rows.iter().filter(|r| r.id == id).collect();
    let retired = mine
        .iter()
        .find(|r| r.state == LeafState::Retired)
        .expect("the fast leaf is retired, not dropped");
    assert_eq!(deopts(&retired.obs), 1);
    assert_eq!(retired.regalloc, RegallocChoice::Fast);
    let live = mine
        .iter()
        .find(|r| r.state == LeafState::Live)
        .expect("the rebuilt leaf is live");
    assert_eq!(deopts(&live.obs), 0);
    assert_eq!(live.obs.id, id);
}

/// A heap-swap `clear` drops every leaf; their counts move to the dropped
/// totals instead of vanishing.
#[test]
fn jit_obs_clear_folds_counts_into_dropped_totals() {
    let f = unary_add_one();
    let run = |arg: Value| try_run_compiled(std::ptr::null_mut(), &f, Value::NIL, &[arg]).unwrap();
    assert_eq!(run(Value::NIL), None);
    assert_eq!(run(Value::NIL), None);
    let id = f.jit_runtime().compiled_id().expect("compiled");
    let (rows, dropped0) = leaf_report_rows();
    assert!(rows.iter().any(|r| r.id == id));
    clear();
    let (rows, dropped) = leaf_report_rows();
    assert!(!rows.iter().any(|r| r.id == id), "cleared");
    assert_eq!(dropped.leaves, dropped0.leaves + 1);
    assert_eq!(
        dropped.deopt_at + dropped.deopt_rerun,
        dropped0.deopt_at + dropped0.deopt_rerun + 2
    );
}

/// Evicting a function drops its OSR leaves; their counts are folded too.
#[test]
fn jit_obs_evict_compiled_folds_osr_counts() {
    let leaf = super::super::compile::lower_nullary_leaf(
        &[Op::Constant(0), Op::Nil, Op::Add, Op::Return],
        &[Value::make_int(5)],
    )
    .expect("compiles");
    assert_eq!(leaf.call_for_test(&[]), None);
    let id = 4_241;
    OSR_CACHE.with(|c| {
        c.borrow_mut().insert(
            (id, 3),
            Some(OsrEntry {
                leaf: Rc::new(leaf),
                stack_depth: 0,
                bind_depth: 0,
            }),
        )
    });
    let (rows, dropped0) = leaf_report_rows();
    let osr = rows
        .iter()
        .find(|r| r.id == id && r.state == LeafState::Osr)
        .expect("the OSR leaf is reported");
    assert_eq!(deopts(&osr.obs), 1);
    evict_compiled(id);
    let (rows, dropped) = leaf_report_rows();
    assert!(!rows.iter().any(|r| r.id == id));
    assert_eq!(dropped.leaves, dropped0.leaves + 1);
    assert_eq!(dropped.deopt_at, dropped0.deopt_at + 1);
}

/// A leaf the AOT prewarm inserts carries the id it was inserted under.
#[test]
fn jit_obs_aot_prepopulated_leaf_has_id() {
    let leaf = super::super::compile::lower_nullary_leaf(
        &[Op::Constant(0), Op::Return],
        &[Value::make_int(7)],
    )
    .expect("compiles");
    let id = 4_242;
    assert_eq!(prepopulate_aot_leaves(vec![(id, leaf)]), vec![id]);
    let (rows, _) = leaf_report_rows();
    let row = rows
        .iter()
        .find(|r| r.id == id)
        .expect("the prepopulated leaf is reported");
    assert_eq!(row.obs.id, id);
    assert_eq!(row.state, LeafState::Live);
}
