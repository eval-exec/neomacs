//! CFG surgery preserves the executable SSA graph and complete source frames.

use super::{Dominance, cleanup};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::opt::eval::{Inputs, Outcome, evaluate};
use crate::emacs_core::jit::opt::ir::*;
use crate::emacs_core::jit::opt::mem::{AliasClass, Effects};
use crate::emacs_core::jit::opt::types::TypeSet;
use crate::emacs_core::value::Value as LispValue;

fn emit(func: &mut Func, block: Block, op: Opcode, ty: TypeSet) -> Value {
    let inst = Inst(func.insts.len() as u32);
    let value = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty,
        rep: Rep::Tagged,
        def: ValueDef::Inst(inst),
    });
    func.insts.push(InstData {
        op,
        args: vec![],
        result: Some(value),
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc: func.blocks[block.index()].pc,
    });
    func.blocks[block.index()].insts.push(inst);
    value
}

fn constant(func: &mut Func, block: Block, value: i64) -> Value {
    let bits = ValueBits::from_value(LispValue::fixnum(value));
    let index = func.consts.len() as u32;
    let mut constants = func.consts.to_vec();
    constants.push(bits);
    func.consts = constants.into();
    emit(
        func,
        block,
        Opcode::Const(index),
        TypeSet::for_constant(bits),
    )
}

fn frame(func: &mut Func, pc: u32, stack: &[Value], parent: Option<FrameId>) -> FrameId {
    func.intern_frame(FrameState {
        pc,
        stack: stack.into(),
        handlers: 2,
        binds: 3,
        parent,
        site: parent.map(|_| InlineSiteId(7)),
    })
}

fn poll(func: &mut Func, block: Block, frame: FrameId) {
    let inst = Inst(func.insts.len() as u32);
    func.insts.push(InstData {
        op: Opcode::Poll,
        args: vec![],
        result: None,
        eff: Effects::UNKNOWN,
        mem: AliasClass::Unknown,
        frame: Some(frame),
        pc: func.blocks[block.index()].pc,
    });
    func.blocks[block.index()].insts.push(inst);
}

fn state(func: &mut Func, block: Block, pre: &[Value], post: &[Value], frame: FrameId) {
    let pc = func.blocks[block.index()].pc as usize;
    if func.source_states.len() <= pc {
        func.source_states.resize(pc + 1, None);
    }
    func.source_states[pc] = Some(SourceState {
        pre: pre.into(),
        post: post.into(),
        frame,
        block,
    });
}

fn preds(func: &mut Func) {
    let mut incoming = vec![vec![]; func.blocks.len()];
    for (index, block) in func.blocks.iter().enumerate() {
        for edge in block.term.edges() {
            incoming[edge.target.index()].push(Block(index as u32));
        }
    }
    for (block, mut incoming) in func.blocks.iter_mut().zip(incoming) {
        incoming.sort_unstable();
        incoming.dedup();
        block.preds = incoming;
    }
}

/// Dead ids precede the entry and live join, so every handle family moves.
fn diamond() -> Func {
    let mut func = Func::new(
        Box::new([]),
        ParamShape {
            required: 1,
            ..ParamShape::default()
        },
        0,
    );
    func.blocks = [90, 0, 1, 2, 3].map(BlockData::new).into();
    func.entry = Block(1);
    let dead = constant(&mut func, Block(0), 99);
    func.blocks[0].term = Term::Return(dead);
    let dead_frame = frame(&mut func, 90, &[], None);
    state(&mut func, Block(0), &[], &[dead], dead_frame);
    let arg = emit(&mut func, Block(1), Opcode::Arg(0), TypeSet::TOP);
    let entry_frame = frame(&mut func, 0, &[arg], None);
    state(&mut func, Block(1), &[arg], &[arg], entry_frame);
    func.blocks[1].term = Term::Branch {
        flag: arg,
        if_true: Edge {
            target: Block(2),
            args: vec![],
        },
        if_false: Edge {
            target: Block(3),
            args: vec![],
        },
    };
    let left = constant(&mut func, Block(2), 11);
    let right = constant(&mut func, Block(3), 22);
    for (block, value) in [(Block(2), left), (Block(3), right)] {
        let pc = func.blocks[block.index()].pc;
        let snapshot = frame(&mut func, pc, &[arg], None);
        state(&mut func, block, &[arg], &[arg, value], snapshot);
        func.blocks[block.index()].term = Term::Jump(Edge {
            target: Block(4),
            args: vec![value],
        });
    }
    let merged = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty: TypeSet::FIXNUM,
        rep: Rep::Tagged,
        def: ValueDef::Param {
            block: Block(4),
            index: 0,
        },
    });
    func.blocks[4].params.push(merged);
    let snapshot = frame(&mut func, 3, &[arg, merged], Some(entry_frame));
    poll(&mut func, Block(4), snapshot);
    state(
        &mut func,
        Block(4),
        &[arg, merged],
        &[arg, merged],
        snapshot,
    );
    func.blocks[4].term = Term::Return(merged);
    func.entry_stacks = vec![
        Box::new([]),
        vec![arg].into(),
        vec![arg].into(),
        vec![arg].into(),
        vec![arg, merged].into(),
    ];
    preds(&mut func);
    func.verify().expect("well-formed fixture");
    func
}

