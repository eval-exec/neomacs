//! Range/LICM reference parity and exact original-source replay.
//!
//! GNU31.1 expectations are the unchanged 67-line O3.3/O3.5 pack: phi-zero-trip,
//! phi-add/mul-promotes, cold-loop-{after-effect,zero-trip}, vector/record bounds
//! ordering and array-phi-{new-longer,shorter,record}. GNU bytecode.c:1244-1254
//! promotes 1+/- at the fixnum boundary; data.c:2568-2616 checks index, array,
//! then bounds. alloc.c:2480-2520 creates fresh Float identities. No replacement
//! Lisp arithmetic, array semantics or evaluator callbacks are introduced here.
//!
//! Float payload coverage is reference-only. Aref
//! below retains its Tier-0 internal checks: it cannot certify native BCE.

use super::build::{BuildInput, build};
use super::eval::{EvalError, Inputs, Outcome, Snapshot, evaluate};
use super::ir::*;
use super::mem::{AliasClass, Effects};
use super::passes::{bools, fold, licm, range, reps_lift};
use super::types::{Range, TypeSet};
use crate::emacs_core::bytecode::{ByteCodeFunction, Op, Vm};
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{
    Context, push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::{LambdaParams, Value as LispValue};

// Fixtures and all root/observation storage belong to this invocation's
// mutator. They install no compiler or runtime cache and restore on unwind.
struct Roots(usize);
impl Roots {
    fn new(values: &[LispValue]) -> Self {
        let saved = save_scratch_gc_roots();
        push_scratch_gc_roots(values);
        Self(saved)
    }
}
impl Drop for Roots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn bytecode(ops: Vec<Op>, constants: Vec<LispValue>, arity: usize) -> ByteCodeFunction {
    let mut source = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|_| crate::emacs_core::intern::intern("opt-range-reference-arg"))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    source.lexical = true;
    source.max_stack = 128;
    source.ops = ops;
    source.constants = constants.into();
    source
}

fn plan(source: &ByteCodeFunction, prefix: usize, osr: Option<(usize, usize)>) -> Func {
    let params = ParamShape {
        required: source.params.required.len(),
        ..ParamShape::default()
    };
    let cfg = crate::emacs_core::jit::compile::analyze_cfg(
        source.executable_ops(),
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        params.native_arity(),
    )
    .expect("source CFG");
    let constants = source
        .constants
        .iter()
        .copied()
        .map(ValueBits::from_value)
        .collect::<Vec<_>>();
    let mut func = build(BuildInput {
        ops: source.executable_ops(),
        constants: &constants,
        cfg: &cfg,
        params,
        dynamic_prefix: prefix,
        fused: None,
        osr: osr.map(|(pc, depth)| OsrEntry {
            entry_pc: pc as u32,
            depth,
            header: Block(0),
        }),
    })
    .expect("source SSA");
    func.verify().expect("original SSA verifies");
    reps_lift::run(&mut func, &[]).expect("real numeric lift");
    fold::run(&mut func).expect("real Fold");
    bools::run(&mut func).expect("real Boolean pipeline");
    func.verify().expect("typed baseline verifies");
    func
}

fn text(value: LispValue) -> String {
    crate::emacs_core::print::print_value(&value)
}

#[derive(Debug, PartialEq, Eq)]
enum Answer {
    Value(String),
    Signal(SymId, Vec<String>, Option<String>),
}
fn answer(result: Result<LispValue, Flow>) -> Answer {
    match result {
        Ok(value) => Answer::Value(text(value)),
        Err(flow) => {
            let signal = flow.as_signal().expect("fixture has a GNU signal");
            Answer::Signal(
                signal.symbol,
                signal.data.iter().copied().map(text).collect(),
                signal.raw_data.as_ref().map(|value| text(*value)),
            )
        }
    }
}

fn tier0(ctx: &mut Context, source: &ByteCodeFunction, args: &[LispValue]) -> Answer {
    let _args = Roots::new(args);
    let _constants = Roots::new(&source.constants);
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    answer(vm.execute(source, args.to_vec()))
}

fn resume(ctx: &mut Context, source: &ByteCodeFunction, pc: usize, stack: &[LispValue]) -> Answer {
    // The synthetic source has no raw bytecode; preserve every source op and
    // its original pc while giving Tier-0 resume the supported test marker.
    let mut resumed_source = source.clone();
    resumed_source.seal_hand_assembled_ops_for_test();
    assert_eq!(resumed_source.executable_ops(), source.executable_ops());
    let source = &resumed_source;
    let _stack = Roots::new(stack);
    let _constants = Roots::new(&source.constants);
    let spec_base = ctx.specpdl.len();
    let condition_base = ctx.condition_stack_depth_for_test();
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    answer(vm.run_resumed_frame(
        source,
        LispValue::NIL,
        pc,
        stack,
        0,
        &[],
        spec_base,
        condition_base,
    ))
}

