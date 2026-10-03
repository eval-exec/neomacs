//! Invariant motion and exact loop-entry recovery boundaries.
//! Reference/native fixtures separately establish runtime/replay correctness.

use super::*;
use crate::emacs_core::jit::opt::{
    ir::*,
    mem::{AliasClass, Effects},
    types::{Range, TypeSet},
};
use crate::emacs_core::value::Value as LispValue;

fn emit(
    func: &mut Func,
    block: Block,
    pc: u32,
    op: Opcode,
    args: &[Value],
    ty: TypeSet,
    rep: Rep,
) -> Value {
    let id = Inst(func.insts.len() as u32);
    let result = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty,
        rep,
        def: ValueDef::Inst(id),
    });
    func.insts.push(InstData {
        op,
        args: args.to_vec(),
        result: Some(result),
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc,
    });
    func.blocks[block.index()].insts.push(id);
    result
}
fn id(func: &Func, value: Value) -> Inst {
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
fn checked(func: &mut Func, value: Value, stack: &[Value]) {
    let inst = id(func, value);
    let pc = func.insts[inst.index()].pc;
    let snapshot = frame(func, pc, stack);
    func.insts[inst.index()].eff = Effects::MAY_DEOPT;
    func.insts[inst.index()].frame = Some(snapshot);
}
fn pool(func: &mut Func, block: Block, pc: u32, index: u32) -> Value {
    let ty = TypeSet::for_constant(func.consts[index as usize]);
    emit(
        func,
        block,
        pc,
        Opcode::Const(index),
        &[],
        ty,
        Rep::TaggedFix,
    )
}
fn owner(func: &Func, value: Value) -> Block {
    let inst = id(func, value);
    Block(
        func.blocks
            .iter()
            .position(|block| block.insts.contains(&inst))
            .expect("attached") as u32,
    )
}
fn observations(before: &Func, after: &Func) {
    assert_eq!(
        &after.frames[..before.frames.len()],
        before.frames.as_slice()
    );
    assert_eq!(after.entry_stacks, before.entry_stacks);
    assert_eq!(after.source_states.len(), before.source_states.len());
    for (a, b) in after.source_states.iter().zip(&before.source_states) {
        match (a, b) {
            (Some(a), Some(b)) => {
                assert_eq!(a.pre, b.pre);
                assert_eq!(a.post, b.post);
                assert_eq!(a.block, b.block);
                assert_eq!(a.frame, b.frame);
            }
            (None, None) => {}
            _ => panic!("source mapping changed"),
        }
    }
    for (a, b) in after.values.iter().zip(&before.values) {
        assert_eq!(a.ty, b.ty);
        assert_eq!(a.rep, b.rep);
        assert_eq!(a.def, b.def);
    }
    for (a, b) in after.insts.iter().zip(&before.insts) {
        assert_eq!(a.pc, b.pc);
        assert_eq!(a.result, b.result);
        assert_eq!(a.frame, b.frame);
    }
}

/// Header guard n executes before the header exit, including zero trips. The
/// varying i phi can never appear in the new preheader replay frame: its actual
/// initial edge argument zero must replace it. Body-only guards remain later.
fn loop_ir() -> (Func, Value, Value, Value, Value) {
    let mut func = Func::new(
        [0, 1, 2]
            .into_iter()
            .map(|n| ValueBits::from_value(LispValue::make_int(n)))
            .collect::<Vec<_>>()
            .into(),
        ParamShape {
            required: 2,
            ..ParamShape::default()
        },
        0,
    );
    for pc in [0, 10, 20, 30] {
        func.blocks.push(BlockData::new(pc));
    }
    let n = emit(
        &mut func,
        Block(0),
        0,
        Opcode::Arg(0),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let body_input = emit(
        &mut func,
        Block(0),
        0,
        Opcode::Arg(1),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let zero = pool(&mut func, Block(0), 1, 0);
    let iv = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty: TypeSet::FIXNUM,
        rep: Rep::TaggedFix,
        def: ValueDef::Param {
            block: Block(1),
            index: 0,
        },
    });
    func.blocks[1].params.push(iv);
    let head_n = emit(
        &mut func,
        Block(1),
        11,
        Opcode::CheckType(TypeSet::FIXNUM),
        &[n],
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    checked(&mut func, head_n, &[n, iv]);
    let flag = emit(
        &mut func,
        Block(1),
        12,
        Opcode::FixCmp(Cmp::Lt),
        &[iv, head_n],
        TypeSet::BOOLEAN,
        Rep::Bool,
    );
    let one = pool(&mut func, Block(2), 20, 1);
    let next = emit(
        &mut func,
        Block(2),
        21,
        Opcode::FixAdd { checked: true },
        &[iv, one],
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    checked(&mut func, next, &[n, iv, one]);
    let poll_frame = frame(&mut func, 22, &[n, next]);
    let poll = Inst(func.insts.len() as u32);
    func.insts.push(InstData {
        op: Opcode::Poll,
        args: vec![],
        result: None,
        eff: Effects::MAY_GC,
        mem: AliasClass::Unknown,
        frame: Some(poll_frame),
        pc: 22,
    });
    func.blocks[2].insts.push(poll);
    func.blocks[0].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![zero],
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
        args: vec![next],
    });
    func.blocks[3].preds = vec![Block(1)];
    func.blocks[3].term = Term::Return(iv);
    func.entry_stacks = vec![
        vec![].into(),
        vec![n, iv].into(),
        vec![n, iv].into(),
        vec![n, iv].into(),
    ];
    let entry_frame = frame(&mut func, 10, &[n, iv]);
    func.source_states.resize_with(11, || None);
    func.source_states[10] = Some(SourceState {
        pre: vec![n, iv].into(),
        post: vec![n, iv].into(),
        frame: entry_frame,
        block: Block(1),
    });
    func.verify().unwrap();
    (func, n, body_input, head_n, one)
}

#[test]
fn opt_licm_hoists_real_static_fix_const_preserving_original_observation() {
    let (mut func, _, _, _, one) = loop_ir();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert!(stats.pure_hoisted >= 1);
    let old = &func.insts[id(&func, one).index()];
    assert!(matches!(old.op, Opcode::Refine(_)));
    assert_eq!(old.args.len(), 1);
    let moved = old.args[0];
    assert_eq!(owner(&func, moved), Block(0));
    assert_eq!(func.insts[id(&func, moved).index()].op, Opcode::Const(1));
    assert_eq!(func.insts[id(&func, moved).index()].frame, None);
    observations(&before, &func);
    func.verify().unwrap();
}

#[test]
fn opt_licm_anticipated_guard_uses_actual_preheader_edge_replay_stack() {
    let (mut func, n, _, head_n, _) = loop_ir();
    let before = func.clone();
    let zero = func.blocks[0].term.edges()[0].args[0];
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.guards_hoisted, 1);
    let body_guard = &func.insts[id(&func, head_n).index()];
    assert!(matches!(body_guard.op, Opcode::Refine(_)));
    let moved = body_guard.args[0];
    assert_eq!(owner(&func, moved), Block(0));
    let check = &func.insts[id(&func, moved).index()];
    assert_eq!(check.op, Opcode::CheckType(TypeSet::FIXNUM));
    assert_eq!(check.pc, 10);
    let replay = &func.frames[check.frame.unwrap().index()];
    assert_eq!(replay.pc, 10);
    assert_eq!(replay.stack.as_ref(), &[n, zero]);
    observations(&before, &func);
    func.verify().unwrap();
}

