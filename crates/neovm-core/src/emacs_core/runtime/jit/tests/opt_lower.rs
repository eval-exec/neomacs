//! Passes-off opt lowering is a real native tier, with baseline observation
//! and exact deopt state. Threading: contexts/leaves are owned by each test.

use super::compile_pipeline_tests::{captured_clif, function, mask_code_text};
use super::*;
use crate::emacs_core::jit::opt::ir::ParamShape;

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_deopt_for_test(false);
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_deopt_for_test(false);
    }
}
fn lower(f: &ByteCodeFunction, osr: Option<usize>) -> CompiledLeaf {
    let params = ParamShape {
        required: f.params.required.len(),
        optional: f.params.optional.len(),
        has_rest: f.params.rest.is_some(),
    };
    lower_leaf_full_osr_with_opt(
        f.executable_ops(),
        &f.constants,
        params.native_arity(),
        f.executable_gnu_byte_offset_map(),
        None,
        osr,
        f.jit_runtime().patched_prefix(),
        Some(params),
    )
    .unwrap()
}
fn count_loop() -> ByteCodeFunction {
    // n, i; while i<n: i++; return i. n is globally invariant.
    function(
        vec![
            Op::Constant(0),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(9),
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(1),
            Op::Return,
        ],
        vec![Value::make_int(0)],
        1,
    )
}
#[test]
fn opt_lower_global_ssa_loop_matches_tier0_and_reports_opt() {
    let _settings = Settings::enter();
    let f = count_loop();
    let leaf = lower(&f, None);
    assert_eq!(leaf.tier(), LeafTier::Opt);
    let mut ev = Context::new();
    for n in [0, 1, 10, 10000] {
        let args = [Value::make_int(n)];
        let expected = Vm::from_context(&mut ev)
            .execute(&f, args.to_vec())
            .unwrap();
        assert_eq!(
            leaf.call(&mut ev as *mut Context as *mut u8, &args),
            NativeRun::Ok(expected.bits())
        );
    }
}
#[test]
fn opt_lower_diamond_and_else_pop_preserve_global_values() {
    let _settings = Settings::enter();
    let f = function(
        vec![
            Op::StackRef(0),
            Op::GotoIfNilElsePop(5),
            Op::Constant(0),
            Op::Add,
            Op::Goto(6),
            Op::Pop,
            Op::Return,
        ],
        vec![Value::make_int(7)],
        1,
    );
    let leaf = lower(&f, None);
    let mut ev = Context::new();
    for arg in [Value::NIL, Value::make_int(11)] {
        let expected = Vm::from_context(&mut ev).execute(&f, vec![arg]).unwrap();
        assert_eq!(
            leaf.call(&mut ev as *mut Context as *mut u8, &[arg]),
            NativeRun::Ok(expected.bits())
        );
    }
}
#[test]
fn opt_lower_force_deopt_captures_exact_stack_after_heap_effect() {
    let _settings = Settings::enter();
    let f = function(
        vec![
            Op::StackRef(1),
            Op::Constant(0),
            Op::Setcar,
            Op::Pop,
            Op::StackRef(0),
            Op::Add1,
            Op::Return,
        ],
        vec![Value::make_int(7)],
        2,
    );
    let leaf = lower(&f, None);
    let mut ev = Context::new();
    let pair = Value::cons(Value::make_int(1), Value::NIL);
    let arg = Value::symbol("opt-bad-number");
    let NativeRun::DeoptAt(frame) = leaf.call(&mut ev as *mut Context as *mut u8, &[pair, arg])
    else {
        panic!("expected exact deopt")
    };
    assert_eq!(frame.pc, 5);
    assert_eq!(frame.stack.as_slice(), &[pair, arg, arg]);
    assert_eq!(pair.cons_car(), Value::make_int(7));
    force_deopt_for_test(true);
    let forced_leaf = lower(&f, None);
    let fresh_pair = Value::cons(Value::make_int(1), Value::NIL);
    let NativeRun::DeoptAt(frame) = forced_leaf.call(
        &mut ev as *mut Context as *mut u8,
        &[fresh_pair, Value::make_int(4)],
    ) else {
        panic!("forced deopt")
    };
    assert_eq!(frame.pc, 2);
    assert_eq!(
        frame.stack.as_slice(),
        &[
            fresh_pair,
            Value::make_int(4),
            fresh_pair,
            Value::make_int(7)
        ]
    );
    assert_eq!(fresh_pair.cons_car(), Value::make_int(1));
}
#[test]
fn opt_lower_osr_prunes_prologue_and_checks_untouched_snapshot() {
    let _settings = Settings::enter();
    let f = count_loop();
    let leaf = lower(&f, Some(1));
    assert_eq!(leaf.tier(), LeafTier::Opt);
    let mut ev = Context::new();
    let args = [
        Value::make_int(10000).bits() as i64,
        Value::make_int(42).bits() as i64,
    ];
    assert_eq!(
        leaf.call_premarshaled(&mut ev as *mut Context as *mut u8, args.as_ptr()),
        NativeRun::Ok(Value::make_int(10000).bits())
    );
    let snapshot = [Value::make_int(10000), Value::NIL];
    let snapshot_bits = snapshot.map(|value| value.bits() as i64);
    let NativeRun::DeoptAt(frame) =
        leaf.call_premarshaled(&mut ev as *mut Context as *mut u8, snapshot_bits.as_ptr())
    else {
        panic!("entry guard")
    };
    assert_eq!(frame.pc, 1);
    assert_eq!(frame.stack.as_slice(), &snapshot);
}
#[test]
fn opt_legacy_knob_preserves_normalized_clif() {
    let f = count_loop();
    force_opt_for_test(Some(OptMode::parse(None)), None);
    let original = captured_clif(|| {
        compile_bytecode_function_with(&f, None).unwrap();
    });
    force_opt_for_test(Some(OptMode::Legacy), None);
    let legacy = captured_clif(|| {
        compile_bytecode_function_with(&f, None).unwrap();
    });
    force_opt_for_test(None, None);
    let original: Vec<_> = original
        .iter()
        .map(|c| mask_code_text(&c.replace('_', "")))
        .collect();
    let legacy: Vec<_> = legacy
        .iter()
        .map(|c| mask_code_text(&c.replace('_', "")))
        .collect();
    assert!(!original.is_empty());
    assert_eq!(original, legacy);
}

