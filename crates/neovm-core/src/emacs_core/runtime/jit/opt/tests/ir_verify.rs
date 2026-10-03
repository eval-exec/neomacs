use super::ir::*;
use super::mem::{AliasClass, Effects};
use super::types::TypeSet;
use super::verify::VerifyError;

fn empty() -> Func {
    let mut func = Func::new(Box::new([]), ParamShape::default(), 0);
    func.blocks.push(BlockData::new(0));
    func
}

fn emit(
    func: &mut Func,
    block: Block,
    op: Opcode,
    args: Vec<Value>,
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
        args,
        result: Some(value),
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc: func.blocks[block.index()].pc,
    });
    func.blocks[block.index()].insts.push(inst);
    value
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

fn frame(func: &mut Func, values: &[Value]) -> FrameId {
    func.intern_frame(FrameState {
        pc: 0,
        stack: values.into(),
        handlers: 0,
        binds: 0,
        parent: None,
        site: None,
    })
}

fn const_value(func: &mut Func, block: Block, bits: u64) -> Value {
    let index = func.consts.len() as u32;
    let mut constants = func.consts.to_vec();
    constants.push(ValueBits(bits));
    func.consts = constants.into_boxed_slice();
    emit(
        func,
        block,
        Opcode::Const(index),
        vec![],
        TypeSet::for_constant(ValueBits(bits)),
        Rep::Tagged,
    )
}

fn guarded(func: &mut Func, input: Value) -> (Value, Inst) {
    let state = frame(func, &[input]);
    let result = emit(
        func,
        Block(0),
        Opcode::CheckType(TypeSet::FIXNUM),
        vec![input],
        TypeSet::FIXNUM,
        Rep::Tagged,
    );
    let inst = match func.values[result.index()].def {
        ValueDef::Inst(inst) => inst,
        _ => unreachable!(),
    };
    func.insts[inst.index()].eff = Effects::MAY_DEOPT;
    func.insts[inst.index()].frame = Some(state);
    (result, inst)
}

#[test]
fn opt_verify_accepts_global_invariant_and_real_phi() {
    let mut func = empty();
    func.blocks
        .extend([BlockData::new(1), BlockData::new(2), BlockData::new(3)]);
    let invariant = const_value(&mut func, Block(0), 6);
    let flag = const_value(&mut func, Block(0), 8);
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
    let a = const_value(&mut func, Block(1), 10);
    let b = const_value(&mut func, Block(2), 14);
    let merged = param(&mut func, Block(3), TypeSet::FIXNUM, Rep::Tagged);
    func.blocks[1].preds = vec![Block(0)];
    func.blocks[2].preds = vec![Block(0)];
    func.blocks[3].preds = vec![Block(1), Block(2)];
    func.blocks[1].term = Term::Jump(Edge {
        target: Block(3),
        args: vec![a],
    });
    func.blocks[2].term = Term::Jump(Edge {
        target: Block(3),
        args: vec![b],
    });
    let state = frame(&mut func, &[invariant, merged]);
    func.blocks[3].term = Term::Deopt(state);
    assert_eq!(func.verify(), Ok(()));
}

