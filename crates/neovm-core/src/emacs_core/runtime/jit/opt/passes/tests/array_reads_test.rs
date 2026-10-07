//! Array proof timing, admission and mutable-layout invalidation.
//! These are bounded compiler metadata tests, not GNU expected-value oracles.
//! Native/source parity is independently exercised by opt_arrays.rs draft.
//! Context precedes all symbols/heap constants; plans/tables are compiler-owned.

use super::super::array_reads::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::compile::{analyze_cfg, compile_pipeline_tests::function};
use crate::emacs_core::jit::opt::{
    build,
    ir::*,
    mem::{AliasClass, Effects},
    types::{Range, TypeSet},
};
use crate::emacs_core::value::{Value as LispValue, ValueKind, VecLikeType};

fn prepared(source: &ByteCodeFunction, prefix: usize) -> Func {
    let arity = source.params.required.len();
    let cfg = analyze_cfg(
        source.executable_ops(),
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        arity,
    )
    .unwrap();
    let pool = source
        .constants
        .iter()
        .copied()
        .map(ValueBits::from_value)
        .collect::<Vec<_>>();
    build::build(build::BuildInput {
        ops: source.executable_ops(),
        constants: &pool,
        cfg: &cfg,
        params: ParamShape {
            required: arity,
            ..Default::default()
        },
        dynamic_prefix: prefix,
        fused: None,
        osr: None,
    })
    .unwrap()
}
fn hints(source: &ByteCodeFunction, prefix: usize) -> ArrayAdmission {
    let constant_types = source
        .constants
        .iter()
        .enumerate()
        .skip(prefix)
        .filter_map(|(n, &value)| {
            let ty = match value.kind() {
                ValueKind::Veclike(VecLikeType::Vector) => TypeSet::VECTOR,
                ValueKind::Veclike(VecLikeType::Record) => TypeSet::RECORD,
                _ => return None,
            };
            Some((n as u32, (ValueBits::from_value(value), ty)))
        })
        .collect();
    ArrayAdmission {
        constant_types,
        ..Default::default()
    }
}
fn repeated(array: LispValue) -> ByteCodeFunction {
    // Keep the same SSA array identity across both reads. Two independent
    // Const instructions have distinct origins; these metadata tests must not
    // invent the broader constant-identity proof that their validator declines.
    function(
        vec![
            Op::Constant(0),
            Op::Dup,
            Op::Constant(1),
            Op::Aref,
            Op::StackRef(1),
            Op::Constant(2),
            Op::Aref,
            Op::List(2),
            Op::Return,
        ],
        vec![array, LispValue::fixnum(1), LispValue::fixnum(0)],
        0,
    )
}
fn optimized(source: &ByteCodeFunction) -> (Func, ArrayReadProofs) {
    let mut func = prepared(source, 0);
    let mut table = ArrayReadProofs::default();
    assert_eq!(
        lift(&mut func, &mut table, &hints(source, 0))
            .unwrap()
            .reads_lifted,
        2
    );
    proved_second(&mut func, &mut table);
    (func, table)
}
/// Independently assemble one valid numeric certificate from the first retained
/// successful Bounds. These verifier fixtures do not depend on Range choosing
/// an elimination; native source tests cover that separate pass behavior.
fn proved_second(func: &mut Func, table: &mut ArrayReadProofs) {
    let mut ordered = table
        .reads
        .iter()
        .map(|(&id, proof)| (id, proof.clone()))
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(_, proof)| proof.pc);
    assert_eq!(ordered.len(), 2);
    let (_, first) = &ordered[0];
    let (read, second) = &ordered[1];
    let floor = func.values[first.checked_index.index()]
        .ty
        .range()
        .unwrap()
        .lo
        .max(0)
        + 1;
    let length_ty = TypeSet::fixnum_range(Range {
        lo: floor,
        hi: Range::FULL.hi,
    });
    let (_, length_view) = insert(
        func,
        Some(second.bounds),
        None,
        InstData {
            op: Opcode::Refine(length_ty),
            args: vec![second.length],
            result: None,
            eff: Effects::PURE,
            mem: AliasClass::None,
            frame: None,
            pc: second.pc,
        },
        Some((length_ty, Rep::RawInt)),
    );
    let result_ty = func.values[second.bounds_result.index()].ty;
    let check = &mut func.insts[second.bounds.index()];
    check.op = Opcode::Refine(result_ty);
    check.args = vec![second.checked_index];
    check.eff = Effects::PURE;
    check.mem = AliasClass::None;
    table.reads.get_mut(read).unwrap().witness = BoundsWitness::Elided(NumericBoundsWitness {
        index_view: second.checked_index,
        length_view: length_view.unwrap(),
        floor: LengthFloorWitness {
            guard: first.bounds,
            length_read: first.length_read,
            index: first.checked_index,
            floor,
        },
    });
    func.array_reads = table.clone();
    verify_reads(func, table).expect("independent retained Bounds certificate must be valid");
    func.verify()
        .expect("ordinary IR and published certificate must both be valid");
}
/// The intentionally poisoned table is an external admission candidate. Keep
/// ordinary IR verification independent so failure comes from its real proof
/// validator, including after Func::verify gains sidecar validation.
fn ordinary_ir_with_external_proof(func: &mut Func) {
    func.array_reads = ArrayReadProofs::default();
    func.verify().unwrap();
}
fn current(table: &ArrayReadProofs) -> (Inst, ArrayReadProof) {
    table
        .reads
        .iter()
        .find(|(_, p)| matches!(p.witness, BoundsWitness::Elided(_)))
        .map(|(&i, p)| (i, p.clone()))
        .expect("source repeated lower index really elides")
}
fn insert(
    func: &mut Func,
    before: Option<Inst>,
    after: Option<Inst>,
    mut inst: InstData,
    ty: Option<(TypeSet, Rep)>,
) -> (Inst, Option<Value>) {
    let id = Inst(func.insts.len() as u32);
    let result = ty.map(|(ty, rep)| {
        let value = Value(func.values.len() as u32);
        func.values.push(ValueData {
            ty,
            rep,
            def: ValueDef::Inst(id),
        });
        value
    });
    inst.result = result;
    func.insts.push(inst);
    for block in &mut func.blocks {
        let position = before
            .and_then(|i| block.insts.iter().position(|&at| at == i))
            .or_else(|| {
                after.and_then(|i| block.insts.iter().position(|&at| at == i).map(|p| p + 1))
            });
        if let Some(position) = position {
            block.insts.insert(position, id);
            return (id, result);
        }
    }
    panic!("insertion point attached")
}

