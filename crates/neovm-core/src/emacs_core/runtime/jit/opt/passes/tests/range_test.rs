//! Range proofs and conservative interval boundaries.
//! Identity first API can establish actual missing-transform REDs after each
//! fixture verifies. These scalar fixtures do not certify native array BCE.

use super::*;
use crate::emacs_core::jit::opt::{
    ir::*,
    mem::{AliasClass, Effects},
    types::{Range, TypeSet},
};
use crate::emacs_core::value::Value as LispValue;

fn empty(blocks: usize, args: usize) -> Func {
    let mut func = Func::new(
        vec![0, 1, 2, LispValue::MOST_POSITIVE_FIXNUM]
            .into_iter()
            .map(|n| ValueBits::from_value(LispValue::make_int(n)))
            .collect::<Vec<_>>()
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
fn inst(func: &Func, value: Value) -> Inst {
    let ValueDef::Inst(inst) = func.values[value.index()].def else {
        panic!("instruction value")
    };
    inst
}
fn guard(func: &mut Func, value: Value, stack: &[Value]) {
    let id = inst(func, value);
    let frame = func.intern_frame(FrameState {
        pc: func.insts[id.index()].pc,
        stack: stack.into(),
        handlers: 0,
        binds: 0,
        parent: None,
        site: None,
    });
    func.insts[id.index()].eff = Effects::MAY_DEOPT;
    func.insts[id.index()].frame = Some(frame);
}
fn literal(func: &mut Func, pool: u32) -> Value {
    let ty = TypeSet::for_constant(func.consts[pool as usize]);
    emit(
        func,
        Block(0),
        0,
        Opcode::Const(pool),
        &[],
        ty,
        Rep::TaggedFix,
    )
}
fn checked_input(func: &mut Func, arg_index: u16, range: Range) -> (Value, Value) {
    let arg = emit(
        func,
        Block(0),
        0,
        Opcode::Arg(arg_index),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let checked = emit(
        func,
        Block(0),
        1,
        Opcode::CheckType(TypeSet::fixnum_range(range)),
        &[arg],
        TypeSet::fixnum_range(range),
        Rep::TaggedFix,
    );
    guard(func, checked, &[arg]);
    (arg, checked)
}
fn arithmetic(func: &mut Func, block: Block, pc: u32, op: Opcode, a: Value, b: Value) -> Value {
    let result = emit(
        func,
        block,
        pc,
        op,
        &[a, b],
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    guard(func, result, &[a, b]);
    result
}
fn snapshots(before: &Func, after: &Func) {
    assert_eq!(before.frames, after.frames);
    assert_eq!(before.entry_stacks, after.entry_stacks);
    assert_eq!(before.source_states.len(), after.source_states.len());
    for (old, new) in before.values.iter().zip(&after.values) {
        assert_eq!(old.ty, new.ty);
        assert_eq!(old.rep, new.rep);
        assert_eq!(old.def, new.def);
    }
    for (old, new) in before.insts.iter().zip(&after.insts) {
        assert_eq!(old.pc, new.pc);
        assert_eq!(old.frame, new.frame);
        assert_eq!(old.result, new.result);
    }
}

#[test]
fn opt_range_branch_upper_bound_only_elides_taken_edge_increment() {
    let mut func = empty(3, 1);
    let (original_arg, input) = checked_input(&mut func, 0, Range::FULL);
    let hi = literal(&mut func, 3);
    let one = literal(&mut func, 1);
    let flag = emit(
        &mut func,
        Block(0),
        2,
        Opcode::FixCmp(Cmp::Lt),
        &[input, hi],
        TypeSet::BOOLEAN,
        Rep::Bool,
    );
    let taken = arithmetic(
        &mut func,
        Block(1),
        3,
        Opcode::FixAdd { checked: true },
        input,
        one,
    );
    let other = arithmetic(
        &mut func,
        Block(2),
        4,
        Opcode::FixAdd { checked: true },
        input,
        one,
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
    for block in [1, 2] {
        func.blocks[block].preds = vec![Block(0)];
    }
    func.blocks[1].term = Term::Return(taken);
    func.blocks[2].term = Term::Return(other);
    func.verify().unwrap();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.overflow_checks_elided, 1);
    assert!(stats.range_views > 0);
    assert_eq!(
        func.insts[inst(&func, taken).index()].op,
        Opcode::FixAdd { checked: false }
    );
    assert_eq!(
        func.insts[inst(&func, other).index()].op,
        Opcode::FixAdd { checked: true }
    );
    assert_eq!(func.values[original_arg.index()].ty, TypeSet::TOP);
    assert_eq!(func.values[input.index()].ty, TypeSet::FIXNUM);
    snapshots(&before, &func);
    func.verify().unwrap();
}

#[test]
fn opt_range_signed_multiply_extrema_keep_unsafe_neighbor_checked() {
    let mut func = empty(1, 3);
    let (_, a) = checked_input(&mut func, 0, Range { lo: -8, hi: 8 });
    let (_, b) = checked_input(&mut func, 1, Range { lo: -8, hi: 8 });
    let (_, big) = checked_input(
        &mut func,
        2,
        Range {
            lo: Range::FULL.hi - 1,
            hi: Range::FULL.hi,
        },
    );
    let two = literal(&mut func, 2);
    let safe = arithmetic(
        &mut func,
        Block(0),
        2,
        Opcode::FixMul { checked: true },
        a,
        b,
    );
    let unsafe_mul = arithmetic(
        &mut func,
        Block(0),
        3,
        Opcode::FixMul { checked: true },
        big,
        two,
    );
    func.blocks[0].term = Term::Return(unsafe_mul);
    func.verify().unwrap();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.overflow_checks_elided, 1);
    assert_eq!(
        func.insts[inst(&func, safe).index()].op,
        Opcode::FixMul { checked: false }
    );
    assert_eq!(
        func.insts[inst(&func, unsafe_mul).index()].op,
        Opcode::FixMul { checked: true }
    );
    snapshots(&before, &func);
    func.verify().unwrap();
}

#[test]
fn opt_range_resultful_static_bounds_keep_result_frame_and_noresult_guard() {
    let mut func = empty(1, 0);
    let index = literal(&mut func, 1);
    let len = literal(&mut func, 2);
    let index_ty = func.values[index.index()].ty;
    let checked = emit(
        &mut func,
        Block(0),
        1,
        Opcode::CheckBounds,
        &[index, len],
        index_ty,
        Rep::TaggedFix,
    );
    guard(&mut func, checked, &[index, len]);
    let no_result = Inst(func.insts.len() as u32);
    let frame = func.intern_frame(FrameState {
        pc: 2,
        stack: vec![index, len].into(),
        handlers: 0,
        binds: 0,
        parent: None,
        site: None,
    });
    func.insts.push(InstData {
        op: Opcode::CheckBounds,
        args: vec![index, len],
        result: None,
        eff: Effects::MAY_DEOPT,
        mem: AliasClass::None,
        frame: Some(frame),
        pc: 2,
    });
    func.blocks[0].insts.push(no_result);
    func.blocks[0].term = Term::Return(checked);
    func.verify().unwrap();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.bounds_checks_elided, 1);
    assert!(matches!(
        func.insts[inst(&func, checked).index()].op,
        Opcode::Refine(_)
    ));
    assert_eq!(func.insts[inst(&func, checked).index()].args, vec![index]);
    assert_eq!(func.insts[no_result.index()].op, Opcode::CheckBounds);
    snapshots(&before, &func);
    func.verify().unwrap();
}

#[test]
fn opt_range_full_unknown_guarded_inputs_keep_overflow_checks() {
    let mut func = empty(1, 2);
    let (_, a) = checked_input(&mut func, 0, Range::FULL);
    let (_, b) = checked_input(&mut func, 1, Range::FULL);
    let sum = arithmetic(
        &mut func,
        Block(0),
        2,
        Opcode::FixAdd { checked: true },
        a,
        b,
    );
    let product = arithmetic(
        &mut func,
        Block(0),
        3,
        Opcode::FixMul { checked: true },
        a,
        b,
    );
    func.blocks[0].term = Term::Return(product);
    func.verify().unwrap();
    let before = func.display().to_string();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats, RangeStats::default());
    assert_eq!(
        func.insts[inst(&func, sum).index()].op,
        Opcode::FixAdd { checked: true }
    );
    assert_eq!(func.display().to_string(), before);
}

#[test]
fn opt_range_dynamic_prefix_literal_is_not_a_static_interval_proof() {
    let mut func = empty(1, 0);
    func.dynamic_prefix = 1;
    // The patched word happens to be zero in this compile template; it may be
    // any valid word in another instance, so the actual guarded range is FULL.
    let prefix = emit(
        &mut func,
        Block(0),
        0,
        Opcode::EnvConst(0),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let input = emit(
        &mut func,
        Block(0),
        1,
        Opcode::CheckType(TypeSet::FIXNUM),
        &[prefix],
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    guard(&mut func, input, &[prefix]);
    let one = literal(&mut func, 1);
    let sum = arithmetic(
        &mut func,
        Block(0),
        2,
        Opcode::FixAdd { checked: true },
        input,
        one,
    );
    func.blocks[0].term = Term::Return(sum);
    func.verify().unwrap();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.overflow_checks_elided, 0);
    assert_eq!(
        func.insts[inst(&func, sum).index()].op,
        Opcode::FixAdd { checked: true }
    );
    func.verify().unwrap();
}

#[test]
fn opt_range_actual_solver_budget_bail_is_transactional() {
    let mut func = empty(1, 0);
    // A verified forward alias chain requires more monotone alias scans than
    // the documented 64-scan cap. This tests a real skip, not a fake failure.
    for index in 0..70 {
        func.values.push(ValueData {
            ty: TypeSet::FIXNUM,
            rep: Rep::TaggedFix,
            def: ValueDef::Alias(Value(index + 1)),
        });
    }
    let terminal = literal(&mut func, 1);
    assert_eq!(terminal, Value(70));
    func.blocks[0].term = Term::Return(Value(0));
    func.verify().unwrap();
    let before = func.display().to_string();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.analysis_bailed, 1);
    assert_eq!(stats.overflow_checks_elided, 0);
    assert_eq!(stats.range_views, 0);
    assert_eq!(func.display().to_string(), before);
    func.verify().unwrap();
}

#[test]
fn opt_range_correlated_result_declaration_keeps_check_when_endpoint_proof_is_wider() {
    // These are valid narrow semantic result declarations, not impossible
    // annotations: x-x is exactly zero and x*x cannot be negative. The interval
    // solver intentionally does not use correlation between identical inputs.
    // Reps/native endpoint admission must therefore conservatively retain the
    // checked form when their independent result interval is wider.
    for (op, output_range) in [
        (Opcode::FixSub { checked: true }, Range { lo: 0, hi: 0 }),
        (Opcode::FixMul { checked: true }, Range { lo: 0, hi: 64 }),
    ] {
        let mut func = empty(1, 1);
        let (original, input) = checked_input(&mut func, 0, Range { lo: -8, hi: 8 });
        let result = arithmetic(&mut func, Block(0), 2, op.clone(), input, input);
        func.values[result.index()].ty = TypeSet::fixnum_range(output_range);
        func.blocks[0].term = Term::Return(result);
        func.verify().unwrap();
        let before = func.clone();
        let display = func.display().to_string();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats, RangeStats::default());
        let retained = &func.insts[inst(&func, result).index()];
        assert_eq!(retained.op, op);
        assert_eq!(retained.args, vec![input, input]);
        assert_eq!(retained.eff, Effects::MAY_DEOPT);
        assert_eq!(
            retained.frame,
            before.insts[inst(&before, result).index()].frame
        );
        assert_eq!(
            func.values[result.index()].ty,
            TypeSet::fixnum_range(output_range)
        );
        assert_eq!(func.values[original.index()].ty, TypeSet::TOP);
        assert_eq!(func.display().to_string(), display);
        snapshots(&before, &func);
        func.verify().unwrap();
    }
}