#[test]
fn opt_verify_rejects_non_dominating_operand() {
    let mut func = empty();
    func.blocks.push(BlockData::new(1));
    let late = const_value(&mut func, Block(1), 8);
    func.blocks[0].term = Term::Branch {
        flag: late,
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
    func.blocks[1].term = Term::Return(late);
    assert!(
        matches!(func.verify(), Err(VerifyError::NonDominating { value, .. }) if value == late)
    );
}

#[test]
fn opt_verify_rejects_same_instruction_framestate_result() {
    let mut func = empty();
    let input = const_value(&mut func, Block(0), 6);
    let (result, guard) = guarded(&mut func, input);
    let bad_frame = frame(&mut func, &[result]);
    func.insts[guard.index()].frame = Some(bad_frame);
    func.blocks[0].term = Term::Return(result);
    assert!(
        matches!(func.verify(), Err(VerifyError::NonDominating { value, .. }) if value == result)
    );
}

#[test]
fn opt_verify_rejects_missing_guard_frame() {
    let mut func = empty();
    let input = const_value(&mut func, Block(0), 6);
    let (result, guard) = guarded(&mut func, input);
    func.insts[guard.index()].frame = None;
    func.blocks[0].term = Term::Return(result);
    assert_eq!(func.verify(), Err(VerifyError::MissingFrame(guard)));
}

#[test]
fn opt_verify_rejects_missing_safepoint_frame() {
    let mut func = empty();
    let value = emit(
        &mut func,
        Block(0),
        Opcode::Opaque(crate::emacs_core::bytecode::opcode::Op::Nil),
        vec![],
        TypeSet::NIL,
        Rep::Tagged,
    );
    func.blocks[0].term = Term::Return(value);
    assert_eq!(func.verify(), Err(VerifyError::MissingFrame(Inst(0))));
}

#[test]
fn opt_verify_rejects_guard_declared_pure() {
    let mut func = empty();
    let input = const_value(&mut func, Block(0), 6);
    let (result, guard) = guarded(&mut func, input);
    func.insts[guard.index()].eff = Effects::PURE;
    func.blocks[0].term = Term::Return(result);
    assert_eq!(func.verify(), Err(VerifyError::MissingGuardEffect(guard)));
}

#[test]
fn opt_verify_rejects_representation_mismatch() {
    let mut func = empty();
    let value = const_value(&mut func, Block(0), 6);
    let untagged = emit(
        &mut func,
        Block(0),
        Opcode::UntagFix,
        vec![value],
        TypeSet::FIXNUM,
        Rep::Tagged,
    );
    func.blocks[0].term = Term::Return(untagged);
    assert!(
        matches!(func.verify(), Err(VerifyError::RepMismatch { value, .. }) if value == untagged)
    );
}

#[test]
fn opt_verify_rejects_num_pair_publication() {
    let mut func = empty();
    let pair = param(
        &mut func,
        Block(0),
        TypeSet::FIXNUM.join(TypeSet::FLOAT),
        Rep::NumPair,
    );
    let nil = const_value(&mut func, Block(0), 0);
    let inst = Inst(func.insts.len() as u32);
    func.insts.push(InstData {
        op: Opcode::PublishRoot,
        args: vec![pair],
        result: None,
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc: 0,
    });
    func.blocks[0].insts.push(inst);
    func.blocks[0].term = Term::Return(nil);
    assert_eq!(func.verify(), Err(VerifyError::PublishedNonTagged(pair)));
}

#[test]
fn opt_verify_rejects_edge_representation_mismatch() {
    let mut func = empty();
    func.blocks.push(BlockData::new(1));
    let value = const_value(&mut func, Block(0), 6);
    let raw = param(&mut func, Block(1), TypeSet::FIXNUM, Rep::RawInt);
    func.blocks[0].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![value],
    });
    func.blocks[1].preds = vec![Block(0)];
    let state = frame(&mut func, &[raw]);
    func.blocks[1].term = Term::Deopt(state);
    assert!(
        matches!(func.verify(), Err(VerifyError::RepMismatch { value: bad, .. }) if bad == value)
    );
}

#[test]
fn opt_verify_checks_parent_frame_dominance() {
    let mut func = empty();
    let input = const_value(&mut func, Block(0), 6);
    let (result, guard) = guarded(&mut func, input);
    let parent = frame(&mut func, &[result]);
    let child = func.intern_frame(FrameState {
        pc: 1,
        stack: vec![input].into(),
        handlers: 0,
        binds: 0,
        parent: Some(parent),
        site: Some(InlineSiteId(0)),
    });
    func.insts[guard.index()].frame = Some(child);
    func.blocks[0].term = Term::Return(result);
    assert!(
        matches!(func.verify(), Err(VerifyError::NonDominating { value, .. }) if value == result)
    );
}

#[test]
fn opt_verify_rejects_frame_parent_cycle() {
    let mut func = empty();
    let nil = const_value(&mut func, Block(0), 0);
    let state = frame(&mut func, &[nil]);
    func.frames[state.index()].parent = Some(state);
    func.blocks[0].term = Term::Deopt(state);
    assert_eq!(func.verify(), Err(VerifyError::FrameCycle(state)));
}

#[test]
fn opt_verify_rejects_source_stack_from_later_op() {
    let mut func = empty();
    let early = const_value(&mut func, Block(0), 0);
    let late = const_value(&mut func, Block(0), 6);
    func.insts[1].pc = 1;
    let state = frame(&mut func, &[late]);
    func.source_states = vec![Some(SourceState {
        pre: vec![late].into(),
        post: vec![early].into(),
        frame: state,
        block: Block(0),
    })];
    func.blocks[0].term = Term::Return(late);
    assert!(
        matches!(func.verify(), Err(VerifyError::NonDominating { value, .. }) if value == late)
    );
}

