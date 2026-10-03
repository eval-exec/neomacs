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

// O3.5 proof-bearing unchecked arithmetic and independent bounds transport.
use crate::emacs_core::jit::opt::types::Range;

fn range_unchecked_for_reps(
    func: &mut Func,
    block: Block,
    pc: u32,
    op: Opcode,
    a: Value,
    b: Value,
    ty: TypeSet,
) -> Value {
    emit(func, block, pc, op, &[a, b], ty, Rep::TaggedFix)
}

fn range_input_for_reps(func: &mut Func, index: u16, interval: Range) -> Value {
    let original = emit(
        func,
        Block(0),
        0,
        Opcode::Arg(index),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let checked_ty = TypeSet::fixnum_range(interval);
    let checked = emit(
        func,
        Block(0),
        0,
        Opcode::CheckType(checked_ty),
        &[original],
        checked_ty,
        Rep::TaggedFix,
    );
    let state = frame(func, 0, &[original]);
    let id = inst_of(func, checked);
    func.insts[id.index()].eff = Effects::MAY_DEOPT;
    func.insts[id.index()].frame = Some(state);
    checked
}

#[test]
fn opt_reps_range_proven_loop_increment_keeps_grounded_raw_phi_web() {
    let (mut func, carried, next, one) = loop_add();
    let add = inst_of(&func, next);
    // The actual loop tests i < 100 and starts at 0. Range may expose this
    // taken-edge interval without narrowing the original full-domain phi.
    let narrowed = emit(
        &mut func,
        Block(2),
        2,
        Opcode::Refine(TypeSet::fixnum_range(Range { lo: 0, hi: 99 })),
        &[carried],
        TypeSet::fixnum_range(Range { lo: 0, hi: 99 }),
        Rep::TaggedFix,
    );
    let view_id = func.blocks[2].insts.pop().unwrap();
    assert_eq!(view_id, inst_of(&func, narrowed));
    func.blocks[2].insts.insert(0, view_id);
    func.insts[add.index()].args[0] = narrowed;
    func.insts[add.index()].op = Opcode::FixAdd { checked: false };
    func.insts[add.index()].eff = Effects::PURE;
    let recovery = func.insts[add.index()].frame.unwrap();
    func.entry_stacks = vec![
        vec![].into(),
        vec![carried].into(),
        vec![carried, one].into(),
        vec![carried].into(),
    ];
    func.source_states.resize_with(4, || None);
    func.source_states[2] = Some(SourceState {
        pre: vec![carried, one].into(),
        post: vec![next].into(),
        frame: recovery,
        block: Block(2),
    });
    func.verify().unwrap();
    let original_frames = func.frames.clone();
    let original_entries = func.entry_stacks.clone();
    let original_count = func.values.len();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.raw_phis, 1);
    assert_eq!(stats.raw_arithmetic, 1);
    assert_eq!(func.values[carried.index()].rep, Rep::RawInt);
    assert_eq!(func.values[carried.index()].ty, TypeSet::FIXNUM);
    assert_eq!(func.values[next.index()].rep, Rep::RawInt);
    assert_eq!(
        func.values[narrowed.index()].ty.range(),
        Some(Range { lo: 0, hi: 99 })
    );
    assert_eq!(func.frames, original_frames);
    assert_eq!(func.entry_stacks, original_entries);
    assert_eq!(func.insts[add.index()].frame, Some(recovery));
    assert_eq!(func.insts[add.index()].pc, 2);
    assert_eq!(func.insts[add.index()].result, Some(next));
    assert_eq!(
        func.source_states[2].as_ref().unwrap().pre.as_ref(),
        &[carried, one]
    );
    assert_eq!(
        func.source_states[2].as_ref().unwrap().post.as_ref(),
        &[next]
    );
    assert_eq!(func.source_states[2].as_ref().unwrap().frame, recovery);
    assert!(func.values.len() >= original_count);
    func.verify().unwrap();
}

