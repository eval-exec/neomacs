//! Frozen GNU bodies through the real loader and normal T2/cache lifecycle.
//! Threading: all contexts, roots, Rc leaves and counter snapshots are
//! owned by this invocation's mutator. No new runtime cache/state is introduced.

use crate::emacs_core::jit::compile::opt_census::SelectedTier;

use super::*;
use crate::emacs_core::eval::{
    push_scratch_gc_root_slot, push_scratch_gc_roots, restore_scratch_gc_roots,
    save_scratch_gc_roots, set_scratch_gc_root,
};
use crate::emacs_core::jit::{bg, cache, feedback, stats, tier2};
use crate::emacs_core::load::{self, LoadOptions, MissingFilePolicy};
use crate::emacs_core::value::{ValueKind, list_to_vec};
use crate::tagged::gc::MemoryUseCountSlot;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

#[path = "fixtures/opt_sink_allocation_oracle.rs"]
mod oracle;

const CALLS: usize = 32;
// A resource cap, not an admission/budget override. Debug compilation can
// spend more CPU than one million short native calls replenish at the real
// 2% rate. Keep the unchanged 120-second cap and all real admission decisions.
const MAX_WARM_CALLS: usize = 10_000_000;
const MAX_WARM_TIME: Duration = Duration::from_secs(120);

/// Compiler settings match the campaign's normal service configuration. Heat,
/// profit/admission decisions, T2 stability and CPU budgeting remain real.
struct Settings;
impl Settings {
    fn enter(sink: bool) -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_profit_for_test(Some(OptProfitMode::Off));
        force_opt_passes_for_test(Some(OptPasses {
            sink,
            ..OptPasses::ALL
        }));
        force_flonum_mode_for_test(Some(FlonumMode::Resident));
        force_tier2_for_test(Some(Tier2Knob {
            on: true,
            window: Tier2Knob::DEFAULT_WINDOW,
            loop_credit: Tier2Knob::DEFAULT_LOOP_CREDIT,
        }));
        force_tier2_policy_for_test(Some(Tier2PolicyKnob::from_env(|_| None)));
        force_inline2_for_test(Some(Inline2Mode::All));
        force_spec_sources_for_test(Some(true));
        bg::force_mode_for_test(Some(bg::BgMode::Legacy));
        feedback::force_feedback_mode_for_test(Some(feedback::FeedbackMode::Use));
        force_deopt_for_test(false);
        stats::force_observe_for_test(stats::ObserveOverride {
            stats: true,
            naming: true,
            entry_count: true,
        });
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_profit_for_test(None);
        force_opt_passes_for_test(None);
        force_flonum_mode_for_test(None);
        force_tier2_for_test(None);
        force_tier2_policy_for_test(None);
        force_inline2_for_test(None);
        force_spec_sources_for_test(None);
        bg::force_mode_for_test(None);
        feedback::force_feedback_mode_for_test(None);
        force_deopt_for_test(false);
        stats::force_observe_for_test(stats::ObserveOverride::default());
    }
}

struct Roots(usize);
impl Roots {
    fn enter() -> Self {
        Self(save_scratch_gc_roots())
    }
    fn add(&self, values: &[Value]) {
        push_scratch_gc_roots(values);
    }
}
impl Drop for Roots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

#[derive(Clone, Copy)]
struct Fixture {
    functions: [Value; 6],
}
impl Fixture {
    fn float(self) -> Value {
        self.functions[0]
    }
    fn setup(self) -> Value {
        self.functions[1]
    }
    fn advance(self) -> Value {
        self.functions[2]
    }
    fn forces(self) -> Value {
        self.functions[3]
    }
    fn offset(self) -> Value {
        self.functions[4]
    }
    fn energy(self) -> Value {
        self.functions[5]
    }
}

fn constant_value(spec: oracle::ConstantSpec, objects: &[Value]) -> Value {
    match spec {
        oracle::ConstantSpec::Object(id) => objects[id],
        oracle::ConstantSpec::Integer(n) => Value::make_int(n),
        oracle::ConstantSpec::Symbol(name) => Value::symbol(name),
    }
}