#[test]
fn opt_array_lift_actual_constant_kinds_have_real_ordered_guards() {
    let _ctx = Context::new();
    let vector = LispValue::vector(vec![LispValue::fixnum(13), LispValue::fixnum(23)]);
    let record =
        LispValue::make_record(vec![LispValue::symbol("record-tag"), LispValue::fixnum(29)]);
    for array in [vector, record] {
        let source = repeated(array);
        let before = prepared(&source, 0);
        let mut func = before.clone();
        let mut table = ArrayReadProofs::default();
        let stats = lift(&mut func, &mut table, &hints(&source, 0)).unwrap();
        func.verify().unwrap();
        verify_reads(&func, &table).unwrap();
        assert_eq!(stats.reads_lifted, 2);
        assert_eq!(stats.bounds_inserted, 2);
        assert_eq!(func.frames, before.frames);
        assert_eq!(func.entry_stacks, before.entry_stacks);
        let mut by_pc = table
            .reads
            .iter()
            .map(|(&id, p)| (p.pc, id, p))
            .collect::<Vec<_>>();
        by_pc.sort_by_key(|(pc, _, _)| *pc);
        for (_, id, p) in by_pc {
            assert_eq!(func.insts[id.index()].op, Opcode::Opaque(Op::Aref));
            assert_eq!(
                func.insts[id.index()].result,
                before.insts[id.index()].result
            );
            assert_eq!(func.insts[id.index()].frame, before.insts[id.index()].frame);
            assert_eq!(func.values[p.length.index()].rep, Rep::RawInt);
            let ValueDef::Inst(index_guard) = func.values[p.checked_index.index()].def else {
                panic!("real index guard")
            };
            let ValueDef::Inst(shape_guard) = func.values[p.guarded_base.index()].def else {
                panic!("real shape guard")
            };
            let block = func.blocks.iter().find(|b| b.insts.contains(&id)).unwrap();
            let order = |inst: Inst| block.insts.iter().position(|&x| x == inst).unwrap();
            assert!(
                order(index_guard) < order(p.bounds) && order(shape_guard) < order(p.length_read)
            );
            assert!(order(p.length_read) < order(p.bounds) && order(p.bounds) < order(id));
        }
    }
}

