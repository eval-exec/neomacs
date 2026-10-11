//! Structural SSA tests; behavioural expectations are covered by Tier-0/GNU
//! differential tests in the independent IR evaluator and native lowering.

use super::*;
use crate::emacs_core::bytecode::chunk::{ByteCodeFunction, GnuByteOffsetMapEntry};
use crate::emacs_core::jit::compile::analyze_cfg;
use crate::emacs_core::value::{HashTableTest, LambdaParams, Value as LispValue};

fn make(ops: &[Op], constants: &[LispValue], params: ParamShape) -> Func {
    make_with(ops, constants, params, 0, None, None, None)
}

#[allow(clippy::too_many_arguments)]
fn make_with(
    ops: &[Op],
    constants: &[LispValue],
    params: ParamShape,
    prefix: usize,
    map: Option<&[GnuByteOffsetMapEntry]>,
    fused: Option<&FusedBody>,
    osr: Option<OsrEntry>,
) -> Func {
    let cfg = analyze_cfg(ops, constants, map, params.native_arity()).expect("baseline CFG");
    let bits = constants
        .iter()
        .copied()
        .map(ValueBits::from_value)
        .collect::<Vec<_>>();
    let func = build(BuildInput {
        ops,
        constants: &bits,
        cfg: &cfg,
        params,
        dynamic_prefix: prefix,
        fused,
        osr,
    })
    .expect("global SSA");
    super::super::verify::verify(&func)
        .unwrap_or_else(|error| panic!("{error:?}\n{}", func.display()));
    func
}

fn one_arg() -> ParamShape {
    ParamShape {
        required: 1,
        optional: 0,
        has_rest: false,
    }
}

fn loop_ops() -> Vec<Op> {
    vec![
        Op::Constant(0),
        Op::StackRef(0),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(10),
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(1),
        Op::Return,
        Op::StackRef(0),
        Op::Return,
    ]
}

#[test]
fn opt_ssa_stack_shuffles_preserve_single_definition() {
    let func = make(
        &[
            Op::Dup,
            Op::StackRef(1),
            Op::StackSet(1),
            Op::DiscardN(0x81),
            Op::Return,
        ],
        &[],
        one_arg(),
    );
    assert_eq!(func.census.phis, 0);
    assert_eq!(func.insts.len(), 1, "only the entry Arg load");
    let initial = func.entry_stacks[func.entry.index()][0];
    let source = func.source_states[4].as_ref().unwrap().block;
    assert!(matches!(func.blocks[source.index()].term, Term::Return(value) if value == initial));
    for source in func.source_states.iter().flatten() {
        assert!(
            source
                .pre
                .iter()
                .chain(source.post.iter())
                .all(|&value| value == initial)
        );
    }
}

#[test]
fn opt_ssa_completed_forward_edges_do_not_allocate_provisional_values() {
    let params = ParamShape {
        required: 64,
        optional: 0,
        has_rest: false,
    };
    let ops = [Op::Goto(1), Op::Goto(2), Op::Goto(3), Op::Return];
    let func = make(&ops, &[], params);
    assert_eq!(func.values.len(), 64, "only the original Arg definitions");
    assert_eq!(func.census.phis, 0);
    assert!(
        func.values
            .iter()
            .all(|value| !matches!(value.def, ValueDef::Alias(_)))
    );
    let original = &func.entry_stacks[func.entry.index()];
    for state in func.source_states.iter().flatten() {
        assert_eq!(&state.pre, original, "the complete GNU stack stays live");
        assert_eq!(&state.post, original);
        assert_eq!(&func.frames[state.frame.index()].stack, original);
    }
}

#[test]
fn opt_ssa_completed_refinement_edge_supplies_its_dominating_value() {
    let func = make(
        &[
            Op::Dup,
            Op::Consp,
            Op::GotoIfNil(5),
            Op::Car,
            Op::Return,
            Op::Return,
        ],
        &[],
        one_arg(),
    );
    let car = func.source_states[3].as_ref().unwrap();
    let entry_value = car.pre[0];
    let ValueDef::Inst(inst) = func.values[entry_value.index()].def else {
        panic!("the completed edge must directly supply its refinement");
    };
    assert!(matches!(
        func.insts[inst.index()].op,
        Opcode::Refine(TypeSet::CONS)
    ));
    assert!(func.blocks[car.block.index()].params.is_empty());
    assert!(
        !func
            .values
            .iter()
            .any(|value| matches!(value.def, ValueDef::Alias(_)))
    );
}