fn load_fixture(ctx: &mut Context, roots: &Roots) -> Fixture {
    // This is the repository path, deliberately without a tmp fallback. The
    // exact same path/load API must succeed in the lead's checkout.
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/emacs_core/runtime/jit/compile/tests/fixtures/opt_sink_allocation.el");
    load::load_file_with_options(
        ctx,
        &path,
        LoadOptions::implicit_dependency(MissingFilePolicy::Signal),
    )
    .expect("frozen primitive graph loader must succeed in bare Context");
    let graph = ctx
        .obarray
        .symbol_value_copied("t34-o36-fixture-objects")
        .expect("loader's graph binding");
    roots.add(&[graph]);
    let objects = graph.as_vector_data().expect("42-node graph");
    assert_eq!(objects.len(), oracle::OBJECTS.len());
    for (id, &spec) in oracle::OBJECTS.iter().enumerate() {
        match spec {
            oracle::ObjectSpec::Float(bits) => {
                assert!(matches!(objects[id].kind(), ValueKind::Float));
                assert_eq!(objects[id].xfloat().to_bits(), bits, "float node {id}");
            }
            oracle::ObjectSpec::Vector(edges) => {
                let slots = objects[id].as_vector_data().expect("body vector");
                assert_eq!(slots.len(), 7);
                for (slot, &target) in edges.iter().enumerate() {
                    assert_eq!(
                        slots[slot].bits(),
                        objects[target].bits(),
                        "graph edge {id}:{slot}"
                    );
                }
            }
        }
        for prior in 0..id {
            assert_ne!(
                objects[id].bits(),
                objects[prior].bits(),
                "different GNU graph nodes must not be interned together"
            );
        }
    }
    for &(name, spec) in &oracle::GLOBALS {
        assert_eq!(
            ctx.obarray.symbol_value_copied(name).unwrap().bits(),
            constant_value(spec, objects).bits(),
            "global {name}"
        );
    }
    let functions = std::array::from_fn(|index| {
        let spec = oracle::FUNCTIONS[index];
        let value = ctx
            .obarray
            .symbol_function(spec.name)
            .expect("named compiled function");
        let source = value
            .get_bytecode_data()
            .expect("real GNU byte-code object");
        assert_eq!(source.arglist, Value::make_int(spec.descriptor));
        assert_eq!(source.max_stack.get(), spec.max_stack as usize);
        assert_eq!(
            source.gnu_bytecode_bytes.as_ref().unwrap().as_slice(),
            spec.bytes
        );
        assert_eq!(source.constants.len(), spec.constants.len());
        for (actual, &expected) in source.constants.iter().zip(spec.constants) {
            assert_eq!(
                actual.bits(),
                constant_value(expected, objects).bits(),
                "original pool graph for {}",
                spec.name
            );
        }
        assert!(
            !source.executable_ops().is_empty(),
            "loaded GNU decode must succeed"
        );
        assert!(
            source.jit_runtime().compiled_id().is_none(),
            "lookup is still cold"
        );
        value
    });
    roots.add(&functions);
    Fixture { functions }
}

fn source(value: Value) -> &'static ByteCodeFunction {
    value.get_bytecode_data().expect("rooted loaded bytecode")
}

fn tier0(ctx: &mut Context, value: Value, args: &[Value]) -> Value {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(source(value), args.to_vec())
        .expect("original GNU Tier-0 body")
}

fn float_bits(value: Value) -> u64 {
    assert!(matches!(value.kind(), ValueKind::Float));
    value.xfloat().to_bits()
}

fn expected_float(n: i64) -> u64 {
    oracle::FLOAT_ANSWERS
        .iter()
        .find(|&&(count, _)| count == n)
        .unwrap()
        .1
}

fn normal_opt(value: Value) -> Option<Rc<CompiledLeaf>> {
    let id = source(value).jit_runtime().compiled_id()?;
    let ptr = cache::compiled_leaf_ptr_for_test(id)?;
    // SAFETY: lookup and immediate owned Rc clone are consecutive; no Lisp,
    // cache clear, heap replacement or runtime callback occurs between them.
    let leaf = cache::current_leaf_of(unsafe { &(*ptr).obs })?;
    (leaf.selected_tier() == SelectedTier::Opt
        && leaf.obs.osr_pc.is_none()
        && leaf.obs.t2.origin == tier2::T2Origin::Upgrade(tier2::T2Upgrade::Feedback))
    .then_some(leaf)
}

