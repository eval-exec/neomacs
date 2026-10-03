//! Grounded numeric webs and exact source-state boundaries.

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
            ValueBits::from_value(LispValue::make_int(0)),
            ValueBits::from_value(LispValue::make_int(1)),
            ValueBits::from_value(LispValue::make_int(100)),
            ValueBits::from_value(LispValue::T),
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

fn literal(func: &mut Func, block: Block, pool: u32) -> Value {
    let pc = func.blocks[block.index()].pc;
    let ty = TypeSet::for_constant(func.consts[pool as usize]);
    emit(
        func,
        block,
        pc,
        Opcode::Const(pool),
        &[],
        ty,
        Rep::TaggedFix,
    )
}

fn param(func: &mut Func, block: Block, ty: TypeSet, rep: Rep) -> Value {
    let value = Value(func.values.len() as u32);
    let index = func.blocks[block.index()].params.len() as u32;
    func.values.push(ValueData {
        ty,
        rep,
        def: ValueDef::Param { block, index },
    });
    func.blocks[block.index()].params.push(value);
    value
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

fn inst_of(func: &Func, value: Value) -> Inst {
    let ValueDef::Inst(inst) = func.values[value.index()].def else {
        panic!("instruction result")
    };
    inst
}

fn checked(func: &mut Func, block: Block, pc: u32, op: Opcode, args: &[Value]) -> Value {
    let state = frame(func, pc, args);
    let value = emit(func, block, pc, op, args, TypeSet::FIXNUM, Rep::TaggedFix);
    let id = inst_of(func, value);
    func.insts[id.index()].frame = Some(state);
    func.insts[id.index()].eff = Effects::MAY_DEOPT;
    value
}

fn loop_add() -> (Func, Value, Value, Value) {
    let mut func = empty(4, 0);
    let zero = literal(&mut func, Block(0), 0);
    let one = literal(&mut func, Block(0), 1);
    let bound = literal(&mut func, Block(0), 2);
    let carried = param(&mut func, Block(1), TypeSet::FIXNUM, Rep::TaggedFix);
    let condition = emit(
        &mut func,
        Block(1),
        1,
        Opcode::FixCmp(Cmp::Lt),
        &[carried, bound],
        TypeSet::BOOLEAN,
        Rep::Bool,
    );
    let next = checked(
        &mut func,
        Block(2),
        2,
        Opcode::FixAdd { checked: true },
        &[carried, one],
    );
    func.blocks[0].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![zero],
    });
    func.blocks[1].preds = vec![Block(0), Block(2)];
    func.blocks[1].term = Term::Branch {
        flag: condition,
        if_true: Edge {
            target: Block(2),
            args: vec![],
        },
        if_false: Edge {
            target: Block(3),
            args: vec![],
        },
    };
    func.blocks[2].preds = vec![Block(1)];
    func.blocks[2].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![next],
    });
    func.blocks[3].preds = vec![Block(1)];
    func.blocks[3].term = Term::Return(carried);
    func.verify().unwrap();
    (func, carried, next, one)
}

#[test]
fn opt_reps_grounded_loop_phi_checked_add_and_compare_select_raw() {
    let (mut func, carried, next, _) = loop_add();
    let original_frame = func.insts[inst_of(&func, next).index()].frame;
    let stats = run(&mut func).unwrap();
    assert_eq!(func.values[carried.index()].rep, Rep::RawInt);
    assert_eq!(func.values[next.index()].rep, Rep::RawInt);
    assert_eq!(stats.raw_phis, 1);
    assert_eq!(stats.raw_arithmetic, 1);
    assert_eq!(
        func.insts[inst_of(&func, next).index()].frame,
        original_frame
    );
    let comparison = func.blocks[1]
        .insts
        .iter()
        .copied()
        .find(|&id| matches!(func.insts[id.index()].op, Opcode::FixCmp(_)))
        .unwrap();
    for &value in &func.insts[comparison.index()].args {
        assert_eq!(
            func.values[func.resolve(value).unwrap().index()].rep,
            Rep::RawInt
        );
    }
    let Term::Return(returned) = func.blocks[3].term else {
        panic!("Lisp return")
    };
    assert!(matches!(
        func.insts[inst_of(&func, returned).index()].op,
        Opcode::TagFix
    ));
    func.verify().unwrap();
}

