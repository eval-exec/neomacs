//! Verified fixtures for dominator reuse and conservative mutable epochs.

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

fn inst_of(func: &Func, value: Value) -> Inst {
    let ValueDef::Inst(inst) = func.values[value.index()].def else {
        panic!("instruction result")
    };
    inst
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

fn ordered(func: &mut Func, value: Value, eff: Effects, mem: AliasClass, stack: &[Value]) {
    let inst = inst_of(func, value);
    let pc = func.insts[inst.index()].pc;
    let state = frame(func, pc, stack);
    let data = &mut func.insts[inst.index()];
    data.eff = eff;
    data.mem = mem;
    data.frame = Some(state);
}

fn arg(func: &mut Func, index: u16) -> Value {
    emit(
        func,
        Block(0),
        0,
        Opcode::Arg(index),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    )
}

fn literal(func: &mut Func, block: Block, pool: u32) -> Value {
    let ty = TypeSet::for_constant(func.consts[pool as usize]);
    emit(func, block, 0, Opcode::Const(pool), &[], ty, Rep::TaggedFix)
}

fn checked_cons(func: &mut Func, input: Value) -> Value {
    let result = emit(
        func,
        Block(0),
        1,
        Opcode::CheckType(TypeSet::CONS),
        &[input],
        TypeSet::CONS,
        Rep::Tagged,
    );
    ordered(func, result, Effects::MAY_DEOPT, AliasClass::None, &[input]);
    result
}

#[derive(Clone, Copy)]
enum Field {
    Car,
    Cdr,
}

impl Field {
    fn load(self) -> Opcode {
        match self {
            Self::Car => Opcode::LoadCar,
            Self::Cdr => Opcode::LoadCdr,
        }
    }
    fn store(self) -> Opcode {
        Opcode::Opaque(match self {
            Self::Car => Op::Setcar,
            Self::Cdr => Op::Setcdr,
        })
    }
    fn alias(self) -> AliasClass {
        match self {
            Self::Car => AliasClass::ConsCar,
            Self::Cdr => AliasClass::ConsCdr,
        }
    }
    fn other(self) -> Self {
        match self {
            Self::Car => Self::Cdr,
            Self::Cdr => Self::Car,
        }
    }
}

fn load(func: &mut Func, block: Block, pc: u32, field: Field, base: Value) -> Value {
    let result = emit(
        func,
        block,
        pc,
        field.load(),
        &[base],
        TypeSet::TOP,
        Rep::Tagged,
    );
    ordered(func, result, Effects::READ_HEAP, field.alias(), &[base]);
    result
}

fn store(func: &mut Func, block: Block, pc: u32, field: Field, base: Value, value: Value) -> Value {
    let result = emit(
        func,
        block,
        pc,
        field.store(),
        &[base, value],
        TypeSet::TOP,
        Rep::Tagged,
    );
    ordered(
        func,
        result,
        Effects::WRITE_HEAP.with(Effects::MAY_DEOPT),
        field.alias(),
        &[base, value],
    );
    result
}

fn comparison(func: &mut Func, block: Block, pc: u32, a: Value, b: Value) -> Value {
    emit(
        func,
        block,
        pc,
        Opcode::FixCmp(Cmp::Lt),
        &[a, b],
        TypeSet::BOOLEAN,
        Rep::Bool,
    )
}

fn boolean_view(func: &mut Func, block: Block, pc: u32, value: Value) -> Value {
    emit(
        func,
        block,
        pc,
        Opcode::BoolToLisp,
        &[value],
        TypeSet::BOOLEAN,
        Rep::Tagged,
    )
}

fn copy_of(func: &Func, result: Value, leader: Value) {
    let data = &func.insts[inst_of(func, result).index()];
    assert!(matches!(data.op, Opcode::Refine(_)), "{:?}", data.op);
    assert_eq!(data.args, vec![leader]);
    assert_eq!(data.eff, Effects::PURE);
    assert_eq!(data.mem, AliasClass::None);
}

fn retained(func: &Func, value: Value, old: &InstData) {
    let new = &func.insts[inst_of(func, value).index()];
    assert_eq!(new.op, old.op);
    assert_eq!(new.args, old.args);
    assert_eq!(new.result, old.result);
    assert_eq!(new.eff, old.eff);
    assert_eq!(new.mem, old.mem);
    assert_eq!(new.pc, old.pc);
    assert_eq!(new.frame, old.frame);
}

fn observers_preserved(before: &Func, after: &Func) {
    assert_eq!(before.frames, after.frames);
    assert_eq!(before.entry_stacks, after.entry_stacks);
    assert_eq!(before.entry, after.entry);
    assert_eq!(before.osr, after.osr);
    assert_eq!(before.source_states.len(), after.source_states.len());
    for (old, new) in before.source_states.iter().zip(&after.source_states) {
        match (old, new) {
            (Some(old), Some(new)) => {
                assert_eq!(old.pre, new.pre);
                assert_eq!(old.post, new.post);
                assert_eq!(old.frame, new.frame);
                assert_eq!(old.block, new.block);
            }
            (None, None) => {}
            _ => panic!("source-state shape changed"),
        }
    }
    for (old, new) in before.values.iter().zip(&after.values) {
        assert_eq!(old.def, new.def);
        assert_eq!(old.rep, new.rep);
        assert_eq!(old.ty, new.ty);
    }
    for (old, new) in before.insts.iter().zip(&after.insts) {
        assert_eq!(old.pc, new.pc);
        assert_eq!(old.frame, new.frame);
        assert_eq!(old.result, new.result);
    }
}

fn diamond(func: &mut Func, flag: Value) {
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
    for arm in [1, 2] {
        func.blocks[arm].preds = vec![Block(0)];
        func.blocks[arm].term = Term::Jump(Edge {
            target: Block(3),
            args: vec![],
        });
    }
    func.blocks[3].preds = vec![Block(1), Block(2)];
}

#[test]
fn opt_gvn_reuses_dominating_pure_fixnum_comparison_preserving_observers() {
    let mut func = empty(1, 0);
    let a = literal(&mut func, Block(0), 1);
    let b = literal(&mut func, Block(0), 2);
    let first = comparison(&mut func, Block(0), 2, a, b);
    let second = comparison(&mut func, Block(0), 3, a, b);
    let observed = frame(&mut func, 3, &[a, b, first]);
    let second_inst = inst_of(&func, second);
    func.insts[second_inst.index()].frame = Some(observed);
    func.source_states.resize_with(4, || None);
    func.source_states[3] = Some(SourceState {
        pre: vec![a, b, first].into(),
        post: vec![a, b, second].into(),
        frame: observed,
        block: Block(0),
    });
    func.entry_stacks = vec![vec![].into()];
    let returned = boolean_view(&mut func, Block(0), 4, second);
    func.blocks[0].term = Term::Return(returned);
    func.verify().unwrap();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.pure_reuses, 1);
    copy_of(&func, second, first);
    observers_preserved(&before, &func);
    func.verify().unwrap();
}

