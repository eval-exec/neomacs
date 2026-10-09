//! Typed integer operations must follow verified SSA and retain GNU recovery.
//! GNU arithmetic/phi flows are frozen in tmp/t34-o33-o35-gnu-prep; answers here
//! come from forced Tier-0 execution. Threading: plans, roots and compiler-only
//! overrides belong to one test invocation and are restored on exit.

use crate::emacs_core::jit::compile::opt_census::SelectedTier;

use super::compile_pipeline_tests::{captured_clif, function};
use super::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{
    bytecode_branch_poll_count, push_scratch_gc_roots, reset_bytecode_branch_poll_count,
    restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::opt::mem::{AliasClass, Effects};
use crate::emacs_core::jit::opt::types::TypeSet;
use crate::emacs_core::jit::opt::{build, eval, ir};
use crate::emacs_core::print::print_value;

#[cfg(test)]
#[path = "opt_range_licm_test.rs"]
mod range_licm_tests;

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses {
            reps: true,
            bool_rep: true,
            ..OptPasses::default()
        }));
        force_deopt_for_test(false);
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
        force_deopt_for_test(false);
        lowering::force_clif_dump_for_test(None);
    }
}

/// Uses the owning mutator's existing scratch-root stack, with no new state.
struct Roots(usize);
impl Roots {
    fn new(values: &[Value]) -> Self {
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

fn plan(source: &ByteCodeFunction) -> ir::Func {
    let arity = source.params.required.len();
    let cfg = analyze_cfg(
        source.executable_ops(),
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        arity,
    )
    .unwrap();
    let constants = source
        .constants
        .iter()
        .copied()
        .map(ir::ValueBits::from_value)
        .collect::<Vec<_>>();
    build::build(build::BuildInput {
        ops: source.executable_ops(),
        constants: &constants,
        cfg: &cfg,
        params: ir::ParamShape {
            required: arity,
            ..ir::ParamShape::default()
        },
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })
    .unwrap()
}

fn lower(source: &ByteCodeFunction, plan: &ir::Func) -> CompiledLeaf {
    plan.verify()
        .expect("manual typed IR verifies before native lowering");
    let leaf = lower_opt_ir_for_test(
        source.executable_ops(),
        &source.constants,
        source.params.required.len(),
        source.executable_gnu_byte_offset_map(),
        plan,
    )
    .expect("verified typed integer IR has native support");
    assert_eq!(leaf.selected_tier(), SelectedTier::Opt);
    leaf
}

fn tier0(ctx: &mut Context, source: &ByteCodeFunction, args: &[Value]) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(source, args.to_vec())
}

fn native_ok(ctx: &mut Context, leaf: &CompiledLeaf, args: &[Value]) -> Value {
    match leaf.call(ctx as *mut Context as *mut u8, args) {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        other => panic!("typed native success must not fall back: {other:?}"),
    }
}

fn resumed(
    ctx: &mut Context,
    source: &ByteCodeFunction,
    resume: &DeoptResume,
) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.run_resumed_frame(
        source,
        Value::NIL,
        resume.pc,
        &resume.stack,
        resume.handlers,
        &resume.binds,
        resume.spec_base,
        resume.cond_base,
    )
}

fn instruction_at(plan: &ir::Func, pc: u32, source_op: &Op) -> ir::Inst {
    ir::Inst(
        plan.insts
            .iter()
            .position(|inst| {
                inst.pc == pc && matches!(&inst.op, ir::Opcode::Opaque(op) if op == source_op)
            })
            .unwrap() as u32,
    )
}

fn append_value(
    plan: &mut ir::Func,
    op: ir::Opcode,
    args: Vec<ir::Value>,
    ty: TypeSet,
    rep: ir::Rep,
    frame: Option<ir::FrameId>,
    pc: u32,
) -> (ir::Inst, ir::Value) {
    let id = ir::Inst(plan.insts.len() as u32);
    let value = ir::Value(plan.values.len() as u32);
    let eff = if matches!(op, ir::Opcode::CheckType(_)) {
        Effects::MAY_DEOPT
    } else {
        Effects::PURE
    };
    plan.values.push(ir::ValueData {
        ty,
        rep,
        def: ir::ValueDef::Inst(id),
    });
    plan.insts.push(ir::InstData {
        op,
        args,
        result: Some(value),
        eff,
        mem: AliasClass::None,
        frame,
        pc,
    });
    (id, value)
}