#[test]
fn opt_licm_body_only_guard_stays_after_existing_header_exit_and_poll() {
    let (mut func, n, body_input, _, _) = loop_ir();
    let body_guard = emit(
        &mut func,
        Block(2),
        23,
        Opcode::CheckType(TypeSet::FIXNUM),
        &[body_input],
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    checked(&mut func, body_guard, &[n, body_input]);
    let before_guard = func.insts[id(&func, body_guard).index()].clone();
    func.verify().unwrap();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.guards_hoisted, 1, "only anticipated n");
    let retained = &func.insts[id(&func, body_guard).index()];
    assert_eq!(retained.op, before_guard.op);
    assert_eq!(retained.args, before_guard.args);
    assert_eq!(owner(&func, body_guard), Block(2));
    let polls_before = before
        .insts
        .iter()
        .filter(|inst| inst.op == Opcode::Poll)
        .count();
    assert_eq!(
        func.insts
            .iter()
            .filter(|inst| inst.op == Opcode::Poll)
            .count(),
        polls_before
    );
    observations(&before, &func);
    func.verify().unwrap();
}

#[test]
fn opt_licm_mutable_length_backing_and_record_slot_ignore_immutable_hints() {
    for op in [Opcode::LoadVecLen, Opcode::LoadRecTag] {
        let (mut func, _, body_input, _, _) = loop_ir();
        let guarded = emit(
            &mut func,
            Block(0),
            2,
            Opcode::CheckType(TypeSet::VECTOR.join(TypeSet::RECORD)),
            &[body_input],
            TypeSet::VECTOR.join(TypeSet::RECORD),
            Rep::Tagged,
        );
        checked(&mut func, guarded, &[body_input]);
        let value = emit(
            &mut func,
            Block(2),
            23,
            op.clone(),
            &[guarded],
            TypeSet::TOP,
            Rep::Tagged,
        );
        let value_id = id(&func, value);
        func.insts[value_id.index()].eff = Effects::READ_HEAP;
        func.insts[value_id.index()].mem = AliasClass::Immutable;
        func.verify().unwrap();
        let before = func.clone();
        let stats = run(&mut func).unwrap();
        assert_eq!(stats.immutable_loads_hoisted, 0);
        assert_eq!(func.insts[value_id.index()].op, op);
        assert_eq!(owner(&func, value), Block(2));
        observations(&before, &func);
        func.verify().unwrap();
    }
}