// Resumption always uses the real Tier-0 opcode, with the original source
// stack/pc. A new anticipated guard can replay the header; it cannot replay a
// previously committed preheader effect. Exact snapshots remain available.
fn reference(
    ctx: &mut Context,
    source: &ByteCodeFunction,
    func: &Func,
    inputs: Inputs<'_>,
) -> (Answer, Option<Snapshot>) {
    let _args = Roots::new(inputs.args);
    let _osr = Roots::new(inputs.osr_stack);
    let _prefix = Roots::new(inputs.prefix);
    let _constants = Roots::new(&source.constants);
    let run = match evaluate(func, ctx, inputs) {
        Ok(run) => run,
        Err(EvalError::Flow(flow)) => return (answer(Err(flow)), None),
        Err(other) => panic!("reference fixture invalid: {other:?}"),
    };
    match run.outcome {
        Outcome::Returned(bits) => (answer(Ok(bits.to_value())), None),
        Outcome::Deopt(snapshot) => {
            assert_eq!(snapshot.handlers, 0);
            assert_eq!(snapshot.binds, 0);
            let stack = snapshot
                .stack
                .iter()
                .map(|bits| bits.to_value())
                .collect::<Vec<_>>();
            (
                resume(ctx, source, snapshot.pc as usize, &stack),
                Some(snapshot),
            )
        }
    }
}

fn reset_side(ctx: &mut Context) {
    ctx.eval_str("(setq t34-o335-side nil)").unwrap();
}
fn side(ctx: &Context) -> String {
    text(
        ctx.obarray
            .symbol_value_copied("t34-o335-side")
            .unwrap_or(LispValue::NIL),
    )
}
fn arguments(values: &[i64]) -> Vec<LispValue> {
    values.iter().copied().map(LispValue::fixnum).collect()
}
fn operation(func: &Func, pc: u32, predicate: impl Fn(&Opcode) -> bool) -> Inst {
    Inst(
        func.insts
            .iter()
            .position(|inst| inst.pc == pc && predicate(&inst.op))
            .expect("source operation") as u32,
    )
}
fn checked_add(func: &Func, pc: u32) -> Inst {
    operation(func, pc, |op| {
        matches!(op, Opcode::FixAdd { checked: true })
    })
}
fn before(func: &mut Func, target: Inst, instructions: &[Inst]) {
    for block in &mut func.blocks {
        if let Some(position) = block.insts.iter().position(|&inst| inst == target) {
            block
                .insts
                .splice(position..position, instructions.iter().copied());
            return;
        }
    }
    panic!("attached target");
}
fn append(
    func: &mut Func,
    op: Opcode,
    args: &[Value],
    ty: TypeSet,
    rep: Rep,
    pc: u32,
    frame: Option<FrameId>,
    eff: Effects,
    mem: AliasClass,
) -> (Inst, Value) {
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
        eff,
        mem,
        frame,
        pc,
    });
    (inst, value)
}

fn retained_observations(original: &Func, candidate: &Func) {
    assert_eq!(
        &candidate.frames[..original.frames.len()],
        original.frames.as_slice(),
        "all original frame IDs/content remain"
    );
    assert_eq!(candidate.entry_stacks, original.entry_stacks);
    assert_eq!(candidate.source_states.len(), original.source_states.len());
    for (actual, expected) in candidate.source_states.iter().zip(&original.source_states) {
        match (actual, expected) {
            (Some(actual), Some(expected)) => {
                assert_eq!(actual.pre, expected.pre);
                assert_eq!(actual.post, expected.post);
                assert_eq!(actual.frame, expected.frame);
                assert_eq!(actual.block, expected.block);
            }
            (None, None) => {}
            _ => panic!("source mapping added or removed"),
        }
    }
    for (index, expected) in original.insts.iter().enumerate() {
        let actual = &candidate.insts[index];
        assert_eq!(actual.result, expected.result, "original result identity");
        if matches!(
            expected.op,
            Opcode::Arg(_) | Opcode::OsrSlot(_) | Opcode::EnvConst(_)
        ) {
            assert_eq!(actual.op, expected.op);
            let value = expected.result.unwrap();
            assert_eq!(
                candidate.values[value.index()].ty,
                original.values[value.index()].ty
            );
            assert_eq!(
                candidate.values[value.index()].rep,
                original.values[value.index()].rep
            );
            assert_eq!(
                candidate.values[value.index()].def,
                original.values[value.index()].def
            );
        }
        if matches!(
            expected.op,
            Opcode::Poll
                | Opcode::Call { .. }
                | Opcode::Opaque(Op::VarSet(_) | Op::Aset | Op::Setcar | Op::Setcdr)
        ) {
            assert_eq!(actual.op, expected.op);
            assert_eq!(actual.args, expected.args);
            assert_eq!(actual.eff, expected.eff);
            assert_eq!(actual.mem, expected.mem);
            assert_eq!(actual.frame, expected.frame);
            assert_eq!(actual.pc, expected.pc);
        }
    }
}

fn counter() -> ByteCodeFunction {
    bytecode(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(0),
            Op::StackRef(3),
            Op::Lss,
            Op::GotoIfNil(14),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Add,
            Op::StackSet(2),
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(2),
            Op::StackRef(1),
            Op::Return,
        ],
        arguments(&[0, 0]),
        1,
    )
}