fn answer(func: &Func, args: &[LispValue]) -> (ValueBits, Vec<(u32, Vec<ValueBits>, u16, u16)>) {
    let run = evaluate(
        func,
        &mut Context::new(),
        Inputs {
            args,
            ..Inputs::default()
        },
    )
    .expect("reference execution");
    let Outcome::Returned(value) = run.outcome else {
        panic!("unexpected deopt")
    };
    (
        value,
        run.trace
            .into_iter()
            .map(|s| (s.pc, s.stack, s.handlers, s.binds))
            .collect(),
    )
}

#[test]
fn opt_cfg_folded_branch_compacts_ids_and_preserves_full_frames() {
    let mut func = diamond();
    let expected = answer(&func, &[LispValue::t()]);
    let Term::Branch { if_true, .. } = func.blocks[1].term.clone() else {
        unreachable!()
    };
    func.blocks[1].term = Term::Jump(if_true);
    cleanup(&mut func).expect("compact folded graph");
    func.verify().expect("valid compact graph");
    assert_eq!(func.entry, Block(0));
    assert_eq!(func.blocks.len(), 3);
    assert_eq!(func.entry_stacks.len(), 3);
    assert!(func.source_states[2].is_none());
    assert!(func.source_states[90].is_none());
    assert_eq!(
        func.blocks[2].params.len(),
        1,
        "live join identity stays explicit"
    );
    let source = func.source_states[3].as_ref().unwrap();
    let snapshot = &func.frames[source.frame.index()];
    assert_eq!(snapshot.stack.as_ref(), source.pre.as_ref());
    assert_eq!(
        (snapshot.handlers, snapshot.binds, snapshot.site),
        (2, 3, Some(InlineSiteId(7)))
    );
    assert_eq!(
        func.frames[snapshot.parent.unwrap().index()].stack.as_ref(),
        &source.pre[..1]
    );
    assert_eq!(
        func.insts.iter().filter(|i| i.op == Opcode::Poll).count(),
        1
    );
    assert_eq!(answer(&func, &[LispValue::t()]), expected);
    assert_eq!(func.census.blocks, func.blocks.len());
    assert_eq!(func.census.insts, func.insts.len());
    assert_eq!(func.census.phis, 1);
    assert_eq!(func.census.frames, func.frames.len());
    let old_frame = source.frame;
    let saved_snapshot = snapshot.clone();
    let existing = func.intern_frame(saved_snapshot);
    assert_eq!(existing, old_frame, "frame interning uses compact handles");
}

#[test]
fn opt_cfg_live_join_and_duplicate_switch_edges_keep_distinct_inputs() {
    let mut func = diamond();
    let Term::Branch {
        flag,
        if_true,
        if_false,
    } = func.blocks[1].term.clone()
    else {
        unreachable!()
    };
    func.blocks[1].term = Term::Switch {
        value: flag,
        table: flag,
        cases: vec![
            SwitchCase {
                key: 0,
                edge: if_true.clone(),
            },
            SwitchCase {
                key: 1,
                edge: if_true,
            },
        ],
        default: if_false,
    };
    cleanup(&mut func).expect("compact switch graph");
    func.verify().expect("valid compact switch");
    assert_eq!(func.blocks.len(), 4);
    assert_eq!(
        func.blocks[1].preds,
        vec![Block(0)],
        "duplicate switch edges are one predecessor"
    );
    assert_eq!(func.blocks[3].preds, vec![Block(1), Block(2)]);
    assert_eq!(func.blocks[3].params.len(), 1);
    let inputs = [1, 2].map(|b| func.blocks[b].term.edges()[0].args[0]);
    assert_ne!(inputs[0], inputs[1]);
}

