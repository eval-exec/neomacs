//! Structural regressions for closed webs and unchanged observation identities.

use super::*;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::opt::{
    ir::*,
    mem::{AliasClass, Effects},
    types::TypeSet,
};
use crate::emacs_core::value::Value as LispValue;

fn empty(blocks: usize, args: usize) -> Func {
    let mut func = Func::new(
        vec![
            ValueBits::from_value(LispValue::NIL),
            ValueBits::from_value(LispValue::T),
            ValueBits::from_value(LispValue::make_int(0)),
        ]
        .into(),
        ParamShape {
            required: args,
            ..ParamShape::default()
        },
        0,
    );
    for pc in 0..blocks {
        func.blocks.push(BlockData::new(pc as u32));
    }
    func
}

fn emit(
    func: &mut Func,
    block: Block,
    pc: u32,
    op: Opcode,
    args: &[Value],
    ty: TypeSet,
    rep: Rep,
) -> Value {
    let inst = Inst(func.insts.len() as u32);
    let value = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty,
        rep,
        def: ValueDef::Inst(inst),
    });
    func.insts.push(InstData {
        op,
        args: args.to_vec(),
        result: Some(value),
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc,
    });
    func.blocks[block.index()].insts.push(inst);
    value
}

fn param(func: &mut Func, block: Block) -> Value {
    let value = Value(func.values.len() as u32);
    let index = func.blocks[block.index()].params.len() as u32;
    func.values.push(ValueData {
        ty: TypeSet::TOP,
        rep: Rep::Tagged,
        def: ValueDef::Param { block, index },
    });
    func.blocks[block.index()].params.push(value);
    value
}

fn constant(func: &mut Func, block: Block, value: bool) -> Value {
    let pc = func.blocks[block.index()].pc;
    emit(
        func,
        block,
        pc,
        Opcode::Const(value as u32),
        &[],
        if value { TypeSet::T } else { TypeSet::NIL },
        Rep::Tagged,
    )
}

fn frame(func: &mut Func, pc: u32, stack: &[Value]) -> FrameId {
    func.intern_frame(FrameState {
        pc,
        stack: stack.into(),
        handlers: 0,
        binds: 0,
        parent: None,
        site: None,
    })
}

fn attach_frame(func: &mut Func, value: Value, frame: FrameId) {
    let ValueDef::Inst(inst) = func.values[value.index()].def else {
        panic!("instruction result")
    };
    func.insts[inst.index()].frame = Some(frame);
}

#[test]
fn opt_bools_promotes_seeded_two_phi_cycle_and_return_view() {
    let mut func = empty(4, 1);
    let flag = emit(
        &mut func,
        Block(0),
        0,
        Opcode::Arg(0),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let yes = constant(&mut func, Block(0), true);
    let a = param(&mut func, Block(1));
    let b = param(&mut func, Block(2));
    let returned = param(&mut func, Block(3));
    func.blocks[0].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![yes],
    });
    func.blocks[1].preds = vec![Block(0), Block(2)];
    func.blocks[1].term = Term::Branch {
        flag,
        if_true: Edge {
            target: Block(2),
            args: vec![a],
        },
        if_false: Edge {
            target: Block(3),
            args: vec![a],
        },
    };
    func.blocks[2].preds = vec![Block(1)];
    func.blocks[2].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![b],
    });
    func.blocks[3].preds = vec![Block(1)];
    func.blocks[3].term = Term::Return(returned);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(
        stats.phi_params, 3,
        "a seed grounds the whole closed cyclic web"
    );
    for value in [yes, a, b, returned] {
        assert_eq!(func.values[value.index()].rep, Rep::Bool);
    }
    assert_eq!(
        func.values[flag.index()].rep,
        Rep::Tagged,
        "unknown caller identity remains tagged"
    );
    assert_eq!(
        stats.tagged_views, 1,
        "the Lisp return is an explicit semantic view"
    );
    func.verify().unwrap();
}

