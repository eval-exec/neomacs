//! Verified fixtures for read reuse: which repeated car/cdr reads become
//! views of the first, and which instructions and joins end availability.

use super::*;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::opt::{
    mem::{AliasClass, Effects},
    types::TypeSet,
};
use crate::emacs_core::value::Value as LispValue;

fn empty(blocks: usize, args: usize) -> Func {
    let mut func = Func::new(
        vec![ValueBits::from_value(LispValue::make_int(1))].into(),
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
    (ty, rep): (TypeSet, Rep),
    (eff, mem): (Effects, AliasClass),
) -> Value {
    let inst = Inst(func.insts.len() as u32);
    let value = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty,
        rep,
        def: ValueDef::Inst(inst),
    });
    let frame = (eff != Effects::PURE).then(|| {
        func.intern_frame(FrameState {
            pc,
            stack: args.into(),
            handlers: 0,
            binds: 0,
            parent: None,
            site: None,
        })
    });
    func.insts.push(InstData {
        op,
        args: args.to_vec(),
        result: Some(value),
        eff,
        mem,
        frame,
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

fn arg(func: &mut Func) -> Value {
    emit(
        func,
        Block(0),
        0,
        Opcode::Arg(0),
        &[],
        (TypeSet::TOP, Rep::Tagged),
        (Effects::PURE, AliasClass::None),
    )
}

/// `(car x)`, `(cdr x)`, `(car-safe x)` or `(cdr-safe x)` as the builder
/// emits it.
fn read(func: &mut Func, block: Block, pc: u32, op: Op, base: Value) -> Value {
    let (eff, mem) = crate::emacs_core::jit::opt::build::op_effects(&op);
    emit(
        func,
        block,
        pc,
        Opcode::Opaque(op),
        &[base],
        (TypeSet::TOP, Rep::Tagged),
        (eff, mem),
    )
}

fn check(func: &mut Func, block: Block, pc: u32, ty: TypeSet, input: Value) -> Value {
    emit(
        func,
        block,
        pc,
        Opcode::CheckType(ty),
        &[input],
        (
            ty,
            if ty == TypeSet::FIXNUM {
                Rep::TaggedFix
            } else {
                Rep::Tagged
            },
        ),
        (Effects::MAY_DEOPT, AliasClass::None),
    )
}

fn setcdr(func: &mut Func, block: Block, pc: u32, base: Value, value: Value) -> Value {
    let (eff, mem) = crate::emacs_core::jit::opt::build::op_effects(&Op::Setcdr);
    emit(
        func,
        block,
        pc,
        Opcode::Opaque(Op::Setcdr),
        &[base, value],
        (TypeSet::TOP, Rep::Tagged),
        (eff, mem),
    )
}

fn view_of(func: &Func, result: Value, leader: Value) {
    let data = &func.insts[inst_of(func, result).index()];
    assert!(matches!(data.op, Opcode::Refine(_)), "{:?}", data.op);
    assert_eq!(data.args, vec![leader]);
    assert_eq!(data.eff, Effects::PURE);
    assert_eq!(data.mem, AliasClass::None);
}

fn still_read(func: &Func, result: Value) {
    let data = &func.insts[inst_of(func, result).index()];
    assert!(matches!(data.op, Opcode::Opaque(_)), "{:?}", data.op);
}

fn returning(func: &mut Func, block: usize, value: Value) {
    func.blocks[block].term = Term::Return(value);
}

#[test]
fn opt_reads_reuse_a_repeated_car_and_cdr_across_guards_and_pure_ops() {
    for op in [Op::Car, Op::Cdr] {
        let mut func = empty(1, 1);
        let x = arg(&mut func);
        let first = read(&mut func, Block(0), 1, op.clone(), x);
        let fixnum = check(&mut func, Block(0), 2, TypeSet::FIXNUM, first);
        let _compare = emit(
            &mut func,
            Block(0),
            3,
            Opcode::FixCmp(Cmp::Lt),
            &[fixnum, fixnum],
            (TypeSet::BOOLEAN, Rep::Bool),
            (Effects::PURE, AliasClass::None),
        );
        let second = read(&mut func, Block(0), 4, op, x);
        returning(&mut func, 0, second);
        func.verify().unwrap();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats.reuses, 1);
        view_of(&func, second, first);
        func.verify().unwrap();
    }
}

#[test]
fn opt_reads_reuse_a_read_of_the_same_word_through_its_guard() {
    let mut func = empty(1, 1);
    let x = arg(&mut func);
    let list = check(&mut func, Block(0), 1, TypeSet::LIST, x);
    let first = read(&mut func, Block(0), 2, Op::Car, list);
    let again = check(&mut func, Block(0), 3, TypeSet::LIST, x);
    let second = read(&mut func, Block(0), 4, Op::Car, again);
    returning(&mut func, 0, second);
    func.verify().unwrap();
    assert_eq!(run(&mut func).unwrap().reuses, 1);
    view_of(&func, second, first);
}

#[test]
fn opt_reads_a_store_or_call_between_ends_reuse_even_of_the_other_field() {
    let mut func = empty(1, 1);
    let x = arg(&mut func);
    let first = read(&mut func, Block(0), 1, Op::Car, x);
    let _store = setcdr(&mut func, Block(0), 2, x, first);
    let second = read(&mut func, Block(0), 3, Op::Car, x);
    let _length = {
        let (eff, mem) = crate::emacs_core::jit::opt::build::op_effects(&Op::Length);
        emit(
            &mut func,
            Block(0),
            4,
            Opcode::Opaque(Op::Length),
            &[x],
            (TypeSet::TOP, Rep::Tagged),
            (eff, mem),
        )
    };
    let third = read(&mut func, Block(0), 5, Op::Car, x);
    returning(&mut func, 0, third);
    func.verify().unwrap();
    assert_eq!(run(&mut func).unwrap().reuses, 0);
    still_read(&func, second);
    still_read(&func, third);
}

#[test]
fn opt_reads_a_strict_read_serves_a_lenient_one_but_not_the_reverse() {
    let mut func = empty(1, 1);
    let x = arg(&mut func);
    let strict = read(&mut func, Block(0), 1, Op::Car, x);
    let lenient = read(&mut func, Block(0), 2, Op::CarSafe, x);
    returning(&mut func, 0, lenient);
    func.verify().unwrap();
    assert_eq!(run(&mut func).unwrap().reuses, 1);
    view_of(&func, lenient, strict);

    // `(car-safe x)` of a non-list is nil, where `(car x)` signals.
    let mut func = empty(1, 1);
    let x = arg(&mut func);
    let _lenient = read(&mut func, Block(0), 1, Op::CdrSafe, x);
    let strict = read(&mut func, Block(0), 2, Op::Cdr, x);
    returning(&mut func, 0, strict);
    func.verify().unwrap();
    assert_eq!(run(&mut func).unwrap().reuses, 0);
    still_read(&func, strict);
}

#[test]
fn opt_reads_flow_down_a_lone_predecessor_but_not_into_a_join() {
    // b0 -> {b1, b2} -> b3: b1 reuses b0's read, b3 (a join) does not.
    let mut func = empty(4, 1);
    let x = arg(&mut func);
    let first = read(&mut func, Block(0), 1, Op::Car, x);
    let flag = emit(
        &mut func,
        Block(0),
        2,
        Opcode::IsNonNil,
        &[first],
        (TypeSet::BOOLEAN, Rep::Bool),
        (Effects::PURE, AliasClass::None),
    );
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
    let in_arm = read(&mut func, Block(1), 3, Op::Car, x);
    for arm in [1, 2] {
        func.blocks[arm].preds = vec![Block(0)];
        func.blocks[arm].term = Term::Jump(Edge {
            target: Block(3),
            args: vec![],
        });
    }
    func.blocks[3].preds = vec![Block(1), Block(2)];
    let at_join = read(&mut func, Block(3), 4, Op::Car, x);
    returning(&mut func, 3, at_join);
    func.verify().unwrap();
    assert_eq!(run(&mut func).unwrap().reuses, 1);
    view_of(&func, in_arm, first);
    still_read(&func, at_join);
}

#[test]
fn opt_reads_a_poll_ends_reuse() {
    let mut func = empty(1, 1);
    let x = arg(&mut func);
    let _first = read(&mut func, Block(0), 1, Op::Cdr, x);
    let poll = Inst(func.insts.len() as u32);
    let state = func.intern_frame(FrameState {
        pc: 2,
        stack: vec![x].into(),
        handlers: 0,
        binds: 0,
        parent: None,
        site: None,
    });
    func.insts.push(InstData {
        op: Opcode::Poll,
        args: vec![],
        result: None,
        eff: Effects::MAY_GC.with(Effects::MAY_REENTER),
        mem: AliasClass::Unknown,
        frame: Some(state),
        pc: 2,
    });
    func.blocks[0].insts.push(poll);
    let second = read(&mut func, Block(0), 3, Op::Cdr, x);
    returning(&mut func, 0, second);
    func.verify().unwrap();
    assert_eq!(run(&mut func).unwrap().reuses, 0);
    still_read(&func, second);
}

#[test]
fn opt_reads_a_loop_header_starts_empty_but_its_body_reuses_within_an_iteration() {
    // b0 -> b1 (header) -> {b2 -> b1, b3}: the header rereads after the
    // back edge, b2 reuses the header's read.
    let mut func = empty(4, 1);
    let x = arg(&mut func);
    let _preheader = read(&mut func, Block(0), 1, Op::Car, x);
    func.blocks[0].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![],
    });
    let header = read(&mut func, Block(1), 2, Op::Car, x);
    let flag = emit(
        &mut func,
        Block(1),
        3,
        Opcode::IsNonNil,
        &[header],
        (TypeSet::BOOLEAN, Rep::Bool),
        (Effects::PURE, AliasClass::None),
    );
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
    let body = read(&mut func, Block(2), 4, Op::Car, x);
    let _store = setcdr(&mut func, Block(2), 5, x, body);
    func.blocks[2].preds = vec![Block(1)];
    func.blocks[2].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![],
    });
    func.blocks[3].preds = vec![Block(1)];
    returning(&mut func, 3, header);
    func.verify().unwrap();
    assert_eq!(run(&mut func).unwrap().reuses, 1);
    still_read(&func, header);
    view_of(&func, body, header);
}
