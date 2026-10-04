//! Current-array bounds provenance and conservative mutable-layout barriers.
//! Baselines use the real checked lift and verify ordinary IR plus the sidecar.
//! This fixture suite does not claim native execution; helper owns actual Arefs
//! through native baseline/candidate, mutation/error/full-frame regressions.

use super::super::array_reads::{ArrayAdmission, BoundsWitness, lift};
use super::*;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::opt::{
    ir::*,
    mem::{AliasClass, Effects},
    types::{Range, TypeSet},
};
use crate::emacs_core::value::Value as LispValue;

fn empty(blocks: usize) -> (Func, Value, Value, Value) {
    let mut func = Func::new(
        (0..7)
            .map(|n| ValueBits::from_value(LispValue::make_int(n)))
            .collect::<Vec<_>>()
            .into(),
        ParamShape {
            required: 3,
            ..ParamShape::default()
        },
        0,
    );
    for pc in 0..blocks {
        func.blocks.push(BlockData::new(pc as u32));
    }
    let array = emit(
        &mut func,
        Block(0),
        0,
        Opcode::Arg(0),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let callee = emit(
        &mut func,
        Block(0),
        0,
        Opcode::Arg(1),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let index = emit(
        &mut func,
        Block(0),
        0,
        Opcode::Arg(2),
        &[],
        TypeSet::TOP,
        Rep::Tagged,
    );
    (func, array, callee, index)
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
    let id = Inst(func.insts.len() as u32);
    let value = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty,
        rep,
        def: ValueDef::Inst(id),
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
    func.blocks[block.index()].insts.push(id);
    value
}
fn inst(func: &Func, value: Value) -> Inst {
    let ValueDef::Inst(id) = func.values[value.index()].def else {
        panic!("instruction")
    };
    id
}
fn framed(func: &mut Func, id: Inst, stack: &[Value], eff: Effects, mem: AliasClass) -> FrameId {
    let frame = func.intern_frame(FrameState {
        pc: func.insts[id.index()].pc,
        stack: stack.into(),
        handlers: 0,
        binds: 0,
        parent: None,
        site: None,
    });
    func.insts[id.index()].eff = eff;
    func.insts[id.index()].mem = mem;
    func.insts[id.index()].frame = Some(frame);
    frame
}
fn checked(func: &mut Func, input: Value, ty: TypeSet, rep: Rep) -> Value {
    let result = emit(func, Block(0), 0, Opcode::CheckType(ty), &[input], ty, rep);
    let id = inst(func, result);
    framed(func, id, &[input], Effects::MAY_DEOPT, AliasClass::None);
    result
}
fn literal(func: &mut Func, block: Block, pool: u32) -> Value {
    let ty = TypeSet::for_constant(func.consts[pool as usize]);
    emit(func, block, 0, Opcode::Const(pool), &[], ty, Rep::TaggedFix)
}
fn read(func: &mut Func, block: Block, pc: u32, base: Value, index: Value) -> (Inst, Value) {
    let result = emit(
        func,
        block,
        pc,
        Opcode::Opaque(Op::Aref),
        &[base, index],
        TypeSet::TOP,
        Rep::Tagged,
    );
    let id = inst(func, result);
    let frame = framed(
        func,
        id,
        &[base, index],
        super::super::super::build::op_effects(&Op::Aref).0,
        AliasClass::VecElem,
    );
    func.source_states.resize_with(pc as usize + 1, || None);
    func.source_states[pc as usize] = Some(SourceState {
        pre: vec![base, index].into(),
        post: vec![result].into(),
        frame,
        block,
    });
    (id, result)
}
fn prepared(func: &mut Func) -> ArrayReadProofs {
    func.verify().unwrap();
    let mut proofs = ArrayReadProofs::default();
    let count = func
        .insts
        .iter()
        .filter(|i| i.op == Opcode::Opaque(Op::Aref))
        .count();
    assert_eq!(
        lift(func, &mut proofs, &ArrayAdmission::default())
            .unwrap()
            .reads_lifted,
        count
    );
    func.verify().unwrap();
    array_reads::verify_reads(func, &proofs).unwrap();
    proofs
}
fn snapshots(before: &Func, after: &Func) {
    assert_eq!(before.frames, after.frames);
    assert_eq!(before.entry_stacks, after.entry_stacks);
    assert_eq!(before.source_states.len(), after.source_states.len());
    for (old, new) in before.source_states.iter().zip(&after.source_states) {
        match (old, new) {
            (None, None) => {}
            (Some(a), Some(b)) => {
                assert_eq!(a.pre, b.pre);
                assert_eq!(a.post, b.post);
                assert_eq!(a.frame, b.frame);
                assert_eq!(a.block, b.block);
            }
            _ => panic!("source state identity"),
        }
    }
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
fn opt_range_array_retained_guard_provenance_elides_later_fresh_bounds() {
    let (mut func, arg, _, _) = empty(1);
    let base = checked(
        &mut func,
        arg,
        TypeSet::VECTOR.join(TypeSet::RECORD),
        Rep::Tagged,
    );
    let four = literal(&mut func, Block(0), 4);
    let two = literal(&mut func, Block(0), 2);
    let zero = literal(&mut func, Block(0), 0);
    let (first, _) = read(&mut func, Block(0), 1, base, four);
    let (second, _) = read(&mut func, Block(0), 2, base, two);
    let (third, result) = read(&mut func, Block(0), 3, base, zero);
    func.blocks[0].term = Term::Return(result);
    let mut proofs = prepared(&mut func);
    let before = func.clone();
    let first_proof = proofs.reads[&first].clone();
    let stats = run_with_array_proofs(&mut func, &mut proofs).unwrap();
    assert_eq!(stats.bounds_checks_elided, 2);
    assert_eq!(
        func.insts[first_proof.bounds.index()].op,
        Opcode::CheckBounds
    );
    assert!(matches!(
        proofs.reads[&first].witness,
        BoundsWitness::Checked
    ));
    for id in [second, third] {
        let proof = &proofs.reads[&id];
        let BoundsWitness::Elided(witness) = &proof.witness else {
            panic!("actual bounds elimination")
        };
        assert_eq!(
            witness.floor.guard, first_proof.bounds,
            "an elided check is never a new floor source"
        );
        assert_eq!(witness.floor.length_read, first_proof.length_read);
        assert_eq!(witness.floor.index, first_proof.checked_index);
        assert_eq!(witness.floor.floor, 5);
        assert_eq!(func.values[proof.length.index()].rep, Rep::RawInt);
        assert_eq!(func.values[witness.length_view.index()].rep, Rep::RawInt);
        assert_eq!(
            func.values[witness.length_view.index()].ty.range(),
            Some(Range {
                lo: 5,
                hi: Range::FULL.hi
            })
        );
        assert!(matches!(
            func.insts[proof.bounds.index()].op,
            Opcode::Refine(_)
        ));
        assert_eq!(func.insts[proof.bounds.index()].args, [proof.checked_index]);
        assert_eq!(
            func.insts[proof.bounds.index()].result,
            Some(proof.bounds_result)
        );
    }
    snapshots(&before, &func);
    array_reads::verify_reads(&func, &proofs).unwrap();
    func.verify().unwrap();
}

#[test]
fn opt_range_array_guard_floor_uses_lower_endpoint_not_upper() {
    let (mut func, arg, _, index_arg) = empty(1);
    let base = checked(&mut func, arg, TypeSet::VECTOR, Rep::Tagged);
    let index = checked(
        &mut func,
        index_arg,
        TypeSet::fixnum_range(Range { lo: 0, hi: 4 }),
        Rep::TaggedFix,
    );
    let two = literal(&mut func, Block(0), 2);
    let (first, _) = read(&mut func, Block(0), 1, base, index);
    let (second, result) = read(&mut func, Block(0), 2, base, two);
    func.blocks[0].term = Term::Return(result);
    let mut proofs = prepared(&mut func);
    let before = func.clone();
    let stats = run_with_array_proofs(&mut func, &mut proofs).unwrap();
    assert_eq!(
        stats.bounds_checks_elided, 0,
        "actual index zero only certifies length >=1"
    );
    for id in [first, second] {
        assert!(matches!(proofs.reads[&id].witness, BoundsWitness::Checked));
    }
    snapshots(&before, &func);
    array_reads::verify_reads(&func, &proofs).unwrap();
}

#[test]
fn opt_range_array_mutable_layout_barriers_keep_next_bounds_checked() {
    for barrier in [
        Opcode::Poll,
        Opcode::Call { site: 0 },
        Opcode::Opaque(Op::Aset),
        Opcode::AllocCons,
        Opcode::OpaqueBool(Op::Consp),
    ] {
        let (mut func, arg, callee, _) = empty(1);
        let base = checked(&mut func, arg, TypeSet::VECTOR, Rep::Tagged);
        let four = literal(&mut func, Block(0), 4);
        let two = literal(&mut func, Block(0), 2);
        let (first, _) = read(&mut func, Block(0), 1, base, four);
        let arguments = match &barrier {
            Opcode::Call { .. } => vec![callee],
            Opcode::Opaque(Op::Aset) => vec![base, two, four],
            Opcode::AllocCons => vec![two, four],
            Opcode::OpaqueBool(_) => vec![base],
            _ => vec![],
        };
        let id = Inst(func.insts.len() as u32);
        if barrier == Opcode::Poll {
            func.insts.push(InstData {
                op: barrier.clone(),
                args: arguments,
                result: None,
                eff: Effects::PURE,
                mem: AliasClass::None,
                frame: None,
                pc: 2,
            });
            func.blocks[0].insts.push(id);
        } else {
            let (ty, rep) = if matches!(barrier, Opcode::OpaqueBool(_)) {
                (TypeSet::BOOLEAN, Rep::Bool)
            } else {
                (TypeSet::TOP, Rep::Tagged)
            };
            let result = emit(&mut func, Block(0), 2, barrier.clone(), &arguments, ty, rep);
            assert_eq!(inst(&func, result), id);
        }
        // Opcode barriers remain authoritative even under verifier-accepted
        // weak hint effects. An actual source effect may be more conservative.
        framed(
            &mut func,
            id,
            &[base, two, four],
            Effects::PURE,
            AliasClass::None,
        );
        let (second, result) = read(&mut func, Block(0), 3, base, two);
        func.blocks[0].term = Term::Return(result);
        let mut proofs = prepared(&mut func);
        let before = func.clone();
        let stats = run_with_array_proofs(&mut func, &mut proofs).unwrap();
        assert_eq!(stats.bounds_checks_elided, 0, "{barrier:?}");
        assert!(matches!(
            proofs.reads[&first].witness,
            BoundsWitness::Checked
        ));
        assert!(matches!(
            proofs.reads[&second].witness,
            BoundsWitness::Checked
        ));
        assert_eq!(func.insts[id.index()].op, barrier);
        snapshots(&before, &func);
        array_reads::verify_reads(&func, &proofs).unwrap();
    }
}

#[test]
fn opt_range_array_unique_predecessor_boundary_resets_layout_floor() {
    let (mut func, arg, _, _) = empty(2);
    let base = checked(&mut func, arg, TypeSet::VECTOR, Rep::Tagged);
    let four = literal(&mut func, Block(0), 4);
    let two = literal(&mut func, Block(0), 2);
    let (first, _) = read(&mut func, Block(0), 1, base, four);
    let (second, result) = read(&mut func, Block(1), 2, base, two);
    func.blocks[0].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![],
    });
    func.blocks[1].preds = vec![Block(0)];
    func.blocks[1].term = Term::Return(result);
    let mut proofs = prepared(&mut func);
    let before = func.clone();
    let stats = run_with_array_proofs(&mut func, &mut proofs).unwrap();
    assert_eq!(stats.bounds_checks_elided, 0);
    for id in [first, second] {
        assert!(matches!(proofs.reads[&id].witness, BoundsWitness::Checked));
    }
    snapshots(&before, &func);
    array_reads::verify_reads(&func, &proofs).unwrap();
}

#[test]
fn opt_range_array_declines_branch_only_interval_before_counting_bce() {
    let (mut func, arg, _, index_arg) = empty(3);
    let base = checked(&mut func, arg, TypeSet::VECTOR, Rep::Tagged);
    let index = checked(
        &mut func,
        index_arg,
        TypeSet::fixnum_range(Range { lo: 0, hi: 10 }),
        Rep::TaggedFix,
    );
    let three = literal(&mut func, Block(0), 3);
    let four = literal(&mut func, Block(0), 4);
    let flag = emit(
        &mut func,
        Block(0),
        1,
        Opcode::FixCmp(Cmp::Lt),
        &[index, three],
        TypeSet::BOOLEAN,
        Rep::Bool,
    );
    let (first, _) = read(&mut func, Block(1), 2, base, four);
    let edge_ty = TypeSet::fixnum_range(Range { lo: 0, hi: 2 });
    let edge_index = emit(
        &mut func,
        Block(1),
        3,
        Opcode::Refine(edge_ty),
        &[index],
        edge_ty,
        Rep::TaggedFix,
    );
    let (second, result) = read(&mut func, Block(1), 4, base, edge_index);
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
    func.blocks[1].term = Term::Return(result);
    func.blocks[2].term = Term::Return(index);
    let mut proofs = prepared(&mut func);
    let supported = array_reads::checked_index_ranges(&func, &proofs).unwrap();
    assert!(supported.contains_key(&proofs.reads[&first].bounds));
    assert!(
        !supported.contains_key(&proofs.reads[&second].bounds),
        "a declared Refine is not an interval guard"
    );
    let before = func.clone();
    let stats = run_with_array_proofs(&mut func, &mut proofs).unwrap();
    assert_eq!(
        stats.bounds_checks_elided, 0,
        "unsupported proof is declined before nonvacuity census"
    );
    assert!(matches!(
        proofs.reads[&second].witness,
        BoundsWitness::Checked
    ));
    snapshots(&before, &func);
    array_reads::verify_reads(&func, &proofs).unwrap();
}

#[test]
fn opt_range_array_old_length_snapshot_cannot_inherit_later_guard_floor() {
    let (mut func, arg, _, _) = empty(1);
    let base = checked(&mut func, arg, TypeSet::VECTOR, Rep::Tagged);
    let four = literal(&mut func, Block(0), 4);
    let two = literal(&mut func, Block(0), 2);
    let (first, _) = read(&mut func, Block(0), 1, base, four);
    let (second, result) = read(&mut func, Block(0), 2, base, two);
    func.blocks[0].term = Term::Return(result);
    let mut proofs = prepared(&mut func);
    let earlier_check = proofs.reads[&first].bounds;
    let old_length = proofs.reads[&second].length_read;
    // Valid read-only input: both lengths were loaded before the first Bounds.
    // The second load is already shape-guarded, has no index dependency and
    // keeps its original frame/PC. It must not retroactively gain a later fact.
    let order = &mut func.blocks[0].insts;
    let old_position = order.iter().position(|&id| id == old_length).unwrap();
    order.remove(old_position);
    let before_guard = order.iter().position(|&id| id == earlier_check).unwrap();
    order.insert(before_guard, old_length);
    // This synthetic input intentionally reorders a future-pc read before
    // the earlier guard. Its first pc therefore has no bytecode post-state:
    // the verifier's ordered source cursor would stop at the future pc before
    // the earlier result exists. Keep the real frames and second source state,
    // with an explicit partial mapping fixed BEFORE baseline verification.
    // Range must preserve every supplied mapping unchanged after this setup.
    func.source_states[1] = None;
    func.verify().unwrap();
    array_reads::verify_reads(&func, &proofs).unwrap();
    let before = func.clone();
    let stats = run_with_array_proofs(&mut func, &mut proofs).unwrap();
    assert_eq!(stats.bounds_checks_elided, 0);
    assert!(matches!(
        proofs.reads[&second].witness,
        BoundsWitness::Checked
    ));
    assert_eq!(
        func.insts[proofs.reads[&second].bounds.index()].op,
        Opcode::CheckBounds
    );
    snapshots(&before, &func);
    array_reads::verify_reads(&func, &proofs).unwrap();
}