#[test]
fn opt_gvn_reuses_total_boolean_semantic_projection() {
    let mut func = empty(1, 0);
    let flag = emit(
        &mut func,
        Block(0),
        0,
        Opcode::BoolConst(true),
        &[],
        TypeSet::T,
        Rep::Bool,
    );
    let first = boolean_view(&mut func, Block(0), 1, flag);
    let second = boolean_view(&mut func, Block(0), 2, flag);
    func.blocks[0].term = Term::Return(second);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.pure_reuses, 1);
    copy_of(&func, second, first);
    func.verify().unwrap();
}

#[test]
fn opt_gvn_reuses_repeated_proven_cons_car_and_cdr() {
    for field in [Field::Car, Field::Cdr] {
        let mut func = empty(1, 1);
        let input = arg(&mut func, 0);
        let base = checked_cons(&mut func, input);
        let first = load(&mut func, Block(0), 2, field, base);
        let second = load(&mut func, Block(0), 3, field, base);
        func.blocks[0].term = Term::Return(second);
        func.verify().unwrap();
        let before = func.clone();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats.load_reuses, 1);
        copy_of(&func, second, first);
        observers_preserved(&before, &func);
        func.verify().unwrap();
    }
}

#[test]
fn opt_gvn_reuses_load_through_single_predecessor_dominated_chain() {
    let mut func = empty(2, 1);
    let input = arg(&mut func, 0);
    let base = checked_cons(&mut func, input);
    let first = load(&mut func, Block(0), 2, Field::Car, base);
    let second = load(&mut func, Block(1), 3, Field::Car, base);
    func.blocks[0].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![],
    });
    func.blocks[1].preds = vec![Block(0)];
    func.blocks[1].term = Term::Return(second);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.load_reuses, 1);
    copy_of(&func, second, first);
    func.verify().unwrap();
}