#[test]
fn opt_bools_single_unknown_argument_or_environment_prunes_phi_web() {
    for environment in [false, true] {
        let mut func = empty(4, 1);
        func.dynamic_prefix = usize::from(environment);
        let unknown = emit(
            &mut func,
            Block(0),
            0,
            if environment {
                Opcode::EnvConst(0)
            } else {
                Opcode::Arg(0)
            },
            &[],
            TypeSet::BOOLEAN,
            Rep::Tagged,
        );
        let yes = emit(
            &mut func,
            Block(0),
            0,
            Opcode::Const(1),
            &[],
            TypeSet::T,
            Rep::Tagged,
        );
        let joined = param(&mut func, Block(3));
        func.blocks[0].term = Term::Branch {
            flag: unknown,
            if_true: Edge {
                target: Block(1),
                args: vec![],
            },
            if_false: Edge {
                target: Block(2),
                args: vec![],
            },
        };
        func.blocks[1].preds = vec![Block(0)];
        func.blocks[2].preds = vec![Block(0)];
        func.blocks[3].preds = vec![Block(1), Block(2)];
        func.blocks[1].term = Term::Jump(Edge {
            target: Block(3),
            args: vec![yes],
        });
        func.blocks[2].term = Term::Jump(Edge {
            target: Block(3),
            args: vec![unknown],
        });
        func.blocks[3].term = Term::Return(joined);
        func.verify().unwrap();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats.constant_producers, 1);
        assert_eq!(
            stats.phi_params, 0,
            "one unproved incoming value rejects the dependent web"
        );
        assert_eq!(func.values[joined.index()].rep, Rep::Tagged);
        assert_eq!(func.values[unknown.index()].rep, Rep::Tagged);
        assert_eq!(
            stats.tagged_views, 1,
            "only the proved input gets a tagged edge view"
        );
        func.verify().unwrap();
    }
}

#[test]
fn opt_bools_ungrounded_phi_cycle_remains_tagged() {
    let mut func = empty(3, 0);
    let result = emit(
        &mut func,
        Block(0),
        0,
        Opcode::Const(2),
        &[],
        TypeSet::FIXNUM,
        Rep::Tagged,
    );
    func.blocks[0].term = Term::Return(result);
    let a = param(&mut func, Block(1));
    let b = param(&mut func, Block(2));
    func.blocks[1].preds = vec![Block(2)];
    func.blocks[2].preds = vec![Block(1)];
    func.blocks[1].term = Term::Jump(Edge {
        target: Block(2),
        args: vec![a],
    });
    func.blocks[2].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![b],
    });
    func.verify().unwrap();
    let before = func.display().to_string();
    assert_eq!(run(&mut func).unwrap(), BoolStats::default());
    assert_eq!(
        func.display().to_string(),
        before,
        "a seedless closed SCC proves no Boolean value"
    );
}

#[test]
fn opt_bools_singleton_nil_test_refinement_keeps_its_original_definition() {
    for yes in [false, true] {
        let mut func = empty(1, 0);
        let constant = constant(&mut func, Block(0), yes);
        let flag = emit(
            &mut func,
            Block(0),
            1,
            Opcode::IsNonNil,
            &[constant],
            TypeSet::BOOLEAN,
            Rep::Bool,
        );
        let output = emit(
            &mut func,
            Block(0),
            2,
            Opcode::BoolToLisp,
            &[flag],
            TypeSet::BOOLEAN,
            Rep::Tagged,
        );
        func.blocks[0].term = Term::Return(output);
        let original_defs: Vec<_> = func.values.iter().map(|value| value.def).collect();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats.nil_tests, 1);
        assert_eq!(
            stats.refinements, 1,
            "the existing Lisp projection becomes a Boolean identity"
        );
        for value in [flag, output] {
            assert_eq!(
                func.values[value.index()].ty,
                if yes { TypeSet::T } else { TypeSet::NIL }
            );
            assert_eq!(func.values[value.index()].def, original_defs[value.index()]);
            let ValueDef::Inst(inst) = func.values[value.index()].def else {
                panic!("same definition")
            };
            assert!(matches!(func.insts[inst.index()].op, Opcode::Refine(_)));
        }
        func.verify().unwrap();
    }
}

