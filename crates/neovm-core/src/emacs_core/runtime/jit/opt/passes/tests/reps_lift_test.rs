//! Numeric lifting uses the existing Tier-0 arithmetic, not alternate semantics.

use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op, Vm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::{
    push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::opt::{
    build::{BuildInput, build},
    eval::{EvalError, Inputs, Outcome, evaluate},
    ir::*,
    mem::{AliasClass, Effects},
    types::TypeSet,
};
use crate::emacs_core::value::{LambdaParams, Value as LispValue};

// Test roots belong to this invocation's mutator; no compiler metadata is
// installed in a runtime cache and all scratch roots are restored on unwind.
struct Roots(usize);
impl Roots {
    fn new(values: &[LispValue]) -> Self {
        let saved = save_scratch_gc_roots();
        push_scratch_gc_roots(values);
        Self(saved)
    }
}
impl Drop for Roots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn source(ops: Vec<Op>, constants: Vec<LispValue>, arity: usize) -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|_| crate::emacs_core::intern::intern("opt-reps-lift-arg"))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    function.lexical = true;
    function.max_stack = 64;
    function.ops = ops;
    function.constants = constants.into();
    function
}

fn plan(function: &ByteCodeFunction, prefix: usize) -> Func {
    let params = ParamShape {
        required: function.params.required.len(),
        ..ParamShape::default()
    };
    let cfg = crate::emacs_core::jit::compile::analyze_cfg(
        function.executable_ops(),
        &function.constants,
        function.executable_gnu_byte_offset_map(),
        params.native_arity(),
    )
    .expect("source CFG");
    let constants = function
        .constants
        .iter()
        .copied()
        .map(ValueBits::from_value)
        .collect::<Vec<_>>();
    build(BuildInput {
        ops: function.executable_ops(),
        constants: &constants,
        cfg: &cfg,
        params,
        dynamic_prefix: prefix,
        fused: None,
        osr: None,
    })
    .expect("source SSA")
}

fn tier0(ctx: &mut Context, function: &ByteCodeFunction, args: &[LispValue]) -> ValueBits {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    ValueBits::from_value(vm.execute(function, args.to_vec()).expect("Tier-0 result"))
}

fn returned(ctx: &mut Context, func: &Func, args: &[LispValue], prefix: &[LispValue]) -> ValueBits {
    let run = evaluate(
        func,
        ctx,
        Inputs {
            args,
            prefix,
            ..Inputs::default()
        },
    )
    .expect("reference execution");
    let Outcome::Returned(answer) = run.outcome else {
        panic!("in-range arithmetic must return")
    };
    answer
}

fn operation(func: &Func, source_op: &Op) -> (Inst, Value) {
    let (index, inst) = func
        .insts
        .iter()
        .enumerate()
        .find(|(_, inst)| {
            matches!(&inst.op, Opcode::Opaque(op) | Opcode::OpaqueBool(op) if op == source_op)
        })
        .expect("original operation");
    (Inst(index as u32), inst.result.expect("numeric result"))
}

fn assert_observation_ids(before: &Func, after: &Func) {
    assert_eq!(
        after.frames, before.frames,
        "original full frames remain interned"
    );
    assert_eq!(after.entry_stacks, before.entry_stacks);
    assert_eq!(after.source_states.len(), before.source_states.len());
    for (actual, expected) in after.source_states.iter().zip(&before.source_states) {
        match (actual, expected) {
            (Some(actual), Some(expected)) => {
                assert_eq!(actual.pre, expected.pre);
                assert_eq!(actual.post, expected.post);
                assert_eq!(actual.frame, expected.frame);
                assert_eq!(actual.block, expected.block);
            }
            (None, None) => {}
            _ => panic!("source state added or removed during lifting"),
        }
    }
    for (index, inst) in before.insts.iter().enumerate() {
        if matches!(
            inst.op,
            Opcode::Arg(_) | Opcode::OsrSlot(_) | Opcode::EnvConst(_)
        ) {
            let result = inst.result.unwrap();
            assert_eq!(after.insts[index].op, inst.op);
            assert_eq!(after.insts[index].result, Some(result));
            assert_eq!(
                after.values[result.index()].def,
                before.values[result.index()].def
            );
            assert_eq!(after.values[result.index()].rep, Rep::Tagged);
            assert_eq!(
                after.values[result.index()].ty,
                before.values[result.index()].ty
            );
        }
    }
}