#[test]
fn opt_gvn_forwards_retained_opaque_cons_setters_exact_stored_value() {
    for field in [Field::Car, Field::Cdr] {
        let mut func = empty(1, 2);
        let input = arg(&mut func, 0);
        let value = arg(&mut func, 1);
        let base = checked_cons(&mut func, input);
        let setter = store(&mut func, Block(0), 2, field, base, value);
        let old = func.insts[inst_of(&func, setter).index()].clone();
        let read = load(&mut func, Block(0), 3, field, base);
        func.blocks[0].term = Term::Return(read);
        func.verify().unwrap();
        let before = func.clone();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats.store_forwards, 1);
        assert_eq!(stats.load_reuses, 0);
        copy_of(&func, read, value);
        retained(&func, setter, &old);
        observers_preserved(&before, &func);
        func.verify().unwrap();
    }
}

#[test]
fn opt_gvn_opposite_cons_field_store_preserves_load_epoch() {
    for field in [Field::Car, Field::Cdr] {
        let mut func = empty(1, 2);
        let input = arg(&mut func, 0);
        let value = arg(&mut func, 1);
        let base = checked_cons(&mut func, input);
        let first = load(&mut func, Block(0), 2, field, base);
        let setter = store(&mut func, Block(0), 3, field.other(), base, value);
        let old = func.insts[inst_of(&func, setter).index()].clone();
        let second = load(&mut func, Block(0), 4, field, base);
        func.blocks[0].term = Term::Return(second);
        func.verify().unwrap();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats.load_reuses, 1);
        assert_eq!(stats.store_forwards, 0);
        copy_of(&func, second, first);
        retained(&func, setter, &old);
        func.verify().unwrap();
    }
}

#[test]
fn opt_gvn_same_field_alias_store_kills_old_load_and_forwards_new_value() {
    let mut func = empty(1, 2);
    let input = arg(&mut func, 0);
    let value = arg(&mut func, 1);
    let base = checked_cons(&mut func, input);
    let alias = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty: TypeSet::CONS,
        rep: Rep::Tagged,
        def: ValueDef::Alias(base),
    });
    let first = load(&mut func, Block(0), 2, Field::Car, base);
    let setter = store(&mut func, Block(0), 3, Field::Car, alias, value);
    let old = func.insts[inst_of(&func, setter).index()].clone();
    let second = load(&mut func, Block(0), 4, Field::Car, base);
    func.blocks[0].term = Term::Return(second);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.store_forwards, 1);
    assert_eq!(stats.load_reuses, 0);
    copy_of(&func, second, value);
    assert_ne!(func.insts[inst_of(&func, second).index()].args, vec![first]);
    retained(&func, setter, &old);
    func.verify().unwrap();
}

#[test]
fn opt_gvn_distinct_runtime_base_same_field_store_invalidates_possible_alias() {
    let mut func = empty(1, 3);
    let a = arg(&mut func, 0);
    let b = arg(&mut func, 1);
    let value = arg(&mut func, 2);
    let base_a = checked_cons(&mut func, a);
    let base_b = checked_cons(&mut func, b);
    let _first = load(&mut func, Block(0), 2, Field::Car, base_a);
    store(&mut func, Block(0), 3, Field::Car, base_b, value);
    let second = load(&mut func, Block(0), 4, Field::Car, base_a);
    func.blocks[0].term = Term::Return(second);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.load_reuses, 0);
    assert_eq!(stats.store_forwards, 0);
    assert_eq!(
        func.insts[inst_of(&func, second).index()].op,
        Opcode::LoadCar
    );
    func.verify().unwrap();
}

#[test]
fn opt_gvn_one_arm_mutation_invalidates_join_but_local_join_reuse_remains() {
    for mutated_arm in [Block(1), Block(2)] {
        let mut func = empty(4, 3);
        let input = arg(&mut func, 0);
        let value = arg(&mut func, 1);
        let flag = arg(&mut func, 2);
        let base = checked_cons(&mut func, input);
        let _entry = load(&mut func, Block(0), 2, Field::Car, base);
        store(&mut func, mutated_arm, 3, Field::Car, base, value);
        let first = load(&mut func, Block(3), 4, Field::Car, base);
        let second = load(&mut func, Block(3), 5, Field::Car, base);
        diamond(&mut func, flag);
        func.blocks[3].term = Term::Return(second);
        func.verify().unwrap();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats.load_reuses, 1);
        assert_eq!(stats.store_forwards, 0);
        assert_eq!(
            func.insts[inst_of(&func, first).index()].op,
            Opcode::LoadCar
        );
        copy_of(&func, second, first);
        func.verify().unwrap();
    }
}