#[test]
fn opt_array_lift_unknown_string_and_dynamic_prefix_are_not_kind_evidence() {
    let _ctx = Context::new();
    let array = LispValue::vector(vec![LispValue::fixnum(1)]);
    let text = LispValue::string("x");
    let unknown = function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Aref, Op::Return],
        vec![LispValue::fixnum(0)],
        1,
    );
    let known_text = function(
        vec![Op::Constant(0), Op::Constant(1), Op::Aref, Op::Return],
        vec![text, LispValue::fixnum(0)],
        0,
    );
    let prefix = function(
        vec![Op::Constant(0), Op::Constant(1), Op::Aref, Op::Return],
        vec![array, LispValue::fixnum(0)],
        0,
    );
    for (source, dynamic) in [(&unknown, 0), (&known_text, 0), (&prefix, 1)] {
        let before = prepared(source, dynamic);
        let mut func = before.clone();
        let mut table = ArrayReadProofs::default();
        assert_eq!(
            lift(&mut func, &mut table, &hints(source, dynamic))
                .unwrap()
                .reads_lifted,
            0
        );
        assert!(table.reads.is_empty());
        assert_eq!(func.insts.len(), before.insts.len());
        func.verify().unwrap();
    }
}

#[test]
fn opt_array_final_proof_rejects_future_current_interval_guard() {
    let _ctx = Context::new();
    let source = repeated(LispValue::vector(vec![
        LispValue::fixnum(13),
        LispValue::fixnum(23),
    ]));
    let (mut func, mut table) = optimized(&source);
    let (read, proof) = current(&table);
    let ty = func.values[proof.checked_index.index()].ty;
    let (_, view) = insert(
        &mut func,
        None,
        Some(read),
        InstData {
            op: Opcode::CheckType(ty),
            args: vec![proof.checked_index],
            result: None,
            eff: Effects::MAY_DEOPT,
            mem: AliasClass::None,
            frame: Some(proof.frame),
            pc: proof.pc,
        },
        Some((ty, Rep::TaggedFix)),
    );
    let BoundsWitness::Elided(witness) = &mut table.reads.get_mut(&read).unwrap().witness else {
        panic!("elided")
    };
    witness.index_view = view.unwrap();
    ordinary_ir_with_external_proof(&mut func);
    assert_eq!(
        verify_reads(&func, &table).unwrap_err().reason,
        "current-owner-numeric-proof"
    );
}

#[test]
fn opt_array_final_proof_rejects_future_floor_interval_guard() {
    let _ctx = Context::new();
    let source = repeated(LispValue::vector(vec![
        LispValue::fixnum(13),
        LispValue::fixnum(23),
    ]));
    let (mut func, mut table) = optimized(&source);
    let (read, proof) = current(&table);
    let BoundsWitness::Elided(witness) = &proof.witness else {
        panic!("elided")
    };
    let floor = witness.floor.clone();
    let ty = func.values[floor.index.index()].ty;
    let (_, view) = insert(
        &mut func,
        Some(proof.length_read),
        None,
        InstData {
            op: Opcode::CheckType(ty),
            args: vec![floor.index],
            result: None,
            eff: Effects::MAY_DEOPT,
            mem: AliasClass::None,
            frame: Some(proof.frame),
            pc: proof.pc,
        },
        Some((ty, Rep::TaggedFix)),
    );
    let BoundsWitness::Elided(witness) = &mut table.reads.get_mut(&read).unwrap().witness else {
        panic!("elided")
    };
    witness.floor.index = view.unwrap();
    ordinary_ir_with_external_proof(&mut func);
    assert_eq!(
        verify_reads(&func, &table).unwrap_err().reason,
        "current-owner-numeric-proof"
    );
}

