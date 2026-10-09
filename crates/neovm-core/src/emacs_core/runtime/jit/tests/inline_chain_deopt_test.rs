//! Synthetic chain metadata/readback tests: no native chain producer exists.
use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::compile::{self, DeoptCells, NativeRun};
use crate::emacs_core::value::LambdaParams;
use std::cell::Cell;

fn function(arity: usize, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|i| intern(&format!("chain-arg-{i}")))
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

fn physical(pc: u32, len: u32) -> VFrameMeta {
    VFrameMeta {
        kind: VFrameKind::PhysicalBytecode,
        pc,
        stack: SpillRange { start: 0, len },
        binds: 0,
        handlers: 0,
        bt: BtState::Physical,
    }
}

fn callee() -> Value {
    Value::make_bytecode(function(
        1,
        vec![Op::StackRef(0), Op::Add1, Op::Return],
        vec![],
    ))
}

fn two_frames() -> DeoptChain {
    DeoptChain {
        frames: vec![
            physical(2, 2),
            VFrameMeta {
                kind: VFrameKind::Bytecode {
                    func: RelocIdx(0),
                    link: Link::Bcall { nargs: 1 },
                },
                pc: 0,
                stack: SpillRange { start: 2, len: 1 },
                binds: 0,
                handlers: 0,
                bt: BtState::Virtual,
            },
        ]
        .into_boxed_slice(),
    }
}

fn synthetic_leaf(
    values: &[Value],
    chain: DeoptChain,
    functions: &[Value],
) -> compile::CompiledLeaf {
    let mut leaf = compile::lower_nullary_leaf(&[Op::Constant(0), Op::Return], &[Value::NIL])
        .expect("compiles");
    // Native entry is never called after swapping these address-stable boxes:
    // the test models the cold emitter's final spill and then calls readback.
    leaf.deopt_spill = values.iter().map(|v| Cell::new(v.bits() as i64)).collect();
    leaf.deopt_meta.depth.set(values.len() as i64);
    leaf.deopt_meta.chain.set(0);
    leaf.chains = vec![chain].into_boxed_slice();
    leaf.reloc_data = functions.to_vec().into_boxed_slice();
    leaf
}

#[test]
fn inline_chain_deopt_readback_splits_binds_and_preserves_cross_frame_cons_aliases() {
    let mut ctx = Context::new();
    let f = callee();
    let pair = Value::cons(Value::make_int(7), Value::NIL);
    ctx.try_specbind(intern("chain-outer-binding"), Value::make_int(1))
        .unwrap();
    ctx.push_backtrace_frame(Value::symbol("chain-callee"), &[Value::make_int(11)]);
    ctx.try_specbind(intern("chain-inner-binding"), Value::make_int(2))
        .unwrap();
    ctx.jit_bind_stack.extend([99, 0, 2]);
    let mut chain = two_frames();
    chain.frames[0].stack.len = 3;
    chain.frames[0].binds = 1;
    chain.frames[1].stack = SpillRange { start: 3, len: 3 };
    chain.frames[1].binds = 1;
    chain.frames[1].bt = BtState::Materialized { spec_offset: 1 };
    let float = Value::make_float(1.5);
    let leaf = synthetic_leaf(
        &[
            pair,
            Value::symbol("chain-callee"),
            Value::make_int(11),
            pair,
            Value::make_int(-17),
            float,
        ],
        chain,
        &[f],
    );
    leaf.deopt_meta.pc.set(17);
    let NativeRun::DeoptAt(resume) =
        leaf.deopt_at_outcome(&mut ctx as *mut Context as *mut u8, Some((0, 1)), None)
    else {
        panic!("a valid chain reads back");
    };
    assert_eq!(ctx.jit_bind_stack.as_slice(), &[99]);
    assert_eq!(
        resume.pc, 2,
        "physical caller resumes at its original call pc"
    );
    assert_eq!(resume.inlined.as_ref().unwrap().guard_site_pc, 17);
    assert_eq!(leaf.obs.snapshot().deopt_pcs, vec![(17, 1)]);
    assert_eq!(resume.binds, vec![0]);
    let inner = &resume.inlined.as_ref().unwrap().frames[0];
    assert_eq!(inner.binds, vec![2]);
    assert_eq!(inner.function.bits(), f.bits());
    assert_eq!(
        inner.stack[0].bits(),
        resume.stack[0].bits(),
        "one rebuilt object across frames"
    );
    assert_eq!(inner.stack[1].as_fixnum(), Some(-17));
    assert_eq!(inner.stack[2].as_float(), Some(1.5));
    assert_eq!(inner.backtrace, ChainBacktrace::Materialized { index: 1 });
    assert_eq!(leaf.obs.snapshot().chain_deopts, 1);
    assert_eq!(
        leaf.obs.snapshot().chain_pcs,
        vec![(f.get_bytecode_data().unwrap().source_id, 0, 1)]
    );
}