fn insert_before(plan: &mut ir::Func, target: ir::Inst, inserted: Vec<ir::Inst>) {
    let block = plan
        .blocks
        .iter_mut()
        .find(|block| block.insts.contains(&target))
        .unwrap();
    let at = block.insts.iter().position(|&id| id == target).unwrap();
    block.insts.splice(at..at, inserted);
}

/// Guard an unknown tagged value, then introduce an explicit typed view. The
/// source identity and all GNU pre-operation stacks remain untouched.
fn converted(
    plan: &mut ir::Func,
    input: ir::Value,
    rep: ir::Rep,
    frame: ir::FrameId,
    pc: u32,
    inserted: &mut Vec<ir::Inst>,
) -> ir::Value {
    let canonical = plan.resolve(input).unwrap();
    let mut value = input;
    let mut actual = plan.values[canonical.index()].rep;
    if actual == rep {
        return value;
    }
    if actual.is_tagged()
        && (!plan.values[canonical.index()].ty.is_subset(TypeSet::FIXNUM)
            || plan.values[canonical.index()].ty.is_bottom())
    {
        let (id, checked) = append_value(
            plan,
            ir::Opcode::CheckType(TypeSet::FIXNUM),
            vec![value],
            TypeSet::FIXNUM,
            actual,
            Some(frame),
            pc,
        );
        inserted.push(id);
        value = checked;
    }
    if actual.is_tagged() {
        let (id, raw) = append_value(
            plan,
            ir::Opcode::UntagFix,
            vec![value],
            TypeSet::FIXNUM,
            ir::Rep::RawInt,
            None,
            pc,
        );
        inserted.push(id);
        value = raw;
        actual = ir::Rep::RawInt;
    }
    if rep == ir::Rep::TaggedFix {
        assert_eq!(actual, ir::Rep::RawInt);
        let (id, tagged) = append_value(
            plan,
            ir::Opcode::TagFix,
            vec![value],
            TypeSet::FIXNUM,
            rep,
            None,
            pc,
        );
        inserted.push(id);
        value = tagged;
    } else {
        assert_eq!(rep, ir::Rep::RawInt);
    }
    value
}

fn typed_binary(plan: &mut ir::Func, id: ir::Inst, opcode: ir::Opcode, rep: ir::Rep) {
    let original = plan.insts[id.index()].clone();
    let frame = original.frame.unwrap();
    let mut inserted = Vec::new();
    let args = original
        .args
        .iter()
        .map(|&value| converted(plan, value, rep, frame, original.pc, &mut inserted))
        .collect();
    insert_before(plan, id, inserted);
    let result = original.result.unwrap();
    plan.values[result.index()].rep = rep;
    plan.values[result.index()].ty = TypeSet::FIXNUM;
    plan.insts[id.index()].op = opcode;
    plan.insts[id.index()].args = args;
    plan.insts[id.index()].eff = Effects::MAY_DEOPT;
    plan.insts[id.index()].mem = AliasClass::None;
}

fn typed_compare(plan: &mut ir::Func, id: ir::Inst, cmp: ir::Cmp, rep: ir::Rep) {
    let original = plan.insts[id.index()].clone();
    let mut inserted = Vec::new();
    let args = original
        .args
        .iter()
        .map(|&value| {
            converted(
                plan,
                value,
                rep,
                original.frame.unwrap(),
                original.pc,
                &mut inserted,
            )
        })
        .collect();
    insert_before(plan, id, inserted);
    let result = original.result.unwrap();
    plan.values[result.index()].rep = ir::Rep::Bool;
    plan.values[result.index()].ty = TypeSet::BOOLEAN;
    plan.insts[id.index()].op = ir::Opcode::FixCmp(cmp);
    plan.insts[id.index()].args = args;
    plan.insts[id.index()].eff = Effects::PURE;
    plan.insts[id.index()].mem = AliasClass::None;
    for inst in &mut plan.insts {
        if inst.op == ir::Opcode::IsNonNil && inst.args == [result] {
            inst.op = ir::Opcode::Refine(TypeSet::BOOLEAN);
        }
    }
}