fn cache_diagnostic(value: Value) -> String {
    let runtime = source(value).jit_runtime();
    let Some(id) = runtime.compiled_id() else {
        return format!("no compiled_id; heat={}", runtime.heat());
    };
    let Some(ptr) = cache::compiled_leaf_ptr_for_test(id) else {
        return format!("compiled_id={id}; no current leaf; heat={}", runtime.heat());
    };
    // SAFETY: read-only current-mutator inspection, with no Lisp/cache call.
    let leaf = unsafe { &*ptr };
    format!(
        "id={id} tier={:?} obs={:?} policy={:?} compile_stats={:?}",
        leaf.selected_tier(),
        leaf.obs.snapshot(),
        leaf.obs.t2.policy.borrow(),
        stats::compile_stats_snapshot()
    )
}

fn warm_normal(
    ctx: &mut Context,
    roots: &Roots,
    trigger: Value,
    args: &[Value],
    targets: &[Value],
) -> Vec<Rc<CompiledLeaf>> {
    // This Rust caller has no enclosing bytecode loop Poll. Keep all caller
    // values rooted and drain dead warm allocations outside physical brackets.
    roots.add(&[trigger]);
    roots.add(args);
    roots.add(targets);
    let result_root = push_scratch_gc_root_slot(Value::NIL);
    let began = Instant::now();
    for call in 0..MAX_WARM_CALLS {
        let result = ctx
            .funcall_general(trigger, args.to_vec())
            .expect("ordinary normal prewarm");
        set_scratch_gc_root(result_root, result);
        if call % 2048 == 2047 && ctx.tagged_heap.bytes_since_gc_exact() >= 8 * 1024 * 1024 {
            ctx.gc_collect_exact();
        }
        let leaves = targets
            .iter()
            .copied()
            .map(normal_opt)
            .collect::<Option<Vec<_>>>();
        if let Some(leaves) = leaves {
            for leaf in &leaves {
                assert!(
                    leaf.obs.entry_counted,
                    "entry counting must precede warming"
                );
                roots.add(leaf.reloc_values());
            }
            tracing::info!(target: "neovm_jit", warm_calls = call + 1,
                "physical probe acquired normally warmed Opt leaves");
            return leaves;
        }
        if began.elapsed() >= MAX_WARM_TIME {
            break;
        }
    }
    panic!(
        "normal admission refused within {MAX_WARM_CALLS} calls/{MAX_WARM_TIME:?}: {:?}",
        targets
            .iter()
            .copied()
            .map(cache_diagnostic)
            .collect::<Vec<_>>()
    );
}

fn assert_current(value: Value, leaf: &Rc<CompiledLeaf>) {
    let current = normal_opt(value).expect("same normal Opt cache entry remains installed");
    assert!(
        Rc::ptr_eq(&current, leaf),
        "tier/source cache replacement invalidates sample"
    );
}

#[derive(Clone, Copy, Debug)]
struct LeafCounts {
    entries: u64,
    deopts: u64,
    signals: u64,
    fallback_entries: Option<u64>,
}
fn leaf_counts(leaf: &CompiledLeaf) -> LeafCounts {
    LeafCounts {
        entries: leaf.obs.entries.get(),
        deopts: leaf.obs.deopt_at.get() + leaf.obs.deopt_rerun.get() + leaf.obs.chain_deopts.get(),
        signals: leaf.obs.signals.get(),
        fallback_entries: leaf
            .tier1_fallback
            .borrow()
            .as_ref()
            .map(|t1| t1.obs.entries.get()),
    }
}
fn assert_leaf_delta(leaf: &CompiledLeaf, before: LeafCounts, entries: u64) {
    let after = leaf_counts(leaf);
    assert_eq!(after.entries - before.entries, entries);
    assert_eq!(
        after.deopts, before.deopts,
        "no deopt/fallback in physical bracket"
    );
    assert_eq!(
        after.signals, before.signals,
        "no signal in physical bracket"
    );
    assert_eq!(
        after.fallback_entries, before.fallback_entries,
        "retained T1 must not run"
    );
}

