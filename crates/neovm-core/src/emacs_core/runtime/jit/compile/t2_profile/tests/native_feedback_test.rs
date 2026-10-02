//! Use-mode call feedback follows the owning T1's native profile window.
//! These tests execute the emitted calls; no interpreter warmup can supply
//! the target transitions or increment these compiled-only counters.

use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::jit::bg::{BgMode, force_mode_for_test};
use crate::emacs_core::jit::cache::{self, OsrProbe};
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::compile::lowering::RegallocPolicy;
use crate::emacs_core::jit::compile::{
    CompileRequest, Tier2Knob, Tier2PolicyKnob, compile_bytecode_function_requested,
    force_deopt_for_test, force_profit_gate_for_test, force_spec_sources_for_test,
    force_tier2_for_test, force_tier2_policy_for_test,
};
use crate::emacs_core::jit::feedback::{
    CallTarget, FeedbackMode, STABLE_WINDOW, force_feedback_mode_for_test,
};
use crate::emacs_core::jit::stats::CompileOrigin;
use crate::emacs_core::jit::tier2::{
    CompileTier, DISARMED, T2Origin, T2State, T2Upgrade, rearm_fallback,
};

struct Overrides;
impl Overrides {
    fn use_t1(window: u32, loop_credit: u32) -> Self {
        force_feedback_mode_for_test(Some(FeedbackMode::Use));
        force_tier2_for_test(Some(Tier2Knob {
            on: true,
            window,
            loop_credit,
        }));
        force_tier2_policy_for_test(Some(Tier2PolicyKnob {
            stable: 2,
            attempts: 1,
            budget_pct: 0,
            floor_ms: 5,
            max_reopt: 3,
        }));
        force_spec_sources_for_test(Some(false));
        force_profit_gate_for_test(false);
        force_deopt_for_test(false);
        force_mode_for_test(Some(BgMode::Sync));
        crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
        Self
    }
}
impl Drop for Overrides {
    fn drop(&mut self) {
        force_feedback_mode_for_test(None);
        force_tier2_for_test(None);
        force_tier2_policy_for_test(None);
        force_spec_sources_for_test(None);
        force_profit_gate_for_test(true);
        force_mode_for_test(None);
        crate::emacs_core::jit::inline::force_inline_for_test(None);
    }
}

fn callback(tag: i64, arity: usize) -> Value {
    let value = Value::make_bytecode(function(
        vec![Op::Constant(0), Op::Return],
        vec![Value::make_int(tag)],
        arity,
    ));
    crate::emacs_core::eval::push_scratch_gc_root(value);
    value
}

fn run(ctx: &mut Context, f: &ByteCodeFunction, args: &[Value]) -> Value {
    let bits = cache::try_run_compiled(ctx, f, Value::NIL, args)
        .expect("no signal")
        .expect("the caller ran natively");
    Value::from_bits(bits)
}

fn leaf(f: &ByteCodeFunction) -> &'static CompiledLeaf {
    let id = f.jit_runtime().compiled_id().expect("compiled caller");
    let ptr = cache::compiled_leaf_ptr_for_test(id).expect("cached caller");
    // SAFETY: this test's cache retains the leaf; no clear occurs while used.
    unsafe { &*ptr }
}

fn source_count(site: &CallSiteFeedback) -> usize {
    match site.target() {
        CallTarget::Sources(sources) => sources.len(),
        target => panic!("expected source feedback, got {target:?}"),
    }
}

#[test]
fn tier2_native_use_observes_a_stable_window_transition_and_reopens() {
    // Entry-only must record too; native feedback is not a loop-credit knob.
    for loop_credit in [0, 64] {
        let _settings = Overrides::use_t1(2, loop_credit);
        let mut ctx = Context::new();
        let a = callback(1, 0);
        let b = callback(2, 0);
        let c = callback(3, 0);
        let f = function(vec![Op::StackRef(0), Op::Call(0), Op::Return], vec![], 1);
        let site = f
            .jit_runtime()
            .call_sites_for(f.executable_ops(), &f.constants)
            .site_at(1)
            .expect("dynamic call");
        site.observe(a);
        site.set_count_for_test(STABLE_WINDOW);
        assert_eq!(run(&mut ctx, &f, &[a]), Value::make_int(1));
        assert_eq!(run(&mut ctx, &f, &[a]), Value::make_int(1));
        let t1 = leaf(&f);
        assert_eq!(t1.obs.t2.state.get(), T2State::Idle);
        assert_eq!(
            t1.obs.t2.budget.get(),
            2,
            "first request opened the stable window"
        );
        assert_eq!(run(&mut ctx, &f, &[b]), Value::make_int(2));
        assert_eq!(
            source_count(site),
            2,
            "native target transition during stability"
        );
        assert_eq!(
            site.count(),
            STABLE_WINDOW + 3,
            "Use ignores the old site cutoff"
        );
        let after_transition = site.count();
        assert_eq!(run(&mut ctx, &f, &[b]), Value::make_int(2));
        assert_eq!(
            t1.obs.t2.state.get(),
            T2State::Kept,
            "the changed snapshot exhausted the one allowed attempt"
        );
        assert_eq!(t1.obs.t2.budget.get(), DISARMED);
        assert_eq!(run(&mut ctx, &f, &[c]), Value::make_int(3));
        assert_eq!(
            site.count(),
            after_transition,
            "compiled count stops when the leaf closes"
        );
        assert_eq!(
            source_count(site),
            2,
            "closed native call observes no new target"
        );
        rearm_fallback(f.jit_runtime(), t1);
        assert_eq!(run(&mut ctx, &f, &[c]), Value::make_int(3));
        assert_eq!(site.count(), after_transition + 1);
        assert_eq!(
            source_count(site),
            3,
            "C9's reopened window records beyond 15000"
        );
    }
}