#[test]
fn opt_range_reference_branch_proof_elides_increment_only_on_taken_edge() {
    let mut ctx = Context::new();
    let source = bytecode(
        vec![
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(6),
            Op::Add1,
            Op::Return,
            Op::Return,
        ],
        vec![],
        2,
    );
    let original = plan(&source, 0, None);
    let increment = checked_add(&original, 4);
    let cases = [
        [0, LispValue::MOST_NEGATIVE_FIXNUM],
        [1, 0],
        [0, 0],
        [
            LispValue::MOST_POSITIVE_FIXNUM,
            LispValue::MOST_POSITIVE_FIXNUM - 1,
        ],
        [
            LispValue::MOST_POSITIVE_FIXNUM,
            LispValue::MOST_POSITIVE_FIXNUM,
        ],
    ];
    // Baseline execution precedes both pass invocation and any elision claim.
    for case in cases {
        let args = arguments(&case);
        assert_eq!(
            reference(
                &mut ctx,
                &source,
                &original,
                Inputs {
                    args: &args,
                    ..Inputs::default()
                }
            )
            .0,
            tier0(&mut ctx, &source, &args)
        );
    }
    let mut candidate = original.clone();
    let stats = range::run(&mut candidate).unwrap();
    candidate.verify().unwrap();
    assert_eq!(stats.overflow_checks_elided, 1);
    assert!(
        stats.range_views > 0,
        "point proof exposed to the native verifier"
    );
    assert_eq!(
        candidate.insts[increment.index()].op,
        Opcode::FixAdd { checked: false }
    );
    retained_observations(&original, &candidate);
    for case in cases {
        let args = arguments(&case);
        assert_eq!(
            reference(
                &mut ctx,
                &source,
                &candidate,
                Inputs {
                    args: &args,
                    ..Inputs::default()
                }
            )
            .0,
            tier0(&mut ctx, &source, &args)
        );
    }
}

#[test]
fn opt_range_reference_real_counter_keeps_accumulator_overflow_guard() {
    let mut ctx = Context::new();
    let source = counter();
    let original = plan(&source, 0, None);
    let sum = checked_add(&original, 8);
    let increment = checked_add(&original, 11);
    let trips = [0, 1, 17, 254, 255, 510];
    for n in trips {
        let args = arguments(&[n]);
        let expected = tier0(&mut ctx, &source, &args);
        crate::emacs_core::eval::reset_bytecode_branch_poll_count();
        let actual = reference(
            &mut ctx,
            &source,
            &original,
            Inputs {
                args: &args,
                ..Inputs::default()
            },
        );
        assert_eq!(actual.0, expected);
        assert!(actual.1.is_none());
        assert_eq!(
            crate::emacs_core::eval::bytecode_branch_poll_count(),
            n as usize / 255
        );
    }
    let mut candidate = original.clone();
    let stats = range::run(&mut candidate).unwrap();
    candidate.verify().unwrap();
    assert_eq!(
        stats.overflow_checks_elided, 1,
        "only the IV is bounded by i<n"
    );
    assert_eq!(
        candidate.insts[increment.index()].op,
        Opcode::FixAdd { checked: false }
    );
    assert_eq!(
        candidate.insts[sum.index()].op,
        Opcode::FixAdd { checked: true }
    );
    retained_observations(&original, &candidate);
    for n in trips {
        let args = arguments(&[n]);
        let expected = tier0(&mut ctx, &source, &args);
        crate::emacs_core::eval::reset_bytecode_branch_poll_count();
        assert_eq!(
            reference(
                &mut ctx,
                &source,
                &candidate,
                Inputs {
                    args: &args,
                    ..Inputs::default()
                }
            )
            .0,
            expected
        );
        assert_eq!(
            crate::emacs_core::eval::bytecode_branch_poll_count(),
            n as usize / 255
        );
    }
}

fn raw_phi() -> ByteCodeFunction {
    // Transcribed operation indices from frozen t34-o335-raw-phi byte offsets.
    bytecode(
        vec![
            Op::Constant(0),
            Op::StackRef(3),
            Op::StackRef(1),
            Op::StackRef(6),
            Op::Lss,
            Op::GotoIfNil(21),
            Op::StackRef(2),
            Op::GotoIfNil(13),
            Op::Dup,
            Op::StackRef(4),
            Op::Add,
            Op::StackSet(1),
            Op::Goto(17),
            Op::Dup,
            Op::StackRef(4),
            Op::Mul,
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Add1,
            Op::StackSet(2),
            Op::Goto(2),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::List(2),
            Op::Return,
        ],
        arguments(&[0]),
        4,
    )
}

#[test]
fn opt_range_reference_phi_promotion_preserves_original_cold_frame() {
    let mut ctx = Context::new();
    let source = raw_phi();
    let original = plan(&source, 0, None);
    let hi = LispValue::MOST_POSITIVE_FIXNUM;
    let cases = [
        [
            LispValue::fixnum(0),
            LispValue::fixnum(hi),
            LispValue::fixnum(1),
            LispValue::T,
        ],
        [
            LispValue::fixnum(2),
            LispValue::fixnum(hi),
            LispValue::fixnum(1),
            LispValue::T,
        ],
        [
            LispValue::fixnum(2),
            LispValue::fixnum(hi),
            LispValue::fixnum(2),
            LispValue::NIL,
        ],
        [
            LispValue::fixnum(2),
            LispValue::fixnum(LispValue::MOST_NEGATIVE_FIXNUM),
            LispValue::fixnum(-1),
            LispValue::T,
        ],
    ];
    let baselines = cases
        .iter()
        .map(|args| {
            let expected = tier0(&mut ctx, &source, args);
            let baseline = reference(
                &mut ctx,
                &source,
                &original,
                Inputs {
                    args,
                    ..Inputs::default()
                },
            );
            assert_eq!(baseline.0, expected);
            baseline
        })
        .collect::<Vec<_>>();
    assert!(
        baselines[0].1.is_none(),
        "zero-trip never inspects the seed"
    );
    assert_eq!(baselines[1].1.as_ref().unwrap().pc, 10);
    assert_eq!(baselines[2].1.as_ref().unwrap().pc, 15);
    let mut candidate = original.clone();
    let stats = range::run(&mut candidate).unwrap();
    candidate.verify().unwrap();
    assert_eq!(
        stats.overflow_checks_elided, 1,
        "IV only; promotion arms remain checked"
    );
    assert_eq!(
        candidate.insts[checked_add(&original, 10).index()].op,
        Opcode::FixAdd { checked: true }
    );
    assert_eq!(
        candidate.insts[operation(&original, 15, |op| matches!(
            op,
            Opcode::FixMul { checked: true }
        ))
        .index()]
        .op,
        Opcode::FixMul { checked: true }
    );
    retained_observations(&original, &candidate);
    for (args, expected) in cases.iter().zip(baselines) {
        let actual = reference(
            &mut ctx,
            &source,
            &candidate,
            Inputs {
                args,
                ..Inputs::default()
            },
        );
        assert_eq!(actual.0, expected.0);
        assert_eq!(
            actual.1, expected.1,
            "exact original PC/full stack on promotion"
        );
    }
}