#[derive(Clone, Copy, Debug)]
struct HeapCounts {
    floats: u64,
    conses: u64,
    context_gc: u64,
    heap_gc: usize,
    compiles: u64,
}
fn heap_counts(ctx: &Context) -> HeapCounts {
    assert!(!ctx.gc_driver_active);
    assert!(!ctx.gc_stress);
    assert!(!ctx.tagged_heap.mark_in_progress());
    assert!(!ctx.tagged_heap.sweep_in_progress());
    assert_eq!(ctx.tagged_heap.gc_threshold(), usize::MAX);
    assert!(ctx.tagged_heap.bytes_since_gc_exact() < ctx.tagged_heap.gc_threshold());
    let physical = ctx.tagged_heap.memory_use_counts_snapshot();
    HeapCounts {
        floats: physical[MemoryUseCountSlot::Floats.index()],
        conses: physical[MemoryUseCountSlot::ConsCells.index()],
        context_gc: ctx.gc_count,
        heap_gc: ctx.tagged_heap.gc_collections(),
        compiles: stats::compile_stats_snapshot().total_compiles,
    }
}
fn allocation_delta(before: HeapCounts, after: HeapCounts) -> (u64, u64) {
    assert_eq!(
        after.context_gc, before.context_gc,
        "collection would corrupt lifetime delta"
    );
    assert_eq!(after.heap_gc, before.heap_gc);
    assert_eq!(
        after.compiles, before.compiles,
        "load/compile work leaked into physical bracket"
    );
    (
        after.floats.checked_sub(before.floats).unwrap(),
        after.conses.checked_sub(before.conses).unwrap(),
    )
}

fn freeze_gc_outside_brackets(ctx: &mut Context) {
    // Warming may collect normally. Drain a complete collection before the
    // physical snapshots rather than assuming threshold changes drain it.
    ctx.gc_collect_exact();
    ctx.gc_stress = false;
    ctx.set_gc_threshold(usize::MAX);
    let _ = heap_counts(ctx);
}

fn native_ok(ctx: &mut Context, value: Value, leaf: &CompiledLeaf, args: &[Value]) -> Value {
    match leaf.call_consts(
        ctx as *mut Context as *mut u8,
        source(value).jit_constant_base(),
        args,
    ) {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        other => panic!("each measured call must finish in the retained native leaf: {other:?}"),
    }
}

fn check_float_identities(ctx: &mut Context, fixture: Fixture, roots: &Roots, leaf: &CompiledLeaf) {
    let seed = source(fixture.float()).constants[0];
    let zero_a = native_ok(ctx, fixture.float(), leaf, &[Value::make_int(0)]);
    roots.add(&[zero_a]);
    let zero_b = native_ok(ctx, fixture.float(), leaf, &[Value::make_int(0)]);
    roots.add(&[zero_b]);
    assert_eq!(
        zero_a, seed,
        "GNU zero-trip result is the original pool Float"
    );
    assert_eq!(zero_b, seed);
    let one_a = native_ok(ctx, fixture.float(), leaf, &[Value::make_int(1)]);
    roots.add(&[one_a]);
    let one_b = native_ok(ctx, fixture.float(), leaf, &[Value::make_int(1)]);
    roots.add(&[one_b]);
    assert_ne!(one_a.bits(), seed.bits());
    assert_ne!(
        one_a.bits(),
        one_b.bits(),
        "equal arithmetic results are distinct GNU objects"
    );
    assert_eq!(float_bits(one_a), expected_float(1));
    assert_eq!(float_bits(one_b), expected_float(1));
}

fn float_bracket(
    ctx: &mut Context,
    fixture: Fixture,
    leaf: &Rc<CompiledLeaf>,
    n: i64,
    result_roots: &[usize; CALLS],
) -> u64 {
    assert_current(fixture.float(), leaf);
    let args = [Value::make_int(n)];
    let leaf_before = leaf_counts(leaf);
    let before = heap_counts(ctx);
    let mut addresses = [0_usize; CALLS];
    let mut scalar_bits = [0_u64; CALLS];
    for index in 0..CALLS {
        let result = native_ok(ctx, fixture.float(), leaf, &args);
        set_scratch_gc_root(result_roots[index], result);
        addresses[index] = result.bits();
        scalar_bits[index] = float_bits(result);
    }
    let after = heap_counts(ctx);
    let (floats, conses) = allocation_delta(before, after);
    assert_leaf_delta(leaf, leaf_before, CALLS as u64);
    assert_current(fixture.float(), leaf);
    assert_eq!(
        conses, 0,
        "the exact float recurrence has no cons operation"
    );
    for index in 0..CALLS {
        assert_eq!(
            scalar_bits[index],
            expected_float(n),
            "frozen GNU result {index}"
        );
        assert!(
            !addresses[..index].contains(&addresses[index]),
            "32 entered results remain distinct"
        );
    }
    floats
}

