//! GVN's successful values and retained effects are checked through Tier-0.
//!
//! GNU evidence: the frozen O3.4 pack's repeated-cons, store-car/cdr-same-base,
//! aliased/distinct stores, diamond, call-mutates-* and gc-hook-mutates-loads
//! observations; O3.2 swp-disabled/enabled; O3.6 float allocation identity.
//! GNU data.c:659-712 establishes nil reads, setter returns and retained writes;
//! alloc.c:2480 establishes fresh Float identity. No alternate Lisp semantics
//! or test-only evaluator hook is introduced here.

use super::build::{BuildInput, build};
use super::eval::{Inputs, Outcome, Run, evaluate};
use super::ir::*;
use super::mem::{AliasClass, Effects};
use super::passes::gvn;
use super::types::TypeSet;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op, Vm};
use crate::emacs_core::eval::{
    Context, push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::value::{LambdaParams, Value as LispValue};

// Roots and fixture assembly belong to this invocation's mutator. No compiler
// cache or cross-mutator state is installed; scratch roots restore on unwind.
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

struct Program {
    ops: Vec<Op>,
    constants: Vec<LispValue>,
    arity: usize,
    depth: usize,
}
impl Program {
    fn new(arity: usize) -> Self {
        Self {
            ops: Vec::new(),
            constants: Vec::new(),
            arity,
            depth: arity,
        }
    }
    fn arg(&mut self, index: usize) {
        assert!(index < self.arity);
        self.ops.push(Op::StackRef((self.depth - 1 - index) as u16));
        self.depth += 1;
    }
    fn constant(&mut self, value: LispValue) {
        let index = self.constants.len() as u16;
        self.constants.push(value);
        self.ops.push(Op::Constant(index));
        self.depth += 1;
    }
    fn unary(&mut self, op: Op) {
        self.ops.push(op);
    }
    fn binary(&mut self, op: Op) {
        self.ops.push(op);
        self.depth -= 1;
    }
    fn pop(&mut self) {
        self.ops.push(Op::Pop);
        self.depth -= 1;
    }
    fn read(&mut self, index: usize, cdr: bool) {
        self.arg(index);
        self.unary(if cdr { Op::Cdr } else { Op::Car });
    }
    fn store(&mut self, base: usize, value: usize, cdr: bool) {
        self.arg(base);
        self.arg(value);
        self.binary(if cdr { Op::Setcdr } else { Op::Setcar });
    }
    fn call(&mut self, nargs: u16) {
        self.ops.push(Op::Call(nargs));
        self.depth -= nargs as usize;
    }
    fn finish(mut self, count: u16) -> ByteCodeFunction {
        self.ops.extend([Op::List(count), Op::Return]);
        bytecode(self.ops, self.constants, self.arity)
    }
}

fn bytecode(ops: Vec<Op>, constants: Vec<LispValue>, arity: usize) -> ByteCodeFunction {
    let mut source = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|_| crate::emacs_core::intern::intern("opt-gvn-reference-arg"))
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

fn plan(source: &ByteCodeFunction, cons_args: &[usize]) -> Func {
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
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })
    .expect("source SSA");
    for inst in &func.insts {
        if let Opcode::Arg(index) = inst.op {
            if cons_args.contains(&(index as usize)) {
                func.values[inst.result.unwrap().index()].ty = TypeSet::CONS;
            }
        }
    }
    // Recheck existing successful guard/view declarations against the narrower
    // fixture contract. The original declarations/IDs and all guards remain.
    loop {
        let mut changed = false;
        for inst in &func.insts {
            if let Opcode::CheckType(target) | Opcode::Refine(target) = inst.op {
                let input = func.values[inst.args[0].index()].ty;
                let output = inst.result.unwrap();
                let narrowed = func.values[output.index()].ty.meet(input.meet(target));
                if narrowed != func.values[output.index()].ty {
                    func.values[output.index()].ty = narrowed;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Expose field reads after the retained LIST guard, as production Fold/GVN
    // does. A successful LIST read is total (nil returns nil), so its load has
    // no signal effect; the preceding CheckType keeps the ordered deopt/frame.
    for inst in &mut func.insts {
        let load = match &inst.op {
            Opcode::Opaque(Op::Car) => Some(Opcode::LoadCar),
            Opcode::Opaque(Op::Cdr) => Some(Opcode::LoadCdr),
            _ => None,
        };
        if let Some(load) = load {
            let input = &func.values[inst.args[0].index()];
            assert!(!input.ty.is_bottom() && input.ty.is_subset(TypeSet::LIST));
            assert_eq!(input.rep, Rep::Tagged);
            inst.op = load;
            inst.eff = Effects::READ_HEAP;
        }
    }
    func.verify()
        .expect("declared typed/opaque fixture verifies");
    func
}

fn reference(ctx: &mut Context, func: &Func, args: &[LispValue]) -> (LispValue, Run) {
    let run = evaluate(
        func,
        ctx,
        Inputs {
            args,
            ..Inputs::default()
        },
    )
    .expect("reference execution");
    let Outcome::Returned(answer) = &run.outcome else {
        panic!("unexpected reference deopt")
    };
    (answer.to_value(), run)
}

fn tier0(ctx: &mut Context, source: &ByteCodeFunction, args: &[LispValue]) -> LispValue {
    let _roots = Roots::new(args);
    let _constants = Roots::new(&source.constants);
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(source, args.to_vec())
        .expect("independent Tier-0 execution")
}

fn children(ctx: &mut Context) -> (LispValue, LispValue) {
    let cell = ctx
        .eval_str("(cons (list 'car-child) (list 'cdr-child))")
        .unwrap();
    let _cell = Roots::new(&[cell]);
    let replacement = ctx.eval_str("(list 'new-child)").unwrap();
    (cell, replacement)
}

fn items(mut value: LispValue) -> Vec<LispValue> {
    let mut out = Vec::new();
    while value.is_cons() {
        out.push(value.cons_car());
        value = value.cons_cdr();
    }
    assert!(value.is_nil());
    out
}

fn count(func: &Func, op: &Opcode) -> usize {
    func.insts.iter().filter(|inst| &inst.op == op).count()
}

fn observations(before: &Func, after: &Func) {
    assert_eq!(
        after.frames, before.frames,
        "original complete frames retained"
    );
    assert_eq!(after.entry_stacks, before.entry_stacks);
    assert_eq!(after.source_states.len(), before.source_states.len());
    for (actual, expected) in after.source_states.iter().zip(&before.source_states) {
        match (actual, expected) {
            (Some(actual), Some(expected)) => {
                assert_eq!(actual.pre, expected.pre);
                assert_eq!(actual.post, expected.post);
                assert_eq!(actual.frame, expected.frame);
                assert_eq!(actual.block, expected.block);
            }
            (None, None) => {}
            _ => panic!("source state added or removed"),
        }
    }
    assert_eq!(after.blocks.len(), before.blocks.len());
}

fn emit(
    func: &mut Func,
    block: Block,
    op: Opcode,
    args: Vec<Value>,
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
        args,
        result: Some(result),
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc: func.blocks[block.index()].pc,
    });
    func.blocks[block.index()].insts.push(id);
    result
}

#[test]
fn opt_gvn_reference_dominating_pure_reuse_matches_tier0() {
    let mut ctx = Context::new();
    let constants = [LispValue::fixnum(31), LispValue::fixnum(7)];
    let source = bytecode(
        vec![Op::Constant(0), Op::Constant(1), Op::Add, Op::Return],
        constants.to_vec(),
        0,
    );
    let expected = tier0(&mut ctx, &source, &[]);
    let mut func = Func::new(
        constants.map(ValueBits::from_value).into(),
        ParamShape::default(),
        0,
    );
    func.blocks = vec![BlockData::new(0), BlockData::new(1)];
    let a = emit(
        &mut func,
        Block(0),
        Opcode::Const(0),
        vec![],
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    let b = emit(
        &mut func,
        Block(0),
        Opcode::Const(1),
        vec![],
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    emit(
        &mut func,
        Block(0),
        Opcode::FixAdd { checked: false },
        vec![a, b],
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    func.blocks[0].term = Term::Jump(Edge {
        target: Block(1),
        args: vec![],
    });
    func.blocks[1].preds = vec![Block(0)];
    let duplicate = emit(
        &mut func,
        Block(1),
        Opcode::FixAdd { checked: false },
        vec![a, b],
        TypeSet::FIXNUM,
        Rep::TaggedFix,
    );
    func.blocks[1].term = Term::Return(duplicate);
    func.entry_stacks = vec![Box::new([]), Box::new([])];
    func.verify().expect("dominating typed arithmetic fixture");
    assert_eq!(reference(&mut ctx, &func, &[]).0, expected);
    let before = func.clone();
    let stats = gvn::run(&mut func).expect("GVN");
    func.verify().unwrap();
    assert_eq!(reference(&mut ctx, &func, &[]).0, expected);
    observations(&before, &func);
    assert_eq!(stats.pure_reuses, 1);
    assert_eq!(count(&func, &Opcode::FixAdd { checked: false }), 1);
}

#[test]
fn opt_gvn_reference_repeated_cons_loads_keep_child_identity_and_source_states() {
    let mut ctx = Context::new();
    let mut program = Program::new(1);
    for cdr in [false, false, true, true] {
        program.read(0, cdr);
    }
    let source = program.finish(4);
    let original = plan(&source, &[0]);
    let (cell, _) = children(&mut ctx);
    let _roots = Roots::new(&[cell]);
    let expected = tier0(&mut ctx, &source, &[cell]);
    let _expected = Roots::new(&[expected]);
    let baseline = reference(&mut ctx, &original, &[cell]).0;
    assert_eq!(items(baseline), items(expected));
    let mut candidate = original.clone();
    let stats = gvn::run(&mut candidate).unwrap();
    let (answer, trace) = reference(&mut ctx, &candidate, &[cell]);
    let _answer = Roots::new(&[answer]);
    assert_eq!(items(answer), items(expected));
    let values = items(answer);
    assert_eq!(values[0], values[1]);
    assert_eq!(values[2], values[3]);
    assert_eq!(values[0], cell.cons_car());
    assert_eq!(values[2], cell.cons_cdr());
    let (old_answer, old_run) = reference(&mut ctx, &original, &[cell]);
    let _old_answer = Roots::new(&[old_answer]);
    let return_pc = source.executable_ops().len() as u32 - 1;
    assert_eq!(trace.trace.len(), old_run.trace.len());
    for (actual, expected) in trace.trace.iter().zip(&old_run.trace) {
        assert_eq!(actual.pc, expected.pc);
        assert_eq!(actual.handlers, expected.handlers);
        assert_eq!(actual.binds, expected.binds);
        assert_eq!(actual.printed_stack, expected.printed_stack);
        assert_eq!(actual.stack.len(), expected.stack.len());
        if actual.pc == return_pc {
            // The source List creates a fresh return container on each run.
            // Only that exact slot differs; its child objects and every prior
            // snapshot must retain their original GNU identities.
            let last = actual.stack.len() - 1;
            assert_eq!(&actual.stack[..last], &expected.stack[..last]);
            assert_eq!(actual.stack[last], ValueBits::from_value(answer));
            assert_eq!(expected.stack[last], ValueBits::from_value(old_answer));
            assert_ne!(actual.stack[last], expected.stack[last]);
            assert_eq!(items(actual.stack[last].to_value()), values);
            assert_eq!(items(expected.stack[last].to_value()), values);
        } else {
            assert_eq!(actual.stack, expected.stack);
        }
    }
    observations(&original, &candidate);
    assert_eq!(stats.load_reuses, 2);
    assert_eq!(count(&candidate, &Opcode::LoadCar), 1);
    assert_eq!(count(&candidate, &Opcode::LoadCdr), 1);
}

#[test]
fn opt_gvn_reference_setters_remain_visible_and_forward_exact_heap_value() {
    let mut ctx = Context::new();
    for cdr in [false, true] {
        let mut program = Program::new(2);
        program.read(0, !cdr);
        program.store(0, 1, cdr);
        program.read(0, cdr);
        program.read(0, cdr);
        program.read(0, !cdr);
        let source = program.finish(5);
        let original = plan(&source, &[0]);
        let store_op = Opcode::Opaque(if cdr { Op::Setcdr } else { Op::Setcar });
        let load_op = if cdr {
            Opcode::LoadCdr
        } else {
            Opcode::LoadCar
        };
        let store = original
            .insts
            .iter()
            .find(|inst| inst.op == store_op)
            .unwrap()
            .clone();
        let (cell, replacement) = children(&mut ctx);
        let _roots = Roots::new(&[cell, replacement]);
        let untouched = if cdr {
            cell.cons_car()
        } else {
            cell.cons_cdr()
        };
        // The untouched field is invariant; each stage still performs the real store.
        let expected = items(tier0(&mut ctx, &source, &[cell, replacement]));
        assert_eq!(
            expected,
            vec![untouched, replacement, replacement, replacement, untouched]
        );
        let baseline = items(reference(&mut ctx, &original, &[cell, replacement]).0);
        assert_eq!(baseline, expected);
        let mut candidate = original.clone();
        let stats = gvn::run(&mut candidate).unwrap();
        assert_eq!(
            items(reference(&mut ctx, &candidate, &[cell, replacement]).0),
            expected
        );
        assert_eq!(
            if cdr {
                cell.cons_cdr()
            } else {
                cell.cons_car()
            },
            replacement
        );
        let retained = candidate
            .insts
            .iter()
            .find(|inst| inst.op == store_op)
            .unwrap();
        assert_eq!(retained.eff, store.eff);
        assert_eq!(retained.mem, store.mem);
        assert_eq!(retained.frame, store.frame);
        assert_eq!(retained.pc, store.pc);
        observations(&original, &candidate);
        assert_eq!(count(&candidate, &store_op), 1);
        assert_eq!(count(&candidate, &load_op), 0);
        assert!(stats.store_forwards >= 2);
        assert!(
            stats.load_reuses >= 1,
            "opposite field survives a precise store"
        );
    }
}

#[test]
fn opt_gvn_reference_possible_alias_store_kills_only_the_written_field() {
    let mut ctx = Context::new();
    for cdr in [false, true] {
        let mut program = Program::new(3);
        program.read(0, cdr);
        program.read(0, !cdr);
        program.store(1, 2, cdr);
        program.pop();
        program.read(0, cdr);
        program.read(0, !cdr);
        let source = program.finish(4);
        let original = plan(&source, &[0, 1]);
        // Establish actual baseline semantics for both possible runtime identities.
        for alias in [false, true] {
            let (cell, replacement) = children(&mut ctx);
            let _roots = Roots::new(&[cell, replacement]);
            let other = if alias {
                cell
            } else {
                LispValue::cons(cell.cons_car(), cell.cons_cdr())
            };
            let _other = Roots::new(&[other]);
            let old = if cdr {
                cell.cons_cdr()
            } else {
                cell.cons_car()
            };
            let untouched = if cdr {
                cell.cons_car()
            } else {
                cell.cons_cdr()
            };
            let _old = Roots::new(&[old, untouched]);
            let expected = vec![
                old,
                untouched,
                if alias { replacement } else { old },
                untouched,
            ];
            assert_eq!(
                items(tier0(&mut ctx, &source, &[cell, other, replacement])),
                expected
            );
            // Reset the mutated field without allocating an alternative value.
            if cdr {
                cell.set_cdr(old);
            } else {
                cell.set_car(old);
            }
            assert_eq!(
                items(reference(&mut ctx, &original, &[cell, other, replacement]).0),
                expected
            );
            if cdr {
                cell.set_cdr(old);
            } else {
                cell.set_car(old);
            }
            let mut candidate = original.clone();
            let stats = gvn::run(&mut candidate).unwrap();
            assert_eq!(
                items(reference(&mut ctx, &candidate, &[cell, other, replacement]).0),
                expected
            );
            observations(&original, &candidate);
            assert_eq!(
                stats.store_forwards, 0,
                "unknown SSA bases may alias but are not equal"
            );
            assert_eq!(
                stats.load_reuses, 1,
                "only the opposite field remains available"
            );
            assert_eq!(
                count(
                    &candidate,
                    &if cdr {
                        Opcode::LoadCdr
                    } else {
                        Opcode::LoadCar
                    }
                ),
                2
            );
        }
    }
}

#[test]
fn opt_gvn_reference_one_arm_store_cannot_escape_into_join_loads() {
    let mut ctx = Context::new();
    for cdr in [false, true] {
        let mut program = Program::new(3);
        program.read(0, cdr);
        program.arg(2);
        let branch = program.ops.len();
        program.ops.push(Op::GotoIfNil(0));
        program.depth -= 1;
        program.store(0, 1, cdr);
        program.pop();
        let join = program.ops.len() as u32;
        program.ops[branch] = Op::GotoIfNil(join);
        program.read(0, cdr);
        let source = program.finish(2);
        let original = plan(&source, &[0]);
        for choose in [LispValue::NIL, LispValue::T] {
            let (cell, replacement) = children(&mut ctx);
            let _roots = Roots::new(&[cell, replacement]);
            let old = if cdr {
                cell.cons_cdr()
            } else {
                cell.cons_car()
            };
            let _old = Roots::new(&[old]);
            let expected = vec![old, if choose.is_nil() { old } else { replacement }];
            assert_eq!(
                items(tier0(&mut ctx, &source, &[cell, replacement, choose])),
                expected
            );
            if cdr {
                cell.set_cdr(old);
            } else {
                cell.set_car(old);
            }
            assert_eq!(
                items(reference(&mut ctx, &original, &[cell, replacement, choose]).0),
                expected
            );
            if cdr {
                cell.set_cdr(old);
            } else {
                cell.set_car(old);
            }
            let mut candidate = original.clone();
            let stats = gvn::run(&mut candidate).unwrap();
            assert_eq!(
                items(reference(&mut ctx, &candidate, &[cell, replacement, choose]).0),
                expected
            );
            observations(&original, &candidate);
            assert_eq!(stats.load_reuses, 0);
            assert_eq!(stats.store_forwards, 0);
            assert_eq!(
                count(
                    &candidate,
                    &if cdr {
                        Opcode::LoadCdr
                    } else {
                        Opcode::LoadCar
                    }
                ),
                2
            );
        }
    }
}

#[test]
fn opt_gvn_reference_actual_call_mutation_invalidates_cons_loads() {
    let mut ctx = Context::new();
    for cdr in [false, true] {
        let callback = ctx
            .eval_str(if cdr {
                "(lambda (p x) (setcdr p x))"
            } else {
                "(lambda (p x) (setcar p x))"
            })
            .unwrap();
        let _callback = Roots::new(&[callback]);
        let mut program = Program::new(2);
        program.read(0, cdr);
        program.constant(callback);
        program.arg(0);
        program.arg(1);
        program.call(2);
        program.read(0, cdr);
        let source = program.finish(3);
        let original = plan(&source, &[0]);
        let (cell, replacement) = children(&mut ctx);
        let _roots = Roots::new(&[cell, replacement]);
        let old = if cdr {
            cell.cons_cdr()
        } else {
            cell.cons_car()
        };
        let _old = Roots::new(&[old]);
        let expected = vec![old, replacement, replacement];
        assert_eq!(
            items(tier0(&mut ctx, &source, &[cell, replacement])),
            expected
        );
        if cdr {
            cell.set_cdr(old);
        } else {
            cell.set_car(old);
        }
        assert_eq!(
            items(reference(&mut ctx, &original, &[cell, replacement]).0),
            expected
        );
        if cdr {
            cell.set_cdr(old);
        } else {
            cell.set_car(old);
        }
        let mut candidate = original.clone();
        let stats = gvn::run(&mut candidate).unwrap();
        assert_eq!(
            items(reference(&mut ctx, &candidate, &[cell, replacement]).0),
            expected
        );
        observations(&original, &candidate);
        assert_eq!(stats.load_reuses, 0);
        assert_eq!(stats.store_forwards, 0);
        assert_eq!(
            count(
                &candidate,
                &if cdr {
                    Opcode::LoadCdr
                } else {
                    Opcode::LoadCar
                }
            ),
            2
        );
    }
}

fn gc_fixture(ctx: &mut Context) -> (LispValue, LispValue) {
    ctx.set_gc_threshold(usize::MAX);
    ctx.eval_str(
        "(setq opt-gvn-gc-cell (cons (list 'car-child) (list 'cdr-child))
                        opt-gvn-gc-new (list 'new-child)
                        opt-gvn-gc-count 0
                        post-gc-hook (list (lambda ()
                          (setq opt-gvn-gc-count (1+ opt-gvn-gc-count))
                          (setcar opt-gvn-gc-cell opt-gvn-gc-new))))",
    )
    .unwrap();
    let cell = ctx.obarray.symbol_value_copied("opt-gvn-gc-cell").unwrap();
    let new = ctx.obarray.symbol_value_copied("opt-gvn-gc-new").unwrap();
    (cell, new)
}

#[test]
fn opt_gvn_reference_gc_call_keeps_eliminated_child_alias_live() {
    let mut ctx = Context::new();
    let collect = ctx.eval_str("(lambda () (garbage-collect) nil)").unwrap();
    let _collect = Roots::new(&[collect]);
    let mut program = Program::new(1);
    program.read(0, false);
    program.read(0, false);
    program.constant(collect);
    program.call(0);
    program.pop();
    program.read(0, false);
    let source = program.finish(3);
    let original = plan(&source, &[0]);
    for use_reference in [false, true] {
        let (cell, new) = gc_fixture(&mut ctx);
        let old = cell.cons_car(); // Deliberately no external scratch root for this child.
        let old_gc = ctx.gc_count;
        let answer = if use_reference {
            reference(&mut ctx, &original, &[cell]).0
        } else {
            tier0(&mut ctx, &source, &[cell])
        };
        let _answer = Roots::new(&[answer]);
        assert_eq!(ctx.gc_count - old_gc, 1);
        assert_eq!(
            ctx.obarray.symbol_value_copied("opt-gvn-gc-count"),
            Some(LispValue::fixnum(1))
        );
        assert_eq!(items(answer), vec![old, old, new]);
        assert_eq!(
            crate::emacs_core::print::print_value(&items(answer)[0]),
            "(car-child)"
        );
    }
    let mut candidate = original.clone();
    let stats = gvn::run(&mut candidate).unwrap();
    let (cell, new) = gc_fixture(&mut ctx);
    let old = cell.cons_car();
    let old_gc = ctx.gc_count;
    let answer = reference(&mut ctx, &candidate, &[cell]).0;
    let _answer = Roots::new(&[answer]);
    assert_eq!(ctx.gc_count - old_gc, 1);
    assert_eq!(
        ctx.obarray.symbol_value_copied("opt-gvn-gc-count"),
        Some(LispValue::fixnum(1))
    );
    assert_eq!(items(answer), vec![old, old, new]);
    assert_eq!(
        crate::emacs_core::print::print_value(&items(answer)[0]),
        "(car-child)"
    );
    observations(&original, &candidate);
    assert_eq!(stats.load_reuses, 1);
    assert_eq!(count(&candidate, &Opcode::LoadCar), 2);
}

#[test]
fn opt_gvn_reference_poll_gc_hook_mutation_invalidates_available_load() {
    let mut ctx = Context::new();
    // The independent Tier-0 control reaches the same real post-GC hook.
    let collect = ctx.eval_str("(lambda () (garbage-collect) nil)").unwrap();
    let _collect = Roots::new(&[collect]);
    let mut control = Program::new(1);
    control.read(0, false);
    control.constant(collect);
    control.call(0);
    control.pop();
    control.read(0, false);
    let control = control.finish(2);
    let (cell, new) = gc_fixture(&mut ctx);
    let old = cell.cons_car();
    let expected = tier0(&mut ctx, &control, &[cell]);
    let _expected = Roots::new(&[expected]);
    assert_eq!(items(expected), vec![old, new]);
    let mut program = Program::new(1);
    program.read(0, false);
    program.read(0, false);
    let source = program.finish(2);
    let mut original = plan(&source, &[0]);
    let second = original
        .insts
        .iter()
        .enumerate()
        .filter(|(_, inst)| inst.op == Opcode::LoadCar)
        .nth(1)
        .map(|(index, _)| Inst(index as u32))
        .unwrap();
    let data = original.insts[second.index()].clone();
    let block = original
        .blocks
        .iter()
        .position(|block| block.insts.contains(&second))
        .unwrap();
    let position = original.blocks[block]
        .insts
        .iter()
        .position(|&id| id == second)
        .unwrap();
    let mut polls = Vec::new();
    for _ in 0..255 {
        let id = Inst(original.insts.len() as u32);
        original.insts.push(InstData {
            op: Opcode::Poll,
            args: vec![],
            result: None,
            eff: Effects::UNKNOWN,
            mem: AliasClass::Unknown,
            frame: data.frame,
            pc: data.pc,
        });
        polls.push(id);
    }
    original.blocks[block]
        .insts
        .splice(position..position, polls);
    original
        .verify()
        .expect("255 actual Poll boundaries preserve full frame");
    let (cell, new) = gc_fixture(&mut ctx);
    let old = cell.cons_car();
    let baseline_gc = ctx.gc_count;
    ctx.set_gc_threshold(1);
    // Stress makes this real Poll complete its collection synchronously.
    // Threshold-only GC may start an incremental cycle without running the
    // post-GC mutation hook before the following read. Keep the override within
    // this invocation, not fixture setup or any later baseline.
    let previous_stress = ctx.gc_stress;
    ctx.gc_stress = true;
    crate::emacs_core::eval::reset_bytecode_branch_poll_count();
    let baseline = reference(&mut ctx, &original, &[cell]).0;
    ctx.gc_stress = previous_stress;
    let _baseline = Roots::new(&[baseline]);
    assert_eq!(items(baseline), vec![old, new]);
    assert_eq!(crate::emacs_core::eval::bytecode_branch_poll_count(), 1);
    assert_eq!(ctx.gc_count - baseline_gc, 1);
    assert_eq!(
        ctx.obarray.symbol_value_copied("opt-gvn-gc-count"),
        Some(LispValue::fixnum(1))
    );
    let mut candidate = original.clone();
    let stats = gvn::run(&mut candidate).unwrap();
    let (cell, new) = gc_fixture(&mut ctx);
    let old = cell.cons_car();
    let old_gc = ctx.gc_count;
    ctx.set_gc_threshold(1);
    let previous_stress = ctx.gc_stress;
    ctx.gc_stress = true;
    crate::emacs_core::eval::reset_bytecode_branch_poll_count();
    let answer = reference(&mut ctx, &candidate, &[cell]).0;
    ctx.gc_stress = previous_stress;
    let _answer = Roots::new(&[answer]);
    assert_eq!(ctx.gc_count - old_gc, 1);
    assert_eq!(
        ctx.obarray.symbol_value_copied("opt-gvn-gc-count"),
        Some(LispValue::fixnum(1))
    );
    assert_eq!(items(answer), vec![old, new]);
    assert_eq!(crate::emacs_core::eval::bytecode_branch_poll_count(), 1);
    observations(&original, &candidate);
    assert_eq!(stats.load_reuses, 0);
    assert_eq!(count(&candidate, &Opcode::Poll), 255);
    assert_eq!(count(&candidate, &Opcode::LoadCar), 2);
}

#[test]
fn opt_gvn_reference_fresh_float_and_dynamic_positioned_eq_are_excluded() {
    let mut ctx = Context::new();
    let constants = [LispValue::make_float(1.25), LispValue::make_float(2.0)];
    let _constants = Roots::new(&constants);
    let source = bytecode(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Mul,
            Op::Constant(0),
            Op::Constant(1),
            Op::Mul,
            Op::Eq,
            Op::Return,
        ],
        constants.to_vec(),
        0,
    );
    let original = plan(&source, &[]);
    assert!(
        tier0(&mut ctx, &source, &[]).is_nil(),
        "GNU fresh-allocation identity"
    );
    assert!(reference(&mut ctx, &original, &[]).0.is_nil());
    let mut candidate = original.clone();
    gvn::run(&mut candidate).unwrap();
    assert!(reference(&mut ctx, &candidate, &[]).0.is_nil());
    assert_eq!(count(&candidate, &Opcode::Opaque(Op::Mul)), 2);
    assert_eq!(count(&candidate, &Opcode::Opaque(Op::Eq)), 1);

    let mut typed = Func::new(
        constants.map(ValueBits::from_value).into(),
        ParamShape::default(),
        0,
    );
    typed.blocks.push(BlockData::new(0));
    let a = emit(
        &mut typed,
        Block(0),
        Opcode::Const(0),
        vec![],
        TypeSet::FLOAT,
        Rep::Tagged,
    );
    let b = emit(
        &mut typed,
        Block(0),
        Opcode::Const(1),
        vec![],
        TypeSet::FLOAT,
        Rep::Tagged,
    );
    let raw_a = emit(
        &mut typed,
        Block(0),
        Opcode::LoadF64,
        vec![a],
        TypeSet::FLOAT,
        Rep::RawF64,
    );
    let raw_b = emit(
        &mut typed,
        Block(0),
        Opcode::LoadF64,
        vec![b],
        TypeSet::FLOAT,
        Rep::RawF64,
    );
    let product = emit(
        &mut typed,
        Block(0),
        Opcode::F64Mul,
        vec![raw_a, raw_b],
        TypeSet::FLOAT,
        Rep::RawF64,
    );
    let left = emit(
        &mut typed,
        Block(0),
        Opcode::AllocFloat,
        vec![product],
        TypeSet::FLOAT,
        Rep::Tagged,
    );
    let right = emit(
        &mut typed,
        Block(0),
        Opcode::AllocFloat,
        vec![product],
        TypeSet::FLOAT,
        Rep::Tagged,
    );
    for value in [left, right] {
        let ValueDef::Inst(id) = typed.values[value.index()].def else {
            unreachable!()
        };
        typed.insts[id.index()].eff = Effects::ALLOCATES;
    }
    let flag = emit(
        &mut typed,
        Block(0),
        Opcode::Eq,
        vec![left, right],
        TypeSet::NIL.join(TypeSet::T),
        Rep::Bool,
    );
    let answer = emit(
        &mut typed,
        Block(0),
        Opcode::BoolToLisp,
        vec![flag],
        TypeSet::NIL.join(TypeSet::T),
        Rep::Tagged,
    );
    typed.blocks[0].term = Term::Return(answer);
    typed.entry_stacks = vec![Box::new([])];
    typed.verify().expect("two distinct typed allocations");
    assert!(reference(&mut ctx, &typed, &[]).0.is_nil());
    let before = typed.clone();
    gvn::run(&mut typed).unwrap();
    assert!(reference(&mut ctx, &typed, &[]).0.is_nil());
    assert_eq!(count(&typed, &Opcode::AllocFloat), 2);
    assert_eq!(count(&typed, &Opcode::Eq), 1);
    observations(&before, &typed);

    let bare = LispValue::symbol("opt-gvn-positioned");
    let positioned = ctx
        .eval_str("(position-symbol 'opt-gvn-positioned 7)")
        .unwrap();
    let _positioned = Roots::new(&[positioned]);
    let source = bytecode(
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Eq,
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Eq,
            Op::List(2),
            Op::Return,
        ],
        vec![],
        2,
    );
    let mut original = plan(&source, &[]);
    // Boolean lifting retains original tagged result views and Tier-0 Eq.
    super::passes::bools::run(&mut original).unwrap();
    let mut candidate = original.clone();
    gvn::run(&mut candidate).unwrap();
    for enabled in [false, true] {
        ctx.symbols_with_pos_enabled = enabled;
        let expected = if enabled {
            LispValue::T
        } else {
            LispValue::NIL
        };
        assert_eq!(
            items(tier0(&mut ctx, &source, &[bare, positioned])),
            vec![expected, expected]
        );
        assert_eq!(
            items(reference(&mut ctx, &original, &[bare, positioned]).0),
            vec![expected, expected]
        );
        assert_eq!(
            items(reference(&mut ctx, &candidate, &[bare, positioned]).0),
            vec![expected, expected]
        );
    }
    assert_eq!(count(&candidate, &Opcode::OpaqueBool(Op::Eq)), 2);
    ctx.symbols_with_pos_enabled = false;
}

#[test]
fn opt_gvn_reference_forwarded_values_reconstruct_full_cold_frame_after_store() {
    let mut ctx = Context::new();
    let mut program = Program::new(3);
    program.store(0, 1, false);
    program.pop();
    program.read(0, false);
    program.read(0, false);
    program.arg(2);
    let failing_pc = program.ops.len() as u32;
    program.unary(Op::Car);
    let source = program.finish(3);
    let original = plan(&source, &[0]);
    let unknown = original
        .insts
        .iter()
        .find_map(|inst| match inst.op {
            Opcode::Arg(2) => inst.result,
            _ => None,
        })
        .unwrap();
    assert_eq!(original.values[unknown.index()].ty, TypeSet::TOP);
    let failing_read = original
        .insts
        .iter()
        .find(|inst| inst.pc == failing_pc && inst.op == Opcode::LoadCar)
        .unwrap();
    assert!(
        !original.values[failing_read.args[0].index()]
            .ty
            .is_subset(TypeSet::CONS),
        "the failing read has no declared CONS proof"
    );
    let (cell, replacement) = children(&mut ctx);
    let bad = LispValue::symbol("wrong");
    let args = [cell, replacement, bad];
    let _roots = Roots::new(&args);
    let flow = {
        let mut vm = Vm::from_context(&mut ctx);
        vm.force_interpreter_only_for_test();
        vm.execute(&source, args.to_vec())
            .expect_err("actual Tier-0 wrong-list signal")
    };
    let signal = flow.as_signal().expect("GNU wrong-type flow");
    assert_eq!(signal.symbol_name(), "wrong-type-argument");
    assert_eq!(signal.data, vec![LispValue::symbol("listp"), bad]);
    assert_eq!(
        cell.cons_car(),
        replacement,
        "prior store is already visible"
    );
    let run = evaluate(
        &original,
        &mut ctx,
        Inputs {
            args: &args,
            ..Inputs::default()
        },
    )
    .unwrap();
    let Outcome::Deopt(expected) = run.outcome else {
        panic!("explicit source LIST guard must fail")
    };
    assert_eq!(expected.pc, failing_pc);
    assert_eq!(
        expected.stack,
        [cell, replacement, bad, replacement, replacement, bad].map(ValueBits::from_value)
    );
    let mut candidate = original.clone();
    let stats = gvn::run(&mut candidate).unwrap();
    let run = evaluate(
        &candidate,
        &mut ctx,
        Inputs {
            args: &args,
            ..Inputs::default()
        },
    )
    .unwrap();
    let Outcome::Deopt(actual) = run.outcome else {
        panic!("same explicit cold source guard")
    };
    assert_eq!(actual, expected);
    assert_eq!(cell.cons_car(), replacement);
    observations(&original, &candidate);
    assert_eq!(count(&candidate, &Opcode::Opaque(Op::Setcar)), 1);
    assert!(stats.store_forwards >= 2);
    assert_eq!(
        count(&candidate, &Opcode::LoadCar),
        1,
        "unproven failing read remains"
    );
}

// Append to opt/tests/gvn_eval_test.rs after the active isolated run finishes.
// Existing private Program/plan/Roots/items/observations helpers are intentional.
// Current verifier accepts these hints; the opcode still executes a real GNU
// setter through the reference evaluator and must invalidate its mutable field.

#[test]
fn opt_gvn_reference_typed_setter_nonwrite_hints_invalidate_mutable_field() {
    let mut ctx = Context::new();
    for cdr in [false, true] {
        for hint in [Effects::PURE, Effects::READ_HEAP] {
            let mut program = Program::new(2);
            program.read(0, cdr);
            program.store(0, 1, cdr);
            program.read(0, cdr);
            let source = program.finish(3);
            let mut original = plan(&source, &[0]);
            let opaque_store = Opcode::Opaque(if cdr { Op::Setcdr } else { Op::Setcar });
            let setter = original
                .insts
                .iter()
                .position(|inst| inst.op == opaque_store)
                .unwrap();
            original.insts[setter].op = if cdr {
                Opcode::StoreCdr
            } else {
                Opcode::StoreCar
            };
            original.insts[setter].eff = hint;
            // Result/rep/type, precise field alias, original PC and full frame
            // remain those of the actual source setter. Only its hint differs.
            let retained = original.insts[setter].clone();
            let result = retained.result.unwrap();
            let result_data = original.values[result.index()].clone();
            original
                .verify()
                .expect("hinted typed setter is accepted IR");

            let (cell, replacement) = children(&mut ctx);
            let _roots = Roots::new(&[cell, replacement]);
            let old = if cdr {
                cell.cons_cdr()
            } else {
                cell.cons_car()
            };
            let _old = Roots::new(&[old]);
            let expected = vec![old, replacement, replacement];
            assert_eq!(
                items(tier0(&mut ctx, &source, &[cell, replacement])),
                expected
            );
            if cdr {
                cell.set_cdr(old);
            } else {
                cell.set_car(old);
            }
            let baseline = reference(&mut ctx, &original, &[cell, replacement]).0;
            let _baseline = Roots::new(&[baseline]);
            assert_eq!(items(baseline), expected);
            assert_eq!(
                if cdr {
                    cell.cons_cdr()
                } else {
                    cell.cons_car()
                },
                replacement,
                "the hinted opcode still performs the actual setter"
            );
            if cdr {
                cell.set_cdr(old);
            } else {
                cell.set_car(old);
            }

            let mut candidate = original.clone();
            let stats = gvn::run(&mut candidate).unwrap();
            candidate
                .verify()
                .expect("transformed hinted setter remains verified");
            let answer = reference(&mut ctx, &candidate, &[cell, replacement]).0;
            let _answer = Roots::new(&[answer]);
            assert_eq!(
                items(answer),
                expected,
                "post-store load must observe the changed field: cdr={cdr}, hint={hint:?}"
            );
            assert_eq!(
                if cdr {
                    cell.cons_cdr()
                } else {
                    cell.cons_car()
                },
                replacement
            );
            assert_eq!(stats.load_reuses, 0);
            assert_eq!(
                stats.store_forwards, 0,
                "nonstandard hint uses conservative fallback"
            );
            assert_eq!(
                count(
                    &candidate,
                    &if cdr {
                        Opcode::LoadCdr
                    } else {
                        Opcode::LoadCar
                    }
                ),
                2
            );
            let after = &candidate.insts[setter];
            assert_eq!(after.op, retained.op);
            assert_eq!(after.args, retained.args);
            assert_eq!(after.result, retained.result);
            assert_eq!(after.eff, retained.eff);
            assert_eq!(after.mem, retained.mem);
            assert_eq!(after.frame, retained.frame);
            assert_eq!(after.pc, retained.pc);
            assert_eq!(candidate.values[result.index()].def, result_data.def);
            assert_eq!(candidate.values[result.index()].rep, result_data.rep);
            assert_eq!(candidate.values[result.index()].ty, result_data.ty);
            observations(&original, &candidate);
        }
    }
}

// GNU data.c:659-712 defines car/cdr nil, cons and wrong-type behavior. The
// independent Tier-0 source is executed before the missing-transform assertion.

fn list_guard_signal(
    flow: &crate::emacs_core::error::Flow,
) -> (
    crate::emacs_core::intern::SymId,
    Vec<String>,
    Option<String>,
) {
    let signal = flow.as_signal().expect("list read has a GNU signal");
    (
        signal.symbol,
        signal
            .data
            .iter()
            .map(crate::emacs_core::print::print_value)
            .collect(),
        signal
            .raw_data
            .as_ref()
            .map(crate::emacs_core::print::print_value),
    )
}

fn list_guard_tier0(
    ctx: &mut Context,
    source: &ByteCodeFunction,
    args: &[LispValue],
) -> Result<
    Vec<LispValue>,
    (
        crate::emacs_core::intern::SymId,
        Vec<String>,
        Option<String>,
    ),
> {
    let _args = Roots::new(args);
    let _constants = Roots::new(&source.constants);
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    match vm.execute(source, args.to_vec()) {
        Ok(value) => Ok(items(value)),
        Err(flow) => Err(list_guard_signal(&flow)),
    }
}

fn list_guard_reference(
    ctx: &mut Context,
    source: &ByteCodeFunction,
    func: &Func,
    args: &[LispValue],
) -> (
    Result<
        Vec<LispValue>,
        (
            crate::emacs_core::intern::SymId,
            Vec<String>,
            Option<String>,
        ),
    >,
    Option<super::eval::Snapshot>,
) {
    let _args = Roots::new(args);
    let _constants = Roots::new(&source.constants);
    let run = match evaluate(
        func,
        ctx,
        Inputs {
            args,
            ..Inputs::default()
        },
    ) {
        Ok(run) => run,
        Err(super::eval::EvalError::Flow(flow)) => return (Err(list_guard_signal(&flow)), None),
        Err(other) => panic!("verified LIST fixture failed: {other:?}"),
    };
    match run.outcome {
        Outcome::Returned(bits) => (Ok(items(bits.to_value())), None),
        Outcome::Deopt(snapshot) => {
            assert_eq!(snapshot.handlers, 0);
            assert_eq!(snapshot.binds, 0);
            let stack = snapshot
                .stack
                .iter()
                .map(|bits| bits.to_value())
                .collect::<Vec<_>>();
            let _stack = Roots::new(&stack);
            let spec = ctx.specpdl.len();
            let condition = ctx.condition_stack_depth_for_test();
            // execute() seals hand-assembled test ops before entry; resume
            // expects that marker already installed. Preserve the exact PCs.
            let mut sealed = source.clone();
            sealed.seal_hand_assembled_ops_for_test();
            assert_eq!(sealed.executable_ops(), source.executable_ops());
            let mut vm = Vm::from_context(ctx);
            vm.force_interpreter_only_for_test();
            let result = vm.run_resumed_frame(
                &sealed,
                LispValue::NIL,
                snapshot.pc as usize,
                &stack,
                0,
                &[],
                spec,
                condition,
            );
            let result = match result {
                Ok(value) => Ok(items(value)),
                Err(flow) => Err(list_guard_signal(&flow)),
            };
            (result, Some(snapshot))
        }
    }
}

#[test]
fn opt_gvn_reference_guarded_list_read_between_cons_cdr_retains_nil_and_error_order() {
    let mut ctx = Context::new();
    let (cell, _) = children(&mut ctx);
    let _cell = Roots::new(&[cell]);
    let list = ctx.eval_str("(list 'list-head 'list-tail)").unwrap();
    let _list = Roots::new(&[list]);
    let marker = LispValue::symbol("before-list-read");
    let side = LispValue::symbol("opt-gvn-list-side");
    for middle in [Op::Car, Op::Cdr] {
        let source = bytecode(
            vec![
                Op::Constant(0),
                Op::VarSet(1),
                Op::StackRef(1),
                Op::Cdr,
                Op::StackRef(1),
                middle.clone(),
                Op::Pop,
                Op::StackRef(2),
                Op::Cdr,
                Op::List(2),
                Op::Return,
            ],
            vec![marker, side],
            2,
        );
        let mut original = plan(&source, &[0]);
        let read = Inst(
            original
                .insts
                .iter()
                .position(|inst| {
                    inst.pc == 5 && matches!(inst.op, Opcode::LoadCar | Opcode::LoadCdr)
                })
                .unwrap() as u32,
        );
        // Retain the builder's opaque LIST read, conservative exact effect
        // hint and real unknown-input LIST guard. Only arg0 is contracted CONS.
        original.insts[read.index()].op = Opcode::Opaque(middle.clone());
        original.insts[read.index()].eff = super::build::op_effects(&middle).0;
        assert_eq!(
            original.insts[read.index()].eff,
            super::build::op_effects(&middle).0
        );
        let checked = original.insts[read.index()].args[0];
        assert_eq!(original.values[checked.index()].ty, TypeSet::LIST);
        assert_eq!(original.values[checked.index()].rep, Rep::Tagged);
        original
            .verify()
            .expect("guarded LIST read between repeated CONS cdr");
        let cases = [LispValue::NIL, list, LispValue::fixnum(17)];
        let mut baselines = Vec::new();
        for second in cases {
            let args = [cell, second];
            ctx.eval_str("(setq opt-gvn-list-side nil)").unwrap();
            let expected = list_guard_tier0(&mut ctx, &source, &args);
            assert_eq!(
                ctx.obarray.symbol_value_copied("opt-gvn-list-side"),
                Some(marker)
            );
            ctx.eval_str("(setq opt-gvn-list-side nil)").unwrap();
            let baseline = list_guard_reference(&mut ctx, &source, &original, &args);
            assert_eq!(baseline.0, expected);
            assert_eq!(
                ctx.obarray.symbol_value_copied("opt-gvn-list-side"),
                Some(marker)
            );
            if second.is_fixnum() {
                let snapshot = baseline.1.as_ref().expect("original non-cons guard fails");
                assert_eq!(snapshot.pc, 5);
                assert_eq!(
                    snapshot.stack,
                    [cell, second, cell.cons_cdr(), second].map(ValueBits::from_value)
                );
            } else {
                assert_eq!(baseline.0, Ok(vec![cell.cons_cdr(); 2]));
                assert!(baseline.1.is_none());
                let run = evaluate(
                    &original,
                    &mut ctx,
                    Inputs {
                        args: &args,
                        ..Inputs::default()
                    },
                )
                .unwrap();
                let expected_read = if second.is_nil() {
                    LispValue::NIL
                } else if middle == Op::Car {
                    second.cons_car()
                } else {
                    second.cons_cdr()
                };
                assert_eq!(
                    run.trace
                        .iter()
                        .find(|snapshot| snapshot.pc == 6)
                        .unwrap()
                        .stack
                        .last(),
                    Some(&ValueBits::from_value(expected_read)),
                    "the primitive result retains exact Nil/child identity"
                );
            }
            baselines.push(baseline);
        }
        let mut candidate = original.clone();
        let stats = gvn::run(&mut candidate).unwrap();
        candidate.verify().unwrap();
        for (second, expected) in cases.into_iter().zip(baselines) {
            ctx.eval_str("(setq opt-gvn-list-side nil)").unwrap();
            assert_eq!(
                list_guard_reference(&mut ctx, &source, &candidate, &[cell, second]),
                expected
            );
            assert_eq!(
                ctx.obarray.symbol_value_copied("opt-gvn-list-side"),
                Some(marker)
            );
            if !second.is_fixnum() {
                let run = evaluate(
                    &candidate,
                    &mut ctx,
                    Inputs {
                        args: &[cell, second],
                        ..Inputs::default()
                    },
                )
                .unwrap();
                let expected_read = if second.is_nil() {
                    LispValue::NIL
                } else if middle == Op::Car {
                    second.cons_car()
                } else {
                    second.cons_cdr()
                };
                assert_eq!(
                    run.trace
                        .iter()
                        .find(|snapshot| snapshot.pc == 6)
                        .unwrap()
                        .stack
                        .last(),
                    Some(&ValueBits::from_value(expected_read))
                );
            }
        }
        observations(&original, &candidate);
        assert_eq!(
            stats.load_reuses, 1,
            "LIST read is read-only between the CONS cdr pair"
        );
        assert_eq!(
            candidate.insts[read.index()].op,
            if middle == Op::Car {
                Opcode::LoadCar
            } else {
                Opcode::LoadCdr
            }
        );
        assert_eq!(
            candidate.insts[read.index()].result,
            original.insts[read.index()].result
        );
        assert_eq!(
            candidate.insts[read.index()].frame,
            original.insts[read.index()].frame
        );
        assert_eq!(
            candidate.insts[read.index()].pc,
            original.insts[read.index()].pc
        );
    }

    // The LIST read becomes read-only, while an actual possibly-aliasing
    // setter retains its field-specific barrier. Equal runtime pointers are
    // not a proof that distinct SSA Arg identities are the same address.
    for cdr_write in [false, true] {
        let mut program = Program::new(4);
        program.read(0, true);
        program.read(1, false);
        program.pop();
        program.store(2, 3, cdr_write);
        program.pop();
        program.read(0, true);
        let source = program.finish(2);
        let mut original = plan(&source, &[0, 2]);
        let middle = Inst(
            original
                .insts
                .iter()
                .position(|inst| inst.pc == 3 && inst.op == Opcode::LoadCar)
                .unwrap() as u32,
        );
        original.insts[middle.index()].op = Opcode::Opaque(Op::Car);
        original.insts[middle.index()].eff = super::build::op_effects(&Op::Car).0;
        original.verify().unwrap();
        let store = original
            .insts
            .iter()
            .position(|inst| {
                inst.op == Opcode::Opaque(if cdr_write { Op::Setcdr } else { Op::Setcar })
            })
            .unwrap();
        let original_store = original.insts[store].clone();
        let mut cases = Vec::new();
        let _case_roots = Roots::new(&[]);
        for aliases in [false, true] {
            let (base, replacement) = children(&mut ctx);
            push_scratch_gc_roots(&[base, replacement]);
            let other = if aliases {
                base
            } else {
                LispValue::cons(base.cons_car(), base.cons_cdr())
            };
            push_scratch_gc_roots(&[other]);
            let old_car = other.cons_car();
            let old_cdr = other.cons_cdr();
            let old = base.cons_cdr();
            push_scratch_gc_roots(&[old_car, old_cdr, old]);
            let args = [base, LispValue::NIL, other, replacement];
            let expected = list_guard_tier0(&mut ctx, &source, &args);
            assert_eq!(
                if cdr_write {
                    other.cons_cdr()
                } else {
                    other.cons_car()
                },
                replacement
            );
            assert_eq!(
                expected,
                Ok(vec![
                    old,
                    if cdr_write && aliases {
                        replacement
                    } else {
                        old
                    }
                ])
            );
            other.set_car(old_car);
            other.set_cdr(old_cdr);
            let baseline = list_guard_reference(&mut ctx, &source, &original, &args);
            assert_eq!(
                if cdr_write {
                    other.cons_cdr()
                } else {
                    other.cons_car()
                },
                replacement
            );
            assert_eq!(baseline.0, expected);
            assert!(baseline.1.is_none());
            other.set_car(old_car);
            other.set_cdr(old_cdr);
            // The one outer case-root scope owns every case until replay.
            cases.push((args, old_car, old_cdr, expected));
        }
        let mut candidate = original.clone();
        let stats = gvn::run(&mut candidate).unwrap();
        candidate.verify().unwrap();
        for (args, old_car, old_cdr, expected) in cases {
            assert_eq!(
                list_guard_reference(&mut ctx, &source, &candidate, &args).0,
                expected
            );
            assert_eq!(
                if cdr_write {
                    args[2].cons_cdr()
                } else {
                    args[2].cons_car()
                },
                args[3]
            );
            args[2].set_car(old_car);
            args[2].set_cdr(old_cdr);
        }
        observations(&original, &candidate);
        let retained = &candidate.insts[store];
        assert_eq!(retained.op, original_store.op);
        assert_eq!(retained.args, original_store.args);
        assert_eq!(retained.result, original_store.result);
        assert_eq!(retained.eff, original_store.eff);
        assert_eq!(retained.mem, original_store.mem);
        assert_eq!(retained.frame, original_store.frame);
        assert_eq!(retained.pc, original_store.pc);
        assert_eq!(
            stats.store_forwards, 0,
            "different SSA bases do not forward even when runtime pointers coincide"
        );
        assert_eq!(
            stats.load_reuses,
            usize::from(!cdr_write),
            "same-field kill; opposite-field reuse"
        );
        assert_eq!(
            count(
                &candidate,
                &Opcode::Opaque(if cdr_write { Op::Setcdr } else { Op::Setcar })
            ),
            1,
            "the real setter remains executable"
        );
    }

    // Safe TOP input has no declared LIST proof. It stays opaque, even though
    // the successful nil/cons cases above can use the guarded typed adapter.
    let source = bytecode(
        vec![
            Op::StackRef(1),
            Op::Cdr,
            Op::StackRef(1),
            Op::CarSafe,
            Op::Pop,
            Op::StackRef(2),
            Op::Cdr,
            Op::List(2),
            Op::Return,
        ],
        vec![],
        2,
    );
    let original = plan(&source, &[0]);
    original.verify().unwrap();
    let args = [cell, LispValue::fixnum(17)];
    assert_eq!(
        list_guard_reference(&mut ctx, &source, &original, &args).0,
        list_guard_tier0(&mut ctx, &source, &args)
    );
    let mut candidate = original.clone();
    let stats = gvn::run(&mut candidate).unwrap();
    candidate.verify().unwrap();
    assert_eq!(stats.load_reuses, 0);
    assert_eq!(count(&candidate, &Opcode::Opaque(Op::CarSafe)), 1);
    assert_eq!(
        list_guard_reference(&mut ctx, &source, &candidate, &args),
        list_guard_reference(&mut ctx, &source, &original, &args)
    );
}
