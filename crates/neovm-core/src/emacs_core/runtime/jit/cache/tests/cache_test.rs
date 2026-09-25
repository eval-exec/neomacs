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
    // Exact native outcomes: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
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

/// The perf-map name hint only accepts a symbol whose function IS the leaf
/// being compiled: the innermost frame's, or a speculated callee's.
#[test]
fn jit_obs_name_hint_rejects_mismatched_frame() {
    let mut ev = Context::new();
    let sym = Value::symbol("jit-obs-hint-fn");
    let other = Value::symbol("jit-obs-hint-other");
    let sym_id = sym.as_symbol_id().unwrap();
    ev.obarray
        .set_symbol_function_id(sym_id, Value::make_bytecode(unary_add_one()));
    ev.obarray.set_symbol_function_id(
        other.as_symbol_id().unwrap(),
        Value::make_bytecode(unary_add_one()),
    );
    let id = ev
        .obarray
        .symbol_function_id(sym_id)
        .and_then(|v| v.get_bytecode_data())
        .expect("bytecode")
        .jit_runtime()
        .compiled_id_or_assign();
    let ctx = &ev as *const Context;
    assert_eq!(callee_name_hint(ctx, id), None, "no frame at all");
    ev.push_backtrace_frame(sym, &[Value::make_int(1)]);
    let ctx = &ev as *const Context;
    assert_eq!(callee_name_hint(ctx, id), Some(sym_id), "its own frame");
    ev.push_backtrace_frame(other, &[Value::make_int(1)]);
    let ctx = &ev as *const Context;
    assert_eq!(
        callee_name_hint(ctx, id),
        None,
        "the innermost frame is another function's"
    );
    stats::perf_map::set_pending_callee(sym_id);
    assert_eq!(
        callee_name_hint(ctx, id),
        Some(sym_id),
        "a speculated callee's symbol wins"
    );
    assert_eq!(callee_name_hint(ctx, id), None, "the pending hint is taken");
    assert_eq!(callee_name_hint(std::ptr::null(), id), None);
}

/// Leaves are bound to the obarray they were compiled against (P1.4 §3.6): a
/// GC root walk under another obarray's generation drops them, one under the
/// same generation keeps them.
#[test]
fn a_different_obarray_generation_clears_the_cache() {
    let obarray = Obarray::new();
    let c = Value::make_int(7);
    let f = nullary_fn(vec![Op::Constant(0), Op::Return], vec![c]);
    let id = compile_and_cache_jit_leaf(&f, Some(&obarray)).expect("compiles");
    sync_cache_to_obarray(obarray.generation());
    assert!(is_compiled_for_test(id), "same obarray: the leaf stays");
    let other = Obarray::new();
    assert_ne!(other.generation(), obarray.generation());
    sync_cache_to_obarray(other.generation());
    assert!(
        !is_compiled_for_test(id),
        "another obarray: the cache is dropped"
    );
    // The next compile pins the new obarray.
    let id = compile_and_cache_jit_leaf(&f, Some(&other)).expect("compiles");
    sync_cache_to_obarray(other.generation());
    assert!(is_compiled_for_test(id));
}

/// A leaf the cache retires (here: evicted by a widened `make-closure`
/// prefix) is marked retired, stays allocated, and keeps its reloc constants
/// among the GC roots: an outer native frame or a spec slot may still run it.
#[test]
fn retired_leaf_is_marked_and_its_constants_stay_rooted() {
    // An exact native result: immune to a NEOVM_JIT_FORCE_DEOPT=1 suite run.
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    // (lambda () (car '("reloc-root-probe")))  -- a heap constant in reloc_data
    let payload = Value::list(vec![Value::string("reloc-root-probe")]);
    let f = nullary_fn(vec![Op::Constant(0), Op::Car, Op::Return], vec![payload]);
    let f_val = Value::make_bytecode(f.clone());
    f.jit_runtime().set_hot_for_test();
    let got = try_run_compiled(std::ptr::null_mut(), &f, f_val, &[]).expect("runs");
    let id = f.jit_runtime().compiled_id().expect("compiled");
    assert!(is_compiled_for_test(id), "the body compiles");
    assert!(got.is_some_and(|bits| Value::from_bits(bits).is_string()));
    let leaf = compiled_leaf_ptr_for_test(id).expect("cached");
    // SAFETY: the cache holds the leaf (and, after the eviction, its retired list).
    let leaf = unsafe { &*leaf };
    assert!(!leaf.retired.get(), "a live leaf is not retired");
    let reloc: Vec<usize> = leaf.reloc_values().iter().map(|v| v.bits()).collect();
    assert!(
        reloc.contains(&payload.bits()),
        "the constant is in reloc_data"
    );
    let (entries_before, slots_before) = compiled_cache_probe();

    evict_compiled(id);
    assert_eq!(cache_entry_kind_for_test(id), "none");
    assert!(leaf.retired.get(), "eviction retires the leaf");
    let mut roots = Vec::new();
    collect_jit_reloc_gc_roots(&mut roots);
    assert!(
        roots.iter().any(|v| v.bits() == payload.bits()),
        "a retired leaf's reloc constants stay rooted"
    );
    assert_eq!(
        compiled_cache_probe(),
        (entries_before - 1, slots_before),
        "the entry is gone; the probe still counts the retired leaf's reloc slots"
    );
}