fn float_probe(sink: bool) -> (u64, u64) {
    // Invocation-owned tracing only; physical brackets exclude this setup.
    let _allocation_log = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_ansi(false)
            .without_time()
            .with_env_filter("off,neovm_jit_allocation_probe=info")
            .finish(),
    );
    let _settings = Settings::enter(sink);
    let mut ctx = Context::new();
    let roots = Roots::enter();
    let fixture = load_fixture(&mut ctx, &roots);
    for &(n, expected) in &oracle::FLOAT_ANSWERS {
        let original = tier0(&mut ctx, fixture.float(), &[Value::make_int(n)]);
        roots.add(&[original]);
        assert_eq!(
            float_bits(original),
            expected,
            "input-valid original GNU body"
        );
    }
    let leaves = warm_normal(
        &mut ctx,
        &roots,
        fixture.float(),
        &[Value::make_int(128)],
        &[fixture.float()],
    );
    let leaf = &leaves[0];
    check_float_identities(&mut ctx, fixture, &roots, leaf);
    let low_roots = std::array::from_fn(|_| push_scratch_gc_root_slot(Value::NIL));
    let high_roots = std::array::from_fn(|_| push_scratch_gc_root_slot(Value::NIL));
    freeze_gc_outside_brackets(&mut ctx);
    let low = float_bracket(&mut ctx, fixture, leaf, 128, &low_roots);
    let high = float_bracket(&mut ctx, fixture, leaf, 1024, &high_roots);
    tracing::info!(target: "neovm_jit_allocation_probe", sink, low, high,
        iterations_difference = CALLS * (1024 - 128), "physical float loop allocation probe");
    (low, high)
}

fn fresh_system(ctx: &mut Context, fixture: Fixture, roots: &Roots) -> Value {
    let system = ctx
        .funcall_general(fixture.setup(), Vec::<Value>::new())
        .expect("actual GNU setup");
    roots.add(&[system]);
    ctx.funcall_general(fixture.offset(), vec![system])
        .expect("actual GNU offset-momentum");
    system
}

fn assert_nbody_answer(ctx: &mut Context, fixture: Fixture, system: Value, steps: usize) {
    let &(_, energy_bits, slots) = oracle::NBODY_ANSWERS
        .iter()
        .find(|&&(count, _, _)| count == steps)
        .unwrap();
    let bodies = list_to_vec(&system).expect("proper five-body list");
    assert_eq!(bodies.len(), 5);
    for (body, value) in bodies.into_iter().enumerate() {
        let fields = value.as_vector_data().expect("actual seven-slot body");
        assert_eq!(fields.len(), 7);
        for field in 0..7 {
            assert_eq!(float_bits(fields[field]), slots[7 * body + field]);
        }
    }
    // Outside the snapshot; energy intentionally allocates Float objects.
    let energy = tier0(ctx, fixture.energy(), &[system]);
    assert_eq!(float_bits(energy), energy_bits, "frozen GNU total energy");
}

fn nbody_bracket(
    ctx: &mut Context,
    fixture: Fixture,
    advance: &Rc<CompiledLeaf>,
    forces: &Rc<CompiledLeaf>,
    system: Value,
    dt: Value,
    steps: usize,
) -> u64 {
    assert_current(fixture.advance(), advance);
    assert_current(fixture.forces(), forces);
    let args = [system, dt];
    let advance_before = leaf_counts(advance);
    let forces_before = leaf_counts(forces);
    let before = heap_counts(ctx);
    for _ in 0..steps {
        assert_eq!(
            native_ok(ctx, fixture.advance(), advance, &args),
            Value::NIL,
            "original GNU advance return value"
        );
    }
    let after = heap_counts(ctx);
    let (floats, conses) = allocation_delta(before, after);
    assert_leaf_delta(advance, advance_before, steps as u64);
    assert_leaf_delta(forces, forces_before, 10 * steps as u64);
    assert_current(fixture.advance(), advance);
    assert_current(fixture.forces(), forces);
    assert_eq!(
        conses, 0,
        "setup/GC report lists are outside the actual advance bracket"
    );
    assert_nbody_answer(ctx, fixture, system, steps);
    floats
}