#[test]
fn opt_verify_rejects_instruction_overwriting_block_parameter() {
    let mut func = empty();
    let parameter = param(&mut func, Block(0), TypeSet::NIL, Rep::Tagged);
    func.consts = vec![ValueBits(0)].into();
    func.insts.push(InstData {
        op: Opcode::Const(0),
        args: vec![],
        result: Some(parameter),
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc: 0,
    });
    func.blocks[0].insts.push(Inst(0));
    func.blocks[0].term = Term::Return(parameter);
    assert_eq!(func.verify(), Err(VerifyError::BadDefinition(parameter)));
}

#[test]
fn opt_verify_rejects_alias_cycle() {
    let mut func = empty();
    func.values.push(ValueData {
        ty: TypeSet::TOP,
        rep: Rep::Tagged,
        def: ValueDef::Alias(Value(1)),
    });
    func.values.push(ValueData {
        ty: TypeSet::TOP,
        rep: Rep::Tagged,
        def: ValueDef::Alias(Value(0)),
    });
    func.blocks[0].term = Term::Return(Value(0));
    assert_eq!(func.verify(), Err(VerifyError::AliasCycle(Value(0))));
}

#[test]
fn opt_ir_interns_frames_including_chain_identity() {
    let mut func = empty();
    let nil = const_value(&mut func, Block(0), 0);
    let first = frame(&mut func, &[nil]);
    assert_eq!(first, frame(&mut func, &[nil]));
    let child = func.intern_frame(FrameState {
        parent: Some(first),
        site: Some(InlineSiteId(2)),
        ..func.frames[first.index()].clone()
    });
    assert_ne!(first, child);
    assert_eq!(func.frames.len(), 2);
}

#[test]
fn opt_ir_printer_is_deterministic_and_keeps_site_metadata() {
    let mut func = empty();
    let nil = const_value(&mut func, Block(0), 0);
    let state = frame(&mut func, &[nil]);
    func.blocks[0].term = Term::Deopt(state);
    let printed = format!("{}", func.display());
    assert_eq!(printed, format!("{}", func.display()));
    assert!(printed.contains("const0 = 0x0000000000000000"));
    assert!(printed.contains("f0 = pc:0 stack:[Value(0)]"));
    assert!(printed.contains("parent:None site:None"));
    assert!(printed.contains("b0("));
}

#[test]
fn opt_ir_roots_retain_gnu_dead_stack_and_derived_pointer_base() {
    let mut func = empty();
    let dead_gnu = param(&mut func, Block(0), TypeSet::CONS, Rep::Tagged);
    let base = param(&mut func, Block(0), TypeSet::VECTOR, Rep::Tagged);
    let derived = param(&mut func, Block(0), TypeSet::VECTOR, Rep::RawPtr { base });
    let compiler_live = param(&mut func, Block(0), TypeSet::STRING, Rep::Tagged);
    let pair = param(
        &mut func,
        Block(0),
        TypeSet::FIXNUM.join(TypeSet::FLOAT),
        Rep::NumPair,
    );
    let raw = param(&mut func, Block(0), TypeSet::FLOAT, Rep::RawF64);
    let state = frame(&mut func, &[dead_gnu, pair, raw]);
    assert_eq!(
        func.roots_for(state, &[derived, compiler_live, pair, base]),
        Some(vec![dead_gnu, base, compiler_live])
    );
}

#[test]
fn opt_verify_rejects_derived_pointer_without_heap_base() {
    let mut func = empty();
    let base = param(&mut func, Block(0), TypeSet::FIXNUM, Rep::Tagged);
    let derived = param(&mut func, Block(0), TypeSet::VECTOR, Rep::RawPtr { base });
    let state = frame(&mut func, &[derived]);
    func.blocks[0].term = Term::Deopt(state);
    assert_eq!(func.verify(), Err(VerifyError::TypeMismatch(base)));
}

#[test]
fn opt_verify_accepts_switch_with_raw_target_addresses() {
    let mut func = empty();
    func.blocks.extend([BlockData::new(1), BlockData::new(2)]);
    let value = const_value(&mut func, Block(0), 6);
    let table = const_value(&mut func, Block(0), 0x1005);
    func.blocks[0].term = Term::Switch {
        value,
        table,
        cases: vec![SwitchCase {
            key: 17,
            edge: Edge {
                target: Block(1),
                args: vec![],
            },
        }],
        default: Edge {
            target: Block(2),
            args: vec![],
        },
    };
    for index in 1..3 {
        func.blocks[index].preds = vec![Block(0)];
        func.blocks[index].term = Term::Return(value);
    }
    assert_eq!(func.verify(), Ok(()));
}