#[test]
fn opt_bools_observation_ids_pinned_order_and_cached_tagged_uses_survive() {
    let mut func = empty(1, 1);
    let argument = emit(
        &mut func,
        Block(0),
        0,
        Opcode::Arg(0),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let first_frame = frame(&mut func, 1, &[argument]);
    let predicate = emit(
        &mut func,
        Block(0),
        1,
        Opcode::Opaque(Op::Null),
        &[argument],
        TypeSet::BOOLEAN,
        Rep::Tagged,
    );
    attach_frame(&mut func, predicate, first_frame);
    let second_frame = frame(&mut func, 2, &[argument, predicate, predicate]);
    let comparison = emit(
        &mut func,
        Block(0),
        2,
        Opcode::Opaque(Op::Eq),
        &[predicate, predicate],
        TypeSet::BOOLEAN,
        Rep::Tagged,
    );
    attach_frame(&mut func, comparison, second_frame);
    let guard_frame = frame(&mut func, 3, &[argument, predicate, comparison]);
    let checked = emit(
        &mut func,
        Block(0),
        3,
        Opcode::CheckType(TypeSet::BOOLEAN),
        &[predicate],
        TypeSet::BOOLEAN,
        Rep::Tagged,
    );
    attach_frame(&mut func, checked, guard_frame);
    let ValueDef::Inst(guard) = func.values[checked.index()].def else {
        unreachable!()
    };
    func.insts[guard.index()].eff = Effects::MAY_DEOPT;
    let poll_frame = frame(&mut func, 4, &[argument, predicate, comparison, checked]);
    let poll = Inst(func.insts.len() as u32);
    func.insts.push(InstData {
        op: Opcode::Poll,
        args: vec![],
        result: None,
        eff: Effects::MAY_GC,
        mem: AliasClass::None,
        frame: Some(poll_frame),
        pc: 4,
    });
    func.blocks[0].insts.push(poll);
    func.entry_stacks = vec![vec![argument].into()];
    func.source_states = vec![None; 5];
    for (pc, pre, post, frame) in [
        (1, vec![argument], vec![argument, predicate], first_frame),
        (
            2,
            vec![argument, predicate, predicate],
            vec![argument, comparison],
            second_frame,
        ),
        (
            3,
            vec![argument, predicate, comparison],
            vec![argument, checked, comparison],
            guard_frame,
        ),
        (
            4,
            vec![argument, predicate, comparison, checked],
            vec![argument, predicate, comparison, checked],
            poll_frame,
        ),
    ] {
        func.source_states[pc] = Some(SourceState {
            pre: pre.into(),
            post: post.into(),
            frame,
            block: Block(0),
        });
    }
    func.blocks[0].term = Term::Return(comparison);
    func.verify().unwrap();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.opaque_producers, 2);
    assert_eq!(
        stats.tagged_views, 2,
        "duplicate Eq operands and later guard share the same semantic view"
    );
    assert_eq!(func.frames, before.frames);
    assert_eq!(func.entry_stacks, before.entry_stacks);
    for (after, before) in func.source_states.iter().zip(&before.source_states) {
        if let (Some(after), Some(before)) = (after, before) {
            assert_eq!(after.pre, before.pre);
            assert_eq!(after.post, before.post);
            assert_eq!(after.frame, before.frame);
            assert_eq!(after.block, before.block);
        } else {
            assert!(after.is_none() && before.is_none());
        }
    }
    for original in [guard, poll] {
        assert_eq!(
            func.insts[original.index()].op,
            before.insts[original.index()].op
        );
        assert_eq!(
            func.insts[original.index()].frame,
            before.insts[original.index()].frame
        );
        assert_eq!(
            func.insts[original.index()].eff,
            before.insts[original.index()].eff
        );
    }
    assert_eq!(
        func.values[checked.index()].rep,
        Rep::Tagged,
        "tag-based guard results retain their Lisp representation"
    );
    func.verify().unwrap();
}

#[test]
fn opt_bools_select_refine_and_aliases_form_one_closed_web() {
    let mut func = empty(1, 1);
    let argument = emit(
        &mut func,
        Block(0),
        0,
        Opcode::Arg(0),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let state = frame(&mut func, 1, &[argument]);
    let predicate = emit(
        &mut func,
        Block(0),
        1,
        Opcode::Opaque(Op::Consp),
        &[argument],
        TypeSet::BOOLEAN,
        Rep::Tagged,
    );
    attach_frame(&mut func, predicate, state);
    let flag = emit(
        &mut func,
        Block(0),
        2,
        Opcode::IsNonNil,
        &[predicate],
        TypeSet::BOOLEAN,
        Rep::Bool,
    );
    let yes = constant(&mut func, Block(0), true);
    let no = constant(&mut func, Block(0), false);
    // Put source-PC metadata in order even for these manually emitted seeds.
    for value in [yes, no] {
        let ValueDef::Inst(id) = func.values[value.index()].def else {
            unreachable!()
        };
        func.insts[id.index()].pc = 2;
    }
    let selected = emit(
        &mut func,
        Block(0),
        3,
        Opcode::Select,
        &[flag, yes, no],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let refined = emit(
        &mut func,
        Block(0),
        4,
        Opcode::Refine(TypeSet::BOOLEAN),
        &[selected],
        TypeSet::BOOLEAN,
        Rep::Tagged,
    );
    let alias = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty: TypeSet::TOP,
        rep: Rep::Tagged,
        def: ValueDef::Alias(refined),
    });
    let state = frame(&mut func, 5, &[argument, alias, alias]);
    let comparison = emit(
        &mut func,
        Block(0),
        5,
        Opcode::Opaque(Op::Eq),
        &[alias, alias],
        TypeSet::BOOLEAN,
        Rep::Tagged,
    );
    attach_frame(&mut func, comparison, state);
    func.blocks[0].term = Term::Return(comparison);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.selects, 1);
    assert_eq!(stats.refinements, 1);
    assert_eq!(stats.nil_tests, 1);
    for value in [
        predicate, flag, yes, no, selected, refined, alias, comparison,
    ] {
        assert_eq!(func.values[value.index()].rep, Rep::Bool);
    }
    assert_eq!(func.values[alias.index()].def, ValueDef::Alias(refined));
    let ValueDef::Inst(eq) = func.values[comparison.index()].def else {
        unreachable!()
    };
    assert_eq!(
        func.insts[eq.index()].args[0],
        func.insts[eq.index()].args[1]
    );
    assert_eq!(stats.tagged_views, 2);
    func.verify().unwrap();
}