#[test]
fn opt_range_reference_unknown_osr_and_prefix_keep_unproved_checks() {
    let mut ctx = Context::new();
    let source = counter();
    let osr = plan(&source, 0, Some((2, 3)));
    let osr_stack = arguments(&[3, LispValue::MOST_POSITIVE_FIXNUM, 0]);
    let expected = resume(&mut ctx, &source, 2, &osr_stack);
    let baseline = reference(
        &mut ctx,
        &source,
        &osr,
        Inputs {
            osr_stack: &osr_stack,
            ..Inputs::default()
        },
    );
    assert_eq!(baseline.0, expected);
    assert_eq!(baseline.1.as_ref().unwrap().pc, 8);
    let mut candidate = osr.clone();
    range::run(&mut candidate).unwrap();
    candidate.verify().unwrap();
    assert_eq!(
        candidate.insts[checked_add(&osr, 8).index()].op,
        Opcode::FixAdd { checked: true }
    );
    retained_observations(&osr, &candidate);
    assert_eq!(
        reference(
            &mut ctx,
            &source,
            &candidate,
            Inputs {
                osr_stack: &osr_stack,
                ..Inputs::default()
            }
        ),
        baseline
    );

    // The template's zero is NOT the invocation's prefix seed. Resume with the
    // exact patched source pool, never silently replay a stale template value.
    let prefix = plan(&source, 1, None);
    let patched = bytecode(
        source.ops.clone(),
        arguments(&[LispValue::MOST_POSITIVE_FIXNUM, 0]),
        1,
    );
    let args = arguments(&[3]);
    let environment = arguments(&[LispValue::MOST_POSITIVE_FIXNUM]);
    let expected = tier0(&mut ctx, &patched, &args);
    let baseline = reference(
        &mut ctx,
        &patched,
        &prefix,
        Inputs {
            args: &args,
            prefix: &environment,
            ..Inputs::default()
        },
    );
    assert_eq!(baseline.0, expected);
    assert_eq!(baseline.1.as_ref().unwrap().pc, 8);
    let mut candidate = prefix.clone();
    range::run(&mut candidate).unwrap();
    candidate.verify().unwrap();
    assert_eq!(
        candidate.insts[checked_add(&prefix, 8).index()].op,
        Opcode::FixAdd { checked: true }
    );
    retained_observations(&prefix, &candidate);
    assert_eq!(
        reference(
            &mut ctx,
            &patched,
            &candidate,
            Inputs {
                args: &args,
                prefix: &environment,
                ..Inputs::default()
            }
        ),
        baseline
    );
}

fn guarded_arrays(mut func: Func) -> Func {
    let accesses = func
        .insts
        .iter()
        .enumerate()
        .filter_map(|(index, inst)| {
            matches!(inst.op, Opcode::Opaque(Op::Aref)).then_some(Inst(index as u32))
        })
        .collect::<Vec<_>>();
    for access in accesses {
        let old = func.insts[access.index()].clone();
        let numeric = TypeSet::FIXNUM;
        let array = TypeSet::VECTOR.join(TypeSet::RECORD);
        let (index_guard, index) = append(
            &mut func,
            Opcode::CheckType(numeric),
            &[old.args[1]],
            numeric,
            Rep::TaggedFix,
            old.pc,
            old.frame,
            Effects::MAY_DEOPT,
            AliasClass::None,
        );
        let (array_guard, base) = append(
            &mut func,
            Opcode::CheckType(array),
            &[old.args[0]],
            array,
            Rep::Tagged,
            old.pc,
            old.frame,
            Effects::MAY_DEOPT,
            AliasClass::None,
        );
        let (length_load, length) = append(
            &mut func,
            Opcode::LoadVecLen,
            &[base],
            TypeSet::fixnum_range(Range {
                lo: 0,
                hi: LispValue::MOST_POSITIVE_FIXNUM,
            }),
            Rep::TaggedFix,
            old.pc,
            old.frame,
            Effects::READ_HEAP,
            AliasClass::VecElem,
        );
        let (bounds, checked) = append(
            &mut func,
            Opcode::CheckBounds,
            &[index, length],
            numeric,
            Rep::TaggedFix,
            old.pc,
            old.frame,
            Effects::MAY_DEOPT,
            AliasClass::None,
        );
        before(
            &mut func,
            access,
            &[index_guard, array_guard, length_load, bounds],
        );
        func.insts[access.index()].args = vec![base, checked];
    }
    func.verify()
        .expect("typed current-owner array guard fixture");
    func
}