#[test]
fn opt_cfg_alias_users_and_frame_parent_ids_are_remapped() {
    let mut func = diamond();
    let arg = func.entry_stacks[func.entry.index()][0];
    let alias = Value(func.values.len() as u32);
    let mut data = func.values[arg.index()].clone();
    data.def = ValueDef::Alias(arg);
    func.values.push(data);
    let source = func.source_states[3].as_mut().unwrap();
    source.pre[0] = alias;
    source.post[0] = alias;
    let frame = source.frame;
    func.frames[frame.index()].stack[0] = alias;
    func.entry_stacks[4][0] = alias;
    let Term::Branch { flag, .. } = &mut func.blocks[1].term else {
        unreachable!()
    };
    *flag = alias;
    func.rebuild_frame_intern();
    func.verify().expect("aliases are valid before cleanup");
    cleanup(&mut func).expect("compact aliases");
    func.verify().expect("alias users resolve after compaction");
    assert!(
        !func
            .values
            .iter()
            .any(|v| matches!(v.def, ValueDef::Alias(_)))
    );
    assert_eq!(
        answer(&func, &[LispValue::NIL]).0,
        ValueBits::from_value(LispValue::fixnum(22))
    );
}

#[test]
fn opt_cfg_osr_synthetic_entry_keeps_unknown_seed_and_backedge_poll() {
    let mut func = Func::new(Box::new([]), ParamShape::default(), 0);
    func.blocks = [90, 10, 10, 11, 12, 11].map(BlockData::new).into();
    func.entry = Block(1);
    func.osr = Some(OsrEntry {
        entry_pc: 10,
        depth: 1,
        header: Block(2),
    });
    let dead = constant(&mut func, Block(0), 99);
    func.blocks[0].term = Term::Return(dead);
    let seed = emit(&mut func, Block(1), Opcode::OsrSlot(0), TypeSet::TOP);
    func.blocks[1].term = Term::Jump(Edge {
        target: Block(2),
        args: vec![],
    });
    func.blocks[2].loop_header = Some(LoopId(2));
    func.blocks[2].term = Term::Branch {
        flag: seed,
        if_true: Edge {
            target: Block(3),
            args: vec![],
        },
        if_false: Edge {
            target: Block(4),
            args: vec![],
        },
    };
    func.blocks[3].term = Term::Jump(Edge {
        target: Block(5),
        args: vec![],
    });
    func.blocks[4].term = Term::Return(seed);
    func.blocks[5].term = Term::Jump(Edge {
        target: Block(2),
        args: vec![],
    });
    let snapshot = frame(&mut func, 10, &[seed], None);
    state(&mut func, Block(2), &[seed], &[seed], snapshot);
    poll(&mut func, Block(5), snapshot);
    func.entry_stacks = (0..6)
        .map(|i| {
            if i == 0 {
                Box::new([]) as Box<[Value]>
            } else {
                vec![seed].into()
            }
        })
        .collect();
    preds(&mut func);
    func.verify().expect("valid OSR graph");
    cleanup(&mut func).expect("compact OSR graph");
    func.verify().expect("valid compact OSR graph");
    assert_eq!(func.entry, Block(0));
    let osr = func.osr.as_ref().unwrap();
    assert_eq!((osr.header, osr.entry_pc, osr.depth), (Block(1), 10, 1));
    assert!(func.blocks[osr.header.index()].loop_header.is_some());
    assert_eq!(
        func.blocks[osr.header.index()].preds,
        vec![Block(0), Block(4)]
    );
    let seed = func.entry_stacks[0][0];
    assert_eq!(func.values[seed.index()].ty, TypeSet::TOP);
    assert_eq!(func.values[seed.index()].rep, Rep::Tagged);
    assert_eq!(
        func.insts.iter().filter(|i| i.op == Opcode::Poll).count(),
        1
    );
    let run = evaluate(
        &func,
        &mut Context::new(),
        Inputs {
            osr_stack: &[LispValue::NIL],
            ..Inputs::default()
        },
    )
    .unwrap();
    assert!(matches!(run.outcome, Outcome::Returned(ValueBits(0))));
}