fn arithmetic(op: Op, constant: i64) -> ByteCodeFunction {
    let unary = matches!(op, Op::Add1 | Op::Sub1 | Op::Negate);
    source(
        if unary {
            vec![op, Op::Return]
        } else {
            vec![Op::Constant(0), op, Op::Return]
        },
        if unary {
            Vec::new()
        } else {
            vec![LispValue::fixnum(constant)]
        },
        1,
    )
}

#[test]
fn opt_reps_lift_six_arithmetic_ops_keep_results_and_full_source_ids() {
    let mut ctx = Context::new();
    for op in [Op::Add, Op::Sub, Op::Mul, Op::Add1, Op::Sub1, Op::Negate] {
        let function = arithmetic(op.clone(), 3);
        let original = plan(&function, 0);
        let (inst, result) = operation(&original, &op);
        let mut lifted = original.clone();
        let stats = run(&mut lifted, &[]).unwrap();
        assert_eq!(stats.lifted_arithmetic, 1, "{op:?}");
        assert_eq!(stats.lifted_comparisons, 0);
        assert!(stats.type_guards > 0);
        assert_eq!(lifted.insts[inst.index()].result, Some(result));
        assert_eq!(lifted.values[result.index()].rep, Rep::TaggedFix);
        assert_eq!(lifted.values[result.index()].ty, TypeSet::FIXNUM);
        assert!(matches!(
            lifted.insts[inst.index()].op,
            Opcode::FixAdd { checked: true }
                | Opcode::FixSub { checked: true }
                | Opcode::FixMul { checked: true }
        ));
        assert_observation_ids(&original, &lifted);
        for guard in lifted
            .insts
            .iter()
            .filter(|data| matches!(data.op, Opcode::CheckType(_)))
        {
            assert_eq!(guard.pc, original.insts[inst.index()].pc);
            assert_eq!(guard.frame, original.insts[inst.index()].frame);
            assert!(guard.eff.contains(Effects::MAY_DEOPT));
            assert_eq!(guard.mem, AliasClass::None);
            assert_eq!(
                lifted.values[guard.result.unwrap().index()].rep,
                Rep::TaggedFix
            );
        }
        lifted.verify().unwrap();
        for integer in [-17, -1, 0, 1, 19] {
            let args = [LispValue::fixnum(integer)];
            let expected = tier0(&mut ctx, &function, &args);
            assert_eq!(returned(&mut ctx, &original, &args, &[]), expected);
            assert_eq!(returned(&mut ctx, &lifted, &args, &[]), expected);
        }
    }
}

#[test]
fn opt_reps_lift_comparisons_preserve_tagged_and_existing_bool_results() {
    let mut ctx = Context::new();
    for op in [Op::Eqlsign, Op::Lss, Op::Gtr, Op::Leq, Op::Geq] {
        for already_bool in [false, true] {
            let function = source(vec![op.clone(), Op::Return], Vec::new(), 2);
            let mut original = plan(&function, 0);
            if already_bool {
                crate::emacs_core::jit::opt::passes::bools::run(&mut original).unwrap();
            }
            let (inst, result) = operation(&original, &op);
            let mut lifted = original.clone();
            let stats = run(&mut lifted, &[]).unwrap();
            assert_eq!(stats.lifted_comparisons, 1);
            assert_eq!(stats.lifted_arithmetic, 0);
            assert_eq!(lifted.insts[inst.index()].result, Some(result));
            if already_bool {
                assert!(matches!(lifted.insts[inst.index()].op, Opcode::FixCmp(_)));
                assert_eq!(lifted.values[result.index()].rep, Rep::Bool);
            } else {
                assert_eq!(lifted.insts[inst.index()].op, Opcode::BoolToLisp);
                assert_eq!(lifted.values[result.index()].rep, Rep::Tagged);
                let flag = lifted.insts[inst.index()].args[0];
                let ValueDef::Inst(compare) = lifted.values[flag.index()].def else {
                    panic!("the hidden comparison is attached")
                };
                assert!(matches!(
                    lifted.insts[compare.index()].op,
                    Opcode::FixCmp(_)
                ));
                assert_eq!(lifted.values[flag.index()].rep, Rep::Bool);
            }
            assert_observation_ids(&original, &lifted);
            lifted.verify().unwrap();
            for (a, b) in [(-9, 2), (7, 7), (12, -5)] {
                let args = [LispValue::fixnum(a), LispValue::fixnum(b)];
                let expected = tier0(&mut ctx, &function, &args);
                assert_eq!(returned(&mut ctx, &lifted, &args, &[]), expected);
            }
        }
    }
}