#[test]
fn opt_gvn_sibling_dominator_scopes_keep_pure_leaders_local() {
    let mut func = empty(4, 1);
    let flag = arg(&mut func, 0);
    let a = literal(&mut func, Block(0), 1);
    let b = literal(&mut func, Block(0), 2);
    let left_first = comparison(&mut func, Block(1), 2, a, b);
    let left_second = comparison(&mut func, Block(1), 3, a, b);
    let right_first = comparison(&mut func, Block(2), 2, a, b);
    let right_second = comparison(&mut func, Block(2), 3, a, b);
    diamond(&mut func, flag);
    func.blocks[3].term = Term::Return(flag);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.pure_reuses, 2);
    copy_of(&func, left_second, left_first);
    copy_of(&func, right_second, right_first);
    assert_eq!(
        func.insts[inst_of(&func, right_first).index()].op,
        Opcode::FixCmp(Cmp::Lt)
    );
    func.verify().unwrap();
}

#[test]
fn opt_gvn_sibling_store_epoch_does_not_leak_to_clean_arm() {
    let mut func = empty(4, 3);
    let input = arg(&mut func, 0);
    let value = arg(&mut func, 1);
    let flag = arg(&mut func, 2);
    let base = checked_cons(&mut func, input);
    let first = load(&mut func, Block(0), 2, Field::Car, base);
    store(&mut func, Block(1), 3, Field::Car, base, value);
    let left = load(&mut func, Block(1), 4, Field::Car, base);
    let right = load(&mut func, Block(2), 4, Field::Car, base);
    diamond(&mut func, flag);
    func.blocks[3].term = Term::Return(flag);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.store_forwards, 1);
    assert_eq!(stats.load_reuses, 1);
    copy_of(&func, left, value);
    copy_of(&func, right, first);
    func.verify().unwrap();
}

#[test]
fn opt_gvn_loop_header_reloads_after_backedge_but_reuses_within_iteration() {
    let mut func = empty(4, 3);
    let input = arg(&mut func, 0);
    let value = arg(&mut func, 1);
    let flag = arg(&mut func, 2);
    let base = checked_cons(&mut func, input);
    let _preheader = load(&mut func, Block(0), 2, Field::Car, base);
    let first = load(&mut func, Block(1), 3, Field::Car, base);
    let second = load(&mut func, Block(1), 4, Field::Car, base);
    store(&mut func, Block(2), 5, Field::Car, base, value);
    func.blocks[0].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![],
    });
    func.blocks[1].preds = vec![Block(0), Block(2)];
    func.blocks[1].loop_header = Some(LoopId(0));
    func.blocks[1].term = Term::Branch {
        flag,
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
        args: vec![],
    });
    func.blocks[3].preds = vec![Block(1)];
    func.blocks[3].term = Term::Return(second);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.load_reuses, 1);
    assert_eq!(stats.store_forwards, 0);
    assert_eq!(
        func.insts[inst_of(&func, first).index()].op,
        Opcode::LoadCar
    );
    copy_of(&func, second, first);
    func.verify().unwrap();
}

#[test]
fn opt_gvn_repeated_edges_from_one_predecessor_do_not_create_join_barrier() {
    let mut func = empty(2, 2);
    let input = arg(&mut func, 0);
    let flag = arg(&mut func, 1);
    let base = checked_cons(&mut func, input);
    let first = load(&mut func, Block(0), 2, Field::Car, base);
    let second = load(&mut func, Block(1), 3, Field::Car, base);
    func.blocks[0].term = Term::Branch {
        flag,
        if_true: Edge {
            target: Block(1),
            args: vec![],
        },
        if_false: Edge {
            target: Block(1),
            args: vec![],
        },
    };
    func.blocks[1].preds = vec![Block(0)];
    func.blocks[1].term = Term::Return(second);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.load_reuses, 1);
    copy_of(&func, second, first);
    func.verify().unwrap();
}