#[test]
fn opt_bools_seeded_phi_with_independent_seedless_input_remains_tagged() {
    let mut func = empty(4, 0);
    let yes = constant(&mut func, Block(0), true);
    let merged = param(&mut func, Block(1));
    let a = param(&mut func, Block(2));
    let b = param(&mut func, Block(3));
    func.blocks[0].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![yes],
    });
    func.blocks[1].preds = vec![Block(0), Block(3)];
    func.blocks[1].term = Term::Return(merged);
    func.blocks[2].preds = vec![Block(3)];
    func.blocks[2].term = Term::Jump(Edge {
        target: Block(3),
        args: vec![a],
    });
    func.blocks[3].preds = vec![Block(2)];
    func.blocks[3].term = Term::Branch {
        flag: b,
        if_true: Edge {
            target: Block(2),
            args: vec![b],
        },
        if_false: Edge {
            target: Block(1),
            args: vec![b],
        },
    };
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.constant_producers, 1);
    assert_eq!(
        stats.phi_params, 0,
        "the complete incoming web must be grounded"
    );
    for value in [merged, a, b] {
        assert_eq!(func.values[value.index()].rep, Rep::Tagged);
    }
    assert_eq!(stats.tagged_views, 1);
    func.verify().unwrap();
}

#[test]
fn opt_bools_verifier_requires_whitelisted_arity_and_tagged_operands() {
    let mut func = empty(1, 1);
    let argument = emit(
        &mut func,
        Block(0),
        0,
        Opcode::Arg(0),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let state = frame(&mut func, 1, &[argument]);
    let predicate = emit(
        &mut func,
        Block(0),
        1,
        Opcode::OpaqueBool(Op::Null),
        &[argument],
        TypeSet::BOOLEAN,
        Rep::Bool,
    );
    attach_frame(&mut func, predicate, state);
    let returned = emit(
        &mut func,
        Block(0),
        2,
        Opcode::BoolToLisp,
        &[predicate],
        TypeSet::BOOLEAN,
        Rep::Tagged,
    );
    func.blocks[0].term = Term::Return(returned);
    func.verify().unwrap();
    let ValueDef::Inst(inst) = func.values[predicate.index()].def else {
        unreachable!()
    };
    for unsupported in [Op::Equal, Op::StringEqual, Op::StringLessp, Op::Call(1)] {
        let mut invalid = func.clone();
        invalid.insts[inst.index()].op = Opcode::OpaqueBool(unsupported);
        assert_eq!(invalid.verify(), Err(VerifyError::OperandArity(inst)));
    }
    let mut invalid = func.clone();
    invalid.insts[inst.index()].args.clear();
    assert_eq!(invalid.verify(), Err(VerifyError::OperandArity(inst)));
    let mut invalid = func.clone();
    invalid.insts[inst.index()].frame = None;
    assert_eq!(invalid.verify(), Err(VerifyError::MissingFrame(inst)));
    let mut invalid = func.clone();
    let flag = emit(
        &mut invalid,
        Block(0),
        0,
        Opcode::BoolConst(true),
        &[],
        TypeSet::T,
        Rep::Bool,
    );
    let ValueDef::Inst(flag_inst) = invalid.values[flag.index()].def else {
        unreachable!()
    };
    assert_eq!(invalid.blocks[0].insts.pop(), Some(flag_inst));
    invalid.blocks[0].insts.insert(1, flag_inst);
    invalid.insts[inst.index()].args = vec![flag];
    assert_eq!(
        invalid.verify(),
        Err(VerifyError::RepMismatch {
            inst: Some(inst),
            value: flag
        })
    );
    let mut invalid = func.clone();
    invalid.values[predicate.index()].rep = Rep::Tagged;
    assert_eq!(
        invalid.verify(),
        Err(VerifyError::RepMismatch {
            inst: Some(inst),
            value: predicate
        })
    );
    assert!(Opcode::OpaqueBool(Op::Null).requires_frame(Effects::PURE));
    assert!(!Opcode::OpaqueBool(Op::Null).is_safepoint(Effects::PURE));
    assert!(Opcode::OpaqueBool(Op::Null).is_safepoint(Effects::MAY_GC));
    assert!(Opcode::OpaqueBool(Op::Null).is_safepoint(Effects::MAY_REENTER));
}