#[test]
fn opt_ssa_unemitted_backward_predecessor_keeps_changed_argument_phi() {
    let func = make(
        &[
            Op::Goto(4),
            Op::Add1,
            Op::Goto(4),
            Op::Return,
            Op::Dup,
            Op::GotoIfNotNil(1),
            Op::Return,
        ],
        &[],
        one_arg(),
    );
    let header = func.source_states[1].as_ref().unwrap().block;
    let arg = func.entry_stacks[func.entry.index()][0];
    let carried = func.entry_stacks[header.index()][0];
    assert_ne!(
        arg, carried,
        "the forward jump has not emitted this predecessor yet"
    );
    let loop_join = func.source_states[4].as_ref().unwrap().block;
    assert_eq!(func.blocks[loop_join.index()].params.len(), 1);
    assert_ne!(func.entry_stacks[loop_join.index()][0], arg);
}

#[test]
fn opt_ssa_loop_prunes_invariant_argument_phi() {
    let func = make(&loop_ops(), &[LispValue::fixnum(0)], one_arg());
    let header = func.source_states[1].as_ref().unwrap().block;
    let arg = func.entry_stacks[func.entry.index()][0];
    assert_eq!(func.entry_stacks[header.index()][0], arg);
    assert_eq!(
        func.blocks[header.index()].params.len(),
        1,
        "only changed accumulator is carried"
    );
    assert!(func.blocks[header.index()].loop_header.is_some());
    assert_eq!(
        func.insts
            .iter()
            .filter(|i| matches!(i.op, Opcode::Arg(_)))
            .count(),
        1
    );
    assert_eq!(
        func.insts
            .iter()
            .filter(|i| matches!(i.op, Opcode::Poll))
            .count(),
        1
    );
    assert_eq!(func.census.dead_leaders, 1);
    assert!(func.source_states[9].is_none());
}

fn reloaded_nil_loop_ops() -> [Op; 9] {
    [
        Op::Nil,
        Op::StackRef(1),
        Op::GotoIfNil(8),
        Op::Nil,
        Op::StackSet(1),
        Op::Goto(1),
        Op::Return,
        Op::Return,
        Op::Return,
    ]
}

#[test]
fn opt_ssa_loop_reloaded_nil_reuses_dominating_literal() {
    let func = make(&reloaded_nil_loop_ops(), &[], one_arg());
    let nil = func.source_states[0].as_ref().unwrap().post[1];
    let header = func.source_states[1].as_ref().unwrap().block;
    assert_eq!(func.entry_stacks[header.index()][1], nil);
    assert_eq!(func.census.phis, 0, "both GNU stack slots are invariant");
    let reload = func.source_states[3].as_ref().unwrap();
    assert_eq!(reload.post.as_ref(), &[reload.pre[0], nil, nil]);
    assert_eq!(func.frames[reload.frame.index()].stack, reload.pre);
    assert_eq!(
        func.insts
            .iter()
            .filter(|inst| matches!(inst.op, Opcode::Const(_)))
            .count(),
        1,
        "the back edge reuses the dominating nil definition"
    );
    assert_eq!(
        func.insts
            .iter()
            .filter(|inst| matches!(inst.op, Opcode::Poll))
            .count(),
        1,
        "literal identity leaves GNU quit polling intact"
    );
    assert!(matches!(
        func.blocks[header.index()].term,
        Term::Branch { .. }
    ));
}

#[test]
fn opt_ssa_literal_identity_reuse_requires_dominance() {
    let ops = [
        Op::Constant(0),
        Op::Pop,
        Op::Dup,
        Op::GotoIfNil(10),
        Op::Constant(0),
        Op::Pop,
        Op::Nil,
        Op::Goto(13),
        Op::Return,
        Op::Return,
        Op::Constant(0),
        Op::Pop,
        Op::Nil,
        Op::Return,
    ];
    let func = make(&ops, &[LispValue::fixnum(7)], one_arg());
    let initial = *func.source_states[0].as_ref().unwrap().post.last().unwrap();
    for pc in [4, 10] {
        assert_eq!(
            *func.source_states[pc]
                .as_ref()
                .unwrap()
                .post
                .last()
                .unwrap(),
            initial,
            "the original literal dominates both branch arms"
        );
    }
    let left_nil = *func.source_states[6].as_ref().unwrap().post.last().unwrap();
    let right_nil = *func.source_states[12]
        .as_ref()
        .unwrap()
        .post
        .last()
        .unwrap();
    assert_ne!(
        left_nil, right_nil,
        "neither sibling's nil definition dominates the other"
    );
    let join = func.source_states[13].as_ref().unwrap().block;
    assert_eq!(
        func.blocks[join.index()].params.len(),
        1,
        "passes-off construction does not fold the equal-literal join"
    );
    assert_eq!(
        func.insts
            .iter()
            .filter(|inst| matches!(inst.op, Opcode::Const(0)))
            .count(),
        1
    );
}