#[test]
fn opt_gvn_poll_call_gc_and_reentry_kill_loads_then_start_fresh_epoch() {
    for barrier in [
        Opcode::Poll,
        Opcode::Call { site: 0 },
        Opcode::Builtin(Op::Null),
        Opcode::Opaque(Op::Null),
    ] {
        let mut func = empty(1, 2);
        let input = arg(&mut func, 0);
        let callee = arg(&mut func, 1);
        let base = checked_cons(&mut func, input);
        let _before = load(&mut func, Block(0), 2, Field::Car, base);
        let barrier_inst = if barrier == Opcode::Poll {
            let id = Inst(func.insts.len() as u32);
            let state = frame(&mut func, 3, &[base]);
            func.insts.push(InstData {
                op: barrier.clone(),
                args: vec![],
                result: None,
                eff: Effects::MAY_GC.with(Effects::MAY_REENTER),
                mem: AliasClass::Unknown,
                frame: Some(state),
                pc: 3,
            });
            func.blocks[0].insts.push(id);
            id
        } else {
            let (args, eff) = match barrier {
                Opcode::Call { .. } => (vec![callee], Effects::UNKNOWN),
                Opcode::Builtin(_) => (vec![base], Effects::MAY_GC),
                _ => (vec![base], Effects::MAY_REENTER),
            };
            let result = emit(
                &mut func,
                Block(0),
                3,
                barrier.clone(),
                &args,
                TypeSet::TOP,
                Rep::Tagged,
            );
            ordered(&mut func, result, eff, AliasClass::Unknown, &[base, callee]);
            inst_of(&func, result)
        };
        let old = func.insts[barrier_inst.index()].clone();
        let first = load(&mut func, Block(0), 4, Field::Car, base);
        let second = load(&mut func, Block(0), 5, Field::Car, base);
        func.blocks[0].term = Term::Return(second);
        func.verify().unwrap();
        let before = func.clone();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats.load_reuses, 1);
        assert_eq!(
            func.insts[inst_of(&func, first).index()].op,
            Opcode::LoadCar
        );
        copy_of(&func, second, first);
        assert_eq!(func.insts[barrier_inst.index()].op, old.op);
        assert_eq!(func.insts[barrier_inst.index()].args, old.args);
        assert_eq!(func.insts[barrier_inst.index()].eff, old.eff);
        assert_eq!(func.insts[barrier_inst.index()].frame, old.frame);
        observers_preserved(&before, &func);
        func.verify().unwrap();
    }
}

#[test]
fn opt_gvn_preserves_exact_result_type_and_representation_boundaries() {
    let mut func = empty(1, 0);
    let zero = literal(&mut func, Block(0), 0);
    let singleton = TypeSet::for_constant(func.consts[0]);
    let first = emit(
        &mut func,
        Block(0),
        1,
        Opcode::UntagFix,
        &[zero],
        singleton,
        Rep::RawInt,
    );
    let second = emit(
        &mut func,
        Block(0),
        2,
        Opcode::UntagFix,
        &[zero],
        TypeSet::FIXNUM,
        Rep::RawInt,
    );
    let tagged_fix = emit(
        &mut func,
        Block(0),
        3,
        Opcode::TagFix,
        &[first],
        singleton,
        Rep::TaggedFix,
    );
    let tagged = emit(
        &mut func,
        Block(0),
        4,
        Opcode::TagFix,
        &[first],
        singleton,
        Rep::Tagged,
    );
    func.blocks[0].term = Term::Return(tagged);
    func.verify().unwrap();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats, GvnStats::default());
    assert_eq!(
        func.insts[inst_of(&func, second).index()].op,
        Opcode::UntagFix
    );
    assert_eq!(
        func.insts[inst_of(&func, tagged_fix).index()].op,
        Opcode::TagFix
    );
    assert_eq!(
        func.insts[inst_of(&func, tagged).index()].op,
        Opcode::TagFix
    );
    observers_preserved(&before, &func);
    func.verify().unwrap();
}