#[test]
fn opt_reps_top_seeded_two_phi_cycle_infers_fixnum() {
    let mut func = empty(4, 0);
    let seed = literal(&mut func, Block(0), 0);
    let flag = emit(
        &mut func,
        Block(0),
        0,
        Opcode::Const(3),
        &[],
        TypeSet::T,
        Rep::Tagged,
    );
    let a = param(&mut func, Block(1), TypeSet::TOP, Rep::Tagged);
    let b = param(&mut func, Block(2), TypeSet::TOP, Rep::Tagged);
    func.blocks[0].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![seed],
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
            args: vec![],
        },
    };
    func.blocks[2].preds = vec![Block(1)];
    func.blocks[2].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![b],
    });
    func.blocks[3].preds = vec![Block(1)];
    func.blocks[3].term = Term::Return(a);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.raw_phis, 2);
    for value in [a, b] {
        assert_eq!(func.values[value.index()].rep, Rep::RawInt);
        assert!(!func.values[value.index()].ty.is_bottom());
        assert!(func.values[value.index()].ty.is_subset(TypeSet::FIXNUM));
    }
    func.verify().unwrap();
}

#[test]
fn opt_reps_unknown_arg_environment_and_osr_inputs_prune_phi() {
    for source in [Opcode::Arg(0), Opcode::EnvConst(0), Opcode::OsrSlot(0)] {
        let mut func = empty(4, 1);
        if source == Opcode::EnvConst(0) {
            func.dynamic_prefix = 1;
        }
        if source == Opcode::OsrSlot(0) {
            func.osr = Some(OsrEntry {
                header: Block(0),
                entry_pc: 0,
                depth: 1,
            });
        }
        let unknown = emit(
            &mut func,
            Block(0),
            0,
            source,
            &[],
            TypeSet::TOP,
            Rep::Tagged,
        );
        let seed = literal(&mut func, Block(0), 1);
        let flag = emit(
            &mut func,
            Block(0),
            0,
            Opcode::Const(3),
            &[],
            TypeSet::T,
            Rep::Tagged,
        );
        let joined = param(&mut func, Block(3), TypeSet::TOP, Rep::Tagged);
        func.blocks[0].term = Term::Branch {
            flag,
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
            args: vec![seed],
        });
        func.blocks[2].term = Term::Jump(Edge {
            target: Block(3),
            args: vec![unknown],
        });
        func.blocks[3].term = Term::Return(joined);
        func.verify().unwrap();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats.raw_phis, 0);
        assert_eq!(func.values[joined.index()].rep, Rep::Tagged);
        assert_eq!(func.values[unknown.index()].rep, Rep::Tagged);
        func.verify().unwrap();
    }
}

#[test]
fn opt_reps_seedless_phi_cycle_is_not_a_fixnum_proof() {
    let mut func = empty(3, 0);
    let returned = literal(&mut func, Block(0), 0);
    func.blocks[0].term = Term::Return(returned);
    let a = param(&mut func, Block(1), TypeSet::TOP, Rep::Tagged);
    let b = param(&mut func, Block(2), TypeSet::TOP, Rep::Tagged);
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
    assert_eq!(run(&mut func).unwrap(), RepsStats::default());
    assert_eq!(func.display().to_string(), before);
}