#[test]
fn opt_array_final_proof_rejects_narrow_declared_refine_without_interval_source() {
    let _ctx = Context::new();
    // Dynamic checked first index, then literal0. A successful first bounds
    // legitimately proves length>=1; a declared Refine must not invent>=2.
    let source = function(
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Aref,
            Op::StackRef(2),
            Op::Constant(0),
            Op::Aref,
            Op::List(2),
            Op::Return,
        ],
        vec![LispValue::fixnum(0)],
        2,
    );
    let mut func = prepared(&source, 0);
    let mut table = ArrayReadProofs::default();
    let mut admission = ArrayAdmission {
        site_types: vec![TypeSet::BOTTOM; source.executable_ops().len()],
        ..Default::default()
    };
    // A hint is allowed to select the guard but never narrows the live Arg.
    admission.site_types[2] = TypeSet::VECTOR;
    admission.site_types[5] = TypeSet::VECTOR;
    assert_eq!(
        lift(&mut func, &mut table, &admission)
            .unwrap()
            .reads_lifted,
        2
    );
    proved_second(&mut func, &mut table);
    let (read, proof) = current(&table);
    let BoundsWitness::Elided(witness) = &proof.witness else {
        panic!("elided")
    };
    let floor = witness.floor.clone();
    let old = &func.values[floor.index.index()];
    let rep = old.rep;
    let ty = old.ty.meet(TypeSet::fixnum_range(Range { lo: 1, hi: 1 }));
    let pc = func.insts[floor.guard.index()].pc;
    let (_, view) = insert(
        &mut func,
        Some(floor.guard),
        None,
        InstData {
            op: Opcode::Refine(ty),
            args: vec![floor.index],
            result: None,
            eff: Effects::PURE,
            mem: AliasClass::None,
            frame: None,
            pc,
        },
        Some((ty, rep)),
    );
    let BoundsWitness::Elided(witness) = &mut table.reads.get_mut(&read).unwrap().witness else {
        panic!("elided")
    };
    witness.floor.index = view.unwrap();
    ordinary_ir_with_external_proof(&mut func);
    assert_eq!(
        verify_reads(&func, &table).unwrap_err().reason,
        "floor-index"
    );
}

#[test]
fn opt_array_final_proof_rejects_stale_floor_after_poll_even_if_no_observed_mutation() {
    let _ctx = Context::new();
    let source = repeated(LispValue::vector(vec![
        LispValue::fixnum(13),
        LispValue::fixnum(23),
    ]));
    let (mut func, table) = optimized(&source);
    let (_, proof) = current(&table);
    // Valid authoritative-IR Poll is a conservative metadata test only; no
    // synthetic automatic-hook timing/GNU expected result is invented here.
    insert(
        &mut func,
        Some(proof.length_read),
        None,
        InstData {
            op: Opcode::Poll,
            args: vec![],
            result: None,
            eff: Effects::UNKNOWN,
            mem: AliasClass::Unknown,
            frame: Some(proof.frame),
            pc: proof.pc,
        },
        None,
    );
    ordinary_ir_with_external_proof(&mut func);
    assert_eq!(
        verify_reads(&func, &table).unwrap_err().reason,
        "current-owner-numeric-proof"
    );
}