#[test]
fn opt_gvn_dynamic_prefix_and_swp_equality_are_not_pure_values() {
    let mut func = empty(1, 2);
    func.dynamic_prefix = 1;
    let a = arg(&mut func, 0);
    let b = arg(&mut func, 1);
    let first_env = emit(
        &mut func,
        Block(0),
        1,
        Opcode::EnvConst(0),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let second_env = emit(
        &mut func,
        Block(0),
        2,
        Opcode::EnvConst(0),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let first_prefix = emit(
        &mut func,
        Block(0),
        2,
        Opcode::Const(0),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let second_prefix = emit(
        &mut func,
        Block(0),
        2,
        Opcode::Const(0),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let first = emit(
        &mut func,
        Block(0),
        3,
        Opcode::OpaqueBool(Op::Eq),
        &[a, b],
        TypeSet::BOOLEAN,
        Rep::Bool,
    );
    let second = emit(
        &mut func,
        Block(0),
        4,
        Opcode::OpaqueBool(Op::Eq),
        &[a, b],
        TypeSet::BOOLEAN,
        Rep::Bool,
    );
    for value in [first, second] {
        ordered(
            &mut func,
            value,
            Effects::READ_HEAP.with(Effects::READ_BINDINGS),
            AliasClass::Unknown,
            &[a, b],
        );
    }
    let direct_first = emit(
        &mut func,
        Block(0),
        5,
        Opcode::Eq,
        &[a, b],
        TypeSet::BOOLEAN,
        Rep::Bool,
    );
    let direct_second = emit(
        &mut func,
        Block(0),
        6,
        Opcode::Eq,
        &[a, b],
        TypeSet::BOOLEAN,
        Rep::Bool,
    );
    let returned = boolean_view(&mut func, Block(0), 7, second);
    func.blocks[0].term = Term::Return(returned);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats, GvnStats::default());
    for value in [first_env, second_env] {
        assert_eq!(
            func.insts[inst_of(&func, value).index()].op,
            Opcode::EnvConst(0)
        );
    }
    for value in [first_prefix, second_prefix] {
        assert_eq!(
            func.insts[inst_of(&func, value).index()].op,
            Opcode::Const(0)
        );
    }
    for value in [first, second] {
        assert_eq!(
            func.insts[inst_of(&func, value).index()].op,
            Opcode::OpaqueBool(Op::Eq)
        );
    }
    for value in [direct_first, direct_second] {
        assert_eq!(func.insts[inst_of(&func, value).index()].op, Opcode::Eq);
    }
    func.verify().unwrap();
}

#[test]
fn opt_gvn_float_payloads_and_fresh_allocation_identities_remain_distinct() {
    let mut func = empty(1, 0);
    let zero = literal(&mut func, Block(0), 0);
    let payload = emit(
        &mut func,
        Block(0),
        1,
        Opcode::F64FromFix,
        &[zero],
        TypeSet::FLOAT,
        Rep::RawF64,
    );
    let first_sum = emit(
        &mut func,
        Block(0),
        2,
        Opcode::F64Add,
        &[payload, payload],
        TypeSet::FLOAT,
        Rep::RawF64,
    );
    let second_sum = emit(
        &mut func,
        Block(0),
        3,
        Opcode::F64Add,
        &[payload, payload],
        TypeSet::FLOAT,
        Rep::RawF64,
    );
    let first_float = emit(
        &mut func,
        Block(0),
        4,
        Opcode::AllocFloat,
        &[payload],
        TypeSet::FLOAT,
        Rep::Tagged,
    );
    let second_float = emit(
        &mut func,
        Block(0),
        5,
        Opcode::AllocFloat,
        &[payload],
        TypeSet::FLOAT,
        Rep::Tagged,
    );
    for value in [first_float, second_float] {
        ordered(
            &mut func,
            value,
            Effects::ALLOCATES.with(Effects::MAY_GC),
            AliasClass::None,
            &[payload],
        );
    }
    let first_cons = emit(
        &mut func,
        Block(0),
        6,
        Opcode::AllocCons,
        &[zero, zero],
        TypeSet::CONS,
        Rep::Tagged,
    );
    let second_cons = emit(
        &mut func,
        Block(0),
        7,
        Opcode::AllocCons,
        &[zero, zero],
        TypeSet::CONS,
        Rep::Tagged,
    );
    for value in [first_cons, second_cons] {
        ordered(
            &mut func,
            value,
            Effects::ALLOCATES,
            AliasClass::None,
            &[zero],
        );
    }
    func.blocks[0].term = Term::Return(second_cons);
    func.verify().unwrap();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats, GvnStats::default());
    for value in [
        first_sum,
        second_sum,
        first_float,
        second_float,
        first_cons,
        second_cons,
    ] {
        retained(&func, value, &before.insts[inst_of(&before, value).index()]);
    }
    func.verify().unwrap();
}

#[test]
fn opt_gvn_unproved_cons_loads_are_not_numbered() {
    let mut func = empty(1, 1);
    let base = arg(&mut func, 0);
    let first = load(&mut func, Block(0), 1, Field::Car, base);
    let second = load(&mut func, Block(0), 2, Field::Car, base);
    func.blocks[0].term = Term::Return(second);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats, GvnStats::default());
    for value in [first, second] {
        assert_eq!(
            func.insts[inst_of(&func, value).index()].op,
            Opcode::LoadCar
        );
    }
    func.verify().unwrap();
}

#[test]
fn opt_gvn_invalid_input_is_rejected_without_mutation() {
    let mut func = empty(1, 1);
    let unknown = arg(&mut func, 0);
    let raw = emit(
        &mut func,
        Block(0),
        1,
        Opcode::UntagFix,
        &[unknown],
        TypeSet::FIXNUM,
        Rep::RawInt,
    );
    let returned = emit(
        &mut func,
        Block(0),
        2,
        Opcode::TagFix,
        &[raw],
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    func.blocks[0].term = Term::Return(returned);
    let original_error = func.verify().unwrap_err();
    let before = func.display().to_string();
    assert_eq!(run(&mut func).unwrap_err(), original_error);
    assert_eq!(func.display().to_string(), before);
}

// TMP ONLY: append to passes/tests/gvn_test.rs after root releases fixture
// publication. No production transformation is installed by this draft.

fn list_checked(func: &mut Func, input: Value, pc: u32) -> Value {
    let result = emit(
        func,
        Block(0),
        pc,
        Opcode::CheckType(TypeSet::LIST),
        &[input],
        TypeSet::LIST,
        Rep::Tagged,
    );
    ordered(func, result, Effects::MAY_DEOPT, AliasClass::None, &[input]);
    result
}

fn list_opaque(func: &mut Func, block: Block, pc: u32, op: Op, base: Value) -> Value {
    let effects = super::super::super::build::op_effects(&op).0;
    let alias = match op {
        Op::Car | Op::CarSafe => AliasClass::ConsCar,
        Op::Cdr | Op::CdrSafe => AliasClass::ConsCdr,
        _ => panic!("list field opcode"),
    };
    let result = emit(
        func,
        block,
        pc,
        Opcode::Opaque(op),
        &[base],
        TypeSet::TOP,
        Rep::Tagged,
    );
    ordered(func, result, effects, alias, &[base]);
    result
}

#[test]
fn opt_gvn_lifts_checked_list_fields_and_reuses_repeated_read() {
    for op in [Op::Car, Op::Cdr, Op::CarSafe, Op::CdrSafe] {
        let field = if matches!(op, Op::Car | Op::CarSafe) {
            Field::Car
        } else {
            Field::Cdr
        };
        let mut func = empty(1, 1);
        let input = arg(&mut func, 0);
        let base = list_checked(&mut func, input, 1);
        let first = list_opaque(&mut func, Block(0), 2, op.clone(), base);
        let second = list_opaque(&mut func, Block(0), 3, op, base);
        func.blocks[0].term = Term::Return(second);
        func.verify().unwrap();
        let before = func.clone();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats.load_reuses, 1);
        assert_eq!(stats.store_forwards, 0);
        let first_inst = &func.insts[inst_of(&func, first).index()];
        assert_eq!(first_inst.op, field.load());
        assert_eq!(first_inst.eff, Effects::READ_HEAP);
        assert_eq!(first_inst.mem, field.alias());
        copy_of(&func, second, first);
        observers_preserved(&before, &func);
        func.verify().unwrap();
    }
}

#[test]
fn opt_gvn_list_car_preserves_cons_cdr_across_retained_setcar() {
    let mut func = empty(2, 2);
    let input = arg(&mut func, 0);
    let stored = arg(&mut func, 1);
    let base = checked_cons(&mut func, input);
    let first_cdr = load(&mut func, Block(0), 2, Field::Cdr, base);
    let tail = list_checked(&mut func, first_cdr, 3);
    let list_car = list_opaque(&mut func, Block(0), 4, Op::Car, tail);
    let setter = store(&mut func, Block(1), 5, Field::Car, base, stored);
    let setter_before = func.insts[inst_of(&func, setter).index()].clone();
    let second_cdr = load(&mut func, Block(1), 6, Field::Cdr, base);
    func.blocks[0].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![],
    });
    func.blocks[1].preds = vec![Block(0)];
    func.blocks[1].term = Term::Return(second_cdr);
    func.verify().unwrap();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.load_reuses, 1);
    assert_eq!(stats.store_forwards, 0);
    assert_eq!(
        func.insts[inst_of(&func, list_car).index()].op,
        Opcode::LoadCar
    );
    copy_of(&func, second_cdr, first_cdr);
    retained(&func, setter, &setter_before);
    observers_preserved(&before, &func);
    func.verify().unwrap();
}