#[test]
fn opt_reps_boundary_heavy_straight_mul_keeps_tagged_arithmetic() {
    let mut func = empty(1, 0);
    let a = literal(&mut func, Block(0), 1);
    let b = literal(&mut func, Block(0), 2);
    let product = checked(
        &mut func,
        Block(0),
        1,
        Opcode::FixMul { checked: true },
        &[a, b],
    );
    func.blocks[0].term = Term::Return(product);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(func.values[product.index()].rep, Rep::TaggedFix);
    assert_eq!(stats.raw_arithmetic, 0);
    assert_eq!(stats.tagged_arithmetic, 1);
    assert_eq!(stats.tagged_views, 0);
    func.verify().unwrap();
}

#[test]
fn opt_reps_source_frames_aliases_and_cached_tagged_escapes_survive() {
    let (mut func, carried, next, one) = loop_add();
    let add = inst_of(&func, next);
    let state = func.insts[add.index()].frame.unwrap();
    func.entry_stacks = vec![
        vec![].into(),
        vec![carried].into(),
        vec![carried, one].into(),
        vec![carried].into(),
    ];
    func.source_states = vec![None; 4];
    func.source_states[2] = Some(SourceState {
        pre: vec![carried, one].into(),
        post: vec![next].into(),
        frame: state,
        block: Block(2),
    });
    let alias = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty: TypeSet::FIXNUM,
        rep: Rep::TaggedFix,
        def: ValueDef::Alias(carried),
    });
    let observer_frame = frame(&mut func, 3, &[carried, alias]);
    let observed = emit(
        &mut func,
        Block(3),
        3,
        Opcode::Opaque(Op::Eq),
        &[carried, alias],
        TypeSet::BOOLEAN,
        Rep::Tagged,
    );
    let observer = inst_of(&func, observed);
    func.insts[observer.index()].eff = Effects::MAY_GC.with(Effects::MAY_DEOPT);
    func.insts[observer.index()].frame = Some(observer_frame);
    func.blocks[3].term = Term::Return(observed);
    func.verify().unwrap();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(func.values[carried.index()].rep, Rep::RawInt);
    assert_eq!(func.values[alias.index()].rep, Rep::RawInt);
    assert_eq!(
        func.values[alias.index()].def,
        before.values[alias.index()].def
    );
    assert_eq!(func.frames, before.frames);
    assert_eq!(func.entry_stacks, before.entry_stacks);
    for (before, after) in before.source_states.iter().zip(&func.source_states) {
        match (before, after) {
            (Some(before), Some(after)) => {
                assert_eq!(before.pre, after.pre);
                assert_eq!(before.post, after.post);
                assert_eq!(before.frame, after.frame);
                assert_eq!(before.block, after.block);
            }
            (None, None) => {}
            _ => panic!("source state changed"),
        }
    }
    let observed = &func.insts[observer.index()];
    assert_eq!(
        observed.args[0], observed.args[1],
        "aliases share one semantic view"
    );
    assert_eq!(observed.frame, before.insts[observer.index()].frame);
    assert_eq!(observed.eff, before.insts[observer.index()].eff);
    assert_eq!(observed.pc, before.insts[observer.index()].pc);
    assert_eq!(stats.tagged_views, 1);
    assert!(matches!(
        func.insts[inst_of(&func, observed.args[0]).index()].op,
        Opcode::TagFix
    ));
    func.verify().unwrap();
}

#[test]
fn opt_reps_refine_select_and_aliases_keep_same_definitions() {
    let (mut func, carried, next, _) = loop_add();
    let condition = match func.blocks[1].term {
        Term::Branch { flag, .. } => flag,
        _ => unreachable!(),
    };
    let refined = emit(
        &mut func,
        Block(2),
        2,
        Opcode::Refine(TypeSet::FIXNUM),
        &[next],
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    let selected = emit(
        &mut func,
        Block(2),
        2,
        Opcode::Select,
        &[condition, refined, next],
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    let alias = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty: TypeSet::FIXNUM,
        rep: Rep::TaggedFix,
        def: ValueDef::Alias(selected),
    });
    func.blocks[2].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![alias],
    });
    func.verify().unwrap();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.raw_phis, 1);
    for value in [carried, next, refined, selected, alias] {
        assert_eq!(func.values[value.index()].rep, Rep::RawInt);
        assert_eq!(
            func.values[value.index()].def,
            before.values[value.index()].def
        );
    }
    assert_eq!(func.frames, before.frames);
    let id = inst_of(&func, selected);
    assert_eq!(func.insts[id.index()].args[0], condition);
    for &arm in &func.insts[id.index()].args[1..] {
        assert_eq!(
            func.values[func.resolve(arm).unwrap().index()].rep,
            Rep::RawInt
        );
    }
    func.verify().unwrap();
}