fn repair_views(plan: &mut ir::Func) {
    for _ in 0..=plan.values.len() {
        let mut changed = false;
        for index in 0..plan.values.len() {
            let ir::ValueDef::Alias(_) = plan.values[index].def else {
                continue;
            };
            let canonical = plan.resolve(ir::Value(index as u32)).unwrap();
            let original = plan.values[canonical.index()].clone();
            changed |=
                plan.values[index].rep != original.rep || plan.values[index].ty != original.ty;
            plan.values[index].rep = original.rep;
            plan.values[index].ty = original.ty;
        }
        for inst in &plan.insts {
            let ir::Opcode::Refine(ty) = inst.op else {
                continue;
            };
            let arg = plan.resolve(inst.args[0]).unwrap();
            let result = inst.result.unwrap();
            let rep = plan.values[arg.index()].rep;
            let narrowed = plan.values[arg.index()].ty.meet(ty);
            changed |= plan.values[result.index()].rep != rep
                || plan.values[result.index()].ty != narrowed;
            plan.values[result.index()].rep = rep;
            plan.values[result.index()].ty = narrowed;
        }
        if !changed {
            return;
        }
    }
    panic!("bounded fixture metadata repair converges");
}

fn tag_returns(plan: &mut ir::Func) {
    for index in 0..plan.blocks.len() {
        let ir::Term::Return(value) = plan.blocks[index].term else {
            continue;
        };
        let canonical = plan.resolve(value).unwrap();
        if plan.values[canonical.index()].rep != ir::Rep::RawInt {
            continue;
        }
        let pc = plan
            .source_states
            .iter()
            .enumerate()
            .rev()
            .find_map(|(pc, source)| {
                source
                    .as_ref()
                    .filter(|state| state.block.index() == index)
                    .map(|_| pc as u32)
            })
            .unwrap();
        let (id, tagged) = append_value(
            plan,
            ir::Opcode::TagFix,
            vec![value],
            TypeSet::FIXNUM,
            ir::Rep::TaggedFix,
            None,
            pc,
        );
        plan.blocks[index].insts.push(id);
        plan.blocks[index].term = ir::Term::Return(tagged);
    }
}

fn binary(op: Op) -> ByteCodeFunction {
    function(
        vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return],
        vec![],
        2,
    )
}

#[test]
fn opt_reps_native_checked_integer_boundaries_follow_typed_ir() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let lo = Value::MOST_NEGATIVE_FIXNUM;
    let hi = Value::MOST_POSITIVE_FIXNUM;
    for rep in [ir::Rep::TaggedFix, ir::Rep::RawInt] {
        for (op, typed, pairs) in [
            (
                Op::Add,
                ir::Opcode::FixAdd { checked: true },
                vec![(lo, 0), (hi, 0), (hi, -1), (lo, 1), (-1, 1), (0, 0)],
            ),
            (
                Op::Sub,
                ir::Opcode::FixSub { checked: true },
                vec![(lo, 0), (hi, 0), (hi, 1), (lo, -1), (-1, -1), (0, 0)],
            ),
            (
                Op::Mul,
                ir::Opcode::FixMul { checked: true },
                vec![
                    (lo, 1),
                    (hi, 1),
                    (hi, -1),
                    (lo, 0),
                    (-1, -1),
                    (0, hi),
                    (lo / 2, 2),
                ],
            ),
        ] {
            let source = binary(op.clone());
            let mut changed = plan(&source);
            let id = instruction_at(&changed, 2, &op);
            typed_binary(&mut changed, id, typed, rep);
            repair_views(&mut changed);
            tag_returns(&mut changed);
            let mut leaf = None;
            let clif = captured_clif(|| leaf = Some(lower(&source, &changed)));
            assert_eq!(clif.len(), 1);
            if op == Op::Mul {
                assert!(
                    !clif[0].contains("i128"),
                    "fix-domain multiplication needs no i128: {}",
                    clif[0]
                );
            }
            let leaf = leaf.unwrap();
            for (a, b) in pairs {
                let args = [Value::fixnum(a), Value::fixnum(b)];
                assert_eq!(
                    native_ok(&mut ctx, &leaf, &args),
                    tier0(&mut ctx, &source, &args).unwrap(),
                    "{rep:?} {op:?} {a} {b}"
                );
            }
        }
        // Retaining source Add while changing verified SSA to Mul must change
        // native success. An adapter that merely replays bytecode would fail.
        let source = binary(Op::Add);
        let oracle = binary(Op::Mul);
        let mut changed = plan(&source);
        let id = instruction_at(&changed, 2, &Op::Add);
        typed_binary(&mut changed, id, ir::Opcode::FixMul { checked: true }, rep);
        repair_views(&mut changed);
        tag_returns(&mut changed);
        let args = [Value::fixnum(2), Value::fixnum(3)];
        let expected = tier0(&mut ctx, &oracle, &args).unwrap();
        assert_ne!(expected, tier0(&mut ctx, &source, &args).unwrap());
        let leaf = lower(&source, &changed);
        assert_eq!(native_ok(&mut ctx, &leaf, &args), expected);
        force_deopt_for_test(true);
        let leaf = lower(&source, &changed);
        force_deopt_for_test(false);
        let NativeRun::DeoptAt(frame) = leaf.call(&mut ctx as *mut Context as *mut u8, &args)
        else {
            panic!("forced typed guard deopts");
        };
        assert_eq!(frame.pc, 2);
        assert_eq!(frame.stack, [args[0], args[1], args[0], args[1]]);
    }
}