#[test]
fn opt_reps_unchecked_endpoint_and_result_domains_require_independent_proof() {
    let full = Range::FULL;
    let singleton = |n| Range { lo: n, hi: n };
    let cases = [
        (
            Opcode::FixAdd { checked: false },
            full,
            singleton(0),
            TypeSet::FIXNUM,
            true,
        ),
        (
            Opcode::FixAdd { checked: false },
            full,
            singleton(1),
            TypeSet::FIXNUM,
            false,
        ),
        (
            Opcode::FixSub { checked: false },
            full,
            singleton(0),
            TypeSet::FIXNUM,
            true,
        ),
        (
            Opcode::FixSub { checked: false },
            full,
            singleton(1),
            TypeSet::FIXNUM,
            false,
        ),
        (
            Opcode::FixMul { checked: false },
            full,
            singleton(1),
            TypeSet::FIXNUM,
            true,
        ),
        (
            Opcode::FixMul { checked: false },
            full,
            singleton(-1),
            TypeSet::FIXNUM,
            false,
        ),
        (
            Opcode::FixMul { checked: false },
            singleton(full.hi),
            singleton(-1),
            TypeSet::FIXNUM,
            true,
        ),
        (
            Opcode::FixMul { checked: false },
            singleton(full.hi),
            singleton(2),
            TypeSet::FIXNUM,
            false,
        ),
        (
            Opcode::FixMul { checked: false },
            Range { lo: -8, hi: 8 },
            Range { lo: -8, hi: 8 },
            TypeSet::fixnum_range(Range { lo: -64, hi: 64 }),
            true,
        ),
        // All four signed Mul extrema are required. A narrower declaration
        // cannot hide the reachable -64 result even though fixnum overflow fits.
        (
            Opcode::FixMul { checked: false },
            Range { lo: -8, hi: 8 },
            Range { lo: -8, hi: 8 },
            TypeSet::fixnum_range(Range { lo: -63, hi: 64 }),
            false,
        ),
        (
            Opcode::FixAdd { checked: false },
            singleton(0),
            singleton(1),
            TypeSet::BOTTOM,
            false,
        ),
        (
            Opcode::FixAdd { checked: false },
            singleton(0),
            singleton(1),
            TypeSet::FIXNUM.join(TypeSet::FLOAT),
            false,
        ),
    ];
    for (op, a_range, b_range, output_ty, expected) in cases {
        let mut func = empty(1, 2);
        let a = range_input_for_reps(&mut func, 0, a_range);
        let b = range_input_for_reps(&mut func, 1, b_range);
        let result = range_unchecked_for_reps(&mut func, Block(0), 1, op.clone(), a, b, output_ty);
        func.blocks[0].term = Term::Return(result);
        // A mixed union is not a legal TaggedFix declaration. Keep its
        // defensive classifier negative explicit, without claiming it is a
        // verified transformation input or running the pass on invalid IR.
        if !output_ty.is_subset(TypeSet::FIXNUM) {
            assert!(matches!(func.verify(),
                Err(crate::emacs_core::jit::opt::verify::VerifyError::TypeMismatch(value))
                    if value == result));
            assert!(!grounded_fixnums(&func)[result.index()]);
            continue;
        }
        // Legal FIX-domain and bottom declarations verify first; independent
        // unchecked admission is deliberately stronger than this verifier.
        func.verify().unwrap();
        assert_eq!(
            grounded_fixnums(&func)[result.index()],
            expected,
            "{op:?} {a_range:?} {b_range:?} {output_ty:?}"
        );
        let original = func.insts[inst_of(&func, result).index()].clone();
        run(&mut func).unwrap();
        let current = &func.insts[inst_of(&func, result).index()];
        assert_eq!(current.op, original.op);
        assert_eq!(current.pc, original.pc);
        assert_eq!(current.frame, original.frame);
        assert_eq!(current.result, original.result);
        func.verify().unwrap();
    }
}