#[test]
fn opt_reps_seeded_phi_with_independent_seedless_dependency_stays_tagged() {
    let mut func = empty(4, 0);
    let seed = literal(&mut func, Block(0), 0);
    let merged = param(&mut func, Block(1), TypeSet::TOP, Rep::Tagged);
    let a = param(&mut func, Block(2), TypeSet::TOP, Rep::Tagged);
    let b = param(&mut func, Block(3), TypeSet::TOP, Rep::Tagged);
    func.blocks[0].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![seed],
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
    assert_eq!(stats.raw_phis, 0);
    assert_eq!(stats.raw_values, 0);
    for value in [merged, a, b] {
        assert_eq!(func.values[value.index()].rep, Rep::Tagged);
    }
    func.verify().unwrap();
}

#[test]
fn opt_reps_publish_root_uses_exact_tagged_semantic_view() {
    let (mut func, carried, next, _) = loop_add();
    let semantic = emit(
        &mut func,
        Block(3),
        3,
        Opcode::Refine(TypeSet::FIXNUM),
        &[carried],
        TypeSet::FIXNUM,
        Rep::Tagged,
    );
    let root = Inst(func.insts.len() as u32);
    func.insts.push(InstData {
        op: Opcode::PublishRoot,
        args: vec![semantic],
        result: None,
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc: 3,
    });
    func.blocks[3].insts.push(root);
    func.blocks[3].term = Term::Return(semantic);
    let state = frame(&mut func, 3, &[carried]);
    func.entry_stacks = vec![
        vec![].into(),
        vec![carried].into(),
        vec![carried].into(),
        vec![carried].into(),
    ];
    func.source_states = vec![None; 4];
    func.source_states[3] = Some(SourceState {
        pre: vec![carried].into(),
        post: vec![semantic].into(),
        frame: state,
        block: Block(3),
    });
    func.verify().unwrap();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.raw_phis, 1);
    for value in [carried, next, semantic] {
        assert_eq!(func.values[value.index()].rep, Rep::RawInt);
        assert_eq!(
            func.values[value.index()].def,
            before.values[value.index()].def
        );
    }
    let published = func.insts[root.index()].args[0];
    assert_eq!(
        func.values[func.resolve(published).unwrap().index()].rep,
        Rep::Tagged
    );
    assert!(matches!(
        func.insts[inst_of(&func, published).index()].op,
        Opcode::TagFix
    ));
    assert_eq!(
        func.insts[inst_of(&func, published).index()].args,
        vec![semantic]
    );
    assert_eq!(func.insts[root.index()].eff, before.insts[root.index()].eff);
    assert_eq!(func.insts[root.index()].pc, before.insts[root.index()].pc);
    assert_eq!(func.frames, before.frames);
    assert_eq!(func.entry_stacks, before.entry_stacks);
    let old = before.source_states[3].as_ref().unwrap();
    let new = func.source_states[3].as_ref().unwrap();
    assert_eq!(new.pre, old.pre);
    assert_eq!(new.post, old.post);
    assert_eq!(new.frame, old.frame);
    assert_eq!(new.block, old.block);
    func.verify().unwrap();
}