fn array_rebind() -> ByteCodeFunction {
    let label = LispValue::symbol("array-stored");
    let side = LispValue::symbol("t34-o335-side");
    bytecode(
        vec![
            Op::Constant(0),
            Op::StackRef(4),
            Op::Constant(0),
            Op::StackRef(2),
            Op::StackRef(8),
            Op::Lss,
            Op::GotoIfNil(28),
            Op::Constant(1),
            Op::StackRef(3),
            Op::StackRef(5),
            Op::List(3),
            Op::VarSet(2),
            Op::StackRef(2),
            Op::StackRef(6),
            Op::Eqlsign,
            Op::GotoIfNil(18),
            Op::StackRef(4),
            Op::StackSet(2),
            Op::Dup,
            Op::StackRef(2),
            Op::StackRef(4),
            Op::Aref,
            Op::Add,
            Op::StackSet(1),
            Op::StackRef(2),
            Op::Add1,
            Op::StackSet(3),
            Op::Goto(3),
            Op::StackRef(2),
            Op::StackRef(1),
            Op::StackRef(5),
            Op::List(3),
            Op::Return,
        ],
        vec![LispValue::fixnum(0), label, side],
        5,
    )
}

#[test]
fn opt_range_reference_bounds_result_preserves_error_order_and_identity() {
    let mut ctx = Context::new();
    let array = ctx.eval_str("[7 11 13]").unwrap();
    let _array = Roots::new(&[array]);
    let payload = ctx.eval_str("(list 'heap-companion)").unwrap();
    let _payload = Roots::new(&[payload]);
    let source = bytecode(
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::List(2),
            Op::VarSet(1),
            Op::Dup,
            Op::StackRef(3),
            Op::StackRef(3),
            Op::Aref,
            Op::StackRef(2),
            Op::List(3),
            Op::Return,
        ],
        vec![
            LispValue::symbol("aref-stored"),
            LispValue::symbol("t34-o335-side"),
        ],
        3,
    );
    let original = guarded_arrays(plan(&source, 0, None));
    let bad_index = LispValue::symbol("bad-index");
    let wrong = LispValue::symbol("wrong");
    let cases = [
        (array, LispValue::fixnum(2)),
        (array, LispValue::fixnum(-1)),
        (array, LispValue::fixnum(3)),
        (wrong, bad_index),
        (wrong, LispValue::fixnum(-1)),
    ];
    let mut baselines = Vec::new();
    for (base, index) in cases {
        let args = [base, index, payload];
        reset_side(&mut ctx);
        let expected = tier0(&mut ctx, &source, &args);
        let expected_side = side(&ctx);
        reset_side(&mut ctx);
        let baseline = reference(
            &mut ctx,
            &source,
            &original,
            Inputs {
                args: &args,
                ..Inputs::default()
            },
        );
        assert_eq!(
            baseline.0, expected,
            "index -> array -> bounds ordering uses Tier-0"
        );
        assert_eq!(side(&ctx), expected_side);
        baselines.push((baseline, expected_side));
    }
    let mut candidate = original.clone();
    let stats = range::run(&mut candidate).unwrap();
    candidate.verify().unwrap();
    assert_eq!(
        stats.bounds_checks_elided, 0,
        "unknown mutable length has no positive lower bound"
    );
    retained_observations(&original, &candidate);
    for ((base, index), (expected, expected_side)) in cases.into_iter().zip(baselines) {
        reset_side(&mut ctx);
        let args = [base, index, payload];
        assert_eq!(
            reference(
                &mut ctx,
                &source,
                &candidate,
                Inputs {
                    args: &args,
                    ..Inputs::default()
                }
            ),
            expected
        );
        assert_eq!(side(&ctx), expected_side);
    }

    // A separate numeric proof probe certifies CheckBounds' result identity.
    // It deliberately does NOT certify any cached vector length/backing.
    let numeric_source = bytecode(
        vec![Op::Constant(0), Op::Constant(1), Op::Pop, Op::Return],
        arguments(&[1, 3]),
        0,
    );
    let mut numeric = plan(&numeric_source, 0, None);
    let state = numeric.source_states[2].as_ref().unwrap().clone();
    let index_type = numeric.values[state.pre[0].index()].ty;
    let (guard, checked) = append(
        &mut numeric,
        Opcode::CheckBounds,
        &state.pre,
        index_type,
        Rep::Tagged,
        2,
        Some(state.frame),
        Effects::MAY_DEOPT,
        AliasClass::None,
    );
    let block = state.block.index();
    let position = numeric.blocks[block]
        .insts
        .iter()
        .position(|inst| numeric.insts[inst.index()].pc >= 2)
        .unwrap_or(numeric.blocks[block].insts.len());
    numeric.blocks[block].insts.insert(position, guard);
    numeric.blocks[block].term = Term::Return(checked);
    numeric.verify().unwrap();
    assert_eq!(
        reference(&mut ctx, &numeric_source, &numeric, Inputs::default()).0,
        tier0(&mut ctx, &numeric_source, &[])
    );
    let stats = range::run(&mut numeric).unwrap();
    numeric.verify().unwrap();
    assert_eq!(stats.bounds_checks_elided, 1);
    assert_eq!(numeric.insts[guard.index()].result, Some(checked));
    assert!(matches!(numeric.insts[guard.index()].op, Opcode::Refine(_)));
    assert_eq!(
        reference(&mut ctx, &numeric_source, &numeric, Inputs::default()).0,
        tier0(&mut ctx, &numeric_source, &[]),
        "the live checked-index identity survives replacement"
    );

    // Frozen array-phi-shorter: owner changes on iteration 1. Every length is
    // freshly associated with the current SSA owner, never a preheader base.
    let rebound_source = array_rebind();
    let rebound = guarded_arrays(plan(&rebound_source, 0, None));
    let shorter = ctx.eval_str("[101]").unwrap();
    let longer = ctx.eval_str("[101 103 107]").unwrap();
    let _replacement = Roots::new(&[shorter, longer]);
    for replacement in [shorter, longer] {
        let args = [
            LispValue::fixnum(3),
            array,
            LispValue::fixnum(1),
            replacement,
            payload,
        ];
        reset_side(&mut ctx);
        let expected = tier0(&mut ctx, &rebound_source, &args);
        let expected_side = side(&ctx);
        reset_side(&mut ctx);
        let baseline = reference(
            &mut ctx,
            &rebound_source,
            &rebound,
            Inputs {
                args: &args,
                ..Inputs::default()
            },
        );
        assert_eq!(baseline.0, expected);
        assert_eq!(side(&ctx), expected_side);
        let mut candidate = rebound.clone();
        range::run(&mut candidate).unwrap();
        let motion = licm::run(&mut candidate).unwrap();
        candidate.verify().unwrap();
        assert_eq!(
            motion.immutable_loads_hoisted, 0,
            "mutable vector length is excluded"
        );
        reset_side(&mut ctx);
        assert_eq!(
            reference(
                &mut ctx,
                &rebound_source,
                &candidate,
                Inputs {
                    args: &args,
                    ..Inputs::default()
                }
            ),
            baseline
        );
        assert_eq!(side(&ctx), expected_side);
    }
}