#[test]
fn opt_ssa_osr_reloaded_nil_preserves_unknown_snapshot_seed() {
    let func = make_with(
        &reloaded_nil_loop_ops(),
        &[],
        one_arg(),
        0,
        None,
        None,
        Some(OsrEntry {
            entry_pc: 1,
            depth: 2,
            header: Block(0),
        }),
    );
    assert!(func.source_states[0].is_none());
    let seed = func.entry_stacks[func.entry.index()][1];
    assert_eq!(func.values[seed.index()].ty, TypeSet::TOP);
    assert_eq!(func.values[seed.index()].rep, Rep::Tagged);
    let header = func.osr.as_ref().unwrap().header;
    let carried = func.entry_stacks[header.index()][1];
    let literal = *func.source_states[3].as_ref().unwrap().post.last().unwrap();
    assert_ne!(carried, seed);
    assert_ne!(carried, literal);
    assert_eq!(func.values[carried.index()].ty, TypeSet::TOP);
    assert_eq!(func.values[carried.index()].rep, Rep::Tagged);
    assert_eq!(func.blocks[header.index()].params.as_slice(), &[carried]);
    let branch = func.source_states[2].as_ref().unwrap();
    assert_eq!(branch.pre[1], carried);
    assert_eq!(func.frames[branch.frame.index()].stack[1], carried);
}

#[test]
fn opt_ssa_literal_identity_reuse_excludes_environment_and_heap_constants() {
    let func = make_with(
        &[
            Op::Constant(0),
            Op::Pop,
            Op::Constant(0),
            Op::Pop,
            Op::Constant(1),
            Op::Pop,
            Op::Constant(1),
            Op::Return,
        ],
        &[
            LispValue::NIL,
            LispValue::cons(LispValue::NIL, LispValue::NIL),
        ],
        ParamShape::default(),
        1,
        None,
        None,
        None,
    );
    for (first, second) in [(0, 2), (4, 6)] {
        assert_ne!(
            func.source_states[first].as_ref().unwrap().post[0],
            func.source_states[second].as_ref().unwrap().post[0]
        );
    }
    assert_eq!(
        func.insts
            .iter()
            .filter(|inst| matches!(inst.op, Opcode::EnvConst(0)))
            .count(),
        2
    );
    assert_eq!(
        func.insts
            .iter()
            .filter(|inst| matches!(inst.op, Opcode::Const(1)))
            .count(),
        2
    );
}

#[test]
fn opt_ssa_refined_loop_argument_keeps_its_original_global_identity() {
    let func = make(
        &[
            Op::Dup,
            Op::Consp,
            Op::GotoIfNil(7),
            Op::Dup,
            Op::CarSafe,
            Op::Pop,
            Op::Goto(0),
            Op::Return,
        ],
        &[],
        one_arg(),
    );
    let header = func.source_states[0].as_ref().unwrap().block;
    let initial = func.entry_stacks[func.entry.index()][0];
    assert_eq!(func.entry_stacks[header.index()].as_ref(), &[initial]);
    assert!(
        func.blocks[header.index()].params.is_empty(),
        "a branch refinement changes knowledge, not the invariant Lisp value"
    );
    assert!(
        func.insts
            .iter()
            .any(|inst| matches!(inst.op, Opcode::Refine(ty) if ty == TypeSet::CONS))
    );
}