#[test]
fn tier2_native_use_source_guard_miss_widens_the_stable_snapshot() {
    let _settings = Overrides::use_t1(2, 64);
    force_spec_sources_for_test(Some(true));
    let mut ctx = Context::new();
    let a = callback(1, 0);
    let b = callback(2, 0);
    let a_data = a.get_bytecode_data().expect("callback source");
    // Arm A first so matching source-slot hits have no generic decline.
    assert_eq!(run(&mut ctx, a_data, &[]), Value::make_int(1));
    cache::arm_leaf_slot(&mut ctx as *mut Context, a_data);
    assert!(
        a_data
            .jit_runtime()
            .armed_leaf_slot(cache::leaf_slot_epoch())
            .is_some(),
        "source-slot hit requires the interpreter-entry arming seam"
    );
    let f = function(vec![Op::StackRef(0), Op::Call(0), Op::Return], vec![], 1);
    let site = f
        .jit_runtime()
        .call_sites_for(f.executable_ops(), &f.constants)
        .site_at(1)
        .expect("dynamic call");
    site.observe(a);
    site.set_count_for_test(STABLE_WINDOW);
    assert_eq!(run(&mut ctx, &f, &[a]), Value::make_int(1));
    assert_eq!(run(&mut ctx, &f, &[a]), Value::make_int(1));
    let t1 = leaf(&f);
    assert_eq!(t1.spec_slot_kinds.as_ref(), &[SpecSlotKind::Source]);
    assert_eq!(
        site.count(),
        STABLE_WINDOW,
        "matched source hits keep their existing hot path"
    );
    assert_eq!(t1.obs.t2.state.get(), T2State::Idle);
    assert_eq!(t1.obs.t2.budget.get(), 2);
    assert_eq!(run(&mut ctx, &f, &[b]), Value::make_int(2));
    assert_eq!(
        site.count(),
        STABLE_WINDOW + 1,
        "guard miss enters the Use recorder"
    );
    assert_eq!(source_count(site), 2, "Sources(A) widened to Sources(A,B)");
    assert_eq!(run(&mut ctx, &f, &[b]), Value::make_int(2));
    assert_eq!(
        t1.obs.t2.state.get(),
        T2State::Kept,
        "C7 saw the changed snapshot"
    );
    assert_eq!(
        site.count(),
        STABLE_WINDOW + 1,
        "closed miss no longer records"
    );
}

#[test]
fn tier2_native_use_apply_keeps_tail_spreading_and_leaf_window() {
    let _settings = Overrides::use_t1(100, 0);
    let mut ctx = Context::new();
    let a = callback(7, 0);
    let b = callback(8, 0);
    // (lambda (f) (apply f nil)): Op::Apply spreads its final list argument.
    let f = function(
        vec![Op::StackRef(0), Op::Nil, Op::Apply(1), Op::Return],
        vec![],
        1,
    );
    let site = f
        .jit_runtime()
        .call_sites_for(f.executable_ops(), &f.constants)
        .site_at(2)
        .expect("apply target");
    site.observe(a);
    site.set_count_for_test(STABLE_WINDOW);
    assert_eq!(run(&mut ctx, &f, &[a]), Value::make_int(7));
    assert_eq!(site.count(), STABLE_WINDOW + 1);
    let t1 = leaf(&f);
    t1.obs.t2.state.set(T2State::Kept);
    t1.obs.t2.budget.set(DISARMED);
    assert_eq!(run(&mut ctx, &f, &[b]), Value::make_int(8));
    assert_eq!(site.count(), STABLE_WINDOW + 1);
    assert_eq!(source_count(site), 1);
}