#[test]
fn opt_array_checked_proof_rejects_declared_fix_index_without_executed_tag_guard() {
    let _ctx = Context::new();
    let source = function(
        vec![Op::StackRef(1), Op::StackRef(1), Op::Aref, Op::Return],
        vec![],
        2,
    );
    let mut func = prepared(&source, 0);
    let owner = func
        .insts
        .iter()
        .position(|inst| inst.op == Opcode::Opaque(Op::Aref))
        .map(|n| Inst(n as u32))
        .unwrap();
    let original = func.insts[owner.index()].clone();
    let frame = original.frame.unwrap();
    let array_ty = TypeSet::VECTOR.join(TypeSet::RECORD);
    let (shape, base) = append(
        &mut func,
        Opcode::CheckType(array_ty),
        vec![original.args[0]],
        array_ty,
        Rep::Tagged,
        Effects::MAY_DEOPT,
        AliasClass::None,
        frame,
        original.pc,
    );
    // This is a legal same-rep narrow declaration, not an executed type guard.
    // The final independent array proof may never infer CHECK_FIXNUM from it.
    let (declared, index) = append(
        &mut func,
        Opcode::Refine(TypeSet::FIXNUM),
        vec![original.args[1]],
        TypeSet::FIXNUM,
        Rep::Tagged,
        Effects::PURE,
        AliasClass::None,
        frame,
        original.pc,
    );
    let (length_read, length) = append(
        &mut func,
        Opcode::LoadVecLen,
        vec![base],
        TypeSet::fixnum_range(Range {
            lo: 0,
            hi: Range::FULL.hi,
        }),
        Rep::RawInt,
        Effects::READ_HEAP,
        AliasClass::Unknown,
        frame,
        original.pc,
    );
    let (bounds, bounds_result) = append(
        &mut func,
        Opcode::CheckBounds,
        vec![index, length],
        TypeSet::FIXNUM,
        Rep::Tagged,
        Effects::MAY_DEOPT,
        AliasClass::None,
        frame,
        original.pc,
    );
    func.insts[owner.index()].args = vec![base, bounds_result];
    let block = func
        .blocks
        .iter_mut()
        .find(|block| block.insts.contains(&owner))
        .unwrap();
    let before = block.insts.iter().position(|&inst| inst == owner).unwrap();
    block
        .insts
        .splice(before..before, [shape, declared, length_read, bounds]);
    func.verify()
        .expect("ordinary IR is valid; standalone proof must establish numeric grounding");
    let mut table = ArrayReadProofs::default();
    table.reads.insert(
        owner,
        ArrayReadProof {
            guarded_base: base,
            checked_index: index,
            length_read,
            length,
            bounds,
            bounds_result,
            frame,
            pc: original.pc,
            witness: BoundsWitness::Checked,
        },
    );
    assert!(
        verify_reads(&func, &table).is_err(),
        "retained bounds cannot replace an absent CHECK_FIXNUM before raw index untagging"
    );
}