#[test]
fn opt_ssa_diamond_has_global_identity_and_one_changed_phi() {
    let ops = [
        Op::Dup,
        Op::GotoIfNil(5),
        Op::Constant(0),
        Op::Goto(6),
        Op::Return,
        Op::Constant(1),
        Op::Return,
    ];
    let func = make(
        &ops,
        &[LispValue::fixnum(1), LispValue::fixnum(2)],
        one_arg(),
    );
    let join = func.source_states[6].as_ref().unwrap().block;
    assert_eq!(func.entry_stacks[join.index()].len(), 2);
    assert_eq!(func.blocks[join.index()].params.len(), 1);
    assert_eq!(
        func.entry_stacks[join.index()][0],
        func.entry_stacks[func.entry.index()][0]
    );
    assert!(func.blocks[join.index()].params.iter().all(
        |&v| matches!(func.values[v.index()].def, ValueDef::Param {block, ..} if block == join)
    ));
    assert!(func.census.refinements >= 2);
}

#[test]
fn opt_ssa_else_pop_edges_keep_their_distinct_gnu_depths() {
    let ops = [
        Op::Dup,
        Op::GotoIfNilElsePop(4),
        Op::StackRef(0),
        Op::Goto(4),
        Op::Return,
    ];
    let func = make(&ops, &[], one_arg());
    let state = func.source_states[1].as_ref().unwrap();
    assert_eq!(state.pre.len(), 2);
    assert_eq!(state.post.len(), 1, "fallthrough pops the predicate");
    let Term::Branch {
        if_true, if_false, ..
    } = &func.blocks[state.block.index()].term
    else {
        panic!("branch terminator");
    };
    assert_eq!(func.entry_stacks[if_true.target.index()].len(), 1);
    assert_eq!(func.entry_stacks[if_false.target.index()].len(), 2);
    assert!(func.census.refinements >= 2);
}

#[test]
fn opt_ssa_type_predicate_refines_retained_argument_on_edges() {
    let func = make(
        &[
            Op::Dup,
            Op::Consp,
            Op::GotoIfNil(5),
            Op::Car,
            Op::Return,
            Op::Return,
        ],
        &[],
        one_arg(),
    );
    let cons_refinements = func
        .insts
        .iter()
        .filter(|inst| matches!(inst.op, Opcode::Refine(ty) if ty == TypeSet::CONS))
        .count();
    assert_eq!(cons_refinements, 1);
    let car_guard = func
        .insts
        .iter()
        .find(|inst| matches!(inst.op, Opcode::CheckType(ty) if ty == TypeSet::LIST))
        .unwrap();
    let frame = &func.frames[car_guard.frame.unwrap().index()];
    assert_eq!(frame.pc, 3);
    assert_eq!(frame.stack.len(), 1);
}

#[test]
fn opt_ssa_eq_nil_does_not_narrow_a_possible_symbol_with_position() {
    let func = make(
        &[
            Op::Dup,
            Op::Nil,
            Op::Eq,
            Op::GotoIfNil(5),
            Op::Return,
            Op::Return,
        ],
        &[],
        one_arg(),
    );
    assert_eq!(
        func.census.refinements, 0,
        "a wrapped nil may compare equal under symbols-with-pos-enabled"
    );
}

#[test]
fn opt_ssa_optional_rest_and_prefix_are_native_slots_and_env_loads() {
    let params = ParamShape {
        required: 1,
        optional: 2,
        has_rest: true,
    };
    let func = make_with(
        &[Op::Constant(0), Op::Return],
        &[LispValue::NIL],
        params,
        1,
        None,
        None,
        None,
    );
    assert_eq!(func.entry_stacks[func.entry.index()].len(), 4);
    assert_eq!(
        func.insts
            .iter()
            .filter(|inst| matches!(inst.op, Opcode::Arg(_)))
            .count(),
        4
    );
    let rest = func.entry_stacks[func.entry.index()][3];
    assert_eq!(func.values[rest.index()].ty, TypeSet::LIST);
    assert!(
        func.insts
            .iter()
            .any(|inst| matches!(inst.op, Opcode::EnvConst(0)))
    );
    assert!(
        !func
            .insts
            .iter()
            .any(|inst| matches!(inst.op, Opcode::Const(0)))
    );
}

fn jump_table() -> LispValue {
    let table = LispValue::hash_table(HashTableTest::Eq);
    let _ = table.with_hash_table_mut(|ht| {
        for (sym, offset) in [("opt-ssa-switch-a", 8), ("opt-ssa-switch-b", 12)] {
            let value = LispValue::symbol(sym);
            let key = value.to_hash_key(&ht.test);
            ht.insert(key, value, LispValue::fixnum(offset));
        }
    });
    table
}