#[test]
fn opt_reps_native_fixcmp_crosses_branch_in_both_boolean_modes() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    for boolean in [false, true] {
        force_opt_passes_for_test(Some(OptPasses {
            reps: true,
            bool_rep: boolean,
            ..OptPasses::default()
        }));
        for rep in [ir::Rep::TaggedFix, ir::Rep::RawInt] {
            for (op, cmp) in [
                (Op::Eqlsign, ir::Cmp::Eq),
                (Op::Lss, ir::Cmp::Lt),
                (Op::Gtr, ir::Cmp::Gt),
                (Op::Leq, ir::Cmp::Le),
                (Op::Geq, ir::Cmp::Ge),
            ] {
                let source = function(
                    vec![
                        Op::StackRef(1),
                        Op::StackRef(1),
                        op.clone(),
                        Op::GotoIfNil(6),
                        Op::Constant(0),
                        Op::Return,
                        Op::Constant(1),
                        Op::Return,
                    ],
                    vec![Value::T, Value::NIL],
                    2,
                );
                let mut changed = plan(&source);
                let id = instruction_at(&changed, 2, &op);
                typed_compare(&mut changed, id, cmp, rep);
                repair_views(&mut changed);
                let leaf = lower(&source, &changed);
                for (a, b) in [
                    (Value::MOST_NEGATIVE_FIXNUM, Value::MOST_POSITIVE_FIXNUM),
                    (Value::MOST_POSITIVE_FIXNUM, Value::MOST_NEGATIVE_FIXNUM),
                    (0, 0),
                    (0, 1),
                    (1, 0),
                ] {
                    let args = [Value::fixnum(a), Value::fixnum(b)];
                    assert_eq!(
                        native_ok(&mut ctx, &leaf, &args),
                        tier0(&mut ctx, &source, &args).unwrap(),
                        "{rep:?} {cmp:?} Bool={boolean}"
                    );
                }
            }
        }
    }
}

fn after_store_source(op: Op, step: i64, wrong_type: bool, collect: bool) -> ByteCodeFunction {
    let mut ops = vec![
        Op::StackRef(2),
        Op::Constant(0),
        Op::Add,
        Op::Dup,
        Op::VarRef(1),
        Op::Add1,
        Op::VarSet(1),
    ];
    if collect {
        ops.extend([Op::Constant(3), Op::Call(0), Op::Pop]);
    }
    ops.extend([
        if wrong_type {
            Op::StackRef(3)
        } else {
            Op::Constant(2)
        },
        op,
        Op::Return,
    ]);
    function(
        ops,
        vec![
            Value::fixnum(0),
            Value::symbol("t34-o33-effects"),
            Value::fixnum(step),
            Value::symbol("garbage-collect"),
        ],
        3,
    )
}