#[test]
fn opt_reps_unchecked_mul_cost_matches_plain_raw_product() {
    let mut func = empty(1, 0);
    let a = literal(&mut func, Block(0), 1);
    let b = literal(&mut func, Block(0), 2);
    let product = range_unchecked_for_reps(
        &mut func,
        Block(0),
        1,
        Opcode::FixMul { checked: false },
        a,
        b,
        TypeSet::FIXNUM,
    );
    func.blocks[0].term = Term::Return(product);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.raw_arithmetic, 1);
    assert_eq!(func.values[product.index()].rep, Rep::RawInt);
    assert_eq!(stats.tagged_views, 1);
    assert_eq!(
        func.insts[inst_of(&func, product).index()].op,
        Opcode::FixMul { checked: false }
    );
    let Term::Return(returned) = func.blocks[0].term else {
        panic!("Lisp return")
    };
    assert_eq!(
        func.insts[inst_of(&func, returned).index()].op,
        Opcode::TagFix
    );
    func.verify().unwrap();
}

#[test]
fn opt_reps_unchecked_taken_edge_view_does_not_prove_full_domain_sibling() {
    let mut func = empty(3, 1);
    let input = range_input_for_reps(&mut func, 0, Range::FULL);
    func.consts = func
        .consts
        .iter()
        .copied()
        .chain([ValueBits::from_value(LispValue::make_int(Range::FULL.hi))])
        .collect::<Vec<_>>()
        .into();
    let bound = literal(&mut func, Block(0), 4);
    let one = literal(&mut func, Block(0), 1);
    let condition = emit(
        &mut func,
        Block(0),
        1,
        Opcode::FixCmp(Cmp::Lt),
        &[input, bound],
        TypeSet::BOOLEAN,
        Rep::Bool,
    );
    let edge_ty = TypeSet::fixnum_range(Range {
        lo: Range::FULL.lo,
        hi: Range::FULL.hi - 1,
    });
    let edge_view = emit(
        &mut func,
        Block(1),
        2,
        Opcode::Refine(edge_ty),
        &[input],
        edge_ty,
        Rep::TaggedFix,
    );
    let safe = range_unchecked_for_reps(
        &mut func,
        Block(1),
        2,
        Opcode::FixAdd { checked: false },
        edge_view,
        one,
        TypeSet::FIXNUM,
    );
    let rejected = range_unchecked_for_reps(
        &mut func,
        Block(2),
        3,
        Opcode::FixAdd { checked: false },
        input,
        one,
        TypeSet::FIXNUM,
    );
    func.blocks[0].term = Term::Branch {
        flag: condition,
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
    func.blocks[1].term = Term::Return(safe);
    func.blocks[2].term = Term::Return(rejected);
    func.verify().unwrap();
    let proof = grounded_fixnums(&func);
    assert!(proof[safe.index()]);
    assert!(!proof[rejected.index()]);
    let frames = func.frames.clone();
    run(&mut func).unwrap();
    assert_eq!(func.values[input.index()].ty, TypeSet::FIXNUM);
    assert_eq!(func.values[edge_view.index()].ty, edge_ty);
    let sibling = &func.insts[inst_of(&func, rejected).index()];
    assert_eq!(func.values[sibling.args[0].index()].ty, TypeSet::FIXNUM);
    assert_eq!(func.values[rejected.index()].rep, Rep::TaggedFix);
    assert_eq!(func.frames, frames);
    func.verify().unwrap();
}

#[test]
fn opt_reps_unchecked_declared_fix_arg_does_not_seed_unknown_web() {
    let mut func = empty(1, 1);
    let arg = emit(
        &mut func,
        Block(0),
        0,
        Opcode::Arg(0),
        &[],
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    let zero = literal(&mut func, Block(0), 0);
    let sum = range_unchecked_for_reps(
        &mut func,
        Block(0),
        1,
        Opcode::FixAdd { checked: false },
        arg,
        zero,
        TypeSet::FIXNUM,
    );
    func.blocks[0].term = Term::Return(sum);
    func.verify().unwrap();
    assert!(!grounded_fixnums(&func)[sum.index()]);
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.raw_arithmetic, 0);
    assert_eq!(func.values[sum.index()].rep, Rep::TaggedFix);
    func.verify().unwrap();
}

#[test]
fn opt_reps_bounds_preserves_proven_raw_length_and_tagged_index_result() {
    let mut func = empty(1, 0);
    let index = literal(&mut func, Block(0), 1);
    let length_seed = literal(&mut func, Block(0), 2);
    let length_ty = func.values[length_seed.index()].ty;
    let raw_length = emit(
        &mut func,
        Block(0),
        1,
        Opcode::UntagFix,
        &[length_seed],
        length_ty,
        Rep::RawInt,
    );
    let index_ty = func.values[index.index()].ty;
    let result = emit(
        &mut func,
        Block(0),
        2,
        Opcode::CheckBounds,
        &[index, raw_length],
        index_ty,
        Rep::TaggedFix,
    );
    let check = inst_of(&func, result);
    let recovery = frame(&mut func, 2, &[index, length_seed]);
    func.insts[check.index()].frame = Some(recovery);
    func.insts[check.index()].eff = Effects::MAY_DEOPT;
    func.source_states.resize_with(3, || None);
    func.source_states[2] = Some(SourceState {
        pre: vec![index, length_seed].into(),
        post: vec![result].into(),
        frame: recovery,
        block: Block(0),
    });
    func.entry_stacks = vec![vec![].into()];
    func.blocks[0].term = Term::Return(result);
    func.verify().unwrap();
    let original = func.clone();
    let stats = run(&mut func).unwrap();
    let selected = &func.insts[check.index()];
    assert_eq!(selected.args, [index, raw_length]);
    assert_eq!(func.values[raw_length.index()].rep, Rep::RawInt);
    assert_eq!(func.values[result.index()].rep, Rep::TaggedFix);
    assert_eq!(
        stats.tagged_views, 0,
        "raw length needs no tag/untag boundary"
    );
    assert_eq!(selected.op, Opcode::CheckBounds);
    assert_eq!(selected.pc, 2);
    assert_eq!(selected.frame, Some(recovery));
    assert_eq!(selected.result, Some(result));
    assert_eq!(selected.eff, Effects::MAY_DEOPT);
    assert_eq!(func.frames, original.frames);
    assert_eq!(func.entry_stacks, original.entry_stacks);
    assert_eq!(
        func.source_states[2].as_ref().unwrap().pre,
        original.source_states[2].as_ref().unwrap().pre
    );
    assert_eq!(
        func.source_states[2].as_ref().unwrap().post,
        original.source_states[2].as_ref().unwrap().post
    );
    func.verify().unwrap();
}

#[test]
fn opt_reps_bounds_raw_index_result_accepts_independent_tagged_length() {
    let mut func = empty(1, 0);
    let tagged_index = literal(&mut func, Block(0), 1);
    let tagged_length = literal(&mut func, Block(0), 2);
    let index_ty = func.values[tagged_index.index()].ty;
    let raw_index = emit(
        &mut func,
        Block(0),
        1,
        Opcode::UntagFix,
        &[tagged_index],
        index_ty,
        Rep::RawInt,
    );
    let raw_result = emit(
        &mut func,
        Block(0),
        2,
        Opcode::CheckBounds,
        &[raw_index, tagged_length],
        index_ty,
        Rep::RawInt,
    );
    let check = inst_of(&func, raw_result);
    let recovery = frame(&mut func, 2, &[tagged_index, tagged_length]);
    func.insts[check.index()].frame = Some(recovery);
    func.insts[check.index()].eff = Effects::MAY_DEOPT;
    let returned = emit(
        &mut func,
        Block(0),
        3,
        Opcode::TagFix,
        &[raw_result],
        index_ty,
        Rep::TaggedFix,
    );
    func.blocks[0].term = Term::Return(returned);
    func.verify().unwrap();
    let original_frames = func.frames.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(func.insts[check.index()].args, [raw_index, tagged_length]);
    assert_eq!(func.values[raw_result.index()].rep, Rep::RawInt);
    assert_eq!(func.values[tagged_length.index()].rep, Rep::TaggedFix);
    assert_eq!(
        stats.tagged_views, 0,
        "the explicit semantic return view is original"
    );
    assert_eq!(func.insts[check.index()].result, Some(raw_result));
    assert_eq!(func.insts[check.index()].frame, Some(recovery));
    assert_eq!(func.frames, original_frames);
    func.verify().unwrap();
}

#[test]
fn opt_reps_bounds_noresult_guard_preserves_proven_raw_scalar_operands() {
    let mut func = empty(1, 0);
    let tagged_index = literal(&mut func, Block(0), 1);
    let tagged_length = literal(&mut func, Block(0), 2);
    let index_ty = func.values[tagged_index.index()].ty;
    let length_ty = func.values[tagged_length.index()].ty;
    let index = emit(
        &mut func,
        Block(0),
        1,
        Opcode::UntagFix,
        &[tagged_index],
        index_ty,
        Rep::RawInt,
    );
    let length = emit(
        &mut func,
        Block(0),
        1,
        Opcode::UntagFix,
        &[tagged_length],
        length_ty,
        Rep::RawInt,
    );
    let recovery = frame(&mut func, 2, &[tagged_index, tagged_length]);
    let check = Inst(func.insts.len() as u32);
    func.insts.push(InstData {
        op: Opcode::CheckBounds,
        args: vec![index, length],
        result: None,
        eff: Effects::MAY_DEOPT,
        mem: AliasClass::None,
        frame: Some(recovery),
        pc: 2,
    });
    func.blocks[0].insts.push(check);
    func.blocks[0].term = Term::Return(tagged_index);
    func.verify().unwrap();
    let before = func.insts[check.index()].clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(func.insts[check.index()].args, [index, length]);
    assert_eq!(stats.tagged_views, 0);
    assert_eq!(func.insts[check.index()].op, before.op);
    assert_eq!(func.insts[check.index()].frame, before.frame);
    assert_eq!(func.insts[check.index()].result, None);
    assert_eq!(func.insts[check.index()].pc, before.pc);
    func.verify().unwrap();
}

#[test]
fn opt_reps_bounds_length_boundary_cost_matches_raw_capable_consumer() {
    let mut func = empty(1, 0);
    let index = literal(&mut func, Block(0), 1);
    let length_seed = literal(&mut func, Block(0), 2);
    let length = checked(
        &mut func,
        Block(0),
        1,
        Opcode::FixMul { checked: true },
        &[length_seed, index],
    );
    let index_ty = func.values[index.index()].ty;
    let bounded = emit(
        &mut func,
        Block(0),
        2,
        Opcode::CheckBounds,
        &[index, length],
        index_ty,
        Rep::TaggedFix,
    );
    let check = inst_of(&func, bounded);
    let recovery = frame(&mut func, 2, &[index, length]);
    func.insts[check.index()].eff = Effects::MAY_DEOPT;
    func.insts[check.index()].frame = Some(recovery);
    func.blocks[0].term = Term::Return(bounded);
    func.verify().unwrap();
    let original_frames = func.frames.clone();
    let stats = run(&mut func).unwrap();
    // One checked-Mul conversion benefit, two direct static inputs with zero
    // untag cost, and no fabricated TagFix boundary for the raw-capable length.
    // This is the existing documented heuristic, not measured native cost.
    assert_eq!(stats.raw_arithmetic, 1);
    assert_eq!(func.values[length.index()].rep, Rep::RawInt);
    assert_eq!(func.insts[check.index()].args[1], length);
    assert_eq!(func.insts[check.index()].args[0], index);
    assert_eq!(func.values[bounded.index()].rep, Rep::TaggedFix);
    assert_eq!(stats.tagged_views, 0);
    assert_eq!(func.frames, original_frames);
    assert_eq!(func.insts[check.index()].op, Opcode::CheckBounds);
    assert_eq!(func.insts[check.index()].frame, Some(recovery));
    func.verify().unwrap();
}