/// `SpecCalleeKind::from_spec_disc` inverts `to_spec_disc` (the AOT loader
/// recovers each slot's kind from its baked discriminant).
#[test]
fn spec_callee_kind_disc_round_trips() {
    use crate::emacs_core::jit::compile::SpecCalleeKind;
    for disc in 0..SpecCalleeKind::DISC_COUNT {
        let kind = SpecCalleeKind::from_spec_disc(disc).expect("every disc names a kind");
        assert_eq!(kind.to_spec_disc(), Some(disc));
    }
    assert_eq!(
        SpecCalleeKind::from_spec_disc(SpecCalleeKind::DISC_COUNT),
        None
    );
}

fn lexical_fn(ops: Vec<Op>, constants: Vec<Value>, arity: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|i| crate::emacs_core::intern::SymId(i as u32 + 1))
            .collect(),
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

/// Install a multi-block bytecode callee (the MIR inliner leaves a call to
/// it in place): `(lambda (n) (if n (1- n) 0))`.
fn install_step(ev: &mut crate::emacs_core::eval::Context, name: &str) -> Value {
    let sym = Value::symbol(name);
    let step = lexical_fn(
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
    );
    ev.obarray
        .set_symbol_function_id(sym.as_symbol_id().unwrap(), Value::make_bytecode(step));
    sym
}

/// A leaf clears only its BYTECODE-kind spec slots that cache the dead
/// leaf: the words of other slot kinds are not leaf pointers.
#[test]
fn unlink_spec_slots_to_clears_only_bytecode_slots_caching_the_dead_leaf() {
    use crate::emacs_core::jit::compile::SpecCalleeKind;
    let mut ev = crate::emacs_core::eval::Context::new();
    let step = install_step(&mut ev, "unlink-kind-step");
    // (lambda (x) (unlink-kind-step x))
    let f = lexical_fn(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![step],
        1,
    );
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut leaf =
        crate::emacs_core::jit::compile::compile_bytecode_function_with(&f, Some(&ev.obarray))
            .expect("compiles");
    assert_eq!(&*leaf.spec_slot_kinds, &[SpecCalleeKind::Bytecode]);
    let other = lexical_fn(vec![Op::Constant(0), Op::Return], vec![Value::NIL], 0);
    let dead = crate::emacs_core::jit::compile::compile_bytecode_function_with(&other, None)
        .expect("compiles");
    let dead_ptr: *const CompiledLeaf = &dead;
    let slot = &leaf.spec_slots[0];
    slot.arm_leaf(dead_ptr, std::ptr::null(), false, false);
    assert_eq!(
        leaf.unlink_spec_slots_to(std::ptr::null()),
        0,
        "not the dead leaf"
    );
    assert_eq!(leaf.spec_slots[0].leaf_ptr(), dead_ptr);
    assert_eq!(leaf.unlink_spec_slots_to(dead_ptr), 1);
    assert!(leaf.spec_slots[0].leaf_ptr().is_null(), "cleared");
    // The same word in a slot of another kind is left alone.
    leaf.spec_slot_kinds = Box::new([SpecCalleeKind::SubrGeneral]);
    leaf.spec_slots[0].arm_leaf(dead_ptr, std::ptr::null(), false, false);
    assert_eq!(leaf.unlink_spec_slots_to(dead_ptr), 0);
    assert_eq!(
        leaf.spec_slots[0].leaf_ptr(),
        dead_ptr,
        "a subr slot is untouched"
    );
}

/// The thread-wide walk reaches the spec slots of cached leaves: a caller's
/// slot that cached its callee's leaf is cleared, and the caller keeps
/// running correctly (it re-resolves the callee).
#[test]
fn unlink_spec_slots_clears_a_cached_callers_slot() {
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = crate::emacs_core::eval::Context::new();
    let step = install_step(&mut ev, "unlink-walk-step");
    // (lambda (x) (unlink-walk-step x))
    let f = lexical_fn(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![step],
        1,
    );
    let f_val = Value::make_bytecode(f.clone());
    let ctx = &mut ev as *mut crate::emacs_core::eval::Context;
    let run = || try_run_compiled(ctx, &f, f_val, &[Value::make_int(5)]).expect("no signal");
    for _ in 0..3 {
        assert_eq!(run(), Some(Value::make_int(4).bits()));
    }
    let caller_id = f.jit_runtime().compiled_id().expect("compiled");
    // SAFETY: cached leaves stay allocated for the test's duration.
    let caller = unsafe { &*compiled_leaf_ptr_for_test(caller_id).expect("caller cached") };
    let callee_ptr = caller.spec_slots[0].leaf_ptr();
    assert!(
        !callee_ptr.is_null(),
        "the speculated call cached its callee's leaf"
    );
    assert_eq!(unlink_spec_slots(callee_ptr), 1);
    assert!(caller.spec_slots[0].leaf_ptr().is_null());
    assert_eq!(unlink_spec_slots(callee_ptr), 0, "idempotent");
    assert_eq!(run(), Some(Value::make_int(4).bits()), "still correct");
    assert_eq!(
        caller.spec_slots[0].leaf_ptr(),
        callee_ptr,
        "the next call re-resolved the (still current) callee leaf"
    );
}