#[test]
fn opt_ssa_switch_uses_snapshot_gnu_offset_targets() {
    let ops = [
        Op::Dup,
        Op::Constant(0),
        Op::Switch,
        Op::Constant(1),
        Op::Return,
        Op::Constant(2),
        Op::Return,
        Op::Constant(3),
        Op::Return,
    ];
    let constants = [
        jump_table(),
        LispValue::fixnum(10),
        LispValue::fixnum(20),
        LispValue::fixnum(30),
    ];
    let map = [
        GnuByteOffsetMapEntry::new(8, 5),
        GnuByteOffsetMapEntry::new(12, 7),
    ];
    let func = make_with(&ops, &constants, one_arg(), 0, Some(&map), None, None);
    let block = func.source_states[2].as_ref().unwrap().block;
    let Term::Switch { cases, default, .. } = &func.blocks[block.index()].term else {
        panic!("Switch must have explicit CFG edges");
    };
    let mut raw_targets = cases
        .iter()
        .map(|case| (case.key, func.blocks[case.edge.target.index()].pc))
        .collect::<Vec<_>>();
    raw_targets.sort();
    assert_eq!(raw_targets, [(8, 5), (12, 7)]);
    assert_eq!(func.blocks[default.target.index()].pc, 3);
    assert_eq!(func.source_states[2].as_ref().unwrap().post.len(), 1);
}

#[test]
fn opt_ssa_osr_has_snapshot_entry_and_prunes_prefix() {
    let ops = loop_ops();
    let func = make_with(
        &ops,
        &[LispValue::fixnum(0)],
        one_arg(),
        0,
        None,
        None,
        Some(OsrEntry {
            entry_pc: 1,
            depth: 2,
            header: Block(0),
        }),
    );
    assert!(func.source_states[0].is_none());
    assert_eq!(
        func.insts
            .iter()
            .filter(|inst| matches!(inst.op, Opcode::OsrSlot(_)))
            .count(),
        2
    );
    assert!(
        !func
            .insts
            .iter()
            .any(|inst| matches!(inst.op, Opcode::Arg(_)))
    );
    assert_ne!(func.entry, func.osr.as_ref().unwrap().header);
    assert_eq!(func.entry_stacks[func.entry.index()].len(), 2);
}

#[test]
fn opt_ssa_mutating_loop_ops_and_calls_are_admitted_with_frames() {
    let ops = [
        Op::VarRef(0),
        Op::Pop,
        Op::StackRef(0),
        Op::Constant(1),
        Op::Setcar,
        Op::Pop,
        Op::Constant(2),
        Op::StackRef(1),
        Op::Call(1),
        Op::Pop,
        Op::Dup,
        Op::Cdr,
        Op::StackSet(1),
        Op::Dup,
        Op::GotoIfNotNil(0),
        Op::Return,
    ];
    let constants = [
        LispValue::symbol("opt-ssa-dynamic"),
        LispValue::fixnum(1),
        LispValue::symbol("identity"),
    ];
    let func = make(&ops, &constants, one_arg());
    for wanted in [Op::VarRef(0), Op::Setcar, Op::Call(1)] {
        let inst = func
            .insts
            .iter()
            .find(|inst| matches!(&inst.op, Opcode::Opaque(op) if *op == wanted))
            .expect("loop operation admitted");
        assert!(inst.frame.is_some());
        let frame = &func.frames[inst.frame.unwrap().index()];
        assert_eq!(frame.pc, inst.pc);
    }
    let call = func
        .insts
        .iter()
        .find(|inst| matches!(inst.op, Opcode::Opaque(Op::Call(1))))
        .unwrap();
    assert!(call.eff.contains(Effects::UNKNOWN));
    assert!(
        func.insts
            .iter()
            .any(|inst| matches!(inst.op, Opcode::CheckType(ty) if ty == TypeSet::CONS))
    );
    assert_eq!(
        func.insts
            .iter()
            .filter(|inst| matches!(inst.op, Opcode::Poll))
            .count(),
        1
    );
}

#[test]
fn opt_ssa_float_sites_keep_polymorphic_number_results() {
    let func = make(
        &[Op::Constant(0), Op::Add, Op::Return],
        &[LispValue::make_float(1.5)],
        one_arg(),
    );
    let add = func
        .insts
        .iter()
        .find(|inst| matches!(inst.op, Opcode::Opaque(Op::Add)))
        .unwrap();
    assert_eq!(func.values[add.result.unwrap().index()].ty, TypeSet::NUMBER);
    assert!(add.eff.contains(Effects::MAY_DEOPT));
    assert!(
        !func
            .insts
            .iter()
            .any(|inst| matches!(inst.op, Opcode::CheckType(ty) if ty == TypeSet::FIXNUM))
    );
}