#[test]
fn opt_reps_existing_typed_div_rem_minmax_keep_result_matching_operands() {
    for op in [
        Opcode::FixDiv,
        Opcode::FixRem,
        Opcode::FixMinMax(MinMax::Min),
    ] {
        for rep in [Rep::RawInt, Rep::TaggedFix] {
            let (mut func, carried, _, one) = loop_add();
            let args = if rep == Rep::RawInt {
                let raw_carried = emit(
                    &mut func,
                    Block(3),
                    3,
                    Opcode::UntagFix,
                    &[carried],
                    TypeSet::FIXNUM,
                    Rep::RawInt,
                );
                let raw_one = emit(
                    &mut func,
                    Block(3),
                    3,
                    Opcode::UntagFix,
                    &[one],
                    TypeSet::FIXNUM,
                    Rep::RawInt,
                );
                [raw_carried, raw_one]
            } else {
                [carried, one]
            };
            let result = emit(
                &mut func,
                Block(3),
                3,
                op.clone(),
                &args,
                TypeSet::FIXNUM,
                rep,
            );
            let original = inst_of(&func, result);
            if matches!(op, Opcode::FixDiv | Opcode::FixRem) {
                let state = frame(&mut func, 3, &args);
                func.insts[original.index()].frame = Some(state);
                func.insts[original.index()].eff = Effects::MAY_DEOPT;
            }
            func.blocks[3].term = if rep == Rep::RawInt {
                let semantic = emit(
                    &mut func,
                    Block(3),
                    3,
                    Opcode::TagFix,
                    &[result],
                    TypeSet::FIXNUM,
                    Rep::TaggedFix,
                );
                Term::Return(semantic)
            } else {
                Term::Return(result)
            };
            func.verify().unwrap();
            let before = func.clone();
            run(&mut func).unwrap();
            assert_eq!(func.values[result.index()].rep, rep);
            assert_eq!(func.insts[original.index()].op, op);
            assert_eq!(
                func.insts[original.index()].eff,
                before.insts[original.index()].eff
            );
            assert_eq!(
                func.insts[original.index()].frame,
                before.insts[original.index()].frame
            );
            assert_eq!(
                func.insts[original.index()].pc,
                before.insts[original.index()].pc
            );
            for &operand in &func.insts[original.index()].args {
                assert_eq!(func.values[func.resolve(operand).unwrap().index()].rep, rep);
            }
            func.verify().unwrap();
        }
    }
}

#[test]
fn opt_reps_existing_raw_checktype_preserves_guard_without_grounding_cycle() {
    let mut func = empty(2, 0);
    let returned = literal(&mut func, Block(0), 0);
    func.blocks[0].term = Term::Return(returned);
    let carried = param(&mut func, Block(1), TypeSet::FIXNUM, Rep::RawInt);
    let checked = emit(
        &mut func,
        Block(1),
        1,
        Opcode::CheckType(TypeSet::FIXNUM),
        &[carried],
        TypeSet::FIXNUM,
        Rep::RawInt,
    );
    let guard = inst_of(&func, checked);
    let state = frame(&mut func, 1, &[carried]);
    func.insts[guard.index()].frame = Some(state);
    func.insts[guard.index()].eff = Effects::MAY_DEOPT;
    func.blocks[1].preds = vec![Block(1)];
    func.blocks[1].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![checked],
    });
    func.verify().unwrap();
    let before = func.clone();
    let grounded = grounded_fixnums(&func);
    assert!(!grounded[carried.index()]);
    assert!(!grounded[checked.index()]);
    run(&mut func).unwrap();
    assert_eq!(func.insts[guard.index()].op, before.insts[guard.index()].op);
    assert_eq!(func.insts[guard.index()].args, vec![carried]);
    assert_eq!(
        func.insts[guard.index()].frame,
        before.insts[guard.index()].frame
    );
    assert_eq!(
        func.insts[guard.index()].eff,
        before.insts[guard.index()].eff
    );
    assert_eq!(func.insts[guard.index()].pc, before.insts[guard.index()].pc);
    assert_eq!(func.frames, before.frames);
    assert_eq!(func.values[carried.index()].rep, Rep::RawInt);
    assert_eq!(func.values[checked.index()].rep, Rep::RawInt);
    func.verify().unwrap();
}
