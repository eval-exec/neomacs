//! O3.6 semantic fixtures. Exact loaded GNU bodies and
//! independent sealed Tier-0 programs precede every selected-feature assertion.
//! Threading: contexts, compiler-only plans/overrides and scratch-root scopes
//! belong to this invocation's mutator; no new runtime state is introduced.

use super::compile_pipeline_tests::function;
use super::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{
    bytecode_branch_poll_count, push_scratch_gc_roots, reset_bytecode_branch_poll_count,
    restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::opt::{build, eval, ir, mem, types::TypeSet};
use crate::emacs_core::load::{self, LoadOptions, MissingFilePolicy};
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::{ValueKind, list_to_vec};
use crate::tagged::gc::MemoryUseCountSlot;
use std::path::Path;

#[path = "fixtures/opt_sink_identity_oracle.rs"]
mod oracle;

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_flonum_mode_for_test(Some(FlonumMode::Resident));
        force_deopt_for_test(false);
        Self::sink(false);
        Self
    }
    fn sink(on: bool) {
        force_opt_passes_for_test(Some(OptPasses {
            sink: on,
            ..OptPasses::ALL
        }));
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
        force_flonum_mode_for_test(None);
        force_deopt_for_test(false);
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

fn load(ctx: &mut Context, roots: &Roots) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/emacs_core/runtime/jit/compile/tests/fixtures/opt_sink_identity.el");
    load::load_file_with_options(
        ctx,
        &path,
        LoadOptions::implicit_dependency(MissingFilePolicy::Signal),
    )
    .expect("exact primitive GNU loader must work without bootstrap");
    for spec in oracle::FUNCTIONS {
        let value = named(ctx, spec.name);
        roots.add(&[value]);
        let source = value.get_bytecode_data().unwrap();
        assert_eq!(source.arglist, Value::make_int(spec.descriptor));
        assert_eq!(source.max_stack, spec.max_stack);
        assert_eq!(
            source.gnu_bytecode_bytes.as_ref().unwrap().as_slice(),
            spec.bytes
        );
        assert_eq!(source.constants.len(), spec.constants.len());
        for (&actual, &expected) in source.constants.iter().zip(spec.constants) {
            match expected {
                oracle::Constant::Integer(n) => assert_eq!(actual, Value::make_int(n)),
                oracle::Constant::Float(bits) => assert_eq!(number(actual), Number::Float(bits)),
                oracle::Constant::Symbol(name) => assert_eq!(actual, Value::symbol(name)),
            }
        }
        assert!(!source.executable_ops().is_empty());
    }
}
fn named(ctx: &Context, name: &str) -> Value {
    let value = ctx
        .obarray
        .symbol_function(name)
        .expect("frozen named function");
    assert!(value.get_bytecode_data().is_some());
    value
}
fn source(value: Value) -> &'static ByteCodeFunction {
    value.get_bytecode_data().unwrap()
}

fn original_plan(source: &ByteCodeFunction) -> ir::Func {
    let ops = source.executable_ops();
    let arity = source.params.required.len();
    let cfg = analyze_cfg(
        ops,
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        arity,
    )
    .unwrap();
    let consts = source
        .constants
        .iter()
        .copied()
        .map(ir::ValueBits::from_value)
        .collect::<Vec<_>>();
    let plan = build::build(build::BuildInput {
        ops,
        constants: &consts,
        cfg: &cfg,
        params: ir::ParamShape {
            required: arity,
            ..Default::default()
        },
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })
    .unwrap();
    plan.verify()
        .expect("exact original builder input verifies");
    plan
}
fn prepared(source: &ByteCodeFunction, sink: bool) -> ir::Func {
    prepared_with_reps(source, sink, true)
}

// The retained Opaque Rem can carry a native RawFixnum into a Reps-created
// TaggedFix guard view in the predecessor. That unrelated admission refusal
// is preserved; these two identity/GC fixtures isolate Sink with Reps off.
// The other identity fixtures and normal physical probes use the full prefix.
fn prepared_without_reps(source: &ByteCodeFunction, sink: bool) -> ir::Func {
    prepared_with_reps(source, sink, false)
}

