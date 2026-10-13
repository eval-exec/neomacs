//! Selected recipe execution is compared with real Tier-0. GNU identity
//! provenance: frozen O36 original/loop packs, data.c floatop_arith_driver's
//! fresh make_float, alloc.c Fcons, and GNU eq's exact object identity.
//! Threading: all sources, contexts and roots are local to this test invocation.
use super::{Inputs, Outcome, evaluate};
use crate::emacs_core::bytecode::{ByteCodeFunction, Op, Vm};
use crate::emacs_core::eval::{
    Context, push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::opt::{
    build::{BuildInput, build},
    ir::*,
    passes::sink,
};
use crate::emacs_core::value::{LambdaParams, Value as LispValue};

struct Roots(usize);
impl Roots {
    fn new(values: &[LispValue]) -> Self {
        let base = save_scratch_gc_roots();
        push_scratch_gc_roots(values);
        Self(base)
    }
}
impl Drop for Roots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn source(ops: Vec<Op>, constants: Vec<LispValue>, arity: usize) -> ByteCodeFunction {
    let mut source = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|_| crate::emacs_core::intern::intern("sink-reference-input"))
            .collect(),
        optional: vec![],
        rest: None,
    });
    source.lexical = true;
    source.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(32);
    source.ops = ops;
    source.constants = constants.into();
    source
}
fn plan(source: &ByteCodeFunction) -> Func {
    let params = ParamShape {
        required: source
            .params
            .stack_shape()
            .expect("fixture stack parameters")
            .required(),
        ..Default::default()
    };
    let cfg = crate::emacs_core::jit::compile::analyze_cfg(
        source.executable_ops(),
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        params.native_arity(),
    )
    .unwrap();
    let constants = source
        .constants
        .iter()
        .copied()
        .map(ValueBits::from_value)
        .collect::<Vec<_>>();
    build(BuildInput {
        ops: source.executable_ops(),
        constants: &constants,
        cfg: &cfg,
        params,
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })
    .unwrap()
}
fn tier0(ctx: &mut Context, source: &ByteCodeFunction, args: &[LispValue]) -> LispValue {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(source, args.to_vec()).unwrap()
}
fn returned(run: super::Run) -> LispValue {
    match run.outcome {
        Outcome::Returned(value) => value.to_value(),
        Outcome::Deopt(frame) => panic!("unexpected frame {frame:?}"),
    }
}

#[test]
fn opt_sink_reference_trace_preserves_fresh_aliases_and_distinct_identity() {
    let mut ctx = Context::new();
    let seed = LispValue::make_float(-0.5);
    let _roots = Roots::new(&[seed]);
    for same in [true, false] {
        let mut ops = vec![Op::StackRef(0), Op::Constant(0), Op::Mul];
        if same {
            ops.push(Op::Dup);
        } else {
            ops.extend([Op::StackRef(1), Op::Constant(0), Op::Mul]);
        }
        let compare_pc = ops.len() as u32;
        ops.extend([Op::Eq, Op::Return]);
        let source = source(ops, vec![LispValue::fixnum(2)], 1);
        let mut selected = plan(&source);
        selected.verify().unwrap();
        let expected = tier0(&mut ctx, &source, &[seed]);
        assert_eq!(
            returned(
                evaluate(
                    &selected,
                    &mut ctx,
                    Inputs {
                        args: &[seed],
                        ..Default::default()
                    }
                )
                .unwrap()
            ),
            expected
        );
        let original_frames = selected.frames.clone();
        let original_states = selected
            .source_states
            .iter()
            .map(|state| {
                state.as_ref().map(|state| {
                    (
                        state.block,
                        state.pre.clone(),
                        state.post.clone(),
                        state.frame,
                    )
                })
            })
            .collect::<Vec<_>>();
        let feedback = vec![NumericFeedback::Float; source.executable_ops().len()];
        let stats = sink::run(&mut selected, &feedback).unwrap();
        assert!(stats.numeric_sources > 0);
        selected.verify().unwrap();
        assert_eq!(selected.frames, original_frames);
        assert_eq!(
            selected
                .source_states
                .iter()
                .map(|state| state.as_ref().map(|state| (
                    state.block,
                    state.pre.clone(),
                    state.post.clone(),
                    state.frame
                )))
                .collect::<Vec<_>>(),
            original_states
        );
        let run = evaluate(
            &selected,
            &mut ctx,
            Inputs {
                args: &[seed],
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            run.trace.is_empty(),
            "selected trace must not box for old Snapshot diagnostics"
        );
        let observed = run
            .recipe_trace
            .iter()
            .find(|trace| trace.pc == compare_pc)
            .unwrap();
        let n = observed.identities.len();
        assert!(n >= 2);
        assert_eq!(
            observed.identities[n - 2] == observed.identities[n - 1],
            same
        );
        assert_eq!(returned(run), expected);
    }
}

#[test]
fn opt_sink_reference_zero_trip_keeps_seed_and_overflow_replays_original_frame() {
    let source = source(
        vec![
            Op::StackRef(1),
            Op::GotoIfNil(6),
            Op::StackRef(0),
            Op::Constant(0),
            Op::Add,
            Op::Return,
            Op::Return,
        ],
        vec![LispValue::fixnum(1)],
        2,
    );
    let mut ctx = Context::new();
    let seed = LispValue::make_float(-0.0);
    let _roots = Roots::new(&[seed]);
    let mut selected = plan(&source);
    selected.verify().unwrap();
    let expected = tier0(&mut ctx, &source, &[LispValue::NIL, seed]);
    assert_eq!(expected.bits(), seed.bits());
    let feedback = vec![NumericFeedback::Float; source.executable_ops().len()];
    assert_eq!(
        sink::run(&mut selected, &feedback).unwrap().numeric_sources,
        1
    );
    selected.verify().unwrap();
    let run = evaluate(
        &selected,
        &mut ctx,
        Inputs {
            args: &[LispValue::NIL, seed],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(returned(run).bits(), expected.bits());
    let maximum = LispValue::fixnum(LispValue::MOST_POSITIVE_FIXNUM);
    let run = evaluate(
        &selected,
        &mut ctx,
        Inputs {
            args: &[LispValue::T, maximum],
            ..Default::default()
        },
    )
    .unwrap();
    let Outcome::Deopt(frame) = run.outcome else {
        panic!("exact fixnum overflow must replay Add");
    };
    assert_eq!(frame.pc, 4);
    assert_eq!(frame.stack[0].to_value(), LispValue::T);
    assert_eq!(frame.stack[1].to_value(), maximum);
    assert_eq!(frame.stack[2].to_value(), maximum);
    assert_eq!(frame.stack[3].to_value(), LispValue::fixnum(1));
}
