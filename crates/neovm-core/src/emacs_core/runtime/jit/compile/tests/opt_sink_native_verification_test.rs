//! Native final validation must retain full checks and run each child once.
//! The lead registers the scalar observer before the production API so the
//! old Array+Sink path can show its actual two independent checker entries.
//! All behavior comes from the unchanged sealed source/Tier0/reference path.
//! Threading: this test owns its Context, roots, Func and scalar observation.

use super::compile_pipeline_tests::function;
use super::*;
use crate::emacs_core::eval::{
    push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::opt::{
    build, eval, ir, native_verify_observer,
    passes::array_reads,
    sink_recipes::{self, RecipeKind, RecipeVersionId, SinkVerifyReason, VersionCause},
    verify::VerifyError,
};

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses {
            sink: true,
            range: true,
            ..OptPasses::default()
        }));
        force_flonum_mode_for_test(Some(FlonumMode::Resident));
        crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
        force_deopt_for_test(false);
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
        force_flonum_mode_for_test(None);
        crate::emacs_core::jit::inline::force_inline_for_test(None);
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
fn plan(f: &ByteCodeFunction) -> ir::Func {
    let cfg = analyze_cfg(
        f.executable_ops(),
        &f.constants,
        f.executable_gnu_byte_offset_map(),
        1,
    )
    .unwrap();
    let constants = f
        .constants
        .iter()
        .copied()
        .map(ir::ValueBits::from_value)
        .collect::<Vec<_>>();
    build::build(build::BuildInput {
        ops: f.executable_ops(),
        constants: &constants,
        cfg: &cfg,
        params: ir::ParamShape {
            required: 1,
            ..Default::default()
        },
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })
    .expect("actual builder source must verify before selection")
}
fn lower(f: &ByteCodeFunction, plan: &ir::Func) -> Result<CompiledLeaf, CompileError> {
    lower_opt_ir_for_test(
        f.executable_ops(),
        &f.constants,
        1,
        f.executable_gnu_byte_offset_map(),
        plan,
    )
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pair {
    car: u64,
    cdr: u64,
    same: bool,
}
fn pair(value: Value) -> Pair {
    assert!(value.is_cons());
    let (car, cdr) = (value.cons_car(), value.cons_cdr());
    assert!(car.is_float() && cdr.is_float());
    Pair {
        car: car.xfloat().to_bits(),
        cdr: cdr.xfloat().to_bits(),
        same: car.bits() == cdr.bits(),
    }
}
fn independent(
    f: &ByteCodeFunction,
    plan: &ir::Func,
    leaf: &CompiledLeaf,
    ctx: &mut Context,
    flag: Value,
    roots: &Roots,
) {
    let before = ctx.gc_count;
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    let expected = vm
        .execute(f, vec![flag])
        .expect("sealed Tier0 source baseline");
    assert_eq!(ctx.gc_count - before, 1, "actual source GC completes once");
    roots.add(&[expected]);
    let expected = pair(expected);
    assert_eq!(expected.same, !flag.is_nil(), "actual partial-alias branch");
    let before = ctx.gc_count;
    let run = eval::evaluate(
        plan,
        ctx,
        eval::Inputs {
            args: &[flag],
            ..Default::default()
        },
    )
    .expect("original/selected reference input is executable");
    let eval::Outcome::Returned(reference) = run.outcome else {
        panic!("source reference must return before any quality assertion")
    };
    let reference = reference.to_value();
    roots.add(&[reference]);
    assert_eq!(pair(reference), expected);
    assert_eq!(ctx.gc_count - before, 1);
    let before = ctx.gc_count;
    let outcome = leaf.call_consts(
        ctx as *mut Context as *mut u8,
        f.constants.as_ptr(),
        &[flag],
    );
    let NativeRun::Ok(bits) = outcome else {
        panic!("actual native source success required: {outcome:?}")
    };
    let actual = Value::from_bits(bits);
    roots.add(&[actual]);
    assert_eq!(pair(actual), expected);
    assert_eq!(ctx.gc_count - before, 1);
    assert_eq!(
        ctx.obarray.symbol_value_copied("t34-native-proof-side"),
        Some(flag)
    );
    assert_eq!(ctx.jit_root_stack_top, 0);
}
fn rejected(f: &ByteCodeFunction, poison: &ir::Func) -> VerifyError {
    let error = poison
        .verify()
        .expect_err("complete ordinary+recipe checker rejects poison");
    assert!(
        matches!(
            lower(f, poison),
            Err(CompileError::UnsupportedOp("opt-build:verify"))
        ),
        "low-level backend-final verification is retained"
    );
    error
}

#[test]
fn opt_sink_native_final_validation_rejects_poison_and_checks_recipes_once() {
    let _settings = Settings::enter();
    let roots = Roots::enter();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    ctx.set_gc_threshold(usize::MAX);
    ctx.obarray
        .set_symbol_value("t34-native-proof-side", Value::NIL);
    let seed = Value::make_float(2.0);
    roots.add(&[seed]);
    let array = Value::vector(vec![seed]);
    roots.add(&[array]);
    let factor = Value::make_float(1.5);
    roots.add(&[factor]);
    // Both arms execute actual Aref+Mul; true retains one fresh identity twice,
    // false produces equal payload in a distinct fresh identity. The real GC
    // executes after the join and before the final Cons/return observation.
    let f = function(
        vec![
            Op::StackRef(0),
            Op::VarSet(0), // prior visible effect
            Op::Constant(1),
            Op::Constant(2),
            Op::Aref,
            Op::Constant(3),
            Op::Mul,
            Op::Dup,
            Op::StackRef(2),
            Op::GotoIfNil(11),
            Op::Goto(17),
            Op::Pop,
            Op::Constant(1),
            Op::Constant(2),
            Op::Aref,
            Op::Constant(3),
            Op::Mul,
            Op::Constant(4),
            Op::Call(0),
            Op::Pop,
            Op::Cons,
            Op::Return,
        ],
        vec![
            Value::symbol("t34-native-proof-side"),
            array,
            Value::fixnum(0),
            factor,
            Value::symbol("garbage-collect"),
        ],
        1,
    );
    roots.add(&f.constants);
    let original = plan(&f);
    original.verify().unwrap();
    // Direct IR lowering does not publish the source feedback itself. Train
    // BOTH actually executed numeric sites before taking the first snapshot;
    // snapshot publication marks feedback consumed, so training afterwards
    // cannot repair a prematurely captured FixnumOnly native specialization.
    for flag in [Value::T, Value::NIL] {
        let before = ctx.gc_count;
        let mut vm = Vm::from_context(&mut ctx);
        vm.force_interpreter_only_for_test();
        let value = vm
            .execute(&f, vec![flag])
            .expect("actual Tier0 numeric training");
        roots.add(&[value]);
        assert_eq!(pair(value).same, !flag.is_nil());
        assert_eq!(ctx.gc_count - before, 1);
    }
    let snapshot = super::snapshot::FeedbackSnapshot::take(&f);
    let feedback = snapshot.numeric.clone();
    assert_eq!(feedback[6], NumericFeedback::Float);
    assert_eq!(feedback[16], NumericFeedback::Float);
    // The same actual immutable source snapshot feeds baseline and selected
    // native lowering as well as the Sink frontend, without fabricated hints.
    let _feedback = snapshot.publish();
    // Use the existing site-boxed native oracle for the independent unsunk
    // baseline. Resident's pre-existing partial-phi box identity divergence
    // is retained separately; selected Sink still compiles in Resident mode
    // and must satisfy every original identity/GC/native-success assertion.
    force_flonum_mode_for_test(Some(FlonumMode::Off));
    let baseline = lower(&f, &original);
    force_flonum_mode_for_test(Some(FlonumMode::Resident));
    let baseline = baseline.expect("ordinary site-boxed source native baseline");
    roots.add(baseline.reloc_values());
    for flag in [Value::T, Value::NIL] {
        independent(&f, &original, &baseline, &mut ctx, flag, &roots);
    }
    let mut selected = original.clone();
    let hints = super::array_snapshot::admission(f.executable_ops().len(), &f.constants, 0);
    let mut proofs = array_reads::ArrayReadProofs::default();
    let arrays = array_reads::lift(&mut selected, &mut proofs, &hints).unwrap();
    assert_eq!(arrays.reads_lifted, 2, "two actual guarded source reads");
    assert!(!selected.array_reads.reads.is_empty());
    let sunk = crate::emacs_core::jit::opt::passes::sink::run(&mut selected, &feedback).unwrap();
    assert!(sunk.numeric_sources > 0 && sunk.cons_sources > 0);
    selected.verify().unwrap();
    let (optimized, checks) = native_verify_observer::capture(|| lower(&f, &selected));
    let optimized = optimized.expect("actual selected Array+Sink native lowering");
    roots.add(optimized.reloc_values());
    for flag in [Value::T, Value::NIL] {
        independent(&f, &selected, &optimized, &mut ctx, flag, &roots);
    }

    // Real malformed whole-Func frame: table validity cannot authorize it.
    let mut malformed_frame = selected.clone();
    malformed_frame.frames[0].parent = Some(ir::FrameId(selected.frames.len() as u32 + 1));
    assert!(matches!(
        rejected(&f, &malformed_frame),
        VerifyError::InvalidFrame(_)
    ));

    // Actual current versions must remain point-local. Install a later boxed
    // version into an earlier valid complete frame; payload equality is no proof.
    let cap = sink_recipes::verify_recipes(&selected, &selected.sink_recipes).unwrap();
    let late = selected
        .sink_recipes
        .frames
        .iter()
        .find_map(|(&(point, frame), view)| {
            view.versions.iter().find_map(|&(owner, old)| {
                if !matches!(
                    selected.sink_recipes.owners[&owner].kind,
                    RecipeKind::Number(_)
                ) || cap.guaranteed_boxed(old)
                {
                    return None;
                }
                selected
                    .sink_recipes
                    .versions
                    .iter()
                    .enumerate()
                    .find_map(|(id, version)| {
                        let id = RecipeVersionId(id as u32);
                        (version.owner == owner
                            && id != old
                            && cap.guaranteed_boxed(id)
                            && matches!(version.cause, VersionCause::CacheAfter { .. }))
                        .then_some((point, frame, owner, id))
                    })
            })
        })
        .expect("actual source has an early virtual/full-frame and a later numeric box");
    let mut future = selected.clone();
    future
        .sink_recipes
        .frames
        .get_mut(&(late.0, late.1))
        .unwrap()
        .versions
        .iter_mut()
        .find(|(owner, _)| *owner == late.2)
        .unwrap()
        .1 = late.3;
    assert!(matches!(rejected(&f, &future), VerifyError::Sink(error)
        if matches!(error.reason, SinkVerifyReason::FutureBoxVersion
            | SinkVerifyReason::NonDominatingVersion | SinkVerifyReason::StaleBoxVersion)));

    // Retain the actual partial alias edge's SSA arguments while falsifying
    // its corresponding recipe tuple; the independent edge checker must fail.
    let edge = selected
        .sink_recipes
        .edges
        .first()
        .expect("actual branch creates a selected logical phi");
    let mut wrong_tuple = selected.clone();
    let forged = wrong_tuple
        .sink_recipes
        .edges
        .iter_mut()
        .find(|candidate| {
            candidate.owner_param == edge.owner_param
                && candidate.source == edge.source
                && candidate.edge_index == edge.edge_index
        })
        .unwrap();
    assert!(forged.field_args.len() >= 4);
    forged.field_args[3] = forged.field_args[0];
    assert!(
        matches!(rejected(&f, &wrong_tuple), VerifyError::Sink(error)
        if matches!(error.reason, SinkVerifyReason::WrongEdgeTuple
            | SinkVerifyReason::WrongFieldRepresentation | SinkVerifyReason::LostPartialAlias))
    );
    drop(cap);

    // These are actual checker entries only inside the native validation
    // region. The still-required backend-final validation is outside capture.
    // Current source executes two array and two recipe checks here: genuine
    // quality RED occurs AFTER the semantic and malformed-input controls.
    assert_eq!(checks.ordinary, 1);
    assert_eq!(
        checks.recipes, 1,
        "native final recipe proof is reconstructed only once"
    );
    assert_eq!(
        checks.arrays, 1,
        "native retains the Array proof from that same full check"
    );
}