fn prepared_with_reps(source: &ByteCodeFunction, sink: bool, reps: bool) -> ir::Func {
    force_opt_passes_for_test(Some(OptPasses {
        sink,
        reps,
        ..OptPasses::ALL
    }));
    let _snapshot = publish_numeric_feedback(source); // observed Tier-0 types, never fabricated
    let ops = source.executable_ops();
    let arity = source.params.required.len();
    let cfg = analyze_cfg(
        ops,
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        arity,
    )
    .unwrap();
    let build_plan = || {
        opt_backend::build_plan(
            ops,
            &source.constants,
            &cfg,
            ir::ParamShape {
                required: arity,
                ..Default::default()
            },
            0,
            None,
        )
    };
    let plan = build_plan().unwrap_or_else(|error| {
        // Recover the exact independent verifier error from the ordinary
        // prefix; production admission intentionally returns a short reason.
        assert!(sink, "ordinary native prefix failed: {error:?}");
        force_opt_passes_for_test(Some(OptPasses {
            sink: false,
            reps,
            ..OptPasses::ALL
        }));
        let mut prefix = build_plan().expect("ordinary prefix precedes Sink diagnostic");
        force_opt_passes_for_test(Some(OptPasses {
            sink: true,
            reps,
            ..OptPasses::ALL
        }));
        let feedback = (0..ops.len())
            .map(active_numeric_feedback)
            .collect::<Vec<_>>();
        let detailed = crate::emacs_core::jit::opt::passes::sink::run(&mut prefix, &feedback);
        panic!(
            "actual Sink admission error: {error:?}; detailed={detailed:?}; plan={}",
            prefix.display()
        );
    });
    plan.verify()
        .expect("actual front pipeline candidate verifies");
    plan
}
fn lower(source: &ByteCodeFunction, plan: &ir::Func, roots: &Roots) -> CompiledLeaf {
    plan.verify().unwrap();
    let _snapshot = publish_numeric_feedback(source);
    let leaf = lower_opt_ir_for_test(
        source.executable_ops(),
        &source.constants,
        source.params.required.len(),
        source.executable_gnu_byte_offset_map(),
        plan,
    )
    .unwrap_or_else(|error| {
        panic!(
            "semantic fixtures require actual native Opt lowering: {error:?}; ops={:?}; plan={}",
            source.executable_ops(),
            plan.display()
        )
    });
    assert_eq!(leaf.tier(), LeafTier::Opt);
    roots.add(leaf.reloc_values());
    leaf
}
fn tier0(ctx: &mut Context, source: &ByteCodeFunction, args: &[Value]) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(source, args.to_vec())
}
fn reference(ctx: &mut Context, plan: &ir::Func, args: &[Value]) -> Value {
    let run = eval::evaluate(
        plan,
        ctx,
        eval::Inputs {
            args,
            ..Default::default()
        },
    )
    .unwrap();
    let eval::Outcome::Returned(value) = run.outcome else {
        panic!("reference must return");
    };
    value.to_value()
}
fn native(
    ctx: &mut Context,
    source: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    args: &[Value],
) -> Value {
    match leaf.call_consts(
        ctx as *mut Context as *mut u8,
        source.jit_constant_base(),
        args,
    ) {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        other => panic!("actual native success required: {other:?}"),
    }
}
fn recipe_count(plan: &ir::Func) -> usize {
    plan.values
        .iter()
        .filter(|v| {
            matches!(
                v.rep,
                ir::Rep::RawF64 | ir::Rep::NumPair | ir::Rep::Virtual(_)
            )
        })
        .count()
}
fn selected_feature(base: &ir::Func, selected: &ir::Func) {
    assert!(
        recipe_count(selected) > recipe_count(base),
        "selected Sink must produce actual scalar/virtual identity recipes after semantic parity"
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Number {
    Integer(i64),
    Float(u64),
    Other(usize),
}
fn number(value: Value) -> Number {
    match value.kind() {
        ValueKind::Float => Number::Float(value.xfloat().to_bits()),
        ValueKind::Fixnum(n) => Number::Integer(n),
        _ => Number::Other(value.bits()),
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct PairView {
    car: Number,
    cdr: Number,
    same: bool,
    car_seed: bool,
    cdr_seed: bool,
}
fn pair(value: Value, seed: Value) -> PairView {
    assert!(value.is_cons());
    let (car, cdr) = (value.cons_car(), value.cons_cdr());
    PairView {
        car: number(car),
        cdr: number(cdr),
        same: car.bits() == cdr.bits(),
        car_seed: car.bits() == seed.bits(),
        cdr_seed: cdr.bits() == seed.bits(),
    }
}
fn no_gc(ctx: &mut Context) {
    ctx.gc_stress = false;
    ctx.set_gc_threshold(usize::MAX);
}

#[test]
fn opt_sink_native_float_loop_alias_and_lag_identity() {
    let _settings = Settings::enter();
    let mut features = Vec::new();
    let mut ctx = Context::new();
    let roots = Roots::enter();
    load(&mut ctx, &roots);
    no_gc(&mut ctx);
    let seed = Value::make_float(7.0);
    roots.add(&[seed]);
    let step = Value::make_float(0.0);
    roots.add(&[step]);
    for name in ["t34-o36-loop-add", "t34-o36-loop-lag"] {
        let function = named(&ctx, name);
        let src = source(function);
        let original = original_plan(src);
        // These observations provide authentic per-site Float feedback first.
        for n in [0, 1, 255, 510] {
            let args = [seed, step, Value::make_int(n)];
            roots.add(&args);
            let expected = tier0(&mut ctx, src, &args).unwrap();
            roots.add(&[expected]);
            let observed = pair(expected, seed);
            assert_eq!(observed.car, Number::Float(7.0_f64.to_bits()));
            assert_eq!(observed.cdr, Number::Float(7.0_f64.to_bits()));
            assert_eq!(observed.same, n == 0 || name.ends_with("add"));
            assert_eq!(observed.car_seed, n == 0);
            assert_eq!(
                observed.cdr_seed,
                n == 0 || (name.ends_with("lag") && n == 1)
            );
            let reference = reference(&mut ctx, &original, &args);
            roots.add(&[reference]);
            assert_eq!(pair(reference, seed), observed);
        }
        let base = prepared(src, false);
        let base_leaf = lower(src, &base, &roots);
        let selected = prepared(src, true);
        let selected_leaf = lower(src, &selected, &roots);
        for n in [0, 1, 255, 510] {
            let args = [seed, step, Value::make_int(n)];
            let expected = tier0(&mut ctx, src, &args).unwrap();
            roots.add(&[expected]);
            let expected = pair(expected, seed);
            let actual = native(&mut ctx, src, &base_leaf, &args);
            roots.add(&[actual]);
            assert_eq!(
                pair(actual, seed),
                expected,
                "native baseline before selected assertions"
            );
            let ref_selected = reference(&mut ctx, &selected, &args);
            roots.add(&[ref_selected]);
            assert_eq!(pair(ref_selected, seed), expected);
            let actual = native(&mut ctx, src, &selected_leaf, &args);
            roots.add(&[actual]);
            assert_eq!(pair(actual, seed), expected);
        }
        features.push((base, selected));
    }
    // The original body returns an unproven heap seed on zero trips. A body-
    // only numeric guard must not eagerly unbox this unknown entry identity.
    let heap_seed = Value::list(vec![Value::symbol("heap-seed")]);
    roots.add(&[heap_seed]);
    let src = source(named(&ctx, "t34-o36-loop-add"));
    let selected = prepared(src, true);
    let leaf = lower(src, &selected, &roots);
    let args = [heap_seed, Value::make_int(1), Value::make_int(0)];
    let original = tier0(&mut ctx, src, &args).unwrap();
    roots.add(&[original]);
    let original_ir = original_plan(src);
    let observed = reference(&mut ctx, &original_ir, &args);
    roots.add(&[observed]);
    assert_eq!(pair(observed, heap_seed), pair(original, heap_seed));
    let base = prepared(src, false);
    let base_leaf = lower(src, &base, &roots);
    let actual = native(&mut ctx, src, &base_leaf, &args);
    roots.add(&[actual]);
    assert_eq!(pair(actual, heap_seed), pair(original, heap_seed));
    let actual = native(&mut ctx, src, &leaf, &args);
    roots.add(&[actual]);
    assert_eq!(pair(actual, heap_seed), pair(original, heap_seed));
    for (base, selected) in &features {
        selected_feature(base, selected);
    }
}

#[test]
fn opt_sink_native_float_mixed_box_phi_and_partial_escape() {
    let _settings = Settings::enter();
    let mut features = Vec::new();
    let mut ctx = Context::new();
    let roots = Roots::enter();
    load(&mut ctx, &roots);
    no_gc(&mut ctx);
    let seed = Value::make_float(7.0);
    roots.add(&[seed]);
    for name in [
        "t34-o36-loop-mixed",
        "t34-o36-loop-existing-new",
        "t34-o36-loop-partial-escape",
    ] {
        let src = source(named(&ctx, name));
        let original = original_plan(src);
        let cell = Value::cons(seed, Value::NIL);
        roots.add(&[cell]);
        let args_for = |n| {
            if name.ends_with("partial-escape") {
                vec![seed, Value::make_int(n), cell]
            } else {
                vec![seed, Value::make_int(n)]
            }
        };
        for n in [0, 1, 2, 3, 255, 510] {
            cell.set_car(seed);
            let args = args_for(n);
            let expected = tier0(&mut ctx, src, &args).unwrap();
            roots.add(&[expected]);
            let expected_view = pair(expected, seed);
            assert!(expected_view.same, "shared conditional aliases");
            if name.ends_with("mixed") && n > 0 && n % 2 == 0 {
                assert_eq!(expected_view.car, Number::Integer(n - 1));
            }
            let stored = cell.cons_car();
            let stored_matches = expected.cons_car().bits() == stored.bits();
            cell.set_car(seed);
            let reference = reference(&mut ctx, &original, &args);
            roots.add(&[reference]);
            assert_eq!(pair(reference, seed), expected_view);
            assert_eq!(
                reference.cons_car().bits() == cell.cons_car().bits(),
                stored_matches
            );
        }
        let base = prepared_without_reps(src, false);
        let base_leaf = lower(src, &base, &roots);
        let selected = prepared_without_reps(src, true);
        let selected_leaf = lower(src, &selected, &roots);
        for n in [0, 1, 2, 3, 255, 510] {
            let args = args_for(n);
            cell.set_car(seed);
            let expected = tier0(&mut ctx, src, &args).unwrap();
            roots.add(&[expected]);
            let expected_view = pair(expected, seed);
            let expected_stored = expected.cons_car().bits() == cell.cons_car().bits();
            for (plan, leaf) in [(&base, &base_leaf), (&selected, &selected_leaf)] {
                cell.set_car(seed);
                let actual = native(&mut ctx, src, leaf, &args);
                roots.add(&[actual]);
                assert_eq!(pair(actual, seed), expected_view);
                assert_eq!(
                    actual.cons_car().bits() == cell.cons_car().bits(),
                    expected_stored
                );
                cell.set_car(seed);
                let reference = reference(&mut ctx, plan, &args);
                roots.add(&[reference]);
                assert_eq!(pair(reference, seed), expected_view);
                assert_eq!(
                    reference.cons_car().bits() == cell.cons_car().bits(),
                    expected_stored
                );
            }
        }
        features.push((base, selected));
    }
    for (base, selected) in &features {
        selected_feature(base, selected);
    }
}

fn gc_list_view(result: Value, seed: Value, lag: bool, n: i64) {
    let slots = list_to_vec(&result).unwrap();
    assert_eq!(slots.len(), 3);
    assert_eq!(number(slots[0]), Number::Float(7.0_f64.to_bits()));
    assert_eq!(number(slots[1]), Number::Float(7.0_f64.to_bits()));
    assert_eq!(slots[2].bits(), seed.bits());
    assert_eq!(slots[0].bits() == slots[1].bits(), !lag || n == 0);
    assert_eq!(slots[0].bits() == seed.bits(), n == 0);
    assert_eq!(slots[1].bits() == seed.bits(), n == 0 || (lag && n == 1));
}
fn assert_gc_delta(ctx: &Context, before: (u64, usize), expected: u64) {
    assert_eq!(ctx.gc_count - before.0, expected);
    assert_eq!(
        ctx.tagged_heap.gc_collections() - before.1,
        expected as usize
    );
    assert!(!ctx.tagged_heap.mark_in_progress());
    assert!(!ctx.tagged_heap.sweep_in_progress());
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn opt_sink_native_float_alias_and_distinct_lag_survive_gc() {
    let _settings = Settings::enter();
    let mut features = Vec::new();
    let mut ctx = Context::new();
    let roots = Roots::enter();
    load(&mut ctx, &roots);
    no_gc(&mut ctx);
    let seed = Value::make_float(7.0);
    roots.add(&[seed]);
    for (name, lag) in [
        ("t34-o36-loop-gc-dual", false),
        ("t34-o36-loop-gc-lag", true),
    ] {
        let src = source(named(&ctx, name));
        let original = original_plan(src);
        for n in [255, 510] {
            let before = (ctx.gc_count, ctx.tagged_heap.gc_collections());
            let expected = tier0(&mut ctx, src, &[seed, Value::make_int(n)]).unwrap();
            roots.add(&[expected]);
            gc_list_view(expected, seed, lag, n);
            assert_gc_delta(&ctx, before, (n / 255) as u64);
            let before = (ctx.gc_count, ctx.tagged_heap.gc_collections());
            let reference = reference(&mut ctx, &original, &[seed, Value::make_int(n)]);
            roots.add(&[reference]);
            gc_list_view(reference, seed, lag, n);
            assert_gc_delta(&ctx, before, (n / 255) as u64);
        }
        let base = prepared_without_reps(src, false);
        let base_leaf = lower(src, &base, &roots);
        let selected = prepared_without_reps(src, true);
        let selected_leaf = lower(src, &selected, &roots);
        for n in [255, 510] {
            for (plan, leaf) in [(&base, &base_leaf), (&selected, &selected_leaf)] {
                let before = (ctx.gc_count, ctx.tagged_heap.gc_collections());
                reset_bytecode_branch_poll_count();
                let actual = native(&mut ctx, src, leaf, &[seed, Value::make_int(n)]);
                roots.add(&[actual]);
                gc_list_view(actual, seed, lag, n);
                assert_gc_delta(&ctx, before, (n / 255) as u64);
                assert_eq!(bytecode_branch_poll_count(), (n / 255) as usize);
                let before = (ctx.gc_count, ctx.tagged_heap.gc_collections());
                let reference = reference(&mut ctx, plan, &[seed, Value::make_int(n)]);
                roots.add(&[reference]);
                gc_list_view(reference, seed, lag, n);
                assert_gc_delta(&ctx, before, (n / 255) as u64);
            }
        }
        features.push((base, selected));
    }
    // Independent sealed source: identity consumes/materializes the freshly
    // allocated child; only its parent's recipe field can retain it at GC.
    // No external root is installed for this child's dynamic allocation.
    let one = Value::make_float(1.0);
    roots.add(&[one]);
    let two = Value::make_float(2.0);
    roots.add(&[two]);
    let src = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Nil,
            Op::Cons,
            Op::Call(1),
            Op::Constant(2),
            Op::Constant(3),
            Op::Add,
            Op::Cons,
            Op::StackRef(1),
            Op::Constant(5),
            Op::Gtr,
            Op::GotoIfNil(26),
            Op::StackRef(1),
            Op::Sub1,
            Op::StackSet(2),
            Op::StackRef(1),
            Op::Constant(6),
            Op::Rem,
            Op::Constant(5),
            Op::Eqlsign,
            Op::GotoIfNil(25),
            Op::Constant(4),
            Op::Call(0),
            Op::Pop,
            Op::Goto(9),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![
            Value::symbol("identity"),
            Value::symbol("heap-child"),
            one,
            two,
            Value::symbol("garbage-collect"),
            Value::make_int(0),
            Value::make_int(255),
        ],
        1,
    );
    let original = original_plan(&src);
    let mut observations = Vec::new();
    for n in [255, 510] {
        let args = [Value::make_int(n)];
        let before = (ctx.gc_count, ctx.tagged_heap.gc_collections());
        let expected = tier0(&mut ctx, &src, &args).unwrap();
        roots.add(&[expected]);
        assert_gc_delta(&ctx, before, (n / 255) as u64);
        let expected_print = print_value(&expected);
        let before = (ctx.gc_count, ctx.tagged_heap.gc_collections());
        let observed = reference(&mut ctx, &original, &args);
        roots.add(&[observed]);
        assert_gc_delta(&ctx, before, (n / 255) as u64);
        assert_eq!(print_value(&observed), expected_print);
        observations.push((n, expected_print));
    }
    let base = prepared_without_reps(&src, false);
    let base_leaf = lower(&src, &base, &roots);
    let selected = prepared_without_reps(&src, true);
    let selected_leaf = lower(&src, &selected, &roots);
    for (n, expected_print) in observations {
        for (plan, leaf) in [(&base, &base_leaf), (&selected, &selected_leaf)] {
            let args = [Value::make_int(n)];
            let before = (ctx.gc_count, ctx.tagged_heap.gc_collections());
            reset_bytecode_branch_poll_count();
            let actual = native(&mut ctx, &src, leaf, &args);
            roots.add(&[actual]);
            assert_gc_delta(&ctx, before, (n / 255) as u64);
            assert_eq!(bytecode_branch_poll_count(), (n / 255) as usize);
            assert_eq!(print_value(&actual), expected_print);
            assert!(actual.cons_car().is_cons());
            assert_eq!(actual.cons_car().cons_car(), Value::symbol("heap-child"));
            assert_eq!(number(actual.cons_cdr()), Number::Float(3.0_f64.to_bits()));
            let before = (ctx.gc_count, ctx.tagged_heap.gc_collections());
            let observed = reference(&mut ctx, plan, &args);
            roots.add(&[observed]);
            assert_gc_delta(&ctx, before, (n / 255) as u64);
            assert_eq!(print_value(&observed), expected_print);
        }
    }
    features.push((base, selected));
    for (base, selected) in &features {
        selected_feature(base, selected);
    }
}

fn cons_escape_view(result: Value, cell: Value, a: Value, b: Value) {
    let values = list_to_vec(&result).unwrap();
    assert_eq!(values.len(), 6);
    assert!(values[0].is_cons());
    assert_eq!(values[0].bits(), values[1].bits());
    assert_eq!(values[0].bits(), values[2].bits());
    assert_eq!(cell.cons_car().bits(), values[0].bits());
    assert_eq!(values[0].cons_car().bits(), a.bits());
    assert_eq!(values[0].cons_cdr().bits(), b.bits());
    assert_eq!(&values[3..], &[Value::T; 3]);
}

#[test]
fn opt_sink_native_virtual_cons_list1_eliminated_or_escaped() {
    let _settings = Settings::enter();
    let mut features = Vec::new();
    let mut physical_deltas = Vec::new();
    let mut ctx = Context::new();
    let roots = Roots::enter();
    load(&mut ctx, &roots);
    no_gc(&mut ctx);
    let local = source(named(&ctx, "t34-o36-local-cons"));
    let args = [Value::make_int(3), Value::make_int(4)];
    let original = original_plan(local);
    assert_eq!(tier0(&mut ctx, local, &args).unwrap(), Value::make_int(7));
    assert_eq!(reference(&mut ctx, &original, &args), Value::make_int(7));
    let base = prepared(local, false);
    let base_leaf = lower(local, &base, &roots);
    let selected = prepared(local, true);
    let selected_leaf = lower(local, &selected, &roots);
    assert_eq!(
        native(&mut ctx, local, &base_leaf, &args),
        Value::make_int(7)
    );
    let before =
        ctx.tagged_heap.memory_use_counts_snapshot()[MemoryUseCountSlot::ConsCells.index()];
    assert_eq!(
        native(&mut ctx, local, &selected_leaf, &args),
        Value::make_int(7)
    );
    let after = ctx.tagged_heap.memory_use_counts_snapshot()[MemoryUseCountSlot::ConsCells.index()];
    physical_deltas.push(("nonescaping Cons", after - before));
    features.push((base, selected));
    // Sealed independent List1+Car source; GNU List1 is Fcons(arg,nil).
    let child = Value::list(vec![Value::symbol("car-child")]);
    roots.add(&[child]);
    let list1 = function(
        vec![Op::StackRef(0), Op::List(1), Op::Car, Op::Return],
        vec![],
        1,
    );
    let original = original_plan(&list1);
    assert_eq!(
        tier0(&mut ctx, &list1, &[child]).unwrap().bits(),
        child.bits()
    );
    assert_eq!(
        reference(&mut ctx, &original, &[child]).bits(),
        child.bits()
    );
    let base = prepared(&list1, false);
    let base_leaf = lower(&list1, &base, &roots);
    let selected = prepared(&list1, true);
    let selected_leaf = lower(&list1, &selected, &roots);
    assert_eq!(
        native(&mut ctx, &list1, &base_leaf, &[child]).bits(),
        child.bits()
    );
    let before =
        ctx.tagged_heap.memory_use_counts_snapshot()[MemoryUseCountSlot::ConsCells.index()];
    assert_eq!(
        native(&mut ctx, &list1, &selected_leaf, &[child]).bits(),
        child.bits()
    );
    let after = ctx.tagged_heap.memory_use_counts_snapshot()[MemoryUseCountSlot::ConsCells.index()];
    physical_deltas.push(("nonescaping List1", after - before));
    features.push((base, selected));
    let b = Value::list(vec![Value::symbol("cdr-child")]);
    roots.add(&[b]);
    let cell = Value::cons(Value::NIL, Value::NIL);
    roots.add(&[cell]);
    let escape = source(named(&ctx, "t34-o36-cons-escape"));
    let args = [child, b, cell, Value::symbol("identity")];
    roots.add(&args);
    let expected = tier0(&mut ctx, escape, &args).unwrap();
    roots.add(&[expected]);
    cons_escape_view(expected, cell, child, b);
    let original = original_plan(escape);
    cell.set_car(Value::NIL);
    let observed = reference(&mut ctx, &original, &args);
    roots.add(&[observed]);
    cons_escape_view(observed, cell, child, b);
    let base = prepared(escape, false);
    let base_leaf = lower(escape, &base, &roots);
    let selected = prepared(escape, true);
    let selected_leaf = lower(escape, &selected, &roots);
    for leaf in [&base_leaf, &selected_leaf] {
        cell.set_car(Value::NIL);
        let actual = native(&mut ctx, escape, leaf, &args);
        roots.add(&[actual]);
        cons_escape_view(actual, cell, child, b);
    }
    // The exact frozen GNU return-List1 and two-Cons identity bodies cover
    // separate escaping allocations, whose payloads may be equal.
    let list1 = source(named(&ctx, "t34-o36-return-list1"));
    let original = original_plan(list1);
    let expected_a = tier0(&mut ctx, list1, &[child]).unwrap();
    roots.add(&[expected_a]);
    let expected_b = tier0(&mut ctx, list1, &[child]).unwrap();
    roots.add(&[expected_b]);
    assert_ne!(expected_a.bits(), expected_b.bits());
    assert_eq!(expected_a.cons_car().bits(), child.bits());
    assert!(expected_a.cons_cdr().is_nil());
    let observed = reference(&mut ctx, &original, &[child]);
    roots.add(&[observed]);
    assert_eq!(observed.cons_car().bits(), child.bits());
    assert!(observed.cons_cdr().is_nil());
    let base = prepared(list1, false);
    let base_leaf = lower(list1, &base, &roots);
    let selected = prepared(list1, true);
    let selected_leaf = lower(list1, &selected, &roots);
    for leaf in [&base_leaf, &selected_leaf] {
        let a = native(&mut ctx, list1, leaf, &[child]);
        roots.add(&[a]);
        let b = native(&mut ctx, list1, leaf, &[child]);
        roots.add(&[b]);
        assert_ne!(a.bits(), b.bits());
        assert_eq!(a.cons_car().bits(), child.bits());
        assert!(a.cons_cdr().is_nil());
        assert_eq!(b.cons_car().bits(), child.bits());
        assert!(b.cons_cdr().is_nil());
    }
    let identity = source(named(&ctx, "t34-o36-cons-identity"));
    let original = original_plan(identity);
    let expected = tier0(&mut ctx, identity, &[child, b]).unwrap();
    roots.add(&[expected]);
    assert_eq!(
        list_to_vec(&expected).unwrap(),
        vec![Value::T, Value::NIL, Value::T, Value::T]
    );
    let observed = reference(&mut ctx, &original, &[child, b]);
    roots.add(&[observed]);
    assert_eq!(print_value(&observed), print_value(&expected));
    let base = prepared(identity, false);
    let base_leaf = lower(identity, &base, &roots);
    let selected = prepared(identity, true);
    let selected_leaf = lower(identity, &selected, &roots);
    for leaf in [&base_leaf, &selected_leaf] {
        let actual = native(&mut ctx, identity, leaf, &[child, b]);
        roots.add(&[actual]);
        assert_eq!(print_value(&actual), print_value(&expected));
    }
    for (name, delta) in physical_deltas {
        assert_eq!(delta, 0, "{name} must actually sink");
    }
    for (base, selected) in &features {
        selected_feature(base, selected);
    }
}

fn guard_before_comparison(plan: &mut ir::Func) -> (u32, ir::FrameId) {
    let target = plan
        .insts
        .iter()
        .position(|inst| {
            matches!(
                &inst.op,
                ir::Opcode::Opaque(Op::Lss) | ir::Opcode::OpaqueBool(Op::Lss)
            )
        })
        .expect("actual original < site");
    let pc = plan.insts[target].pc;
    let frame = plan.insts[target].frame.unwrap();
    let input = plan.insts[target].args[0];
    assert!(plan.values[input.index()].rep.is_tagged());
    let inst = ir::Inst(plan.insts.len() as u32);
    let result = ir::Value(plan.values.len() as u32);
    plan.values.push(ir::ValueData {
        ty: TypeSet::FIXNUM,
        rep: ir::Rep::TaggedFix,
        def: ir::ValueDef::Inst(inst),
    });
    plan.insts.push(ir::InstData {
        op: ir::Opcode::CheckType(TypeSet::FIXNUM),
        args: vec![input],
        result: Some(result),
        eff: mem::Effects::MAY_DEOPT,
        mem: mem::AliasClass::None,
        frame: Some(frame),
        pc,
    });
    let block = plan
        .blocks
        .iter_mut()
        .find(|block| block.insts.contains(&ir::Inst(target as u32)))
        .unwrap();
    let position = block
        .insts
        .iter()
        .position(|&id| id == ir::Inst(target as u32))
        .unwrap();
    block.insts.insert(position, inst);
    plan.verify()
        .expect("explicit narrow guard is valid without changing original comparison/effects");
    (pc, frame)
}
fn assert_frame(stack: &[Value], args: &[Value]) {
    // Exact GNU virtual-fail pre-< stack, independently decoded by Tier-0 and
    // original reference. New cons/float bits are compared only within a run.
    assert_eq!(stack.len(), 10);
    for slot in 0..4 {
        assert_eq!(stack[slot].bits(), args[slot].bits());
    }
    assert!(stack[4].is_cons());
    assert_eq!(stack[4].cons_car().bits(), args[3].bits());
    assert_eq!(stack[4].cons_cdr().bits(), args[1].bits());
    assert_eq!(number(stack[5]), Number::Float(3.0_f64.to_bits()));
    assert_eq!(stack[6].bits(), stack[4].bits());
    assert_eq!(stack[7].bits(), stack[5].bits());
    assert_eq!(stack[8].bits(), args[0].bits());
    assert_eq!(stack[9], Value::make_int(1));
}
fn side(ctx: &Context, payload: Value) {
    let value = *ctx.obarray.symbol_value("t34-o36-side").unwrap();
    let items = list_to_vec(&value).unwrap();
    assert_eq!(items, vec![Value::symbol("stored"), payload]);
}
fn resumed(ctx: &mut Context, source_value: Value, resume: &DeoptResume) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.run_resumed_frame(
        source(source_value),
        source_value,
        resume.pc,
        &resume.stack,
        resume.handlers,
        &resume.binds,
        resume.spec_base,
        resume.cond_base,
    )
}

#[test]
fn opt_sink_native_virtual_full_frame_after_visible_store() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let roots = Roots::enter();
    load(&mut ctx, &roots);
    no_gc(&mut ctx);
    ctx.eval_str("(setq t34-o36-watch-count 0) (add-variable-watcher 't34-o36-side (lambda (&rest _) (setq t34-o36-watch-count (1+ t34-o36-watch-count))))").unwrap();
    let function = named(&ctx, "t34-o36-virtual-fail");
    let src = source(function);
    let payload = Value::list(vec![Value::symbol("heap-companion")]);
    roots.add(&[payload]);
    let one = Value::make_float(1.0);
    roots.add(&[one]);
    let two = Value::make_float(2.0);
    roots.add(&[two]);
    let bad = Value::make_float(0.5);
    roots.add(&[bad]);
    let args = [bad, one, two, payload];
    roots.add(&args);
    let original = original_plan(src);
    let expected = tier0(&mut ctx, src, &args).unwrap();
    roots.add(&[expected]);
    side(&ctx, payload);
    let expected_print = print_value(&expected);
    let reference = reference(&mut ctx, &original, &args);
    roots.add(&[reference]);
    assert_eq!(print_value(&reference), expected_print);
    let base = prepared(src, false);
    let base_leaf = lower(src, &base, &roots);
    let actual = native(&mut ctx, src, &base_leaf, &args);
    roots.add(&[actual]);
    assert_eq!(
        print_value(&actual),
        expected_print,
        "real native baseline before explicit guard"
    );
    // Add the real original guard before recipe construction so every exact
    // instruction cut is certified by the selected pass.
    let mut selected = prepared(src, false);
    let original_frames = selected.frames.clone();
    let original_states = selected.source_states.clone();
    let (pc, frame) = guard_before_comparison(&mut selected);
    let _feedback_snapshot = publish_numeric_feedback(src);
    let feedback = (0..src.executable_ops().len())
        .map(active_numeric_feedback)
        .collect::<Vec<_>>();
    Settings::sink(true);
    crate::emacs_core::jit::opt::passes::sink::run(&mut selected, &feedback)
        .expect("original verified guard participates in exact recipe cuts");
    assert_eq!(selected.frames, original_frames);
    assert_eq!(selected.source_states.len(), original_states.len());
    for (actual, expected) in selected.source_states.iter().zip(&original_states) {
        match (actual, expected) {
            (Some(actual), Some(expected)) => {
                assert_eq!(actual.pre, expected.pre);
                assert_eq!(actual.post, expected.post);
                assert_eq!(actual.frame, expected.frame);
                assert_eq!(actual.block, expected.block);
            }
            (None, None) => (),
            _ => panic!("source-state presence changed"),
        }
    }
    let reference = eval::evaluate(
        &selected,
        &mut ctx,
        eval::Inputs {
            args: &args,
            ..Default::default()
        },
    )
    .unwrap();
    let eval::Outcome::Deopt(snapshot) = reference.outcome else {
        panic!("explicit reference guard fails before original successful <");
    };
    let reference_stack = snapshot
        .stack
        .iter()
        .map(|bits| bits.to_value())
        .collect::<Vec<_>>();
    roots.add(&reference_stack);
    assert_frame(&reference_stack, &args);
    assert_eq!(snapshot.pc, pc);
    assert_eq!(snapshot.pc, selected.frames[frame.index()].pc);
    let leaf = lower(src, &selected, &roots);
    ctx.obarray
        .set_symbol_value("t34-o36-watch-count", Value::make_int(0));
    let NativeRun::DeoptAt(resume) = leaf.call_consts(
        &mut ctx as *mut Context as *mut u8,
        src.jit_constant_base(),
        &args,
    ) else {
        panic!("explicit native guard must reconstruct full original frame");
    };
    roots.add(&resume.stack);
    assert_eq!(resume.pc, pc as usize);
    assert_frame(&resume.stack, &args);
    side(&ctx, payload);
    assert_eq!(
        ctx.obarray.symbol_value("t34-o36-watch-count").unwrap(),
        &Value::make_int(1)
    );
    let replayed = resumed(&mut ctx, function, &resume).unwrap();
    roots.add(&[replayed]);
    assert_eq!(print_value(&replayed), expected_print);
    assert_eq!(
        ctx.obarray.symbol_value("t34-o36-watch-count").unwrap(),
        &Value::make_int(1),
        "cold replay must not repeat visible pre-guard store"
    );
    // Actual frozen GNU wrong-type input still reports the original signal,
    // not an invented reference Deopt. The same explicit guard resumes to it.
    let error_args = [Value::symbol("wrong"), one, two, payload];
    let expected_error = tier0(&mut ctx, src, &error_args).unwrap_err();
    side(&ctx, payload);
    let reference_error = eval::evaluate(
        &original,
        &mut ctx,
        eval::Inputs {
            args: &error_args,
            ..Default::default()
        },
    )
    .unwrap_err();
    let eval::EvalError::Flow(reference_error) = reference_error else {
        panic!("original opaque reference must report GNU signal");
    };
    assert_eq!(
        flow_observation(&reference_error),
        flow_observation(&expected_error)
    );
    let NativeRun::DeoptAt(resume) = leaf.call_consts(
        &mut ctx as *mut Context as *mut u8,
        src.jit_constant_base(),
        &error_args,
    ) else {
        panic!("wrong-type path must retain explicit original frame");
    };
    roots.add(&resume.stack);
    assert_frame(&resume.stack, &error_args);
    let actual_error = resumed(&mut ctx, function, &resume).unwrap_err();
    assert_eq!(
        flow_observation(&actual_error),
        flow_observation(&expected_error)
    );
    side(&ctx, payload);
    selected_feature(&base, &selected);
}

fn quit_source(one: Value, two: Value, zero_float: Value) -> ByteCodeFunction {
    function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Add,
            Op::StackRef(1),
            Op::Constant(2),
            Op::Gtr,
            Op::GotoIfNil(21),
            Op::StackRef(1),
            Op::Constant(3),
            Op::Eqlsign,
            Op::GotoIfNil(13),
            Op::True,
            Op::VarSet(4),
            Op::StackRef(0),
            Op::Constant(5),
            Op::Add,
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Sub1,
            Op::StackSet(2),
            Op::Goto(3),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![
            one,
            two,
            Value::make_int(0),
            Value::make_int(255),
            Value::symbol("quit-flag"),
            zero_float,
        ],
        1,
    )
}
fn native_flow(
    ctx: &mut Context,
    src: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    args: &[Value],
    roots: &Roots,
) -> Flow {
    match leaf.call(ctx as *mut Context as *mut u8, args) {
        NativeRun::Signal => take_pending_flow().expect("native pending quit"),
        NativeRun::DeoptAt(resume) => {
            roots.add(&resume.stack);
            let mut vm = Vm::from_context(ctx);
            vm.force_interpreter_only_for_test();
            vm.run_resumed_frame(
                src,
                Value::NIL,
                resume.pc,
                &resume.stack,
                resume.handlers,
                &resume.binds,
                resume.spec_base,
                resume.cond_base,
            )
            .unwrap_err()
        }
        other => panic!("quit fixture must retain baseline flow: {other:?}"),
    }
}

fn flow_observation(flow: &Flow) -> (SymId, Vec<String>, Option<String>) {
    let signal = flow.as_signal().expect("observable Lisp signal");
    (
        signal.symbol,
        signal.data.iter().map(print_value).collect(),
        signal.raw_data.as_ref().map(print_value),
    )
}

#[test]
fn opt_sink_native_virtual_poll_roots_and_quit_255_510() {
    let _settings = Settings::enter();
    let mut features = Vec::new();
    let mut ctx = Context::new();
    let roots = Roots::enter();
    no_gc(&mut ctx);
    let one = Value::make_float(1.0);
    roots.add(&[one]);
    let two = Value::make_float(2.0);
    roots.add(&[two]);
    let zero = Value::make_float(0.0);
    roots.add(&[zero]);
    let src = quit_source(one, two, zero);
    let original = original_plan(&src);
    let good = tier0(&mut ctx, &src, &[Value::make_int(0)]).unwrap();
    roots.add(&[good]);
    assert_eq!(number(good), Number::Float(3.0_f64.to_bits()));
    for n in [255, 510] {
        ctx.set_quit_flag_value(Value::NIL);
        reset_bytecode_branch_poll_count();
        let expected = tier0(&mut ctx, &src, &[Value::make_int(n)]).unwrap_err();
        let polls = bytecode_branch_poll_count();
        assert_eq!(polls, (n / 255) as usize);
        ctx.set_quit_flag_value(Value::NIL);
        reset_bytecode_branch_poll_count();
        let reference = eval::evaluate(
            &original,
            &mut ctx,
            eval::Inputs {
                args: &[Value::make_int(n)],
                ..Default::default()
            },
        )
        .unwrap_err();
        let eval::EvalError::Flow(reference) = reference else {
            panic!("reference quit flow");
        };
        assert_eq!(flow_observation(&reference), flow_observation(&expected));
        let base = prepared(&src, false);
        let base_leaf = lower(&src, &base, &roots);
        let selected = prepared(&src, true);
        let selected_leaf = lower(&src, &selected, &roots);
        for leaf in [&base_leaf, &selected_leaf] {
            ctx.set_quit_flag_value(Value::NIL);
            reset_bytecode_branch_poll_count();
            let actual = native_flow(&mut ctx, &src, leaf, &[Value::make_int(n)], &roots);
            assert_eq!(flow_observation(&actual), flow_observation(&expected));
            assert_eq!(bytecode_branch_poll_count(), polls);
            assert_eq!(ctx.jit_root_stack_top, 0);
        }
        features.push((base, selected));
    }
    ctx.set_quit_flag_value(Value::NIL);
    for (base, selected) in &features {
        selected_feature(base, selected);
    }
}