fn nbody_probe(sink: bool) -> (u64, u64) {
    // Invocation-owned tracing only; physical brackets exclude this setup.
    let _allocation_log = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_ansi(false)
            .without_time()
            .with_env_filter("off,neovm_jit_allocation_probe=info")
            .finish(),
    );
    let _settings = Settings::enter(sink);
    let mut ctx = Context::new();
    let roots = Roots::enter();
    let fixture = load_fixture(&mut ctx, &roots);
    let dt = Value::make_float(0.01);
    roots.add(&[dt]);
    let warm = fresh_system(&mut ctx, fixture, &roots);
    let leaves = warm_normal(
        &mut ctx,
        &roots,
        fixture.advance(),
        &[warm, dt],
        &[fixture.advance(), fixture.forces()],
    );
    let low = fresh_system(&mut ctx, fixture, &roots);
    let high = fresh_system(&mut ctx, fixture, &roots);
    assert_ne!(low.bits(), high.bits());
    let low_bodies = list_to_vec(&low).unwrap();
    let high_bodies = list_to_vec(&high).unwrap();
    for (&a, &b) in low_bodies.iter().zip(&high_bodies) {
        assert_ne!(a.bits(), b.bits());
    }
    assert_nbody_answer(&mut ctx, fixture, low, 0);
    assert_nbody_answer(&mut ctx, fixture, high, 0);
    freeze_gc_outside_brackets(&mut ctx);
    let delta100 = nbody_bracket(&mut ctx, fixture, &leaves[0], &leaves[1], low, dt, 100);
    let delta400 = nbody_bracket(&mut ctx, fixture, &leaves[0], &leaves[1], high, dt, 400);
    tracing::info!(target: "neovm_jit_allocation_probe", sink, delta100, delta400,
        steps_difference = 300, "actual normal-cache GNU nbody physical Float allocation probe");
    (delta100, delta400)
}

#[test]
fn opt_sink_allocation_fixture_load_preserves_frozen_gnu_graph_and_answers() {
    let _settings = Settings::enter(false);
    let mut ctx = Context::new();
    let roots = Roots::enter();
    let fixture = load_fixture(&mut ctx, &roots);
    for &(n, expected) in &oracle::FLOAT_ANSWERS {
        let actual = tier0(&mut ctx, fixture.float(), &[Value::make_int(n)]);
        roots.add(&[actual]);
        assert_eq!(float_bits(actual), expected);
    }
    let zero = tier0(&mut ctx, fixture.float(), &[Value::make_int(0)]);
    assert_eq!(zero.bits(), source(fixture.float()).constants[0].bits());
    let dt = Value::make_float(0.01);
    roots.add(&[dt]);
    for &(steps, _, _) in &oracle::NBODY_ANSWERS {
        let system = fresh_system(&mut ctx, fixture, &roots);
        for _ in 0..steps {
            assert_eq!(
                tier0(&mut ctx, fixture.advance(), &[system, dt]),
                Value::NIL
            );
        }
        assert_nbody_answer(&mut ctx, fixture, system, steps);
    }
}

#[test]
fn opt_sink_allocation_float_normal_cache_predecessor_physical_slope() {
    let (low, high) = float_probe(false);
    assert!(
        high > low,
        "input-valid predecessor slope must be observed, not assumed"
    );
}

#[test]
fn opt_sink_allocation_float_normal_cache_selected_loop_slope_zero() {
    let (low, high) = float_probe(true);
    assert_eq!(
        high, low,
        "target: zero physical loop boxes per iteration across 32 equal-call brackets"
    );
    assert!(
        low >= CALLS as u64,
        "entered observable Float results remain allocated"
    );
}

#[test]
fn opt_sink_allocation_nbody_normal_cache_predecessor_physical_slope() {
    let (low, high) = nbody_probe(false);
    assert!(high > low);
    assert_eq!(
        (high - low) % 300,
        0,
        "report measured whole Float objects per actual advance step"
    );
    // No 95 assertion: that is the design's estimate, not frozen GNU or a measurement.
}

#[test]
fn opt_sink_allocation_nbody_normal_cache_selected_physical_slope_75() {
    let (low, high) = nbody_probe(true);
    assert_eq!(
        high - low,
        75 * 300,
        "design target: 6 observable velocity stores per pair plus 15 position stores per step"
    );
}