#[test]
fn opt_gvn_unknown_list_reader_still_invalidates_cons_load_epoch() {
    let mut func = empty(1, 2);
    let input = arg(&mut func, 0);
    let unknown = arg(&mut func, 1);
    let base = checked_cons(&mut func, input);
    let _first = load(&mut func, Block(0), 2, Field::Cdr, base);
    let read = list_opaque(&mut func, Block(0), 3, Op::Car, unknown);
    let before_read = func.insts[inst_of(&func, read).index()].clone();
    let second = load(&mut func, Block(0), 4, Field::Cdr, base);
    func.blocks[0].term = Term::Return(second);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.load_reuses, 0);
    retained(&func, read, &before_read);
    assert_eq!(
        func.insts[inst_of(&func, second).index()].op,
        Opcode::LoadCdr
    );
    func.verify().unwrap();
}

#[test]
fn opt_gvn_checked_list_reader_requires_exact_read_effect_and_alias() {
    for (eff, alias) in [
        (
            Effects::READ_HEAP
                .with(Effects::MAY_DEOPT)
                .with(Effects::MAY_GC),
            AliasClass::ConsCar,
        ),
        (
            Effects::READ_HEAP.with(Effects::MAY_DEOPT),
            AliasClass::Unknown,
        ),
        (Effects::PURE, AliasClass::ConsCar),
    ] {
        let mut func = empty(1, 2);
        let input = arg(&mut func, 0);
        let list = arg(&mut func, 1);
        let base = checked_cons(&mut func, input);
        let tail = list_checked(&mut func, list, 1);
        let _first = load(&mut func, Block(0), 2, Field::Cdr, base);
        let read = list_opaque(&mut func, Block(0), 3, Op::Car, tail);
        let id = inst_of(&func, read);
        func.insts[id.index()].eff = eff;
        func.insts[id.index()].mem = alias;
        let before_read = func.insts[id.index()].clone();
        let second = load(&mut func, Block(0), 4, Field::Cdr, base);
        func.blocks[0].term = Term::Return(second);
        func.verify().unwrap();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats.load_reuses, 0);
        retained(&func, read, &before_read);
        assert_eq!(
            func.insts[inst_of(&func, second).index()].op,
            Opcode::LoadCdr
        );
        func.verify().unwrap();
    }
}