#[test]
fn opt_reps_lift_float_other_feedback_preserve_original_opaque_plans() {
    for feedback in [NumericFeedback::Float, NumericFeedback::Other] {
        for op in [
            Op::Add,
            Op::Sub,
            Op::Mul,
            Op::Add1,
            Op::Sub1,
            Op::Negate,
            Op::Eqlsign,
            Op::Lss,
            Op::Gtr,
            Op::Leq,
            Op::Geq,
        ] {
            let function = if matches!(op, Op::Eqlsign | Op::Lss | Op::Gtr | Op::Leq | Op::Geq) {
                source(vec![op.clone(), Op::Return], Vec::new(), 2)
            } else {
                arithmetic(op, 3)
            };
            let mut func = plan(&function, 0);
            let original = format!("{}", func.display());
            let snapshot = vec![feedback; function.executable_ops().len()];
            assert_eq!(run(&mut func, &snapshot).unwrap(), LiftStats::default());
            assert_eq!(format!("{}", func.display()), original);
        }
    }
}

fn bridge(
    input_type: TypeSet,
    opcode: Opcode,
    result_type: TypeSet,
    result_rep: Rep,
) -> (Func, Value) {
    let mut func = Func::new(
        Box::new([]),
        ParamShape {
            required: 1,
            ..ParamShape::default()
        },
        0,
    );
    func.blocks.push(BlockData::new(0));
    let input = append(
        &mut func,
        Opcode::Arg(0),
        &[],
        input_type,
        Rep::Tagged,
        None,
    );
    let frame = func.intern_frame(FrameState {
        pc: 0,
        stack: vec![input].into(),
        handlers: 0,
        binds: 0,
        parent: None,
        site: None,
    });
    let result = append(
        &mut func,
        opcode,
        &[input],
        result_type,
        result_rep,
        Some(frame),
    );
    func.blocks[0].term = Term::Return(result);
    (func, input)
}

fn append(
    func: &mut Func,
    op: Opcode,
    args: &[Value],
    ty: TypeSet,
    rep: Rep,
    frame: Option<FrameId>,
) -> Value {
    let inst = Inst(func.insts.len() as u32);
    let value = Value(func.values.len() as u32);
    let eff = if op.is_guard() {
        Effects::MAY_DEOPT
    } else {
        Effects::PURE
    };
    func.values.push(ValueData {
        ty,
        rep,
        def: ValueDef::Inst(inst),
    });
    func.insts.push(InstData {
        op,
        args: args.to_vec(),
        result: Some(value),
        eff,
        mem: AliasClass::None,
        frame,
        pc: 0,
    });
    func.blocks[0].insts.push(inst);
    value
}