fn cold_loop() -> ByteCodeFunction {
    bytecode(
        vec![
            Op::Constant(0),
            Op::StackRef(5),
            Op::StackRef(1),
            Op::StackRef(8),
            Op::Lss,
            Op::GotoIfNil(26),
            Op::Constant(1),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::StackRef(5),
            Op::List(4),
            Op::VarSet(2),
            Op::Dup,
            Op::StackRef(2),
            Op::StackRef(6),
            Op::Eqlsign,
            Op::GotoIfNil(19),
            Op::StackRef(4),
            Op::Goto(20),
            Op::StackRef(6),
            Op::Add,
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Add1,
            Op::StackSet(2),
            Op::Goto(2),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::StackRef(4),
            Op::List(3),
            Op::Return,
        ],
        vec![
            LispValue::fixnum(0),
            LispValue::symbol("loop-stored"),
            LispValue::symbol("t34-o335-side"),
        ],
        6,
    )
}

#[test]
fn opt_licm_reference_zero_trip_and_late_failure_preserve_visible_store() {
    let mut ctx = Context::new();
    let payload = ctx.eval_str("(list 'heap-companion)").unwrap();
    let _payload = Roots::new(&[payload]);
    let wrong = LispValue::symbol("wrong");
    let source = cold_loop();
    let original = plan(&source, 0, None);
    let cases = [
        [
            LispValue::fixnum(0),
            wrong,
            wrong,
            LispValue::fixnum(0),
            wrong,
            payload,
        ],
        [
            LispValue::fixnum(4),
            LispValue::fixnum(0),
            LispValue::fixnum(1),
            LispValue::fixnum(2),
            wrong,
            payload,
        ],
    ];
    let mut baselines = Vec::new();
    for args in &cases {
        reset_side(&mut ctx);
        let expected = tier0(&mut ctx, &source, args);
        let expected_side = side(&ctx);
        reset_side(&mut ctx);
        let baseline = reference(
            &mut ctx,
            &source,
            &original,
            Inputs {
                args,
                ..Inputs::default()
            },
        );
        assert_eq!(baseline.0, expected);
        assert_eq!(side(&ctx), expected_side);
        baselines.push((baseline, expected_side));
    }
    assert!(baselines[0].0.1.is_none());
    assert_eq!(baselines[0].1, "nil");
    assert_eq!(baselines[1].0.1.as_ref().unwrap().pc, 20);
    let late_guards = original
        .insts
        .iter()
        .enumerate()
        .filter(|(_, inst)| inst.pc == 20 && matches!(inst.op, Opcode::CheckType(_)))
        .map(|(index, inst)| (Inst(index as u32), inst.clone()))
        .collect::<Vec<_>>();
    assert!(
        !late_guards.is_empty(),
        "the cold Add has actual body guards"
    );
    let mut candidate = original.clone();
    range::run(&mut candidate).unwrap();
    licm::run(&mut candidate).unwrap();
    candidate.verify().unwrap();
    // An anticipated header n guard may move. The failure after the visible
    // store at body PC20 must retain its actual operation and complete frame.
    for (id, original_guard) in late_guards {
        let retained = &candidate.insts[id.index()];
        assert_eq!(retained.op, original_guard.op);
        assert_eq!(retained.args, original_guard.args);
        assert_eq!(retained.result, original_guard.result);
        assert_eq!(retained.eff, original_guard.eff);
        assert_eq!(retained.mem, original_guard.mem);
        assert_eq!(retained.pc, 20);
        assert_eq!(retained.frame, original_guard.frame);
    }
    retained_observations(&original, &candidate);
    for (args, (baseline, expected_side)) in cases.iter().zip(baselines) {
        reset_side(&mut ctx);
        let actual = reference(
            &mut ctx,
            &source,
            &candidate,
            Inputs {
                args,
                ..Inputs::default()
            },
        );
        assert_eq!(
            actual, baseline,
            "complete original cold stack and error are retained"
        );
        assert_eq!(side(&ctx), expected_side);
    }
}