#[test]
fn tier2_native_use_static_mapping_records_callback_in_its_leaf_window() {
    let _settings = Overrides::use_t1(100, 0);
    let mut ctx = Context::new();
    let a = callback(1, 1);
    let b = callback(2, 1);
    let seq = Value::vector(vec![Value::NIL; 128]);
    crate::emacs_core::eval::push_scratch_gc_root(seq);
    let f = function(
        vec![
            Op::Constant(0),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![Value::symbol("mapc")],
        2,
    );
    let site = f
        .jit_runtime()
        .call_sites_for(f.executable_ops(), &f.constants)
        .site_at(3)
        .expect("mapping callback");
    site.observe(a);
    site.set_count_for_test(STABLE_WINDOW);
    assert_eq!(run(&mut ctx, &f, &[a, seq]), seq);
    let t1 = leaf(&f);
    assert!(
        site.count() > STABLE_WINDOW,
        "static spec recorder stays open"
    );
    assert_eq!(
        t1.obs.t2.budget.get(),
        99,
        "entry-only records without HOF credit"
    );
    t1.obs.t2.state.set(T2State::Due(T2Upgrade::Feedback));
    t1.obs.t2.budget.set(DISARMED);
    let count = site.count();
    // Execute this old activation directly; a cache seam would install T2.
    assert!(
        matches!(t1.call((&mut ctx as *mut Context).cast(), &[b, seq]), NativeRun::Ok(bits) if bits == seq.bits())
    );
    assert_eq!(site.count(), count);
    assert_eq!(source_count(site), 1, "static and fallback paths both stop");
}

#[test]
fn tier2_native_use_off_osr_and_upgrade_leave_native_recording_absent() {
    let _settings = Overrides::use_t1(100, 64);
    force_spec_sources_for_test(Some(true));
    let mut ctx = Context::new();
    let a = callback(1, 1);
    let b = callback(2, 1);
    let f = function(
        vec![
            Op::Constant(0),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(13),
            Op::StackRef(2),
            Op::StackRef(1),
            Op::Call(1),
            Op::Pop,
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(1),
            Op::Return,
        ],
        vec![Value::make_int(0)],
        2,
    );
    let site = f
        .jit_runtime()
        .call_sites_for(f.executable_ops(), &f.constants)
        .site_at(7)
        .expect("loop callback");
    site.observe(a);
    site.set_count_for_test(STABLE_WINDOW);
    let mut off = Tier2Knob {
        on: false,
        window: 100,
        loop_credit: 64,
    };
    for tier in [
        CompileTier::T1,
        CompileTier::Plain,
        CompileTier::Upgrade(T2Upgrade::Feedback),
    ] {
        off.on = tier != CompileTier::T1;
        force_tier2_for_test(Some(off));
        let compiled = compile_bytecode_function_requested(
            &f,
            Some(&ctx.obarray),
            CompileRequest {
                regalloc: RegallocPolicy::Full,
                bypass_profit_gate: true,
                origin: CompileOrigin::Direct,
                tier,
            },
        )
        .expect("entry backend");
        assert!(!compiled.obs.t2.profiling());
        assert!(
            matches!(compiled.call((&mut ctx as *mut Context).cast(), &[b, Value::make_int(4)]), NativeRun::Ok(bits) if bits == Value::make_int(4).bits())
        );
        assert_eq!(
            site.count(),
            STABLE_WINDOW,
            "{tier:?} emitted no native Use recording"
        );
        assert_eq!(source_count(site), 1);
    }
    force_tier2_for_test(Some(Tier2Knob {
        on: true,
        window: 100,
        loop_credit: 64,
    }));
    let result = cache::try_run_osr_probe(
        &mut ctx,
        &f,
        1,
        &[b, Value::make_int(4), Value::make_int(0)],
        &[],
    );
    assert!(
        matches!(result, OsrProbe::Ran(NativeRun::Ok(bits)) if bits == Value::make_int(4).bits())
    );
    let ptr = cache::osr_leaf_ptr_for_test(&f, 1).expect("OSR leaf");
    // SAFETY: the owner retains this OSR leaf through the end of the test.
    assert_eq!(unsafe { &*ptr }.obs.t2.origin, T2Origin::Unprofiled);
    assert_eq!(site.count(), STABLE_WINDOW, "OSR misses do not profile Use");
    assert_eq!(source_count(site), 1);
}

#[test]
fn tier2_native_use_fix_preserves_record_and_census_site_windows() {
    for mode in [FeedbackMode::Record, FeedbackMode::Census] {
        let _settings = Overrides::use_t1(100, 64);
        force_feedback_mode_for_test(Some(mode));
        let mut ctx = Context::new();
        let a = callback(1, 0);
        let b = callback(2, 0);
        let c = callback(3, 0);
        let f = function(vec![Op::StackRef(0), Op::Call(0), Op::Return], vec![], 1);
        let site = f
            .jit_runtime()
            .call_sites_for(f.executable_ops(), &f.constants)
            .site_at(1)
            .expect("dynamic call");
        site.observe(a);
        site.set_count_for_test(STABLE_WINDOW);
        assert_eq!(run(&mut ctx, &f, &[b]), Value::make_int(2));
        let census = mode == FeedbackMode::Census;
        assert_eq!(site.count(), STABLE_WINDOW + u32::from(census));
        assert_eq!(source_count(site), if census { 2 } else { 1 });
        let t1 = leaf(&f);
        t1.obs.t2.state.set(T2State::Kept);
        t1.obs.t2.budget.set(DISARMED);
        assert_eq!(run(&mut ctx, &f, &[c]), Value::make_int(3));
        assert_eq!(site.count(), STABLE_WINDOW + 2 * u32::from(census));
        assert_eq!(source_count(site), if census { 3 } else { 1 });
    }
}

#[test]
fn tier2_native_use_imports_helpers_only_with_actual_t1_emission() {
    use crate::emacs_core::jit::compile::call_feedback::{
        CallSourceScope, FeedbackHolds, recording_site_at,
    };
    use crate::emacs_core::jit::tier2::{BuildScope, cells_for_build};
    use cranelift_codegen::ir::UserFuncName;
    use cranelift_frontend::FunctionBuilderContext;
    use cranelift_module::{Module, default_libcall_names};

    let _settings = Overrides::use_t1(100, 0);
    super::super::shim_refs::force_lazy_shims_for_test(true);
    let mut ctx = Context::new();
    let source = function(vec![Op::StackRef(0), Op::Call(0), Op::Return], vec![], 1);
    source
        .jit_runtime()
        .call_sites_for(source.executable_ops(), &source.constants);
    let _source = CallSourceScope::enter(Some(source.jit_runtime().share_state()), true);
    let _holds = FeedbackHolds::enter();
    // Even a nested T1 BuildScope must not turn an OSR/Plain/T2 RtCtx into
    // a recording leaf. The actual emission owns the window and the imports.
    let _build = BuildScope::enter(CompileTier::T1, source.jit_runtime());
    let mut obs = LeafObs::new(false);
    obs.t2 = cells_for_build();
    for profile in [false, true] {
        for apply in [false, true] {
            let mut builder =
                cranelift_jit::JITBuilder::new(default_libcall_names()).expect("builder");
            super::super::register_shims(&mut builder);
            let mut module = cranelift_jit::JITModule::new(builder);
            let config = module.target_config();
            let ptr_ty = config.pointer_type();
            let mut func = Function::with_name_signature(
                UserFuncName::user(0, 0),
                Signature::new(config.default_call_conv),
            );
            let refs = pure_entry_refs(&mut module, &mut func, config.default_call_conv, ptr_ty)
                .expect("refs");
            let mut scratch = FunctionBuilderContext::new();
            {
                let mut fb = FunctionBuilder::new(&mut func, &mut scratch);
                let entry = fb.create_block();
                fb.switch_to_block(entry);
                let vmctx_var = fb.declare_var(ptr_ty);
                let slot = fb.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let rt = RtCtx {
                    refs,
                    vmctx_var,
                    ptr_ty,
                    call_args_slot: slot,
                    call_result_slot: slot,
                    rootwin: None,
                    heap: None,
                    inline_alloc: false,
                    direct_sites: std::cell::Cell::new(0),
                    inline_entry_cache: None,
                    poll: PollEmit {
                        count: None,
                        t2: profile.then(|| T2Emit::of(&obs).expect("profiling")),
                    },
                };
                assert_eq!(recording_site_at(1, &rt).is_some(), profile);
                let zero = fb.ins().iconst(types::I64, 0);
                let call = emit_feedback_prof_call(&mut fb, &rt, apply, true, &[zero; 6]);
                assert_eq!(call.is_some(), profile);
                fb.ins().return_(&[]);
                fb.seal_all_blocks();
                fb.finalize(config);
            }
            assert_eq!(func.dfg.ext_funcs.len(), usize::from(profile));
            assert!(!func.display().to_string().contains("call_indirect"));
        }
    }
    // Keep Context alive while the compiler scopes hold their source state.
    let _ = &mut ctx;
}
