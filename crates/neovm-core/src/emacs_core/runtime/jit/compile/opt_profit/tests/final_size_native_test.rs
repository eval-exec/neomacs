//! Real static fusion exceeds the cap and retains one baseline emission.
//! Threading: compiler settings, source objects and stats belong to this test.
use super::frontend_tests::Settings;
use super::*;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{
    captured_clif, function, mask_code_text,
};
use crate::emacs_core::jit::compile::opt_census::SelectedTier;
use crate::emacs_core::jit::compile::{
    ByteCodeFunction, Context, NativeRun, OptAdmit, OptEarlyMode, RegallocPolicy, Value,
    compile_bytecode_function_requested, force_opt_early_for_test, force_opt_for_test,
    force_opt_max_ops_for_test,
};
use crate::emacs_core::jit::stats::{self, CompileOrigin};

/// Threading: scalar fuser/observer overrides belong to this test invocation
/// and reset on drop; this scope contains no Lisp state or mutator cache.
struct FixtureScopes;
impl FixtureScopes {
    fn enter() -> Self {
        crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
        stats::force_observe_for_test(stats::ObserveOverride {
            stats: true,
            naming: false,
            entry_count: false,
        });
        Self
    }
}
impl Drop for FixtureScopes {
    fn drop(&mut self) {
        crate::emacs_core::jit::inline::force_inline_for_test(None);
        stats::force_observe_for_test(stats::ObserveOverride {
            stats: false,
            naming: false,
            entry_count: false,
        });
    }
}

fn expanding_loop() -> ByteCodeFunction {
    let mut callee_ops = vec![Op::StackRef(0)];
    callee_ops.extend((0..30).map(|_| Op::Add1));
    callee_ops.push(Op::Return);
    let callee = function(callee_ops, vec![], 1);
    callee.jit_runtime().set_hot_for_test();
    let callee = Value::make_bytecode(callee);
    crate::emacs_core::eval::push_scratch_gc_root(callee);
    function(
        vec![
            Op::Constant(0),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(20),
            Op::Constant(1),
            Op::StackRef(1),
            Op::Call(1),
            Op::StackSet(1),
            Op::Constant(1),
            Op::StackRef(1),
            Op::Call(1),
            Op::Sub1,
            Op::Add1,
            Op::StackSet(1),
            Op::StackRef(2),
            Op::StackRef(1),
            Op::Setcar,
            Op::Pop,
            Op::Goto(1),
            Op::Return,
        ],
        vec![Value::make_int(0), callee],
        2,
    )
}

fn compile_first_sight(f: &ByteCodeFunction) -> super::super::CompiledLeaf {
    compile_bytecode_function_requested(
        f,
        None,
        CompileRequest {
            regalloc: RegallocPolicy::Full,
            bypass_profit_gate: false,
            origin: CompileOrigin::FirstSight,
            tier: CompileTier::T1,
        },
    )
    .unwrap()
}

#[test]
fn opt_profit_final_fused_size_uses_one_legacy_baseline_and_preserves_native_result() {
    let _settings = Settings::enter();
    let _scopes = FixtureScopes::enter();
    let mut context = Context::new();
    force_opt_early_for_test(Some(OptEarlyMode::On));
    force_opt_max_ops_for_test(Some(64));
    let f = expanding_loop();
    assert!(f.executable_ops().len() <= 64);
    assert!(!super::super::body_is_call_heavy(
        f.executable_ops(),
        &f.constants
    ));
    let request = CompileRequest {
        regalloc: RegallocPolicy::Full,
        bypass_profit_gate: false,
        origin: CompileOrigin::FirstSight,
        tier: CompileTier::T1,
    };
    assert_eq!(
        front(request, f.executable_ops(), false, f.jit_runtime()),
        FrontChoice::SelectedAfterMir,
    );
    let feedback =
        vec![crate::emacs_core::jit::NumericFeedback::FixnumOnly; f.executable_ops().len()];
    let fused = crate::emacs_core::jit::inline::fuse_calls(
        f.executable_ops(),
        &f.constants,
        f.executable_gnu_byte_offset_map(),
        2,
        &feedback,
    )
    .expect("the real static fuser must expand both warmed constant calls");
    assert_eq!(fused.regions.len(), 2);
    assert!(fused.ops.len() > 64, "must exercise the final-slice guard");
    assert!(!fused.is_v2(), "retain the original static-fuser contract");
    assert!(!final_size_admitted(
        FrontChoice::SelectedAfterMir,
        fused.ops.len()
    ));

    stats::reset_compile_stats();
    let mut bounded = None;
    let bounded_clif = captured_clif(|| bounded = Some(compile_first_sight(&f)));
    let bounded = bounded.unwrap();
    let counts = stats::compile_stats_snapshot();
    assert_eq!(
        counts.mir_build_failed + counts.mir_tier_rejected,
        1,
        "MIR-first must decline once; an SSA refusal/outer retry would repeat it",
    );
    assert_eq!(bounded_clif.len(), 1, "emit one existing baseline leaf");
    assert_eq!(bounded.selected_tier(), SelectedTier::Baseline);

    // A disabled cap must still follow the existing selected SSA refusal and
    // legacy retry for non-v2 fused side tables. This is the negative control
    // for the one-frontend assertion; neither fused contract is weakened.
    force_opt_max_ops_for_test(Some(0));
    stats::reset_compile_stats();
    let unbounded = compile_first_sight(&f);
    let unbounded_counts = stats::compile_stats_snapshot();
    assert_eq!(
        unbounded_counts.mir_build_failed + unbounded_counts.mir_tier_rejected,
        2,
        "the unchanged static-fused SSA refusal triggers the legacy retry",
    );
    assert_eq!(unbounded.selected_tier(), SelectedTier::Baseline);
    force_opt_max_ops_for_test(Some(64));

    force_opt_for_test(Some(OptMode::Legacy), Some(OptAdmit::ALL));
    let mut legacy = None;
    let legacy_clif = captured_clif(|| legacy = Some(compile_first_sight(&f)));
    let legacy = legacy.unwrap();
    let normalize = |records: Vec<String>| {
        records
            .into_iter()
            .map(|text| mask_code_text(&text.replace('_', "")))
            .collect::<Vec<_>>()
    };
    assert_eq!(normalize(bounded_clif), normalize(legacy_clif));
    let pair = Value::cons(Value::make_int(-1), Value::NIL);
    crate::emacs_core::eval::push_scratch_gc_root(pair);
    for n in [0, 1, 61, 120] {
        let args = [pair, Value::make_int(n)];
        let result = Value::make_int(((n + 59) / 60) * 60);
        let expected = NativeRun::Ok(result.bits());
        assert_eq!(
            bounded.call(&mut context as *mut Context as *mut u8, &args),
            expected
        );
        assert_eq!(
            legacy.call(&mut context as *mut Context as *mut u8, &args),
            expected
        );
        if n > 0 {
            assert_eq!(pair.cons_car(), result);
        }
    }
    assert_eq!(context.jit_root_stack_top, 0);
}