#[test]
fn inline_chain_deopt_native_raw_fixnum_spill_is_retagged_before_readback() {
    compile::force_deopt_for_test(false);
    let mut ctx = Context::new();
    let mut leaf = compile::lower_nullary_leaf(
        &[
            Op::Constant(0),
            Op::Add1,
            Op::Constant(1),
            Op::Max,
            Op::Return,
        ],
        &[Value::make_int(7), Value::make_float(1.5)],
    )
    .unwrap();
    leaf.chains = vec![DeoptChain {
        frames: vec![physical(3, 2)].into_boxed_slice(),
    }]
    .into_boxed_slice();
    leaf.deopt_meta.chain.set(0);
    let NativeRun::DeoptAt(resume) = leaf.call(&mut ctx as *mut Context as *mut u8, &[]) else {
        panic!("mixed Max must deopt");
    };
    assert_eq!(resume.stack[0].as_fixnum(), Some(8));
    assert_eq!(resume.stack[1].as_float(), Some(1.5));
    assert!(resume.inlined.as_ref().unwrap().frames.is_empty());
}

#[test]
fn inline_chain_deopt_native_flonum_spill_boxes_aliases_once() {
    compile::force_deopt_for_test(false);
    let mut ctx = Context::new();
    let f = function(
        3,
        vec![
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Mul,
            Op::Dup,
            Op::StackRef(2),
            Op::Add,
            Op::Return,
        ],
        vec![],
    );
    for pc in [2, 5] {
        f.jit_runtime().record_numeric(
            pc,
            f.ops.len(),
            crate::emacs_core::jit::NumericFeedback::Float,
        );
    }
    compile::force_flonum_mode_for_test(Some(compile::FlonumMode::OpLocal));
    let mut leaf = compile::compile_bytecode_function(&f).unwrap();
    let census = compile::flonum_census();
    compile::force_flonum_mode_for_test(None);
    assert!(
        census.results > 0 && census.cold_boxes > 0,
        "test reached flonum lowering"
    );
    leaf.chains = vec![DeoptChain {
        frames: vec![physical(5, 6)].into_boxed_slice(),
    }]
    .into_boxed_slice();
    leaf.deopt_meta.chain.set(0);
    let NativeRun::DeoptAt(resume) = leaf.call(
        &mut ctx as *mut Context as *mut u8,
        &[
            Value::make_float(1.5),
            Value::make_float(2.0),
            Value::symbol("chain-nonnumeric"),
        ],
    ) else {
        panic!("symbol arithmetic must deopt");
    };
    assert_eq!(resume.stack[3].as_float(), Some(3.0));
    assert_eq!(
        resume.stack[3].bits(),
        resume.stack[4].bits(),
        "one box for the alias"
    );
    assert_ne!(
        resume.stack[3].bits() as i64,
        compile::UNBOXED_FLOAT_TAG_WORD
    );
}