#[test]
fn opt_cfg_dominance_excludes_siblings_and_unreachable_blocks() {
    let func = diamond();
    let dom = Dominance::new(&func).expect("valid graph");
    assert!(!dom.is_reachable(Block(0)));
    assert!(!dom.dominates(Block(0), Block(0)));
    assert!(!dom.dominates(Block(2), Block(3)));
    assert!(!dom.dominates(Block(2), Block(4)));
    assert!(dom.dominates(Block(1), Block(4)));
    assert_eq!(dom.immediate_dominator(Block(4)), Some(Block(1)));
    assert_eq!(dom.reverse_postorder().first(), Some(&Block(1)));
}

#[test]
fn opt_cfg_rejects_live_frame_reference_to_removed_definition() {
    let mut func = diamond();
    let dead = func.insts[0].result.unwrap();
    let source = func.source_states[3].as_ref().unwrap();
    func.frames[source.frame.index()].stack[0] = dead;
    let before = format!("{}", func.display());
    assert!(
        cleanup(&mut func).is_err(),
        "never silently prune a live GNU-stack root"
    );
    assert_eq!(
        format!("{}", func.display()),
        before,
        "failed surgery is atomic"
    );
}

#[test]
fn opt_cfg_truncated_deopt_discards_detached_insts_and_later_states() {
    let mut func = diamond();
    let merged = func.blocks[4].params[0];
    let source = func.source_states[3].as_ref().unwrap();
    let arg = source.pre[0];
    // The test evaluator intentionally rejects deopt through inline parent
    // chains. Keep the complete GNU stack in a flat guard frame here; the
    // independent compaction regression above checks parent-id preservation.
    let mut flat_frame = func.frames[source.frame.index()].clone();
    flat_frame.parent = None;
    flat_frame.site = None;
    let snapshot = func.intern_frame(flat_frame);
    func.source_states[3].as_mut().unwrap().frame = snapshot;
    let guard = Inst(func.insts.len() as u32);
    let guarded = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty: TypeSet::BOTTOM,
        rep: Rep::Tagged,
        def: ValueDef::Inst(guard),
    });
    func.insts.push(InstData {
        op: Opcode::CheckType(TypeSet::CONS),
        args: vec![merged],
        result: Some(guarded),
        eff: Effects::MAY_DEOPT,
        mem: AliasClass::None,
        frame: Some(snapshot),
        pc: 3,
    });
    let tail = func.blocks[4].insts[0];
    func.insts[tail.index()].pc = 4;
    func.blocks[4].insts.insert(0, guard);
    func.source_states[3].as_mut().unwrap().post = vec![arg, guarded].into();
    let later_frame = frame(&mut func, 4, &[arg, guarded], None);
    func.source_states[4] = Some(SourceState {
        pre: vec![arg, guarded].into(),
        post: vec![arg, guarded].into(),
        frame: later_frame,
        block: Block(4),
    });
    func.verify()
        .expect("the contradiction is still an ordinary guard");
    let mut ctx = Context::new();
    let expected = evaluate(
        &func,
        &mut ctx,
        Inputs {
            args: &[LispValue::t()],
            ..Inputs::default()
        },
    )
    .unwrap();
    let Outcome::Deopt(expected_frame) = expected.outcome else {
        panic!("fixnum cannot satisfy cons guard")
    };

    // The folding pass has proved failure and explicitly drops later source
    // states. Cleanup must remove now-detached global instruction/value ids.
    func.blocks[4].insts.clear();
    func.blocks[4].term = Term::Deopt(snapshot);
    let source = func.source_states[3].as_mut().unwrap();
    source.post = source.pre.clone();
    func.source_states[4] = None;
    cleanup(&mut func).expect("compact precise-deopt tail");
    func.verify().expect("no detached definitions remain");
    assert!(
        !func
            .insts
            .iter()
            .any(|i| matches!(i.op, Opcode::Poll | Opcode::CheckType(_)))
    );
    assert!(func.source_states[4].is_none());
    let observed = evaluate(
        &func,
        &mut ctx,
        Inputs {
            args: &[LispValue::t()],
            ..Inputs::default()
        },
    )
    .unwrap();
    let Outcome::Deopt(observed_frame) = observed.outcome else {
        panic!("precise deopt remains")
    };
    assert_eq!(observed_frame, expected_frame);
    assert_eq!(observed.trace, expected.trace);
}