#[test]
fn opt_reps_native_overflow_retags_aliases_and_replays_after_store() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let payload = Value::cons(Value::symbol("heap-companion"), Value::NIL);
    let _roots = Roots::new(&[payload]);
    for rep in [ir::Rep::TaggedFix, ir::Rep::RawInt] {
        for collect in [false, true] {
            for (op, typed, input, step) in [
                (
                    Op::Add,
                    ir::Opcode::FixAdd { checked: true },
                    Value::MOST_POSITIVE_FIXNUM,
                    1,
                ),
                (
                    Op::Add,
                    ir::Opcode::FixAdd { checked: true },
                    Value::MOST_NEGATIVE_FIXNUM,
                    -1,
                ),
                (
                    Op::Sub,
                    ir::Opcode::FixSub { checked: true },
                    Value::MOST_NEGATIVE_FIXNUM,
                    1,
                ),
                (
                    Op::Sub,
                    ir::Opcode::FixSub { checked: true },
                    Value::MOST_POSITIVE_FIXNUM,
                    -1,
                ),
                (
                    Op::Mul,
                    ir::Opcode::FixMul { checked: true },
                    Value::MOST_POSITIVE_FIXNUM,
                    2,
                ),
                (
                    Op::Mul,
                    ir::Opcode::FixMul { checked: true },
                    Value::MOST_NEGATIVE_FIXNUM,
                    -1,
                ),
            ] {
                let source = after_store_source(op.clone(), step, false, collect);
                let pc = if collect { 11 } else { 8 };
                let mut changed = plan(&source);
                let first = instruction_at(&changed, 2, &Op::Add);
                typed_binary(
                    &mut changed,
                    first,
                    ir::Opcode::FixAdd { checked: true },
                    rep,
                );
                let last = instruction_at(&changed, pc, &op);
                typed_binary(&mut changed, last, typed, rep);
                repair_views(&mut changed);
                tag_returns(&mut changed);
                let leaf = lower(&source, &changed);
                let args = [Value::fixnum(input), Value::NIL, payload];
                ctx.eval_str("(setq t34-o33-effects 0)").unwrap();
                let expected = tier0(&mut ctx, &source, &args).unwrap();
                push_scratch_gc_roots(&[expected]);
                let expected_effect = ctx.eval_str("t34-o33-effects").unwrap();
                ctx.eval_str("(setq t34-o33-effects 0)").unwrap();
                let collections = ctx.tagged_heap.gc_collections();
                let NativeRun::DeoptAt(frame) =
                    leaf.call(&mut ctx as *mut Context as *mut u8, &args)
                else {
                    panic!("overflow must recover after the effect");
                };
                assert_eq!(frame.pc, pc as usize);
                assert_eq!(
                    frame.stack,
                    [
                        args[0],
                        args[1],
                        payload,
                        args[0],
                        args[0],
                        Value::fixnum(step)
                    ]
                );
                assert_eq!(
                    frame.stack[3], frame.stack[4],
                    "raw aliases recover the same tagged fixnum"
                );
                assert_eq!(ctx.eval_str("t34-o33-effects").unwrap(), expected_effect);
                if collect {
                    assert!(
                        ctx.tagged_heap.gc_collections() > collections,
                        "the native body must actually execute garbage-collect before overflow"
                    );
                }
                assert_eq!(ctx.jit_root_stack_top, 0);
                let actual = resumed(&mut ctx, &source, &frame).unwrap();
                assert_eq!(print_value(&actual), print_value(&expected));
                assert_eq!(
                    ctx.eval_str("t34-o33-effects").unwrap(),
                    expected_effect,
                    "resumption must not repeat the earlier store"
                );
                assert_eq!(ctx.jit_root_stack_top, 0);
            }
        }
    }
}

fn signal_summary(flow: &Flow) -> (SymId, Vec<String>, Option<String>) {
    let signal = flow.as_signal().expect("arithmetic failure is a signal");
    (
        signal.symbol,
        signal.data.iter().map(print_value).collect(),
        signal.raw_data.as_ref().map(print_value),
    )
}

