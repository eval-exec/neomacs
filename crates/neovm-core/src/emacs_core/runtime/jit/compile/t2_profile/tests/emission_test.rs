use super::*;
use cranelift_codegen::ir::UserFuncName;
use cranelift_frontend::FunctionBuilderContext;
use cranelift_module::{Module, default_libcall_names};

enum Site {
    Entry,
    Poll,
}

fn countdown_clif(site: Site) -> String {
    super::super::shim_refs::force_lazy_shims_for_test(true);
    let mut builder = cranelift_jit::JITBuilder::new(default_libcall_names()).expect("builder");
    super::super::register_shims(&mut builder);
    let mut module = cranelift_jit::JITModule::new(builder);
    let config = module.target_config();
    let ptr_ty = config.pointer_type();
    let call_conv = config.default_call_conv;
    let mut func =
        Function::with_name_signature(UserFuncName::user(0, 0), Signature::new(call_conv));
    let refs = pure_entry_refs(&mut module, &mut func, call_conv, ptr_ty).expect("refs");
    let mut fbctx = FunctionBuilderContext::new();
    {
        let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);
        let entry = fb.create_block();
        fb.switch_to_block(entry);
        let t2 = T2Emit {
            budget: 0x1234,
            obs: 0x5678,
            loop_credit: 64,
        };
        match site {
            Site::Entry => emit_entry_countdown(&mut fb, ptr_ty, &refs, t2),
            Site::Poll => {
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
                    self_direct_source: None,
                    poll: PollEmit {
                        count: None,
                        t2: Some(t2),
                    },
                };
                emit_poll_extras(&mut fb, &rt);
            }
        }
        fb.ins().return_(&[]);
        fb.seal_all_blocks();
        fb.finalize(config);
    }
    let text = func.display().to_string();
    assert_eq!(func.dfg.ext_funcs.len(), 1, "request is imported lazily");
    assert!(!text.contains("call_indirect"), "{text}");
    text
}

#[test]
fn tier2_entry_countdown_tests_zero_only() {
    let text = countdown_clif(Site::Entry);
    assert!(
        (text.contains("icmp eq ") || text.contains("icmp_imm eq ")),
        "{text}"
    );
    assert!(
        !(text.contains("icmp sle ") || text.contains("icmp_imm sle ")),
        "{text}"
    );
}

#[test]
fn tier2_loop_credit_tests_crossing_zero() {
    let text = countdown_clif(Site::Poll);
    assert!(
        (text.contains("icmp sle ") || text.contains("icmp_imm sle ")),
        "{text}"
    );
}

#[test]
fn tier2_profile_shims_are_optional_for_a_leaf() {
    let groups = ShimGroups {
        subr_spec: true,
        cbsym_spec: true,
        tier2_profile: false,
        direct_shapes: false,
        call_census: false,
        direct_framed: false,
        hof: false,
    };
    assert!(!groups.contains(super::super::shim_refs::ShimGroup::Tier2Profile));
}

#[test]
fn tier2_hof_profiler_accepts_dynamic_subrs_and_closes_its_window() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::jit::compile::{Tier2Knob, force_tier2_for_test};
    use crate::emacs_core::jit::tier2::{
        BuildScope, CompileTier, DISARMED, T2State, cells_for_build,
    };
    use crate::emacs_core::value::LambdaParams;

    force_tier2_for_test(Some(Tier2Knob {
        on: true,
        window: 100,
        loop_credit: 64,
    }));
    let source = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let _scope = BuildScope::enter(CompileTier::T1, source.jit_runtime());
    let mut obs = LeafObs::new(false);
    obs.t2 = cells_for_build();
    let mut ctx = Context::new();
    let mapc = intern("mapc");
    let subr = ctx.obarray.symbol_function_id(mapc).expect("native mapc");
    let sequence = Value::list(vec![Value::NIL; 128]);
    let args = [Value::NIL.bits() as i64, sequence.bits() as i64];
    let ctx_ptr = (&mut ctx as *mut Context).cast::<u8>();
    let obs_ptr = std::ptr::from_ref(&*obs);

    credit_generic(
        ctx_ptr,
        Value::from_sym_id(mapc).bits() as i64,
        args.as_ptr(),
        2,
        obs_ptr,
    );
    assert_eq!(
        obs.t2.budget.get(),
        98,
        "symbol mapping callee receives len/64"
    );
    credit_generic(ctx_ptr, subr.bits() as i64, args.as_ptr(), 2, obs_ptr);
    assert_eq!(
        obs.t2.budget.get(),
        96,
        "direct native callee receives len/64"
    );

    ctx.obarray.set_symbol_function("mapc", Value::NIL);
    credit_generic(
        ctx_ptr,
        Value::from_sym_id(mapc).bits() as i64,
        args.as_ptr(),
        2,
        obs_ptr,
    );
    assert_eq!(
        obs.t2.budget.get(),
        96,
        "a replaced function gets no HOF credit"
    );

    obs.t2.state.set(T2State::Kept);
    obs.t2.budget.set(DISARMED);
    credit_generic(ctx_ptr, subr.bits() as i64, args.as_ptr(), 2, obs_ptr);
    assert_eq!(
        obs.t2.budget.get(),
        DISARMED,
        "credit stops when the tier window closes"
    );
    force_tier2_for_test(None);
}