#[test]
fn opt_ssa_binding_depth_is_recorded_before_each_operation() {
    let ops = [
        Op::Constant(1),
        Op::VarBind(0),
        Op::VarRef(0),
        Op::Unbind(1),
        Op::Return,
    ];
    let func = make(
        &ops,
        &[LispValue::symbol("opt-ssa-bind"), LispValue::fixnum(2)],
        ParamShape::default(),
    );
    let bind_frame = &func.frames[func.source_states[1].as_ref().unwrap().frame.index()];
    let ref_frame = &func.frames[func.source_states[2].as_ref().unwrap().frame.index()];
    let unbind_frame = &func.frames[func.source_states[3].as_ref().unwrap().frame.index()];
    let return_frame = &func.frames[func.source_states[4].as_ref().unwrap().frame.index()];
    assert_eq!(
        (
            bind_frame.binds,
            ref_frame.binds,
            unbind_frame.binds,
            return_frame.binds
        ),
        (0, 1, 1, 0)
    );
}

#[test]
fn opt_ssa_handlers_bail_with_a_specific_contract_reason() {
    let ops = [
        Op::PushConditionCase(4),
        Op::Nil,
        Op::PopHandler,
        Op::Return,
        Op::Return,
    ];
    let cfg = analyze_cfg(&ops, &[], None, 0).unwrap();
    let result = build(BuildInput {
        ops: &ops,
        constants: &[],
        cfg: &cfg,
        params: ParamShape::default(),
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    });
    assert!(matches!(
        result,
        Err(CompileError::UnsupportedOp("opt-build:PushConditionCase"))
    ));
}

#[test]
fn opt_ssa_dead_handler_leaders_do_not_block_a_reachable_body() {
    let func = make(
        &[
            Op::Goto(4),
            Op::PushConditionCase(4),
            Op::PopHandler,
            Op::Return,
            Op::Nil,
            Op::Return,
        ],
        &[],
        ParamShape::default(),
    );
    assert!(func.source_states[1..4].iter().all(Option::is_none));
    assert_eq!(func.census.dead_leaders, 2);
}

#[test]
fn opt_ssa_fused_v2_regions_intern_exact_caller_replay_frames() {
    struct InlineGuard;
    impl Drop for InlineGuard {
        fn drop(&mut self) {
            crate::emacs_core::jit::inline::force_inline_for_test(None);
        }
    }
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    let _guard = InlineGuard;
    let mut callee = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::intern("opt-ssa-callee-arg")],
        optional: vec![],
        rest: None,
    });
    callee.lexical = true;
    callee.ops = vec![Op::Dup, Op::CarSafe, Op::Return];
    callee.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    callee.seal_hand_assembled_ops_for_test();
    callee.jit_runtime().set_hot_for_test();
    let constants = [LispValue::make_bytecode(callee)];
    let ops = [Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
    let fused = crate::emacs_core::jit::inline::fuse_calls_v2(
        &ops,
        &constants,
        None,
        1,
        &vec![crate::emacs_core::jit::NumericFeedback::FixnumOnly; ops.len()],
    )
    .expect("pure constant fusion");
    let func = make_with(
        &fused.ops,
        &fused.constants,
        one_arg(),
        0,
        fused.offset_map.as_deref(),
        Some(&fused),
        None,
    );
    assert_eq!(fused.regions.len(), 1);
    let region = &fused.regions[0];
    let first = func.source_states[region.start].as_ref().unwrap().frame;
    for pc in region.start..region.end {
        if let Some(state) = &func.source_states[pc] {
            assert_eq!(state.frame, first);
        }
    }
    let frame = &func.frames[first.index()];
    assert_eq!(frame.pc, 2);
    assert_eq!(frame.stack.len(), 3);
    assert_eq!(frame.site, Some(InlineSiteId(0)));
    assert_eq!(
        frame.parent, None,
        "current pure replay fuser has no materialized callee frame"
    );
    assert_eq!(
        func.insts
            .iter()
            .filter(|inst| matches!(inst.op, Opcode::InlineEntry(0)))
            .count(),
        1
    );
}