#[test]
fn inline_chain_deopt_owned_payload_resumes_virtual_frames() {
    let mut ctx = Context::new();
    let inner = callee();
    let caller = function(
        0,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Call(1),
            Op::Add1,
            Op::Return,
        ],
        vec![inner, Value::make_int(11)],
    );
    let leaf = synthetic_leaf(
        &[
            Value::symbol("chain-running-callee"),
            Value::make_int(11),
            Value::make_int(11),
        ],
        two_frames(),
        &[inner],
    );
    let NativeRun::DeoptAt(resume) =
        leaf.deopt_at_outcome(&mut ctx as *mut Context as *mut u8, None, None)
    else {
        panic!("valid readback")
    };
    ctx.depth = 1; // The physical caller's invoker owns this depth.
    let result =
        compile::resumed_chain::resume_deopt(&mut ctx, &caller, Value::NIL, &leaf, *resume)
            .unwrap();
    assert_eq!(result.as_fixnum(), Some(13));
    assert_eq!(ctx.depth, 1);
    assert!(ctx.specpdl.is_empty() && ctx.bc_buf.is_empty() && ctx.bc_frames.is_empty());
}

#[test]
fn inline_chain_deopt_consumer_uses_native_guard_pc_for_repeat_threshold() {
    use crate::emacs_core::jit::reopt::{self, DeoptCause, ReoptKnobs};
    compile::force_deopt_for_test(false);
    reopt::force_reopt_for_test(Some(ReoptKnobs {
        site_limit: 2,
        ..ReoptKnobs::stress()
    }));
    let mut ctx = Context::new();
    let inner = callee();
    let caller = function(
        0,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Call(1),
            Op::Add1,
            Op::Return,
        ],
        vec![inner, Value::make_int(11)],
    );
    let mut chain = two_frames();
    chain.frames[1].pc = 1; // The callee's Add1 guard.
    let leaf = synthetic_leaf(
        &[
            Value::symbol("chain-running-callee"),
            Value::make_int(11),
            Value::make_int(11),
        ],
        chain,
        &[inner],
    );
    // An earlier overflow at this native guard counted in both the census
    // and the eligible policy history; semantic exits count only in the former.
    leaf.obs.note_deopt_at(17);
    leaf.obs.note_reopt_deopt_at(17);
    assert_eq!(leaf.obs.reopt_deopt_count_at(17), 1);
    leaf.deopt_meta.pc.set(17);
    leaf.deopt_meta
        .reason
        .set(DeoptCause::ArithOverflow.reason_code());
    let NativeRun::DeoptAt(resume) =
        leaf.deopt_at_outcome(&mut ctx as *mut Context as *mut u8, None, None)
    else {
        panic!("valid chain readback")
    };
    assert_eq!(leaf.obs.deopt_count_at(17), 2);
    assert_eq!(
        leaf.obs.reopt_deopt_count_at(17),
        1,
        "readback records the census before the consumer classifies the exit"
    );
    assert_eq!(
        leaf.obs.deopt_count_at(2),
        0,
        "caller resume pc has no guard count"
    );
    ctx.depth = 1;
    let result =
        compile::resumed_chain::resume_deopt(&mut ctx, &caller, Value::NIL, &leaf, *resume)
            .unwrap();
    assert_eq!(result.as_fixnum(), Some(13));
    assert_eq!(leaf.obs.reopt_deopt_count_at(17), 2);
    assert_eq!(
        leaf.obs.reopt_deopt_count_at(2),
        0,
        "caller resume pc has no eligible guard count"
    );
    assert_eq!(
        leaf.obs.reopt_deopt_count_at(1),
        0,
        "inner source pc has no eligible native guard count"
    );
    assert_eq!(
        inner
            .get_bytecode_data()
            .unwrap()
            .jit_runtime()
            .numeric_feedback(1),
        crate::emacs_core::jit::NumericFeedback::Other,
        "the second eligible overflow widens the inner Add1, using the leaf's native pc count"
    );
    reopt::force_reopt_for_test(None);
}

