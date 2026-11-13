//! The OSR entry guards fixnum loop state once and the loop's own guards on
//! it fold; a non-fixnum slot deoptimizes at the entry with the slots as
//! they came, and a loop that carries a non-fixnum is left alone.

use super::*;
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::{
    push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::compile::analyze_cfg;
use crate::emacs_core::jit::opt::build::{BuildInput, build};
use crate::emacs_core::jit::opt::eval::{Inputs, Outcome, evaluate};
use crate::emacs_core::jit::opt::passes::{fold, reps_lift};
use neovm_value::value::Value as LispValue;

/// `(let ((i 0)) (while (< i n) (setq i (1+ i))) i)` with `n` the argument:
/// the OSR header is pc 1 with the stack `[n i]`.
fn count_loop_ops() -> Vec<Op> {
    vec![
        Op::Constant(0),
        Op::StackRef(0),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(9),
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(1),
        Op::Return,
    ]
}

/// `(let ((x nil)) (while (< i n) ... (setq x nil)))`-shaped: the loop
/// carries a nil, so its state is not all fixnums.
fn nil_carrying_loop_ops() -> Vec<Op> {
    vec![
        Op::Constant(0),
        Op::Nil,
        Op::StackRef(1),
        Op::StackRef(3),
        Op::Lss,
        Op::GotoIfNil(12),
        Op::StackRef(1),
        Op::Add1,
        Op::StackSet(2),
        Op::Nil,
        Op::StackSet(1),
        Op::Goto(2),
        Op::Return,
    ]
}

/// The OSR plan at `header` after integer lifting, as the pipeline has it
/// when this pass runs.
fn osr_plan(ops: &[Op], header: usize, depth: usize) -> Func {
    let constants = [LispValue::make_int(0)];
    let params = ParamShape {
        required: 1,
        ..ParamShape::default()
    };
    let cfg = analyze_cfg(ops, &constants, None, params.native_arity()).expect("CFG");
    let bits: Vec<_> = constants
        .iter()
        .copied()
        .map(ValueBits::from_value)
        .collect();
    let mut func = build(BuildInput {
        ops,
        constants: &bits,
        cfg: &cfg,
        params,
        dynamic_prefix: 0,
        fused: None,
        osr: Some(OsrEntry {
            entry_pc: header as u32,
            depth,
            header: Block(0),
        }),
    })
    .expect("OSR SSA");
    reps_lift::run(&mut func, &[]).expect("lifted");
    func
}

fn checks_outside_entry(func: &Func) -> usize {
    func.blocks
        .iter()
        .enumerate()
        .filter(|&(index, _)| Block(index as u32) != func.entry)
        .flat_map(|(_, block)| &block.insts)
        .filter(|id| matches!(func.insts[id.index()].op, Opcode::CheckType(_)))
        .count()
}

fn run_osr(func: &Func, stack: &[LispValue]) -> Outcome {
    let saved = save_scratch_gc_roots();
    push_scratch_gc_roots(stack);
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let outcome = evaluate(
        func,
        &mut ctx,
        Inputs {
            osr_stack: stack,
            ..Inputs::default()
        },
    )
    .expect("reference execution")
    .outcome;
    restore_scratch_gc_roots(saved);
    outcome
}

#[test]
fn opt_osr_entry_guards_the_counter_once_and_the_loop_guards_fold() {
    let mut plain = osr_plan(&count_loop_ops(), 1, 2);
    let mut func = plain.clone();
    let stats = run(&mut func).expect("speculated");
    assert_eq!(stats.speculated, 1, "{}", func.display());
    assert!(stats.narrowed >= 1);
    let entry = &func.blocks[func.entry.index()];
    let guard = entry
        .insts
        .iter()
        .map(|id| &func.insts[id.index()])
        .find(|inst| matches!(inst.op, Opcode::CheckType(_)))
        .expect("an entry guard");
    let frame = &func.frames[guard.frame.expect("deopt frame").index()];
    assert_eq!(frame.pc, 1);
    assert_eq!(&*frame.stack, &*func.entry_stacks[func.entry.index()]);
    fold::run(&mut plain).expect("folded");
    fold::run(&mut func).expect("folded");
    assert!(checks_outside_entry(&func) < checks_outside_entry(&plain));
    for stack in [
        [LispValue::make_int(10), LispValue::make_int(3)],
        [LispValue::make_int(0), LispValue::make_int(5)],
    ] {
        let (Outcome::Returned(guarded), Outcome::Returned(unguarded)) =
            (run_osr(&func, &stack), run_osr(&plain, &stack))
        else {
            panic!("a fixnum loop state runs to its result");
        };
        assert_eq!(guarded, unguarded);
    }
}

/// The first integer past the fixnum range, a bignum.
fn first_bignum() -> LispValue {
    let value = LispValue::make_integer(
        malachite::Integer::from(LispValue::MOST_POSITIVE_FIXNUM) + malachite::Integer::from(1),
    );
    assert!(value.is_bignum());
    value
}

#[test]
fn opt_osr_entry_non_fixnum_slot_deoptimizes_at_the_entry() {
    let mut func = osr_plan(&count_loop_ops(), 1, 2);
    run(&mut func).expect("speculated");
    fold::run(&mut func).expect("folded");
    let float = LispValue::make_float(1.5);
    let bignum = first_bignum();
    for counter in [LispValue::NIL, float, bignum] {
        let stack = [LispValue::make_int(10), counter];
        let Outcome::Deopt(snapshot) = run_osr(&func, &stack) else {
            panic!("a non-fixnum counter cannot enter the fixnum loop");
        };
        assert_eq!(snapshot.pc, 1);
        let expected: Vec<_> = stack.iter().copied().map(ValueBits::from_value).collect();
        assert_eq!(snapshot.stack, expected, "the slots as they came");
    }
}

/// Values the entry does not guard, and a fixnum counter that leaves the
/// fixnum range inside the loop, behave exactly as without the pass.
#[test]
fn opt_osr_entry_matches_the_unguarded_loop_at_the_fixnum_boundary() {
    let mut plain = osr_plan(&count_loop_ops(), 1, 2);
    let mut func = plain.clone();
    run(&mut func).expect("speculated");
    fold::run(&mut plain).expect("folded");
    fold::run(&mut func).expect("folded");
    let last = LispValue::make_int(LispValue::MOST_POSITIVE_FIXNUM);
    let near = LispValue::make_int(LispValue::MOST_POSITIVE_FIXNUM - 2);
    for stack in [
        [LispValue::make_float(4.5), LispValue::make_int(1)],
        [first_bignum(), near],
        [first_bignum(), last],
    ] {
        match (run_osr(&func, &stack), run_osr(&plain, &stack)) {
            (Outcome::Returned(guarded), Outcome::Returned(unguarded)) => {
                assert_eq!(guarded, unguarded);
            }
            (Outcome::Deopt(guarded), Outcome::Deopt(unguarded)) => {
                assert_eq!(guarded.pc, unguarded.pc);
                assert_eq!(guarded.stack, unguarded.stack);
            }
            _ => panic!("the entry guard changed how {stack:?} leaves the loop"),
        }
    }
}

#[test]
fn opt_osr_entry_leaves_a_loop_that_carries_a_non_fixnum_alone() {
    let mut func = osr_plan(&nil_carrying_loop_ops(), 2, 3);
    let before = func.clone();
    let stats = run(&mut func).expect("verified");
    assert_eq!(stats.speculated, 1, "only the counter: {}", func.display());
    let entry = &func.blocks[func.entry.index()];
    let guards: Vec<_> = entry
        .insts
        .iter()
        .map(|id| &func.insts[id.index()])
        .filter(|inst| matches!(inst.op, Opcode::CheckType(_)))
        .collect();
    assert_eq!(guards.len(), 1);
    let Term::Jump(edge) = &func.blocks[func.entry.index()].term else {
        panic!("entry jump");
    };
    let Term::Jump(old) = &before.blocks[before.entry.index()].term else {
        panic!("entry jump");
    };
    // The carried nil's slot (stack index 1) is passed through unguarded.
    let header = &func.blocks[edge.target.index()];
    let nil_param = header
        .params
        .iter()
        .position(|&param| !func.values[param.index()].ty.is_subset(TypeSet::FIXNUM))
        .expect("the nil-carrying phi");
    assert_eq!(edge.args[nil_param], old.args[nil_param]);
}

#[test]
fn opt_osr_entry_ignores_a_normal_entry() {
    let ops = count_loop_ops();
    let constants = [LispValue::make_int(0)];
    let params = ParamShape {
        required: 1,
        ..ParamShape::default()
    };
    let cfg = analyze_cfg(&ops, &constants, None, params.native_arity()).expect("CFG");
    let bits: Vec<_> = constants
        .iter()
        .copied()
        .map(ValueBits::from_value)
        .collect();
    let mut func = build(BuildInput {
        ops: &ops,
        constants: &bits,
        cfg: &cfg,
        params,
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })
    .expect("SSA");
    assert_eq!(run(&mut func).expect("verified"), OsrEntryStats::default());
}