#[test]
fn opt_array_cfg_cleanup_preserves_compacted_owner_and_numeric_witnesses() {
    use super::super::cfg::cleanup;

    let _ctx = Context::new();
    let source = repeated(LispValue::vector(vec![
        LispValue::fixnum(13),
        LispValue::fixnum(23),
    ]));
    let ordinary = prepared(&source, 0);
    ordinary
        .verify()
        .expect("original source IR must be ordinary-valid");
    let (mut func, table) = optimized(&source);
    func.verify()
        .expect("real lift and published numeric certificate must verify");
    let before_caps = verify_reads(&func, &table).expect("actual baseline native capabilities");
    assert_eq!(before_caps.reads.len(), 2);
    assert_eq!(before_caps.length_reads.len(), 2);
    assert_eq!(before_caps.bounds.len(), 2);
    assert_eq!(
        before_caps
            .reads
            .values()
            .filter(|p| p.bounds_elided)
            .count(),
        1
    );

    // Add a VALID unreachable leading definition so the utility must compact
    // block, instruction and value IDs, rather than only copy an unchanged
    // sidecar. The live source graph and full source/entry/frame semantics are
    // unchanged. All references are shifted before ordinary/table validation.
    let shifted = |value: Value| Value(value.0 + 1);
    for value in &mut func.values {
        value.def = match value.def {
            ValueDef::Inst(id) => ValueDef::Inst(Inst(id.0 + 1)),
            ValueDef::Param { block, index } => ValueDef::Param {
                block: Block(block.0 + 1),
                index,
            },
            ValueDef::Alias(next) => ValueDef::Alias(shifted(next)),
        };
        if let Rep::RawPtr { base } = value.rep {
            value.rep = Rep::RawPtr {
                base: shifted(base),
            };
        }
    }
    for inst in &mut func.insts {
        inst.args.iter_mut().for_each(|v| *v = shifted(*v));
        inst.result = inst.result.map(shifted);
    }
    for block in &mut func.blocks {
        block.params.iter_mut().for_each(|v| *v = shifted(*v));
        block.insts.iter_mut().for_each(|id| id.0 += 1);
        block.preds.iter_mut().for_each(|id| id.0 += 1);
        match &mut block.term {
            Term::Return(value) | Term::Branch { flag: value, .. } => *value = shifted(*value),
            Term::Switch { value, table, .. } => {
                *value = shifted(*value);
                *table = shifted(*table);
            }
            _ => {}
        }
        let edges: Vec<_> = match &mut block.term {
            Term::Jump(edge) => vec![edge],
            Term::Branch {
                if_true, if_false, ..
            } => vec![if_true, if_false],
            Term::Switch { cases, default, .. } => cases
                .iter_mut()
                .map(|c| &mut c.edge)
                .chain([default])
                .collect(),
            _ => vec![],
        };
        for edge in edges {
            edge.target.0 += 1;
            edge.args.iter_mut().for_each(|v| *v = shifted(*v));
        }
    }
    for frame in &mut func.frames {
        frame.stack.iter_mut().for_each(|v| *v = shifted(*v));
    }
    for stack in &mut func.entry_stacks {
        stack.iter_mut().for_each(|v| *v = shifted(*v));
    }
    for source in func.source_states.iter_mut().flatten() {
        source.block.0 += 1;
        source.pre.iter_mut().for_each(|v| *v = shifted(*v));
        source.post.iter_mut().for_each(|v| *v = shifted(*v));
    }
    assert!(
        func.osr.is_none(),
        "normal source fixture has no OSR ambiguity"
    );
    assert!(func.blocks.iter().all(|b| b.loop_header.is_none()));
    func.entry.0 += 1;
    let literal_ty = TypeSet::for_constant(func.consts[1]);
    func.values.insert(
        0,
        ValueData {
            ty: literal_ty,
            rep: Rep::Tagged,
            def: ValueDef::Inst(Inst(0)),
        },
    );
    func.insts.insert(
        0,
        InstData {
            op: Opcode::Const(1),
            args: vec![],
            result: Some(Value(0)),
            eff: Effects::PURE,
            mem: AliasClass::None,
            frame: None,
            pc: 900,
        },
    );
    let mut dead = BlockData::new(900);
    dead.insts.push(Inst(0));
    dead.term = Term::Return(Value(0));
    func.blocks.insert(0, dead);
    func.entry_stacks.insert(0, Box::default());
    func.array_reads = table
        .remap(
            |id| Some(Inst(id.0 + 1)),
            |value| Some(shifted(value)),
            Some,
        )
        .expect("shift all baseline metadata IDs consistently");
    func.rebuild_frame_intern();
    func.verify()
        .expect("unreachable leading definition is valid ordinary IR plus sidecar");
    verify_reads(&func, &func.array_reads).expect("full certificate is valid BEFORE cleanup");
    assert!(
        func.values
            .iter()
            .all(|v| !matches!(v.def, ValueDef::Alias(_))),
        "one known dead definition is the fixture's only SSA compaction hole"
    );
    let before = func.clone();
    let (_, old_elided) = current(&before.array_reads);
    let BoundsWitness::Elided(old_witness) = &old_elided.witness else {
        unreachable!()
    };
    let metadata_only = old_witness.length_view;
    assert!(
        before
            .insts
            .iter()
            .all(|i| !i.args.contains(&metadata_only))
    );
    assert!(
        before
            .frames
            .iter()
            .all(|f| !f.stack.contains(&metadata_only))
    );
    assert!(
        before
            .source_states
            .iter()
            .flatten()
            .all(|s| !s.pre.contains(&metadata_only) && !s.post.contains(&metadata_only))
    );

    cleanup(&mut func).expect("transactional CFG and SSA compaction");
    func.verify()
        .expect("compacted ordinary IR and owned sidecar must verify");
    assert_eq!(
        func.array_reads.reads.len(),
        2,
        "cleanup must preserve both reachable original Aref owners, not silently erase certificates"
    );
    assert_eq!(func.blocks.len() + 1, before.blocks.len());
    assert_eq!(func.insts.len() + 1, before.insts.len());
    assert_eq!(func.values.len() + 1, before.values.len());
    let caps = verify_reads(&func, &func.array_reads).expect("remapped final native capabilities");
    assert_eq!(caps.reads.len(), 2);
    assert_eq!(caps.length_reads.len(), 2);
    assert_eq!(caps.bounds.len(), 2);
    assert_eq!(caps.reads.values().filter(|p| p.bounds_elided).count(), 1);
    let compact = |value: Value| Value(value.0 - 1);
    for (&old_owner, old) in &before.array_reads.reads {
        let owner = Inst(old_owner.0 - 1);
        let proof = func
            .array_reads
            .reads
            .get(&owner)
            .expect("original owner was remapped");
        assert_eq!(proof.guarded_base, compact(old.guarded_base));
        assert_eq!(proof.checked_index, compact(old.checked_index));
        assert_eq!(proof.length_read, Inst(old.length_read.0 - 1));
        assert_eq!(proof.length, compact(old.length));
        assert_eq!(proof.bounds, Inst(old.bounds.0 - 1));
        assert_eq!(proof.bounds_result, compact(old.bounds_result));
        assert_eq!(proof.pc, old.pc);
        assert_eq!(
            func.insts[owner.index()].result,
            before.insts[old_owner.index()].result.map(compact)
        );
        assert_eq!(func.insts[owner.index()].frame, Some(proof.frame));
        assert_eq!(
            func.frames[proof.frame.index()].stack.as_ref(),
            before.frames[old.frame.index()]
                .stack
                .iter()
                .copied()
                .map(compact)
                .collect::<Vec<_>>()
                .as_slice()
        );
        match (&old.witness, &proof.witness) {
            (BoundsWitness::Checked, BoundsWitness::Checked) => {}
            (BoundsWitness::Elided(a), BoundsWitness::Elided(b)) => {
                assert_eq!(b.index_view, compact(a.index_view));
                assert_eq!(b.length_view, compact(a.length_view));
                assert_eq!(b.floor.guard, Inst(a.floor.guard.0 - 1));
                assert_eq!(b.floor.length_read, Inst(a.floor.length_read.0 - 1));
                assert_eq!(b.floor.index, compact(a.floor.index));
                assert_eq!(b.floor.floor, a.floor.floor);
                let ValueDef::Inst(view) = func.values[b.length_view.index()].def else {
                    panic!("metadata-only numeric view retains its definition")
                };
                assert!(func.blocks.iter().any(|block| block.insts.contains(&view)));
                assert!(matches!(func.insts[view.index()].op, Opcode::Refine(_)));
                assert_eq!(func.insts[view.index()].args, [proof.length]);
                assert!(
                    func.insts.iter().all(|i| !i.args.contains(&b.length_view)),
                    "proof retention adds no runtime semantic use"
                );
            }
            _ => panic!("bounds witness kind must survive compaction"),
        }
    }
    for (old, new) in before.source_states.iter().zip(&func.source_states) {
        match (old, new) {
            (None, None) => {}
            (Some(old), Some(new)) => {
                assert_eq!(new.block, Block(old.block.0 - 1));
                assert_eq!(
                    new.pre.as_ref(),
                    old.pre
                        .iter()
                        .copied()
                        .map(compact)
                        .collect::<Vec<_>>()
                        .as_slice()
                );
                assert_eq!(
                    new.post.as_ref(),
                    old.post
                        .iter()
                        .copied()
                        .map(compact)
                        .collect::<Vec<_>>()
                        .as_slice()
                );
                assert_eq!(
                    func.frames[new.frame.index()].pc,
                    before.frames[old.frame.index()].pc
                );
            }
            _ => panic!("reachable full source state must survive"),
        }
    }
}