#[test]
fn inline_chain_deopt_rejects_unsupported_and_malformed_metadata() {
    let ctx = Context::new();
    let f = callee();
    let spill = [
        Value::symbol("chain-callee"),
        Value::make_int(1),
        Value::make_int(1),
    ];
    let check = |chain: &DeoptChain, wanted| {
        assert_eq!(
            chain.readback(&spill, &[], &[f], &ctx, 0, 2).err(),
            Some(wanted)
        );
    };
    let mut chain = two_frames();
    chain.frames[0].handlers = 1;
    check(&chain, ChainReadError::ActiveHandlers);
    chain = two_frames();
    chain.frames[1].handlers = 1;
    check(&chain, ChainReadError::ActiveHandlers);
    chain = two_frames();
    chain.frames[1].kind = VFrameKind::Bytecode {
        func: RelocIdx(0),
        link: Link::Funcall { nargs: 1 },
    };
    check(&chain, ChainReadError::UnsupportedLink);
    chain.frames[1].kind = VFrameKind::Hof {
        kind: HofKind::Mapc,
    };
    check(&chain, ChainReadError::UnsupportedHof);
    chain = two_frames();
    chain.frames[1].stack.start = 1;
    check(&chain, ChainReadError::InvalidSpill);
    chain = two_frames();
    chain.frames[1].kind = VFrameKind::Bytecode {
        func: RelocIdx(1),
        link: Link::Bcall { nargs: 1 },
    };
    check(&chain, ChainReadError::InvalidCallee);
    let mut unboxed = spill;
    unboxed[2] = Value::from_bits(compile::UNBOXED_FLOAT_TAG_WORD as usize);
    assert_eq!(
        two_frames().readback(&unboxed, &[], &[f], &ctx, 0, 2).err(),
        Some(ChainReadError::UnboxedFloat)
    );
}

#[test]
fn inline_chain_deopt_invalid_site_signals_and_resets_before_single_frame() {
    compile::force_deopt_for_test(false);
    let mut ctx = Context::new();
    let leaf = compile::lower_nullary_leaf(
        &[Op::Constant(0), Op::Constant(1), Op::Max, Op::Return],
        &[Value::make_float(1.5), Value::make_int(7)],
    )
    .unwrap();
    leaf.deopt_meta.chain.set(7);
    assert_eq!(
        leaf.call(&mut ctx as *mut Context as *mut u8, &[]),
        NativeRun::Signal
    );
    let flow = compile::take_pending_flow().expect("invalid bytecode flow");
    assert!(
        if matches!(flow.kind(), crate::emacs_core::error::FlowRef::Signal(sig)
        if sig.symbol == intern("invalid-byte-code"))
        {
            drop(flow);
            true
        } else {
            false
        }
    );
    assert_eq!(leaf.deopt_meta.chain.get(), DeoptCells::SINGLE_FRAME);
    let NativeRun::DeoptAt(resume) = leaf.call(&mut ctx as *mut Context as *mut u8, &[]) else {
        panic!("next single-frame deopt remains valid");
    };
    assert!(resume.chain.is_none() && resume.inlined.is_none());
    assert_eq!(resume.stack.len(), 2);
}

#[test]
fn inline_chain_deopt_metadata_is_shareable_and_native_outcome_stays_two_words() {
    fn shared<T: Send + Sync>() {}
    shared::<DeoptChain>();
    assert!(std::mem::size_of::<NativeRun>() <= 16);
    assert_eq!(std::mem::size_of::<DeoptCells>(), 40);
    assert_eq!(std::mem::offset_of!(DeoptCells, chain), 32);
}

#[test]
fn inline_chain_deopt_direct_cold_forwards_invalid_metadata_signal() {
    let mut ctx = Context::new();
    let f = function(0, vec![Op::Constant(0), Op::Return], vec![Value::NIL]);
    let leaf = synthetic_leaf(
        &[Value::NIL],
        DeoptChain {
            frames: Box::from([]),
        },
        &[],
    );
    let outcome = crate::emacs_core::jit::cache::direct_call_cold(
        &mut ctx,
        &f,
        Value::NIL,
        &leaf,
        compile::STATUS_DEOPT_AT,
    );
    assert!(matches!(
        outcome,
        crate::emacs_core::jit::cache::NativeCallOutcome::FlowStashed
    ));
    let flow = compile::take_pending_flow().expect("metadata error is forwarded");
    assert!(matches!(
        flow.kind(),
        crate::emacs_core::error::FlowRef::Signal(_)
    ));
    assert_eq!(leaf.deopt_meta.chain.get(), DeoptCells::SINGLE_FRAME);
    assert_eq!(
        leaf.obs.deopt_rerun.get(),
        0,
        "never replay malformed metadata"
    );
}