#[test]
fn opt_gvn_checked_list_reader_keeps_same_field_possible_alias_kill() {
    // A list-read lift never changes the existing possible-alias kill rule.
    for field in [Field::Car, Field::Cdr] {
        let mut func = empty(1, 2);
        let input = arg(&mut func, 0);
        let other = arg(&mut func, 1);
        let base = list_checked(&mut func, input, 1);
        let first = list_opaque(
            &mut func,
            Block(0),
            2,
            if matches!(field, Field::Car) {
                Op::Car
            } else {
                Op::Cdr
            },
            base,
        );
        let unknown_base = checked_cons(&mut func, other);
        let stored = literal(&mut func, Block(0), 1);
        store(&mut func, Block(0), 3, field, unknown_base, stored);
        let second = list_opaque(
            &mut func,
            Block(0),
            4,
            if matches!(field, Field::Car) {
                Op::Car
            } else {
                Op::Cdr
            },
            base,
        );
        func.blocks[0].term = Term::Return(second);
        func.verify().unwrap();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats.load_reuses, 0);
        assert_eq!(stats.store_forwards, 0);
        assert_eq!(func.insts[inst_of(&func, first).index()].op, field.load());
        assert_eq!(func.insts[inst_of(&func, second).index()].op, field.load());
        func.verify().unwrap();
    }
}