#[test]
fn opt_licm_inconsistent_header_replay_mapping_keeps_guard_in_header() {
    let (mut func, _, _, head_n, _) = loop_ir();
    // Ordinary SourceState need not encode the complete block-entry stack;
    // this remains a valid input, but it supplies no exact LICM replay proof.
    func.entry_stacks[1] = Box::default();
    func.verify().unwrap();
    let before_guard = func.insts[id(&func, head_n).index()].clone();
    let old_frames = func.frames.len();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.guards_hoisted, 0);
    assert_eq!(func.frames.len(), old_frames);
    assert_eq!(func.insts[id(&func, head_n).index()].op, before_guard.op);
    assert_eq!(
        func.insts[id(&func, head_n).index()].args,
        before_guard.args
    );
    func.verify().unwrap();
}

#[test]
fn opt_licm_boolean_const_is_not_moved_without_standalone_native_path() {
    let (mut func, _, _, _, _) = loop_ir();
    let original_pool_len = func.consts.len() as u32;
    func.consts = func
        .consts
        .iter()
        .copied()
        .chain([
            ValueBits::from_value(LispValue::NIL),
            ValueBits::from_value(LispValue::T),
        ])
        .collect::<Vec<_>>()
        .into();
    let nil = emit(
        &mut func,
        Block(2),
        23,
        Opcode::Const(original_pool_len),
        &[],
        TypeSet::NIL,
        Rep::Tagged,
    );
    let t = emit(
        &mut func,
        Block(2),
        24,
        Opcode::Const(original_pool_len + 1),
        &[],
        TypeSet::T,
        Rep::Tagged,
    );
    func.verify().unwrap();
    let before = func.clone();
    run(&mut func).unwrap();
    assert_eq!(
        func.insts[id(&func, nil).index()].op,
        Opcode::Const(original_pool_len)
    );
    assert_eq!(
        func.insts[id(&func, t).index()].op,
        Opcode::Const(original_pool_len + 1)
    );
    assert_eq!(owner(&func, nil), Block(2));
    assert_eq!(owner(&func, t), Block(2));
    observations(&before, &func);
    func.verify().unwrap();
}

#[test]
fn opt_licm_retained_header_phi_is_not_assumed_equal_to_preheader_seed() {
    let (mut func, n, _, head_n, _) = loop_ir();
    let phi = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty: TypeSet::TOP,
        rep: Rep::Tagged,
        def: ValueDef::Param {
            block: Block(1),
            index: 1,
        },
    });
    func.blocks[1].params.push(phi);
    let Term::Jump(entry) = &mut func.blocks[0].term else {
        panic!("preheader")
    };
    entry.args.push(n);
    let Term::Jump(backedge) = &mut func.blocks[2].term else {
        panic!("latch")
    };
    backedge.args.push(phi);
    let guard_id = id(&func, head_n);
    func.insts[guard_id.index()].args[0] = phi;
    func.verify().unwrap();
    let before = func.clone();
    let stats = run(&mut func).unwrap();
    assert_eq!(stats.guards_hoisted, 0);
    assert_eq!(
        func.insts[guard_id.index()].op,
        Opcode::CheckType(TypeSet::FIXNUM)
    );
    assert_eq!(func.insts[guard_id.index()].args, vec![phi]);
    assert_eq!(owner(&func, head_n), Block(1));
    observations(&before, &func);
    func.verify().unwrap();
}

#[test]
fn opt_licm_unchecked_motion_requires_endpoint_subset_of_original_result() {
    let (mut func, _, input, _, _) = loop_ir();
    let interval = TypeSet::fixnum_range(Range { lo: -8, hi: 8 });
    let checked_input = emit(
        &mut func,
        Block(0),
        2,
        Opcode::CheckType(interval),
        &[input],
        interval,
        Rep::TaggedFix,
    );
    checked(&mut func, checked_input, &[input]);
    // Actual x-x is zero for every input, so the original declaration is valid.
    // The independent endpoint method intentionally does not exploit operand
    // correlation: [-8,8]-[-8,8] yields [-16,16], wider than the retained zero.
    let zero_ty = TypeSet::fixnum_range(Range { lo: 0, hi: 0 });
    let difference = emit(
        &mut func,
        Block(2),
        23,
        Opcode::FixSub { checked: false },
        &[checked_input, checked_input],
        zero_ty,
        Rep::TaggedFix,
    );
    func.verify().unwrap();
    let before = func.clone();
    let original = func.insts[id(&func, difference).index()].clone();
    run(&mut func).unwrap();
    let current = &func.insts[id(&func, difference).index()];
    assert_eq!(owner(&func, difference), Block(2));
    assert_eq!(current.op, original.op);
    assert_eq!(current.args, original.args);
    assert_eq!(current.result, original.result);
    assert_eq!(current.frame, original.frame);
    assert_eq!(func.values[difference.index()].ty, zero_ty);
    observations(&before, &func);
    func.verify().unwrap();
}