#[test]
fn opt_knobs_keep_the_backend_default_off_and_admissions_empty() {
    assert_eq!(OptMode::parse(None), OptMode::Legacy);
    assert_eq!(OptMode::parse(Some("opt")), OptMode::Opt);
    assert_eq!(OptMode::parse(Some("off")), OptMode::Off);
    assert_eq!(OptMode::parse(Some("invalid")), OptMode::Legacy);
    assert_eq!(OptAdmit::parse(None), OptAdmit::default());
    assert_eq!(OptAdmit::parse(Some("all")), OptAdmit::ALL);
    let bits = OptAdmit::parse(Some("args, env,switch"));
    assert!(bits.args && bits.env && bits.switch);
    assert!(!bits.vars && !bits.binds && !bits.handlers);
}

#[test]
fn opt_backend_selects_baseline_at_t1_and_opt_at_upgrade() {
    let _settings = Settings::enter();
    let f = count_loop();
    let compile = |tier| {
        compile_bytecode_function_requested(
            &f,
            None,
            CompileRequest {
                regalloc: RegallocPolicy::Full,
                bypass_profit_gate: false,
                origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
                tier,
            },
        )
        .unwrap()
    };
    let t1 = compile(crate::emacs_core::jit::tier2::CompileTier::T1);
    assert_eq!(t1.tier(), LeafTier::Baseline);
    let t2 = compile(crate::emacs_core::jit::tier2::CompileTier::Upgrade(
        crate::emacs_core::jit::tier2::T2Upgrade::Feedback,
    ));
    assert_eq!(t2.tier(), LeafTier::Opt);
    let conservative = compile(crate::emacs_core::jit::tier2::CompileTier::Upgrade(
        crate::emacs_core::jit::tier2::T2Upgrade::Retier,
    ));
    assert_eq!(conservative.tier(), LeafTier::Baseline);
    assert!(matches!(
        t2.obs.t2.origin,
        crate::emacs_core::jit::tier2::T2Origin::Upgrade(_)
    ));
}

#[test]
fn opt_tier2_seam_keeps_rooted_t1_fallback_and_reverts() {
    use crate::emacs_core::jit::{bg, cache, stats, tier2};
    let _settings = Settings::enter();
    bg::force_mode_for_test(Some(bg::BgMode::Sync));
    force_profit_gate_for_test(false);
    let mut knob = Tier2Knob::from_env(|_| None);
    knob.on = true;
    knob.window = 2;
    force_tier2_for_test(Some(knob));
    force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        stable: 1,
        attempts: 4,
        budget_pct: 0,
        floor_ms: 5,
        max_reopt: 3,
    }));
    stats::force_observe_for_test(stats::ObserveOverride {
        entry_count: true,
        ..Default::default()
    });
    let f = function(
        vec![Op::Constant(0), Op::Return],
        vec![Value::make_int(7)],
        0,
    );
    f.jit_runtime().set_hot_for_test();
    let value = Value::make_bytecode(f);
    crate::emacs_core::eval::push_scratch_gc_root(value);
    let f = value.get_bytecode_data().unwrap();
    let mut ctx = Context::new();
    let run = |ctx: &mut Context| {
        assert_eq!(
            cache::try_run_compiled(ctx, f, value, &[]).unwrap(),
            Some(Value::make_int(7).bits())
        );
    };
    run(&mut ctx);
    let id = f.jit_runtime().compiled_id().unwrap();
    let t1 = cache::compiled_leaf_ptr_for_test(id).unwrap();
    for _ in 1..20 {
        run(&mut ctx);
    }
    let t2 = cache::compiled_leaf_ptr_for_test(id).unwrap();
    assert_ne!(t1, t2);
    // Both pointers are retained by this mutator's cache/fallback, with no clear.
    let (t1, t2) = unsafe { (&*t1, &*t2) };
    assert_eq!(t1.tier(), LeafTier::Baseline);
    assert_eq!(t2.tier(), LeafTier::Opt);
    assert_eq!(
        t2.obs.t2.origin,
        tier2::T2Origin::Upgrade(tier2::T2Upgrade::Feedback)
    );
    assert_eq!(t2.obs.t2.budget.get(), tier2::DISARMED);
    assert!(
        t2.obs.entries.get() >= 16,
        "T2 serves the work after its window"
    );
    assert!(cache::revert_t2_to_t1(f, t2));
    assert!(std::ptr::eq(
        cache::compiled_leaf_ptr_for_test(id).unwrap(),
        t1
    ));
    force_tier2_for_test(None);
    force_tier2_policy_for_test(None);
    force_profit_gate_for_test(true);
    bg::force_mode_for_test(None);
}