#[test]
fn opt_licm_reference_anticipated_invariant_guard_replays_loop_entry_once() {
    let mut ctx = Context::new();
    let side_symbol = LispValue::symbol("t34-o335-side");
    let source = bytecode(
        vec![
            Op::VarRef(0),
            Op::Add1,
            Op::VarSet(0),
            Op::Constant(1),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(12),
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(4),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![side_symbol, LispValue::fixnum(0)],
        1,
    );
    let original = plan(&source, 0, None);
    let args = [LispValue::symbol("wrong")];
    ctx.eval_str("(setq t34-o335-side 0)").unwrap();
    let expected = tier0(&mut ctx, &source, &args);
    assert_eq!(side(&ctx), "1");
    ctx.eval_str("(setq t34-o335-side 0)").unwrap();
    let baseline = reference(
        &mut ctx,
        &source,
        &original,
        Inputs {
            args: &args,
            ..Inputs::default()
        },
    );
    assert_eq!(baseline.0, expected);
    assert_eq!(baseline.1.as_ref().unwrap().pc, 6);
    assert_eq!(side(&ctx), "1");
    let mut candidate = original.clone();
    range::run(&mut candidate).unwrap();
    let stats = licm::run(&mut candidate).unwrap();
    candidate.verify().unwrap();
    assert!(
        stats.guards_hoisted > 0,
        "anticipated invariant n guard is reached even on zero-trip"
    );
    retained_observations(&original, &candidate);
    ctx.eval_str("(setq t34-o335-side 0)").unwrap();
    let actual = reference(
        &mut ctx,
        &source,
        &candidate,
        Inputs {
            args: &args,
            ..Inputs::default()
        },
    );
    assert_eq!(actual.0, expected);
    let replay = actual.1.expect("hoisted type guard deopts");
    assert_eq!(replay.pc, 4, "replay the actual loop-entry stack");
    assert_eq!(
        replay.stack,
        args.iter()
            .copied()
            .chain([LispValue::fixnum(0)])
            .map(ValueBits::from_value)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        side(&ctx),
        "1",
        "resumption does not replay the committed preheader store"
    );
    for n in [0, 1, 255, 510] {
        let args = arguments(&[n]);
        ctx.eval_str("(setq t34-o335-side 0)").unwrap();
        let expected = tier0(&mut ctx, &source, &args);
        ctx.eval_str("(setq t34-o335-side 0)").unwrap();
        assert_eq!(
            reference(
                &mut ctx,
                &source,
                &candidate,
                Inputs {
                    args: &args,
                    ..Inputs::default()
                }
            )
            .0,
            expected
        );
        assert_eq!(side(&ctx), "1");
    }
}

#[test]
fn opt_licm_reference_immutable_payload_keeps_existing_float_identity() {
    let mut ctx = Context::new();
    ctx.set_gc_threshold(usize::MAX);
    ctx.gc_stress = false;
    let seed = ctx.eval_str("-0.0").unwrap();
    let _seed = Roots::new(&[seed]);
    let source = bytecode(
        vec![
            Op::Constant(0),
            Op::StackRef(0),
            Op::StackRef(3),
            Op::Lss,
            Op::GotoIfNil(20),
            Op::StackRef(1),
            Op::Pop,
            // Real source collections at completed trips255 and510. Stress
            // only collects after new allocation; a pure payload/fixnum loop
            // cannot use it to promise one completed collection per Poll.
            Op::StackRef(0),
            Op::Constant(1),
            Op::Rem,
            Op::Constant(2),
            Op::Eqlsign,
            Op::GotoIfNil(16),
            Op::Constant(3),
            Op::Call(0),
            Op::Pop,
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(1),
            Op::StackRef(1),
            Op::Return,
        ],
        vec![
            LispValue::fixnum(0),
            LispValue::fixnum(255),
            LispValue::fixnum(254),
            LispValue::symbol("garbage-collect"),
        ],
        2,
    );
    let mut original = plan(&source, 0, None);
    let seed_value = original
        .insts
        .iter()
        .find(|inst| inst.op == Opcode::Arg(1))
        .unwrap()
        .result
        .unwrap();
    // This positive immutable-load fixture has an explicit FLOAT argument
    // contract; it never supplies a symbol to that declaration.
    original.values[seed_value.index()].ty = TypeSet::FLOAT;
    let state = original.source_states[6].as_ref().unwrap().clone();
    let (load, payload) = append(
        &mut original,
        Opcode::LoadF64,
        &[seed_value],
        TypeSet::FLOAT,
        Rep::RawF64,
        6,
        Some(state.frame),
        Effects::READ_HEAP,
        AliasClass::Immutable,
    );
    let (use_payload, _) = append(
        &mut original,
        Opcode::F64Cmp(Cmp::Eq),
        &[payload, payload],
        TypeSet::BOOLEAN,
        Rep::Bool,
        6,
        Some(state.frame),
        Effects::PURE,
        AliasClass::None,
    );
    let block = state.block.index();
    let position = original.blocks[block]
        .insts
        .iter()
        .position(|inst| original.insts[inst.index()].pc >= 6)
        .unwrap_or(original.blocks[block].insts.len());
    original.blocks[block]
        .insts
        .splice(position..position, [load, use_payload]);
    original
        .verify()
        .expect("immutable payload's live owner is a proven existing Float");
    for n in [0, 255, 510] {
        let args = [LispValue::fixnum(n), seed];
        assert_eq!(tier0(&mut ctx, &source, &args), Answer::Value(text(seed)));
        let old_gc = ctx.gc_count;
        let stress = ctx.gc_stress;
        ctx.gc_stress = false;
        crate::emacs_core::eval::reset_bytecode_branch_poll_count();
        let run = evaluate(
            &original,
            &mut ctx,
            Inputs {
                args: &args,
                ..Inputs::default()
            },
        )
        .unwrap();
        ctx.gc_stress = stress;
        let Outcome::Returned(bits) = run.outcome else {
            panic!("in-range fixture returns")
        };
        assert_eq!(
            bits.to_value(),
            seed,
            "existing Float object, including -0.0 identity"
        );
        assert_eq!(
            crate::emacs_core::eval::bytecode_branch_poll_count(),
            n as usize / 255
        );
        assert_eq!(ctx.gc_count - old_gc, n as u64 / 255);
    }
    let mut candidate = original.clone();
    let stats = licm::run(&mut candidate).unwrap();
    candidate.verify().unwrap();
    assert_eq!(stats.immutable_loads_hoisted, 1);
    retained_observations(&original, &candidate);
    for n in [0, 255, 510] {
        let args = [LispValue::fixnum(n), seed];
        let old_gc = ctx.gc_count;
        let stress = ctx.gc_stress;
        ctx.gc_stress = false;
        crate::emacs_core::eval::reset_bytecode_branch_poll_count();
        let run = evaluate(
            &candidate,
            &mut ctx,
            Inputs {
                args: &args,
                ..Inputs::default()
            },
        )
        .unwrap();
        ctx.gc_stress = stress;
        let Outcome::Returned(bits) = run.outcome else {
            panic!("in-range fixture returns")
        };
        assert_eq!(bits.to_value(), seed);
        assert_eq!(
            crate::emacs_core::eval::bytecode_branch_poll_count(),
            n as usize / 255
        );
        assert_eq!(ctx.gc_count - old_gc, n as u64 / 255);
    }

    // Unknown seed negative: the body-only Float guard is not anticipated on
    // zero-trip. Its existing full frame permits baseline replay at Pop; it
    // must not turn a successful zero-trip into a newly observed deopt frame.
    let mut unknown = plan(&source, 0, None);
    let seed_value = unknown
        .insts
        .iter()
        .find(|inst| inst.op == Opcode::Arg(1))
        .unwrap()
        .result
        .unwrap();
    let state = unknown.source_states[6].as_ref().unwrap().clone();
    let (guard, checked) = append(
        &mut unknown,
        Opcode::CheckType(TypeSet::FLOAT),
        &[seed_value],
        TypeSet::FLOAT,
        Rep::Tagged,
        6,
        Some(state.frame),
        Effects::MAY_DEOPT,
        AliasClass::None,
    );
    let (load, payload) = append(
        &mut unknown,
        Opcode::LoadF64,
        &[checked],
        TypeSet::FLOAT,
        Rep::RawF64,
        6,
        Some(state.frame),
        Effects::READ_HEAP,
        AliasClass::Immutable,
    );
    let (use_payload, _) = append(
        &mut unknown,
        Opcode::F64Cmp(Cmp::Eq),
        &[payload, payload],
        TypeSet::BOOLEAN,
        Rep::Bool,
        6,
        Some(state.frame),
        Effects::PURE,
        AliasClass::None,
    );
    let block = state.block.index();
    let position = unknown.blocks[block]
        .insts
        .iter()
        .position(|inst| unknown.insts[inst.index()].pc >= 6)
        .unwrap_or(unknown.blocks[block].insts.len());
    unknown.blocks[block]
        .insts
        .splice(position..position, [guard, load, use_payload]);
    unknown.verify().unwrap();
    let args = [LispValue::fixnum(0), LispValue::symbol("wrong")];
    let baseline = reference(
        &mut ctx,
        &source,
        &unknown,
        Inputs {
            args: &args,
            ..Inputs::default()
        },
    );
    assert_eq!(baseline.0, tier0(&mut ctx, &source, &args));
    assert!(baseline.1.is_none());
    let original_body_guard = unknown.insts[guard.index()].clone();
    let mut candidate = unknown.clone();
    let stats = licm::run(&mut candidate).unwrap();
    candidate.verify().unwrap();
    let retained_body_guard = &candidate.insts[guard.index()];
    assert_eq!(retained_body_guard.op, original_body_guard.op);
    assert_eq!(retained_body_guard.args, original_body_guard.args);
    assert_eq!(retained_body_guard.result, original_body_guard.result);
    assert_eq!(retained_body_guard.frame, original_body_guard.frame);
    assert_eq!(retained_body_guard.pc, original_body_guard.pc);
    assert_eq!(retained_body_guard.eff, original_body_guard.eff);
    assert_eq!(retained_body_guard.mem, original_body_guard.mem);
    assert!(candidate.blocks[block].insts.contains(&guard));
    assert_eq!(
        stats.immutable_loads_hoisted, 0,
        "body-only proof cannot dominate the preheader"
    );
    retained_observations(&unknown, &candidate);
    assert_eq!(
        reference(
            &mut ctx,
            &source,
            &candidate,
            Inputs {
                args: &args,
                ..Inputs::default()
            }
        ),
        baseline
    );
}