#[test]
fn opt_reps_native_type_failure_keeps_original_error_and_effects() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let payload = Value::cons(Value::symbol("heap-companion"), Value::NIL);
    let _roots = Roots::new(&[payload]);
    for rep in [ir::Rep::TaggedFix, ir::Rep::RawInt] {
        let source = after_store_source(Op::Add, 1, true, false);
        let mut changed = plan(&source);
        let first = instruction_at(&changed, 2, &Op::Add);
        typed_binary(
            &mut changed,
            first,
            ir::Opcode::FixAdd { checked: true },
            rep,
        );
        let last = instruction_at(&changed, 8, &Op::Add);
        typed_binary(
            &mut changed,
            last,
            ir::Opcode::FixAdd { checked: true },
            rep,
        );
        repair_views(&mut changed);
        tag_returns(&mut changed);
        let leaf = lower(&source, &changed);
        for bad in [Value::NIL, Value::symbol("wrong"), payload] {
            let args = [Value::fixnum(7), bad, payload];
            ctx.eval_str("(setq t34-o33-effects 0)").unwrap();
            let expected = tier0(&mut ctx, &source, &args).expect_err("Tier-0 wrong-type error");
            let expected_signal = signal_summary(&expected);
            let expected_effect = ctx.eval_str("t34-o33-effects").unwrap();
            ctx.eval_str("(setq t34-o33-effects 0)").unwrap();
            let NativeRun::DeoptAt(frame) = leaf.call(&mut ctx as *mut Context as *mut u8, &args)
            else {
                panic!("wrong type must recover precisely");
            };
            assert_eq!(frame.pc, 8);
            assert_eq!(frame.stack, [args[0], bad, payload, args[0], args[0], bad]);
            assert_eq!(ctx.eval_str("t34-o33-effects").unwrap(), expected_effect);
            let actual = resumed(&mut ctx, &source, &frame).expect_err("resumed original error");
            assert_eq!(signal_summary(&actual), expected_signal);
            assert_eq!(ctx.eval_str("t34-o33-effects").unwrap(), expected_effect);
            assert_eq!(ctx.jit_root_stack_top, 0);
        }
    }
}