#[test]
fn opt_reps_lift_verify_checked_taggedfix_view_and_declared_refinement() {
    let (checked, _) = bridge(
        TypeSet::TOP,
        Opcode::CheckType(TypeSet::FIXNUM),
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    checked
        .verify()
        .expect("a real guard proves an unknown tagged input");
    let (refined, _) = bridge(
        TypeSet::FIXNUM,
        Opcode::Refine(TypeSet::FIXNUM),
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    refined
        .verify()
        .expect("a declared fixnum can use a pure representation view");
}

#[test]
fn opt_reps_lift_verify_rejects_unproved_or_broad_taggedfix_views() {
    let (unknown, input) = bridge(
        TypeSet::TOP,
        Opcode::Refine(TypeSet::FIXNUM),
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    assert!(
        matches!(unknown.verify(), Err(VerifyError::RepMismatch { value, .. }) if value == input)
    );
    for target in [TypeSet::TOP, TypeSet::BOTTOM] {
        let (broad, _) = bridge(
            TypeSet::TOP,
            Opcode::CheckType(target),
            if target.is_bottom() {
                TypeSet::BOTTOM
            } else {
                TypeSet::FIXNUM
            },
            Rep::TaggedFix,
        );
        assert!(
            broad.verify().is_err(),
            "a representation view requires a nonempty fixnum guard"
        );
    }
}

#[test]
fn opt_reps_lift_verify_reverse_tagged_views_keep_proof_and_impossible_guard() {
    let mut ctx = Context::new();
    for (opcode, ty, impossible) in [
        (Opcode::Refine(TypeSet::FIXNUM), TypeSet::FIXNUM, false),
        (Opcode::CheckType(TypeSet::FIXNUM), TypeSet::FIXNUM, false),
        (Opcode::CheckType(TypeSet::LIST), TypeSet::BOTTOM, true),
    ] {
        let (mut func, original) = bridge(
            TypeSet::TOP,
            Opcode::CheckType(TypeSet::FIXNUM),
            TypeSet::FIXNUM,
            Rep::TaggedFix,
        );
        let checked = func.insts[1].result.unwrap();
        let frame = func.insts[1].frame;
        let tagged = append(&mut func, opcode, &[checked], ty, Rep::Tagged, frame);
        func.blocks[0].term = Term::Return(tagged);
        func.verify()
            .expect("both representations carry the same tagged word");
        assert_eq!(func.values[original.index()].ty, TypeSet::TOP);
        assert_eq!(func.values[original.index()].rep, Rep::Tagged);
        let argument = LispValue::fixnum(7);
        if impossible {
            let run = evaluate(
                &func,
                &mut ctx,
                Inputs {
                    args: &[argument],
                    ..Inputs::default()
                },
            )
            .unwrap();
            let Outcome::Deopt(snapshot) = run.outcome else {
                panic!("LIST guard still fails on a fixnum")
            };
            assert_eq!(snapshot.stack, [ValueBits::from_value(argument)]);
            assert_eq!(snapshot.pc, 0);
        } else {
            assert_eq!(
                returned(&mut ctx, &func, &[argument], &[]),
                ValueBits::from_value(argument)
            );
        }
        func.values[tagged.index()].ty = TypeSet::TOP;
        assert_eq!(
            func.verify(),
            Err(VerifyError::TypeMismatch(tagged)),
            "the bridge never widens the successful output's declared type"
        );
    }
}

#[test]
fn opt_reps_lift_verify_untag_requires_nonempty_fixnum_and_tag_result() {
    for input_type in [TypeSet::TOP, TypeSet::BOTTOM] {
        let (mut func, input) = bridge(input_type, Opcode::UntagFix, TypeSet::FIXNUM, Rep::RawInt);
        let raw = func.insts[1].result.unwrap();
        let tagged = append(
            &mut func,
            Opcode::TagFix,
            &[raw],
            TypeSet::FIXNUM,
            Rep::Tagged,
            None,
        );
        func.blocks[0].term = Term::Return(tagged);
        assert_eq!(
            func.verify(),
            Err(VerifyError::TypeMismatch(input)),
            "unknown bits cannot be untagged eagerly"
        );
    }
    let (mut valid, _) = bridge(
        TypeSet::FIXNUM,
        Opcode::UntagFix,
        TypeSet::FIXNUM,
        Rep::RawInt,
    );
    let raw = valid.insts[1].result.unwrap();
    let tagged = append(
        &mut valid,
        Opcode::TagFix,
        &[raw],
        TypeSet::FIXNUM,
        Rep::Tagged,
        None,
    );
    valid.blocks[0].term = Term::Return(tagged);
    valid.verify().unwrap();
    valid.values[tagged.index()].ty = TypeSet::TOP;
    assert_eq!(
        valid.verify(),
        Err(VerifyError::TypeMismatch(tagged)),
        "tagging does not manufacture another Lisp kind"
    );
}

#[test]
fn opt_reps_lift_dynamic_prefix_keeps_unknown_identity_and_early_type_frame() {
    let mut ctx = Context::new();
    // The template contains the unary step itself; it must not be reused as a
    // static literal because this pool slot belongs to the patched prefix.
    let function = source(
        vec![Op::Constant(0), Op::Add1, Op::Return],
        vec![LispValue::fixnum(1)],
        0,
    );
    let original = plan(&function, 1);
    let (inst, _) = operation(&original, &Op::Add1);
    let frame = original.insts[inst.index()].frame.unwrap();
    let mut lifted = original.clone();
    assert_eq!(run(&mut lifted, &[]).unwrap().lifted_arithmetic, 1);
    assert_observation_ids(&original, &lifted);
    for actual in [LispValue::fixnum(-9), LispValue::fixnum(24)] {
        assert_eq!(
            returned(&mut ctx, &lifted, &[], &[actual]),
            returned(&mut ctx, &original, &[], &[actual])
        );
    }
    let actual = LispValue::cons(LispValue::T, LispValue::NIL);
    let _roots = Roots::new(&[actual]);
    let run = evaluate(
        &lifted,
        &mut ctx,
        Inputs {
            prefix: &[actual],
            ..Inputs::default()
        },
    )
    .unwrap();
    let Outcome::Deopt(snapshot) = run.outcome else {
        panic!("current instance must fail before typed arithmetic")
    };
    assert_eq!(snapshot.pc, original.frames[frame.index()].pc);
    assert_eq!(snapshot.stack, [ValueBits::from_value(actual)]);
}

#[test]
fn opt_reps_lift_type_failure_preserves_prior_store_and_complete_frame() {
    let mut ctx = Context::new();
    ctx.eval_str("(setq opt-reps-lift-side nil)").unwrap();
    let function = source(
        vec![
            Op::Constant(0),
            Op::VarSet(1),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Add,
            Op::Return,
        ],
        vec![
            LispValue::fixnum(42),
            LispValue::symbol("opt-reps-lift-side"),
        ],
        3,
    );
    let original = plan(&function, 0);
    let (inst, _) = operation(&original, &Op::Add);
    let frame = original.insts[inst.index()].frame.unwrap();
    let mut lifted = original.clone();
    assert_eq!(run(&mut lifted, &[]).unwrap().lifted_arithmetic, 1);
    assert_observation_ids(&original, &lifted);
    for (index, before) in original.insts.iter().enumerate() {
        if matches!(before.op, Opcode::Opaque(Op::VarSet(_))) {
            assert_eq!(lifted.insts[index].op, before.op);
            assert_eq!(lifted.insts[index].args, before.args);
            assert_eq!(lifted.insts[index].eff, before.eff);
            assert_eq!(lifted.insts[index].frame, before.frame);
        }
    }
    let args = [
        LispValue::cons(LispValue::symbol("heap-companion"), LispValue::NIL),
        LispValue::symbol("wrong"),
        LispValue::fixnum(2),
    ];
    let _roots = Roots::new(&args);
    let tier0_flow = {
        let mut vm = Vm::from_context(&mut ctx);
        vm.force_interpreter_only_for_test();
        vm.execute(&function, args.to_vec())
            .expect_err("Tier-0 wrong-type condition")
    };
    let signal = tier0_flow.as_signal().expect("wrong-type signal");
    let untouched = evaluate(
        &original,
        &mut ctx,
        Inputs {
            args: &args,
            ..Inputs::default()
        },
    )
    .expect_err("opaque reference uses Tier-0");
    let EvalError::Flow(reference_flow) = untouched else {
        panic!("expected Tier-0 flow")
    };
    let reference_signal = reference_flow.as_signal().unwrap();
    assert_eq!(reference_signal.symbol, signal.symbol);
    assert_eq!(reference_signal.data, signal.data);
    let run = evaluate(
        &lifted,
        &mut ctx,
        Inputs {
            args: &args,
            ..Inputs::default()
        },
    )
    .unwrap();
    let Outcome::Deopt(snapshot) = run.outcome else {
        panic!("unknown input must retain an early type guard")
    };
    assert_eq!(snapshot.pc, original.frames[frame.index()].pc);
    assert_eq!(
        snapshot.stack,
        args.iter()
            .chain([&args[1], &args[2]])
            .copied()
            .map(ValueBits::from_value)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        ctx.eval_str("opt-reps-lift-side").unwrap(),
        LispValue::fixnum(42)
    );
}

#[test]
fn opt_reps_lift_overflow_and_unary_boundaries_keep_original_deopt_stack() {
    let mut ctx = Context::new();
    for (op, constant, argument) in [
        (Op::Add, 1, LispValue::MOST_POSITIVE_FIXNUM),
        (Op::Sub, 1, LispValue::MOST_NEGATIVE_FIXNUM),
        (Op::Mul, 2, LispValue::MOST_POSITIVE_FIXNUM),
        (Op::Add1, 0, LispValue::MOST_POSITIVE_FIXNUM),
        (Op::Sub1, 0, LispValue::MOST_NEGATIVE_FIXNUM),
        (Op::Negate, 0, LispValue::MOST_NEGATIVE_FIXNUM),
    ] {
        let function = arithmetic(op.clone(), constant);
        let original = plan(&function, 0);
        let (inst, _) = operation(&original, &op);
        let frame = original.insts[inst.index()].frame.unwrap();
        let mut lifted = original.clone();
        assert_eq!(run(&mut lifted, &[]).unwrap().lifted_arithmetic, 1);
        let argument = LispValue::fixnum(argument);
        let expected = returned(&mut ctx, &original, &[argument], &[]);
        let expected_text = crate::emacs_core::print::print_value(&expected.to_value());
        let tier0_answer = tier0(&mut ctx, &function, &[argument]);
        assert_eq!(
            expected_text,
            crate::emacs_core::print::print_value(&tier0_answer.to_value())
        );
        let run = evaluate(
            &lifted,
            &mut ctx,
            Inputs {
                args: &[argument],
                ..Inputs::default()
            },
        )
        .unwrap();
        let Outcome::Deopt(snapshot) = run.outcome else {
            panic!("GNU fixnum boundary must deopt to generic arithmetic")
        };
        assert_eq!(snapshot.pc, original.frames[frame.index()].pc);
        let mut stack = vec![ValueBits::from_value(argument)];
        if !matches!(op, Op::Add1 | Op::Sub1 | Op::Negate) {
            stack.push(ValueBits::from_value(LispValue::fixnum(constant)));
        }
        assert_eq!(
            snapshot.stack, stack,
            "the synthetic zero/one never enters a GNU frame"
        );
    }
}

#[test]
fn opt_reps_lift_invalid_input_is_transactional() {
    let function = arithmetic(Op::Add1, 0);
    let mut func = plan(&function, 0);
    let (inst, _) = operation(&func, &Op::Add1);
    func.insts[inst.index()].frame = None;
    let original = format!("{}", func.display());
    assert_eq!(run(&mut func, &[]), Err(VerifyError::MissingFrame(inst)));
    assert_eq!(format!("{}", func.display()), original);
}

#[test]
fn opt_reps_lift_reannotates_dependent_list_guard_without_changing_source_ids() {
    let mut ctx = Context::new();
    let function = source(
        vec![Op::Dup, Op::Add1, Op::Dup, Op::Car, Op::Return],
        Vec::new(),
        1,
    );
    let original = plan(&function, 0);
    let guard = Inst(
        original
            .insts
            .iter()
            .position(|data| data.op == Opcode::CheckType(TypeSet::LIST))
            .unwrap() as u32,
    );
    let result = original.insts[guard.index()].result.unwrap();
    let mut lifted = original.clone();
    assert_eq!(run(&mut lifted, &[]).unwrap().lifted_arithmetic, 1);
    assert_observation_ids(&original, &lifted);
    assert_eq!(
        lifted.insts[guard.index()].op,
        original.insts[guard.index()].op
    );
    assert_eq!(
        lifted.insts[guard.index()].args,
        original.insts[guard.index()].args
    );
    assert_eq!(
        lifted.insts[guard.index()].frame,
        original.insts[guard.index()].frame
    );
    assert_eq!(
        lifted.insts[guard.index()].eff,
        original.insts[guard.index()].eff
    );
    assert_eq!(lifted.values[result.index()].ty, TypeSet::BOTTOM);
    assert_eq!(
        lifted.values[result.index()].rep,
        Rep::Tagged,
        "an impossible successful view is never an untagging proof"
    );
    lifted.verify().unwrap();
    let argument = LispValue::fixnum(7);
    let before = evaluate(
        &original,
        &mut ctx,
        Inputs {
            args: &[argument],
            ..Inputs::default()
        },
    )
    .unwrap();
    let after = evaluate(
        &lifted,
        &mut ctx,
        Inputs {
            args: &[argument],
            ..Inputs::default()
        },
    )
    .unwrap();
    let (Outcome::Deopt(before), Outcome::Deopt(after)) = (before.outcome, after.outcome) else {
        panic!("the original list guard must still recover")
    };
    assert_eq!(after, before);
    crate::emacs_core::jit::opt::passes::fold::run(&mut lifted).unwrap();
    let folded = evaluate(
        &lifted,
        &mut ctx,
        Inputs {
            args: &[argument],
            ..Inputs::default()
        },
    )
    .unwrap();
    let Outcome::Deopt(folded) = folded.outcome else {
        panic!("fold preserves the guard deopt")
    };
    assert_eq!(folded, before);
}

/// Source-shaped GNU `cnt`: [n, sum, i] carries two loop phis. Separate
/// constant-pool entries let the negative case make only the sum seed dynamic.
fn counter_source() -> ByteCodeFunction {
    source(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(0),
            Op::StackRef(3),
            Op::Lss,
            Op::GotoIfNil(14),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Add,
            Op::StackSet(2),
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(2),
            Op::StackRef(1),
            Op::Return,
        ],
        vec![LispValue::fixnum(0), LispValue::fixnum(0)],
        1,
    )
}

#[test]
fn opt_reps_lift_real_counter_pipeline_proves_two_phis_and_selects_raw() {
    let function = counter_source();
    let original = plan(&function, 0);
    let header = original.source_states[2].as_ref().unwrap().block;
    assert_eq!(original.blocks[header.index()].params.len(), 2);
    let original_stack = &original.entry_stacks[header.index()];
    assert_eq!(original_stack.len(), 3);
    let phis = [original_stack[1], original_stack[2]];
    for phi in phis {
        assert!(!original.values[phi.index()].ty.is_subset(TypeSet::FIXNUM));
    }
    let polls = original
        .insts
        .iter()
        .filter(|data| data.op == Opcode::Poll)
        .count();
    assert_eq!(polls, 1, "one original backedge poll");

    let mut lifted = original.clone();
    let stats = run(&mut lifted, &[]).unwrap();
    assert_eq!(stats.lifted_arithmetic, 2);
    assert_eq!(stats.lifted_comparisons, 1);
    assert_observation_ids(&original, &lifted);
    for phi in phis {
        let data = &lifted.values[phi.index()];
        assert!(!data.ty.is_bottom());
        assert!(
            data.ty.is_subset(TypeSet::FIXNUM),
            "closed source counter phi must expose its grounded fixnum proof before fold"
        );
        assert_eq!(data.rep, original.values[phi.index()].rep);
        assert_eq!(data.def, original.values[phi.index()].def);
    }
    let argument = original_stack[0];
    assert_eq!(lifted.values[argument.index()].ty, TypeSet::TOP);
    assert_eq!(lifted.values[argument.index()].rep, Rep::Tagged);
    lifted.verify().unwrap();

    crate::emacs_core::jit::opt::passes::fold::run(&mut lifted).unwrap();
    crate::emacs_core::jit::opt::passes::bools::run(&mut lifted).unwrap();
    let reps = crate::emacs_core::jit::opt::passes::reps::run(&mut lifted).unwrap();
    assert_eq!(reps.raw_phis, 2, "actual source phis form one raw web");
    assert_eq!(reps.raw_arithmetic, 2);
    assert_eq!(
        lifted
            .insts
            .iter()
            .filter(|data| data.op == Opcode::Poll)
            .count(),
        polls,
        "proof exposure preserves executable quit polling"
    );
    lifted.verify().unwrap();
    let mut ctx = Context::new();
    for integer in [0, 1, 17, 254, 255, 510] {
        let args = [LispValue::fixnum(integer)];
        let expected = tier0(&mut ctx, &function, &args);
        assert_eq!(returned(&mut ctx, &original, &args, &[]), expected);
        assert_eq!(returned(&mut ctx, &lifted, &args, &[]), expected);
    }

    // A checked arithmetic backedge cannot prove the initial unknown sum.
    // The independently grounded i web still has a static zero seed.
    let unknown_original = plan(&function, 1);
    let header = unknown_original.source_states[2].as_ref().unwrap().block;
    let stack = unknown_original.entry_stacks[header.index()].clone();
    let mut unknown = unknown_original.clone();
    run(&mut unknown, &[]).unwrap();
    assert_observation_ids(&unknown_original, &unknown);
    assert!(
        !unknown.values[stack[1].index()]
            .ty
            .is_subset(TypeSet::FIXNUM)
    );
    assert!(!unknown.values[stack[2].index()].ty.is_bottom());
    assert!(
        unknown.values[stack[2].index()]
            .ty
            .is_subset(TypeSet::FIXNUM)
    );
    unknown.verify().unwrap();
}

#[test]
fn opt_reps_lift_publish_root_keeps_exact_tagged_view_and_original_result() {
    let function = arithmetic(Op::Add1, 0);
    let mut original = plan(&function, 0);
    let (arithmetic, result) = operation(&original, &Op::Add1);
    assert_eq!(original.values[result.index()].rep, Rep::Tagged);
    let block = original.source_states[0].as_ref().unwrap().block;
    let position = original.blocks[block.index()]
        .insts
        .iter()
        .position(|inst| *inst == arithmetic)
        .unwrap();
    let publication = Inst(original.insts.len() as u32);
    original.insts.push(InstData {
        op: Opcode::PublishRoot,
        args: vec![result],
        result: None,
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc: original.insts[arithmetic.index()].pc,
    });
    original.blocks[block.index()]
        .insts
        .insert(position + 1, publication);
    let repeated_publication = Inst(original.insts.len() as u32);
    original
        .insts
        .push(original.insts[publication.index()].clone());
    original.blocks[block.index()]
        .insts
        .insert(position + 2, repeated_publication);
    original.verify().unwrap();

    let mut lifted = original.clone();
    assert_eq!(run(&mut lifted, &[]).unwrap().lifted_arithmetic, 1);
    assert_observation_ids(&original, &lifted);
    assert_eq!(lifted.insts[arithmetic.index()].result, Some(result));
    assert_eq!(lifted.values[result.index()].rep, Rep::TaggedFix);
    assert_eq!(
        lifted.values[result.index()].def,
        original.values[result.index()].def
    );
    let published = &lifted.insts[publication.index()];
    assert_eq!(published.op, Opcode::PublishRoot);
    assert_eq!(published.result, original.insts[publication.index()].result);
    assert_eq!(published.pc, original.insts[publication.index()].pc);
    assert_eq!(published.frame, original.insts[publication.index()].frame);
    assert_eq!(published.eff, original.insts[publication.index()].eff);
    assert_eq!(published.mem, original.insts[publication.index()].mem);
    let view = published.args[0];
    assert_ne!(view, result, "publication has its own exact Tagged view");
    assert_eq!(
        lifted.insts[repeated_publication.index()].args,
        vec![view],
        "later publication in the same block reuses its dominating view"
    );
    assert_eq!(
        lifted.insts[repeated_publication.index()].frame,
        original.insts[repeated_publication.index()].frame
    );
    assert_eq!(
        lifted.insts[repeated_publication.index()].eff,
        original.insts[repeated_publication.index()].eff
    );
    assert_eq!(lifted.values[view.index()].rep, Rep::Tagged);
    assert_eq!(
        lifted.values[view.index()].ty,
        lifted.values[result.index()].ty
    );
    let ValueDef::Inst(view_inst) = lifted.values[view.index()].def else {
        panic!("publication view is an attached instruction")
    };
    let view_inst = &lifted.insts[view_inst.index()];
    assert_eq!(view_inst.op, Opcode::Refine(TypeSet::FIXNUM));
    assert_eq!(view_inst.args, vec![result]);
    assert_eq!(view_inst.eff, Effects::PURE);
    assert_eq!(view_inst.mem, AliasClass::None);
    assert_eq!(view_inst.frame, None);
    assert_eq!(view_inst.pc, published.pc);
    lifted.verify().unwrap();

    let mut ctx = Context::new();
    for integer in [-17, -1, 0, 1, 19] {
        let args = [LispValue::fixnum(integer)];
        let expected = tier0(&mut ctx, &function, &args);
        assert_eq!(returned(&mut ctx, &original, &args, &[]), expected);
        assert_eq!(returned(&mut ctx, &lifted, &args, &[]), expected);
    }
    let frame = original.insts[arithmetic.index()].frame.unwrap();
    for argument in [
        LispValue::NIL,
        LispValue::fixnum(LispValue::MOST_POSITIVE_FIXNUM),
    ] {
        let execution = evaluate(
            &lifted,
            &mut ctx,
            Inputs {
                args: &[argument],
                ..Inputs::default()
            },
        )
        .unwrap();
        let Outcome::Deopt(snapshot) = execution.outcome else {
            panic!("failed arithmetic resumes its original complete frame")
        };
        assert_eq!(snapshot.pc, original.frames[frame.index()].pc);
        assert_eq!(snapshot.stack, vec![ValueBits::from_value(argument)]);
    }
}
