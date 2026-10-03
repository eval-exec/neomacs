//! Requested allocator quality through actual opt and baseline native leaves.
//! Threading: each fixture owns its Context and leaves; existing compiler-test
//! overrides are scoped to this test's compiler thread and restored on drop.

use super::*;
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::inline::force_inline_for_test;
use crate::emacs_core::jit::tier2::{CompileTier, T2Upgrade};

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses::default()));
        force_inline_for_test(Some(false));
        force_deopt_for_test(false);
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
        force_inline_for_test(None);
        force_deopt_for_test(false);
    }
}

/// A call-heavy diamond whose joined callee is car or cdr. Both branches
/// retain the original arguments in their complete GNU operand-stack frames.
fn joined_call() -> ByteCodeFunction {
    function(
        vec![
            Op::StackRef(1),
            Op::GotoIfNil(4),
            Op::Constant(0),
            Op::Goto(5),
            Op::Constant(1),
            Op::StackRef(1),
            Op::Call(1),
            Op::Return,
        ],
        vec![Value::symbol("car"), Value::symbol("cdr")],
        2,
    )
}

fn compile_requested(
    ctx: &Context,
    f: &ByteCodeFunction,
    mode: OptMode,
    regalloc: RegallocPolicy,
    tier: CompileTier,
) -> CompiledLeaf {
    force_opt_for_test(Some(mode), Some(OptAdmit::ALL));
    compile_bytecode_function_requested(
        f,
        Some(&ctx.obarray),
        CompileRequest {
            regalloc,
            bypass_profit_gate: true,
            origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
            tier,
        },
    )
    .expect("the joined call compiles")
}

fn assert_native_parity(ctx: &mut Context, f: &ByteCodeFunction, leaf: &CompiledLeaf) {
    let object = Value::cons(Value::make_int(17), Value::make_int(29));
    let saved = crate::emacs_core::eval::save_scratch_gc_roots();
    crate::emacs_core::eval::push_scratch_gc_root(object);
    for condition in [Value::NIL, Value::T] {
        let args = [condition, object];
        let expected = {
            let mut vm = Vm::from_context(ctx);
            vm.force_interpreter_only_for_test();
            vm.execute(f, args.to_vec()).expect("Tier0 joined call")
        };
        let actual =
            match leaf.call_consts(ctx as *mut Context as *mut u8, f.constants.as_ptr(), &args) {
                NativeRun::Ok(bits) => Value::from_bits(bits),
                other => panic!("the allocator-selected leaf must stay native: {other:?}"),
            };
        assert_eq!(actual, expected, "both joined call edges retain GNU values");
    }
    crate::emacs_core::eval::restore_scratch_gc_roots(saved);
}

#[test]
fn opt_full_request_uses_full_allocator_for_actual_feedback_plan() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = joined_call();
    assert!(body_is_call_heavy(f.executable_ops(), &f.constants));
    let cfg = analyze_cfg(f.executable_ops(), &f.constants, None, 2).unwrap();
    let plan = opt_backend::build_plan(
        f.executable_ops(),
        &f.constants,
        &cfg,
        crate::emacs_core::jit::opt::ir::ParamShape {
            required: 2,
            ..Default::default()
        },
        0,
        None,
    )
    .unwrap();
    assert!(
        plan.blocks
            .iter()
            .any(|block| { block.preds.len() >= 2 && !block.params.is_empty() })
    );
    let leaf = compile_requested(
        &ctx,
        &f,
        OptMode::Opt,
        RegallocPolicy::Full,
        CompileTier::Upgrade(T2Upgrade::Feedback),
    );
    assert_eq!(leaf.tier(), LeafTier::Opt);
    assert_native_parity(&mut ctx, &f, &leaf);
    assert_eq!(
        leaf.regalloc,
        lowering::forced_regalloc().unwrap_or(lowering::RegallocChoice::Full),
        "actual opt feedback code must honor requested Full after the call-heavy gate",
    );
}

#[test]
fn opt_auto_t1_and_off_requests_keep_the_existing_allocator() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = joined_call();
    let cases = [
        (
            OptMode::Opt,
            RegallocPolicy::Auto,
            CompileTier::Upgrade(T2Upgrade::Feedback),
            LeafTier::Opt,
        ),
        (
            OptMode::Opt,
            RegallocPolicy::Full,
            CompileTier::T1,
            LeafTier::Baseline,
        ),
        (
            OptMode::Opt,
            RegallocPolicy::Full,
            CompileTier::Upgrade(T2Upgrade::Retier),
            LeafTier::Baseline,
        ),
        (
            OptMode::Off,
            RegallocPolicy::Full,
            CompileTier::Upgrade(T2Upgrade::Feedback),
            LeafTier::Baseline,
        ),
        (
            OptMode::Legacy,
            RegallocPolicy::Full,
            CompileTier::Upgrade(T2Upgrade::Feedback),
            LeafTier::Baseline,
        ),
    ];
    for (mode, policy, tier, expected_tier) in cases {
        let leaf = compile_requested(&ctx, &f, mode, policy, tier);
        assert_eq!(leaf.tier(), expected_tier, "mode {mode:?}, tier {tier:?}");
        assert_eq!(
            leaf.regalloc,
            lowering::forced_regalloc().unwrap_or(lowering::RegallocChoice::Fast),
            "ordinary call-heavy allocation remains unchanged for {mode:?}/{tier:?}",
        );
        assert_native_parity(&mut ctx, &f, &leaf);
    }
}

#[test]
fn opt_refused_plan_keeps_the_call_heavy_baseline_allocator() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let mut f = joined_call();
    let optional = f.params.required.pop().unwrap();
    f.params.optional.push(optional);
    force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::default()));
    let leaf = compile_bytecode_function_requested(
        &f,
        Some(&ctx.obarray),
        CompileRequest {
            regalloc: RegallocPolicy::Full,
            bypass_profit_gate: true,
            origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
            tier: CompileTier::Upgrade(T2Upgrade::Feedback),
        },
    )
    .expect("a refused opt argument shape retains its baseline compile");
    assert_eq!(leaf.tier(), LeafTier::Baseline);
    assert_eq!(
        leaf.regalloc,
        lowering::forced_regalloc().unwrap_or(lowering::RegallocChoice::Fast),
        "a Full opt request cannot change its refused-plan fallback",
    );
    assert_native_parity(&mut ctx, &f, &leaf);
}