fn raw_countdown(plan: &mut ir::Func) {
    let params = plan
        .blocks
        .iter()
        .flat_map(|block| block.params.iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(params.len(), 1, "a single actual loop-carried counter phi");
    for value in params {
        plan.values[value.index()].rep = ir::Rep::RawInt;
        plan.values[value.index()].ty = TypeSet::FIXNUM;
    }
    repair_views(plan);
    let sub = instruction_at(plan, 9, &Op::Sub1);
    let original = plan.insts[sub.index()].clone();
    let mut constants = plan.consts.to_vec();
    let one = constants.len() as u32;
    constants.push(ir::ValueBits::from_value(Value::fixnum(1)));
    plan.consts = constants.into_boxed_slice();
    let (id, value) = append_value(
        plan,
        ir::Opcode::Const(one),
        vec![],
        TypeSet::FIXNUM,
        ir::Rep::Tagged,
        None,
        9,
    );
    insert_before(plan, sub, vec![id]);
    plan.insts[sub.index()].args.push(value);
    typed_binary(
        plan,
        sub,
        ir::Opcode::FixSub { checked: true },
        ir::Rep::RawInt,
    );
    let compare = instruction_at(plan, 6, &Op::Gtr);
    typed_compare(plan, compare, ir::Cmp::Gt, ir::Rep::RawInt);
    repair_views(plan);
    let state = plan.source_states[3].as_ref().unwrap().clone();
    // The builder may insert a separate Arg-entry block. This guard belongs
    // after the source Cons and before Pop, in the source pc3 block itself.
    let entry = state.block.index();
    let ir::Term::Jump(edge) = &plan.blocks[entry].term else {
        panic!("entry reaches the loop header");
    };
    assert_eq!(edge.args.len(), 1);
    let original_arg = edge.args[0];
    let mut inserted = Vec::new();
    let raw = converted(
        plan,
        original_arg,
        ir::Rep::RawInt,
        state.frame,
        3,
        &mut inserted,
    );
    plan.blocks[entry].insts.extend(inserted);
    let ir::Term::Jump(edge) = &mut plan.blocks[entry].term else {
        unreachable!()
    };
    edge.args[0] = raw;
    repair_views(plan);
    tag_returns(plan);
    assert_eq!(plan.insts[sub.index()].frame, original.frame);
}

#[test]
fn opt_reps_native_raw_phi_transport_survives_gc_polls() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let source = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Cons,
            Op::Pop,
            Op::StackRef(0),
            Op::Constant(2),
            Op::Gtr,
            Op::GotoIfNil(12),
            Op::StackRef(0),
            Op::Sub1,
            Op::StackSet(1),
            Op::Goto(4),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![Value::fixnum(7), Value::fixnum(9), Value::fixnum(0)],
        1,
    );
    let mut changed = plan(&source);
    raw_countdown(&mut changed);
    let leaf = lower(&source, &changed);
    for n in [0, 1, 254, 255, 256, 510] {
        let args = [Value::fixnum(n)];
        reset_bytecode_branch_poll_count();
        let expected = tier0(&mut ctx, &source, &args).unwrap();
        let expected_polls = bytecode_branch_poll_count();
        reset_bytecode_branch_poll_count();
        assert_eq!(native_ok(&mut ctx, &leaf, &args), expected, "n={n}");
        assert_eq!(bytecode_branch_poll_count(), expected_polls, "n={n}");
        assert_eq!(expected_polls, (n / 255) as usize);
    }
    // The heap value has left every GNU stack by the poll, and exists only in
    // the authoritative SSA return. Its allocation and survival cannot be
    // supplied by the interpreter or by an external Rust scratch root.
    let cell = changed.insts[instruction_at(&changed, 2, &Op::Cons).index()]
        .result
        .unwrap();
    for inst in changed
        .insts
        .iter()
        .filter(|inst| inst.op == ir::Opcode::Poll)
    {
        assert!(
            !changed.frames[inst.frame.unwrap().index()]
                .stack
                .contains(&cell)
        );
    }
    changed
        .blocks
        .iter_mut()
        .find(|block| matches!(block.term, ir::Term::Return(_)))
        .unwrap()
        .term = ir::Term::Return(cell);
    let reference = eval::evaluate(
        &changed,
        &mut ctx,
        eval::Inputs {
            args: &[Value::fixnum(0)],
            ..eval::Inputs::default()
        },
    )
    .unwrap();
    let eval::Outcome::Returned(expected) = reference.outcome else {
        panic!("reference typed IR returns");
    };
    let expected = print_value(&expected.to_value());
    let leaf = lower(&source, &changed);
    for n in [255, 510] {
        let collections = ctx.tagged_heap.gc_collections();
        ctx.gc_stress = true;
        reset_bytecode_branch_poll_count();
        let result = native_ok(&mut ctx, &leaf, &[Value::fixnum(n)]);
        ctx.gc_stress = false;
        assert_eq!(print_value(&result), expected);
        assert!(ctx.tagged_heap.gc_collections() > collections);
        assert_eq!(bytecode_branch_poll_count(), (n / 255) as usize);
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

#[test]
fn opt_reps_native_static_fixnum_untag_uses_immediate_payload() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    for number in [
        Value::MOST_NEGATIVE_FIXNUM,
        Value::MOST_POSITIVE_FIXNUM,
        -1,
        0,
        1,
        9_007_199_254_740_993,
    ] {
        let source = function(
            vec![Op::Constant(0), Op::Return],
            vec![Value::fixnum(number)],
            0,
        );
        let mut changed = plan(&source);
        let constant = changed
            .insts
            .iter()
            .find_map(|inst| (inst.op == ir::Opcode::Const(0)).then(|| inst.result.unwrap()))
            .expect("actual immutable source Const");
        let ty = changed.values[constant.index()].ty;
        let (untag, raw) = append_value(
            &mut changed,
            ir::Opcode::UntagFix,
            vec![constant],
            ty,
            ir::Rep::RawInt,
            None,
            1,
        );
        let (tag, tagged) = append_value(
            &mut changed,
            ir::Opcode::TagFix,
            vec![raw],
            ty,
            ir::Rep::TaggedFix,
            None,
            1,
        );
        let block = changed
            .blocks
            .iter_mut()
            .find(|block| matches!(block.term, ir::Term::Return(_)))
            .unwrap();
        block.insts.extend([untag, tag]);
        block.term = ir::Term::Return(tagged);
        let mut leaf = None;
        let clif = captured_clif(|| leaf = Some(lower(&source, &changed)));
        assert_eq!(clif.len(), 1);
        let expected = tier0(&mut ctx, &source, &[]).unwrap();
        assert_eq!(native_ok(&mut ctx, &leaf.unwrap(), &[]), expected);
        // The fixture contains no guards or other arithmetic. An actual
        // immutable fixnum literal supplies its raw payload at compile time;
        // it must not execute a shift to rediscover that payload at runtime.
        assert!(
            !clif[0].lines().any(|line| line.contains(" = sshr")),
            "static fixnum UntagFix still emits a runtime shift: {}",
            clif[0]
        );
    }
}

#[cfg(test)]
#[path = "opt_licm_positive_test.rs"]
mod licm_positive_tests;
