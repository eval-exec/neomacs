//! These tests change the verified IR while retaining the original bytecode.
//! Native behavior must follow the IR. Threading: plans, contexts and roots
//! belong to one test invocation; overrides contain compiler settings only.

use crate::emacs_core::jit::compile::opt_census::SelectedTier;

use super::compile_pipeline_tests::{captured_clif, function};
use super::*;
use crate::emacs_core::eval::{
    bytecode_branch_poll_count, push_scratch_gc_roots, reset_bytecode_branch_poll_count,
    restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::opt::types::TypeSet;
use crate::emacs_core::jit::opt::{build, eval, ir};

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses::default()));
        force_deopt_for_test(false);
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
        force_deopt_for_test(false);
    }
}

struct TailRoots;
impl TailRoots {
    fn enter() -> Self {
        force_tail_unrooted_for_test(Some(true));
        Self
    }
}
impl Drop for TailRoots {
    fn drop(&mut self) {
        force_tail_unrooted_for_test(None);
    }
}

/// Keeps all test inputs on the owning mutator's existing root stack.
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

fn plan(f: &ByteCodeFunction) -> ir::Func {
    let arity = f.params.required.len();
    let cfg = analyze_cfg(
        f.executable_ops(),
        &f.constants,
        f.executable_gnu_byte_offset_map(),
        arity,
    )
    .unwrap();
    let constants = f
        .constants
        .iter()
        .copied()
        .map(ir::ValueBits::from_value)
        .collect::<Vec<_>>();
    build::build(build::BuildInput {
        ops: f.executable_ops(),
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

fn lower(f: &ByteCodeFunction, plan: &ir::Func) -> CompiledLeaf {
    plan.verify().expect("the changed IR verifies");
    let leaf = lower_opt_ir_for_test(
        f.executable_ops(),
        &f.constants,
        f.params.required.len(),
        f.executable_gnu_byte_offset_map(),
        plan,
    )
    .expect("native IR lowering");
    assert_eq!(leaf.selected_tier(), SelectedTier::Opt);
    leaf
}

fn tier0(ctx: &mut Context, f: &ByteCodeFunction, args: &[Value]) -> Value {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(f, args.to_vec()).unwrap()
}

fn reference(ctx: &mut Context, plan: &ir::Func, args: &[Value]) -> Value {
    let run = eval::evaluate(
        plan,
        ctx,
        eval::Inputs {
            args,
            ..eval::Inputs::default()
        },
    )
    .unwrap();
    let eval::Outcome::Returned(bits) = run.outcome else {
        panic!("reference return")
    };
    bits.to_value()
}

fn native(ctx: &mut Context, leaf: &CompiledLeaf, args: &[Value]) -> Value {
    let NativeRun::Ok(bits) = leaf.call(ctx as *mut Context as *mut u8, args) else {
        panic!("native return")
    };
    Value::from_bits(bits)
}

fn binary(op: Op) -> ByteCodeFunction {
    function(
        vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return],
        vec![],
        2,
    )
}

fn opaque(plan: &ir::Func, op: &Op) -> usize {
    plan.insts
        .iter()
        .position(|inst| matches!(&inst.op, ir::Opcode::Opaque(found) if found == op))
        .unwrap()
}

fn clif_fields(expression: &str) -> Vec<&str> {
    expression
        .split(';')
        .next()
        .unwrap()
        .split_whitespace()
        .map(|field| field.trim_end_matches(','))
        .collect()
}

#[test]
fn opt_ir_lower_proven_cons_read_keeps_exact_failed_guard_frame() {
    let _settings = Settings::enter();
    force_opt_passes_for_test(Some(OptPasses {
        fold: true,
        ..OptPasses::default()
    }));
    let mut ctx = Context::new();
    for (op, direct) in [
        (Op::Car, ir::Opcode::LoadCar),
        (Op::Cdr, ir::Opcode::LoadCdr),
    ] {
        let f = function(vec![Op::StackRef(0), op.clone(), Op::Return], vec![], 1);
        let mut rewritten = plan(&f);
        let read = opaque(&rewritten, &op);
        let checked = rewritten.insts[read].args[0];
        let ir::ValueDef::Inst(check) = rewritten.values[checked.index()].def else {
            panic!("checked input")
        };
        rewritten.insts[check.index()].op = ir::Opcode::CheckType(TypeSet::CONS);
        rewritten.values[checked.index()].ty = TypeSet::CONS;
        rewritten.insts[read].op = direct;
        rewritten.insts[read].eff = crate::emacs_core::jit::opt::mem::Effects::READ_HEAP;
        let leaf = lower(&f, &rewritten);
        let cell = Value::cons(Value::fixnum(17), Value::fixnum(29));
        let _roots = Roots::new(&[cell]);
        let expected = tier0(&mut ctx, &f, &[cell]);
        assert_eq!(native(&mut ctx, &leaf, &[cell]), expected);
        for wrong in [Value::NIL, Value::fixnum(3)] {
            let NativeRun::DeoptAt(frame) =
                leaf.call(&mut ctx as *mut Context as *mut u8, &[wrong])
            else {
                panic!("failed CONS speculation must deopt")
            };
            assert_eq!(frame.pc, 1);
            assert_eq!(frame.stack.len(), 2);
            assert!(frame.stack.iter().all(|value| *value == wrong));
        }
    }
}

#[test]
fn opt_ir_lower_typed_cons_read_roots_compiler_only_result_through_gc() {
    let _settings = Settings::enter();
    force_opt_passes_for_test(Some(OptPasses {
        fold: true,
        ..OptPasses::default()
    }));
    let mut ctx = Context::new();
    for (read, direct, mut ops) in [
        (
            Op::Car,
            ir::Opcode::LoadCar,
            vec![
                Op::Constant(0),
                Op::Constant(1),
                Op::Cons,
                Op::Nil,
                Op::Cons,
            ],
        ),
        (
            Op::Cdr,
            ir::Opcode::LoadCdr,
            vec![
                Op::Nil,
                Op::Constant(0),
                Op::Constant(1),
                Op::Cons,
                Op::Cons,
            ],
        ),
    ] {
        ops.extend([
            read.clone(),
            Op::Pop,
            Op::Constant(2),
            Op::Call(0),
            Op::Pop,
            Op::Nil,
            Op::Return,
        ]);
        let f = function(
            ops,
            vec![
                Value::fixnum(7),
                Value::fixnum(9),
                Value::symbol("garbage-collect"),
            ],
            0,
        );
        let mut changed = plan(&f);
        let index = opaque(&changed, &read);
        let loaded = changed.insts[index].result.unwrap();
        let checked = changed.insts[index].args[0];
        let ir::ValueDef::Inst(check) = changed.values[checked.index()].def else {
            panic!("checked cons")
        };
        changed.insts[check.index()].op = ir::Opcode::CheckType(TypeSet::CONS);
        changed.values[checked.index()].ty = TypeSet::CONS;
        changed.insts[index].op = direct;
        changed.insts[index].eff = crate::emacs_core::jit::opt::mem::Effects::READ_HEAP;
        changed
            .blocks
            .iter_mut()
            .find(|b| matches!(b.term, ir::Term::Return(_)))
            .unwrap()
            .term = ir::Term::Return(loaded);
        let expected = crate::emacs_core::print::print_value(&reference(&mut ctx, &changed, &[]));
        let leaf = lower(&f, &changed);
        let collections = ctx.tagged_heap.gc_collections();
        let actual = native(&mut ctx, &leaf, &[]);
        assert_eq!(crate::emacs_core::print::print_value(&actual), expected);
        assert!(ctx.tagged_heap.gc_collections() > collections);
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

#[test]
fn opt_ir_lower_bool_constant_materializes_gnu_t_nil_before_return() {
    let _settings = Settings::enter();
    force_opt_passes_for_test(Some(OptPasses {
        fold: true,
        ..OptPasses::default()
    }));
    let mut ctx = Context::new();
    let f = function(vec![Op::True, Op::Return], vec![], 0);
    for flag in [false, true] {
        let mut changed = plan(&f);
        let index = changed
            .insts
            .iter()
            .position(|i| matches!(i.op, ir::Opcode::Const(_)))
            .unwrap();
        let tagged_result = changed.insts[index].result.unwrap();
        let boolean = ir::Value(changed.values.len() as u32);
        changed.values.push(ir::ValueData {
            ty: TypeSet::BOOLEAN,
            rep: ir::Rep::Bool,
            def: ir::ValueDef::Inst(ir::Inst(index as u32)),
        });
        changed.insts[index].op = ir::Opcode::BoolConst(flag);
        changed.insts[index].result = Some(boolean);
        let conversion = ir::Inst(changed.insts.len() as u32);
        changed.values[tagged_result.index()].def = ir::ValueDef::Inst(conversion);
        changed.values[tagged_result.index()].ty = if flag { TypeSet::T } else { TypeSet::NIL };
        let mut inst = changed.insts[index].clone();
        inst.op = ir::Opcode::BoolToLisp;
        inst.args = vec![boolean];
        inst.result = Some(tagged_result);
        changed.insts.push(inst);
        for block in &mut changed.blocks {
            if let Some(position) = block.insts.iter().position(|id| id.index() == index) {
                block.insts.insert(position + 1, conversion);
                break;
            }
        }
        let expected = reference(&mut ctx, &changed, &[]);
        assert_eq!(native(&mut ctx, &lower(&f, &changed), &[]), expected);
        assert!(expected.is_nil() || expected.is_t());
    }
}

// Require the nil test to consume the retagged arithmetic result itself. A
// mere count of arithmetic/retag operations could pass on cold-frame spills
// while the successful arithmetic result was still dead before its branch.
fn raw_arithmetic_feeds_nil_test(clif: &str) -> bool {
    let definitions: HashMap<_, _> = clif
        .lines()
        .filter_map(|line| line.trim().split_once(" = "))
        .collect();
    let fields = |value: &str| {
        definitions
            .get(value)
            .map_or_else(Vec::new, |expression| clif_fields(expression))
    };
    let opcode = |fields: &[&str], expected: &str| {
        fields
            .first()
            .is_some_and(|field| field.split('.').next() == Some(expected))
    };
    definitions.values().any(|expression| {
        let compare = clif_fields(expression);
        if !opcode(&compare, "icmp") || compare.get(1) != Some(&"ne") || compare.len() < 4 {
            return false;
        }
        let retag = fields(compare[2]);
        if !opcode(&retag, "bor") || retag.len() < 3 {
            return false;
        }
        let shifted = fields(retag[1]);
        if !opcode(&shifted, "ishl") || shifted.len() < 3 {
            return false;
        }
        opcode(&fields(shifted[1]), "iadd")
    })
}

#[test]
fn opt_ir_lower_numeric_truth_is_only_folded_with_bool_pass() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = function(
        vec![
            Op::StackRef(0),
            Op::Sub1,
            Op::GotoIfNil(5),
            Op::Constant(0),
            Op::Goto(6),
            Op::Constant(1),
            Op::Return,
        ],
        vec![Value::fixnum(7), Value::fixnum(41)],
        1,
    );
    let original = plan(&f);
    let mut leaves = Vec::new();
    for bool_rep in [false, true] {
        force_opt_passes_for_test(Some(OptPasses {
            bool_rep,
            ..OptPasses::default()
        }));
        let mut leaf = None;
        let clif = captured_clif(|| leaf = Some(lower(&f, &original)));
        assert_eq!(clif.len(), 1);
        assert_eq!(raw_arithmetic_feeds_nil_test(&clif[0]), !bool_rep);
        assert_eq!(clif[0].contains("iconst.i8 1"), bool_rep);
        let leaf = leaf.unwrap();
        for argument in [Value::fixnum(1), Value::fixnum(0)] {
            // Sub1 produces numerical zero for input 1. Both representations
            // must still take the true arm, exactly as Tier-0 does.
            let expected = tier0(&mut ctx, &f, &[argument]);
            assert_eq!(native(&mut ctx, &leaf, &[argument]), expected);
        }
        leaves.push(leaf);
    }
    for argument in [Value::fixnum(Value::MOST_NEGATIVE_FIXNUM), Value::NIL] {
        let first = leaves[0].call(&mut ctx as *mut Context as *mut u8, &[argument]);
        let second = leaves[1].call(&mut ctx as *mut Context as *mut u8, &[argument]);
        assert_eq!(first, second, "the arithmetic guard's exact frame survives");
        let NativeRun::DeoptAt(frame) = first else {
            panic!("Sub1 overflow or type rejection deopts precisely")
        };
        assert_eq!(frame.pc, 1);
        assert_eq!(frame.stack, vec![argument, argument]);
        assert_eq!(ctx.jit_root_stack_top, 0);
    }

    // A verified transformed IR can branch directly on a numeric SSA value.
    // It must use the same gated truth test as the builder's IsNonNil flag.
    let mut direct = original;
    let number = direct.insts[opaque(&direct, &Op::Sub1)].result.unwrap();
    let ir::Term::Branch { flag, .. } = &mut direct
        .blocks
        .iter_mut()
        .find(|block| matches!(block.term, ir::Term::Branch { .. }))
        .unwrap()
        .term
    else {
        unreachable!()
    };
    *flag = number;
    for bool_rep in [false, true] {
        force_opt_passes_for_test(Some(OptPasses {
            bool_rep,
            ..OptPasses::default()
        }));
        let mut leaf = None;
        let clif = captured_clif(|| leaf = Some(lower(&f, &direct)));
        assert_eq!(raw_arithmetic_feeds_nil_test(&clif[0]), !bool_rep);
        let argument = Value::fixnum(1);
        assert_eq!(
            native(&mut ctx, &leaf.unwrap(), &[argument]),
            reference(&mut ctx, &direct, &[argument])
        );
    }
}

#[test]
fn opt_ir_lower_const_pool_index_controls_native_result() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = function(
        vec![Op::Constant(0), Op::Return],
        vec![Value::fixnum(7), Value::fixnum(41)],
        0,
    );
    let mut changed = plan(&f);
    let inst = changed
        .insts
        .iter_mut()
        .find(|inst| matches!(inst.op, ir::Opcode::Const(0)))
        .unwrap();
    inst.op = ir::Opcode::Const(1);
    changed.values[inst.result.unwrap().index()].ty = TypeSet::for_constant(changed.consts[1]);
    let expected = reference(&mut ctx, &changed, &[]);
    assert_ne!(expected, tier0(&mut ctx, &f, &[]));
    assert_eq!(native(&mut ctx, &lower(&f, &changed), &[]), expected);
}

#[test]
fn opt_ir_lower_opaque_opcode_controls_native_arithmetic() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = binary(Op::Add);
    let mut changed = plan(&f);
    let index = opaque(&changed, &Op::Add);
    changed.insts[index].op = ir::Opcode::Opaque(Op::Sub);
    let leaf = lower(&f, &changed);
    for args in [
        [Value::fixnum(41), Value::fixnum(7)],
        [Value::fixnum(-3), Value::fixnum(11)],
    ] {
        let expected = reference(&mut ctx, &changed, &args);
        assert_ne!(expected, tier0(&mut ctx, &f, &args));
        assert_eq!(native(&mut ctx, &leaf, &args), expected);
    }
}

#[test]
fn opt_ir_lower_opaque_operands_control_native_order() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = binary(Op::Sub);
    let mut changed = plan(&f);
    let index = opaque(&changed, &Op::Sub);
    changed.insts[index].args.reverse();
    let leaf = lower(&f, &changed);
    for args in [
        [Value::fixnum(41), Value::fixnum(7)],
        [Value::fixnum(-3), Value::fixnum(11)],
    ] {
        let expected = reference(&mut ctx, &changed, &args);
        assert_ne!(expected, tier0(&mut ctx, &f, &args));
        assert_eq!(native(&mut ctx, &leaf, &args), expected);
    }
}

#[test]
fn opt_ir_lower_return_operand_controls_native_result() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = binary(Op::Add);
    let mut changed = plan(&f);
    let argument = changed
        .insts
        .iter()
        .find(|inst| matches!(inst.op, ir::Opcode::Arg(0)))
        .unwrap()
        .result
        .unwrap();
    let block = changed
        .blocks
        .iter_mut()
        .find(|block| matches!(block.term, ir::Term::Return(_)))
        .unwrap();
    block.term = ir::Term::Return(argument);
    let args = [Value::fixnum(41), Value::fixnum(7)];
    let expected = reference(&mut ctx, &changed, &args);
    assert_ne!(expected, tier0(&mut ctx, &f, &args));
    assert_eq!(native(&mut ctx, &lower(&f, &changed), &args), expected);
}

#[test]
fn opt_ir_lower_branch_successors_control_native_selection() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = function(
        vec![
            Op::GotoIfNil(3),
            Op::Constant(0),
            Op::Goto(4),
            Op::Constant(1),
            Op::Return,
        ],
        vec![Value::fixnum(7), Value::fixnum(41)],
        1,
    );
    let mut changed = plan(&f);
    let block = changed
        .blocks
        .iter_mut()
        .find(|block| matches!(block.term, ir::Term::Branch { .. }))
        .unwrap();
    let ir::Term::Branch {
        if_true, if_false, ..
    } = &mut block.term
    else {
        unreachable!()
    };
    core::mem::swap(if_true, if_false);
    let mut leaf = None;
    let clif = captured_clif(|| leaf = Some(lower(&f, &changed)));
    let leaf = leaf.unwrap();
    assert_eq!(clif.len(), 1);
    assert!(
        !clif[0].contains("uextend"),
        "a local Bool should reach brif without I64 transport: {}",
        clif[0]
    );
    for argument in [
        Value::NIL,
        Value::T,
        Value::fixnum(0),
        Value::make_float(-0.0),
    ] {
        let _root = Roots::new(&[argument]);
        let expected = reference(&mut ctx, &changed, &[argument]);
        assert_ne!(expected, tier0(&mut ctx, &f, &[argument]));
        assert_eq!(native(&mut ctx, &leaf, &[argument]), expected);
    }

    // Verified IR may transport a Bool through an I64 SSA variable or an
    // explicit I64 block parameter. Both paths must preserve zero/one flags.
    for via_phi in [false, true] {
        let mut changed = plan(&f);
        let producer = ir::Block(
            changed
                .blocks
                .iter()
                .position(|block| matches!(block.term, ir::Term::Branch { .. }))
                .unwrap() as u32,
        );
        let target = ir::Block(changed.blocks.len() as u32);
        let mut term = core::mem::replace(
            &mut changed.blocks[producer.index()].term,
            ir::Term::Unreachable,
        );
        let ir::Term::Branch { flag, .. } = &mut term else {
            unreachable!()
        };
        let original = *flag;
        let mut transport = ir::BlockData::new(changed.blocks[producer.index()].pc);
        transport.preds.push(producer);
        if via_phi {
            let param = ir::Value(changed.values.len() as u32);
            changed.values.push(ir::ValueData {
                ty: TypeSet::BOOLEAN,
                rep: ir::Rep::Bool,
                def: ir::ValueDef::Param {
                    block: target,
                    index: 0,
                },
            });
            transport.params.push(param);
            *flag = param;
        }
        for edge in term.edges() {
            for predecessor in &mut changed.blocks[edge.target.index()].preds {
                if *predecessor == producer {
                    *predecessor = target;
                }
            }
        }
        transport.term = term;
        changed.blocks.push(transport);
        changed.entry_stacks.push(Box::default());
        changed.blocks[producer.index()].term = ir::Term::Jump(ir::Edge {
            target,
            args: if via_phi { vec![original] } else { vec![] },
        });
        let leaf = lower(&f, &changed);
        for argument in [Value::NIL, Value::T, Value::fixnum(0)] {
            let expected = reference(&mut ctx, &changed, &[argument]);
            assert_eq!(expected, tier0(&mut ctx, &f, &[argument]));
            assert_eq!(native(&mut ctx, &leaf, &[argument]), expected);
        }
    }

    // The verifier also accepts a tagged Lisp branch operand. Sub1 lowers
    // through the shared arithmetic emitter as a raw fixnum: numerical zero
    // is still true in Lisp and must never be tested as a raw Bool zero.
    let f = function(
        vec![
            Op::Constant(0),
            Op::Sub1,
            Op::GotoIfNil(5),
            Op::Constant(1),
            Op::Goto(6),
            Op::Constant(2),
            Op::Return,
        ],
        vec![Value::fixnum(1), Value::fixnum(7), Value::fixnum(41)],
        0,
    );
    let mut changed = plan(&f);
    let zero = changed.insts[opaque(&changed, &Op::Sub1)].result.unwrap();
    let ir::Term::Branch { flag, .. } = &mut changed
        .blocks
        .iter_mut()
        .find(|block| matches!(block.term, ir::Term::Branch { .. }))
        .unwrap()
        .term
    else {
        unreachable!()
    };
    *flag = zero;
    let expected = reference(&mut ctx, &changed, &[]);
    assert_eq!(expected, tier0(&mut ctx, &f, &[]));
    assert_eq!(native(&mut ctx, &lower(&f, &changed), &[]), expected);
}

#[test]
fn opt_ir_lower_guard_uses_its_frame_after_a_heap_effect() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = function(
        vec![
            Op::StackRef(1),
            Op::Constant(0),
            Op::Setcar,
            Op::Pop,
            Op::StackRef(0),
            Op::Car,
            Op::Return,
        ],
        vec![Value::fixnum(7)],
        2,
    );
    let mut changed = plan(&f);
    // Moving the failure resume point back over StackRef is safe: the heap
    // store has already completed, and the skipped shuffle has no effect.
    let earlier = changed.source_states[4].as_ref().unwrap().frame;
    let guard = changed
        .insts
        .iter_mut()
        .find(|inst| inst.pc == 5 && matches!(inst.op, ir::Opcode::CheckType(TypeSet::LIST)))
        .unwrap();
    guard.frame = Some(earlier);
    changed.verify().unwrap();
    let leaf = lower(&f, &changed);
    let pair = Value::cons(Value::fixnum(1), Value::NIL);
    let args = [pair, Value::fixnum(19)];
    let _roots = Roots::new(&args);
    let NativeRun::DeoptAt(frame) = leaf.call(&mut ctx as *mut Context as *mut u8, &args) else {
        panic!("list guard deopt")
    };
    assert_eq!(frame.pc, 4);
    assert_eq!(frame.stack.as_slice(), &args);
    assert_eq!(pair.cons_car(), f.constants[0]);
}

#[test]
fn opt_ir_lower_return_keeps_a_fresh_cons_live_through_gc() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Cons,
            Op::Constant(2),
            Op::Call(0),
            Op::Pop,
            Op::Nil,
            Op::Return,
        ],
        vec![
            Value::fixnum(7),
            Value::fixnum(9),
            Value::symbol("garbage-collect"),
        ],
        0,
    );
    let mut changed = plan(&f);
    let cell = changed.insts[opaque(&changed, &Op::Cons)].result.unwrap();
    let block = changed
        .blocks
        .iter_mut()
        .find(|block| matches!(block.term, ir::Term::Return(_)))
        .unwrap();
    block.term = ir::Term::Return(cell);
    let expected = crate::emacs_core::print::print_value(&reference(&mut ctx, &changed, &[]));
    assert_ne!(
        expected,
        crate::emacs_core::print::print_value(&tier0(&mut ctx, &f, &[]))
    );
    let leaf = lower(&f, &changed);
    let collections = ctx.tagged_heap.gc_collections();
    let actual = native(&mut ctx, &leaf, &[]);
    assert_eq!(crate::emacs_core::print::print_value(&actual), expected);
    assert!(ctx.tagged_heap.gc_collections() > collections);
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn opt_ir_lower_deopt_terminator_spills_its_exact_frame() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = binary(Op::Add);
    let mut changed = plan(&f);
    let frame = changed.source_states[3].as_ref().unwrap().frame;
    let block = changed
        .blocks
        .iter_mut()
        .find(|block| matches!(block.term, ir::Term::Return(_)))
        .unwrap();
    block.term = ir::Term::Deopt(frame);
    let args = [Value::fixnum(41), Value::fixnum(7)];
    let run = eval::evaluate(
        &changed,
        &mut ctx,
        eval::Inputs {
            args: &args,
            ..eval::Inputs::default()
        },
    )
    .unwrap();
    let eval::Outcome::Deopt(expected) = run.outcome else {
        panic!("reference deopt")
    };
    let leaf = lower(&f, &changed);
    let NativeRun::DeoptAt(actual) = leaf.call(&mut ctx as *mut Context as *mut u8, &args) else {
        panic!("IR terminator deopt")
    };
    assert_eq!(actual.pc, expected.pc as usize);
    let expected_stack = expected
        .stack
        .iter()
        .copied()
        .map(ir::ValueBits::to_value)
        .collect::<Vec<_>>();
    assert_eq!(actual.stack.as_slice(), expected_stack.as_slice());
}

#[test]
fn opt_ir_lower_rejects_an_unimplemented_typed_opcode() {
    let _settings = Settings::enter();
    let f = function(
        vec![Op::Constant(0), Op::Nil, Op::Cons, Op::Return],
        vec![Value::fixnum(7)],
        0,
    );
    let mut changed = plan(&f);
    let index = opaque(&changed, &Op::Cons);
    changed.insts[index].op = ir::Opcode::AllocCons;
    changed.verify().unwrap();
    let result = lower_opt_ir_for_test(f.executable_ops(), &f.constants, 0, None, &changed);
    assert!(matches!(result, Err(CompileError::UnsupportedOp(_))));

    // Bool is a valid IR representation and a materializable framestate
    // value, but the initial shared-emitter subset must reject it until it
    // can convert zero/one to Lisp nil/t instead of publishing the raw word.
    let mut changed = plan(&f);
    let cons = opaque(&changed, &Op::Cons);
    let inst = ir::Inst(changed.insts.len() as u32);
    let flag = ir::Value(changed.values.len() as u32);
    changed.values.push(ir::ValueData {
        ty: TypeSet::BOOLEAN,
        rep: ir::Rep::Bool,
        def: ir::ValueDef::Inst(inst),
    });
    changed.insts.push(ir::InstData {
        op: ir::Opcode::IsNonNil,
        args: vec![changed.insts[cons].args[0]],
        result: Some(flag),
        eff: crate::emacs_core::jit::opt::mem::Effects::PURE,
        mem: crate::emacs_core::jit::opt::mem::AliasClass::None,
        frame: None,
        pc: changed.insts[cons].pc,
    });
    let block = changed
        .blocks
        .iter_mut()
        .find(|block| block.insts.contains(&ir::Inst(cons as u32)))
        .unwrap();
    let position = block
        .insts
        .iter()
        .position(|id| *id == ir::Inst(cons as u32))
        .unwrap();
    block.insts.insert(position, inst);
    let frame = changed.insts[cons].frame.unwrap();
    let mut frame = changed.frames[frame.index()].clone();
    frame.stack[0] = flag;
    changed.insts[cons].frame = Some(changed.intern_frame(frame));
    changed.verify().expect("Bool framestates verify");
    let result = lower_opt_ir_for_test(f.executable_ops(), &f.constants, 0, None, &changed);
    assert!(matches!(
        result,
        Err(CompileError::UnsupportedOp("opt-emit:frame-representation"))
    ));

    // A Bool guard also needs Lisp materialization before tag/singleton tests.
    // Refuse this verified future representation instead of treating an I8
    // flag as a tagged Lisp word or emitting a mismatched-width comparison.
    let original_frame = changed.source_states[2].as_ref().unwrap().frame;
    changed.insts[cons].frame = Some(original_frame);
    let checked = ir::Value(changed.values.len() as u32);
    let guard = ir::Inst(changed.insts.len() as u32);
    changed.values.push(ir::ValueData {
        ty: TypeSet::BOOLEAN,
        rep: ir::Rep::Bool,
        def: ir::ValueDef::Inst(guard),
    });
    changed.insts.push(ir::InstData {
        op: ir::Opcode::CheckType(TypeSet::BOOLEAN),
        args: vec![flag],
        result: Some(checked),
        eff: crate::emacs_core::jit::opt::mem::Effects::MAY_DEOPT,
        mem: crate::emacs_core::jit::opt::mem::AliasClass::None,
        frame: Some(original_frame),
        pc: 2,
    });
    let block = changed
        .blocks
        .iter_mut()
        .find(|block| block.insts.contains(&inst))
        .unwrap();
    let position = block.insts.iter().position(|id| *id == inst).unwrap();
    block.insts.insert(position + 1, guard);
    changed.verify().expect("Bool guards verify");
    let result = lower_opt_ir_for_test(f.executable_ops(), &f.constants, 0, None, &changed);
    assert!(matches!(
        result,
        Err(CompileError::UnsupportedOp("opt-emit:guard-representation"))
    ));
}

#[test]
fn opt_ir_lower_compiler_only_cons_survives_call_gc() {
    let _settings = Settings::enter();
    let _tail_roots = TailRoots::enter();
    let mut ctx = Context::new();
    let f = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Cons,
            Op::Pop,
            Op::Constant(2),
            Op::Call(0),
            Op::Return,
        ],
        vec![
            Value::fixnum(7),
            Value::fixnum(9),
            Value::symbol("garbage-collect"),
        ],
        0,
    );
    lowering::TAIL_CALLS_UNROOTED.with(|count| count.set(0));
    let _unmodified_leaf = lower(&f, &plan(&f));
    assert_eq!(
        lowering::TAIL_CALLS_UNROOTED.with(|count| count.get()),
        1,
        "the normal final call exercises the baseline tail-root helper"
    );
    let mut changed = plan(&f);
    let cell = changed.insts[opaque(&changed, &Op::Cons)].result.unwrap();
    let call = &changed.insts[opaque(&changed, &Op::Call(0))];
    assert!(
        !changed.frames[call.frame.unwrap().index()]
            .stack
            .contains(&cell),
        "the cons has been popped from the original GNU operand stack"
    );
    changed
        .blocks
        .iter_mut()
        .find(|block| matches!(block.term, ir::Term::Return(_)))
        .unwrap()
        .term = ir::Term::Return(cell);
    let expected = crate::emacs_core::print::print_value(&reference(&mut ctx, &changed, &[]));
    assert_ne!(
        expected,
        crate::emacs_core::print::print_value(&tier0(&mut ctx, &f, &[]))
    );
    lowering::TAIL_CALLS_UNROOTED.with(|count| count.set(0));
    let leaf = lower(&f, &changed);
    assert_eq!(
        lowering::TAIL_CALLS_UNROOTED.with(|count| count.get()),
        0,
        "returning the compiler-only cons forbids tail-root pruning"
    );
    let collections = ctx.tagged_heap.gc_collections();
    let actual = native(&mut ctx, &leaf, &[]);
    assert_eq!(crate::emacs_core::print::print_value(&actual), expected);
    assert!(ctx.tagged_heap.gc_collections() > collections);
    assert_eq!(ctx.jit_root_stack_top, 0);
}

fn countdown() -> ByteCodeFunction {
    function(
        vec![
            Op::StackRef(0),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(8),
            Op::StackRef(0),
            Op::Sub1,
            Op::StackSet(1),
            Op::Goto(0),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![Value::fixnum(0)],
        1,
    )
}

fn assert_terminal_poll_target(f: &ByteCodeFunction, plan: &ir::Func, clif: &str) {
    let cfg = analyze_cfg(
        f.executable_ops(),
        &f.constants,
        f.executable_gnu_byte_offset_map(),
        f.params.required.len(),
    )
    .unwrap();
    // The shared scaffold allocates the source CFG's blocks, then one block
    // per opt IR block. Its block IDs let this check the authoritative target,
    // rather than accepting any intermediate block which eventually jumps there.
    let base = cfg.leaders.len();
    let lines = clif.lines().collect::<Vec<_>>();
    let mut polls = 0;
    for (index, data) in plan.blocks.iter().enumerate() {
        let Some(position) = data
            .insts
            .iter()
            .position(|id| plan.insts[id.index()].op == ir::Opcode::Poll)
        else {
            continue;
        };
        assert!(
            data.insts[position + 1..]
                .iter()
                .all(|id| { matches!(plan.insts[id.index()].op, ir::Opcode::Refine(_)) })
        );
        let ir::Term::Jump(edge) = &data.term else {
            panic!("terminal poll has a Jump")
        };
        let label = format!("block{}", base + index);
        let start = lines
            .iter()
            .position(|line| {
                line.trim_start()
                    .strip_prefix(&label)
                    .is_some_and(|tail| tail.starts_with(':') || tail.starts_with('('))
            })
            .expect("native poll block");
        let body = lines[start + 1..]
            .iter()
            .take_while(|line| !line.trim_start().starts_with("block"))
            .copied()
            .collect::<Vec<_>>();
        let branch = body
            .iter()
            .rev()
            .find(|line| line.trim_start().starts_with("brif "))
            .expect("poll countdown branch");
        let target = format!("block{}", base + edge.target.index());
        assert!(
            branch.contains(&format!(", {target}("))
                || branch.trim_end().ends_with(&format!(", {target}")),
            "the fast poll edge must transfer directly to {target}: {branch}"
        );
        polls += 1;
    }
    assert!(polls > 0, "the shape assertion must observe a real poll");
}

#[test]
fn opt_ir_lower_terminal_poll_carries_refinements_and_phi_to_header() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = function(
        vec![
            Op::StackRef(1),
            Op::Sub1,
            Op::StackSet(2),
            Op::StackRef(1),
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(10),
            Op::StackRef(0),
            Op::Consp,
            Op::GotoIfNotNil(0),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![Value::fixnum(0)],
        2,
    );
    let original = plan(&f);
    let poll_block = original
        .blocks
        .iter()
        .find(|block| {
            block
                .insts
                .iter()
                .any(|id| original.insts[id.index()].op == ir::Opcode::Poll)
        })
        .unwrap();
    assert!(poll_block.insts.iter().any(|id| {
        matches!(
            original.insts[id.index()].op,
            ir::Opcode::Refine(TypeSet::CONS)
        )
    }));
    let ir::Term::Jump(edge) = &poll_block.term else {
        unreachable!()
    };
    assert!(!edge.args.is_empty(), "the countdown crosses a real phi");
    let mut leaf = None;
    let clif = captured_clif(|| leaf = Some(lower(&f, &original)));
    assert_eq!(clif.len(), 1);
    assert_terminal_poll_target(&f, &original, &clif[0]);
    let leaf = leaf.unwrap();
    let pair = Value::cons(Value::fixnum(7), Value::fixnum(9));
    let _roots = Roots::new(&[pair]);
    for n in [255, 256, 511] {
        let args = [Value::fixnum(n), pair];
        let expected = reference(&mut ctx, &original, &args);
        assert_eq!(expected, tier0(&mut ctx, &f, &args));
        ctx.gc_stress = true;
        reset_bytecode_branch_poll_count();
        assert_eq!(native(&mut ctx, &leaf, &args), expected);
        ctx.gc_stress = false;
        assert_eq!(bytecode_branch_poll_count(), ((n - 1) / 255) as usize);
        assert_eq!(pair.cons_car(), Value::fixnum(7));
        assert_eq!(pair.cons_cdr(), Value::fixnum(9));
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

#[test]
fn opt_ir_lower_nonterminal_poll_keeps_following_operation() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = countdown();
    let mut changed = plan(&f);
    let block = changed
        .blocks
        .iter()
        .position(|block| {
            block
                .insts
                .iter()
                .any(|id| changed.insts[id.index()].op == ir::Opcode::Poll)
        })
        .unwrap();
    let poll = changed.blocks[block]
        .insts
        .iter()
        .copied()
        .find(|id| changed.insts[id.index()].op == ir::Opcode::Poll)
        .unwrap();
    let ir::Term::Jump(edge) = &changed.blocks[block].term else {
        unreachable!()
    };
    assert_eq!(edge.args.len(), 1);
    let arg = edge.args[0];
    let arithmetic = changed.insts[opaque(&changed, &Op::Sub1)].clone();
    let id = ir::Inst(changed.insts.len() as u32);
    let value = ir::Value(changed.values.len() as u32);
    changed.values.push(ir::ValueData {
        ty: TypeSet::FIXNUM,
        rep: ir::Rep::Tagged,
        def: ir::ValueDef::Inst(id),
    });
    changed.insts.push(ir::InstData {
        op: ir::Opcode::Opaque(Op::Sub1),
        args: vec![arg],
        result: Some(value),
        eff: arithmetic.eff,
        mem: arithmetic.mem,
        frame: changed.insts[poll.index()].frame,
        pc: changed.insts[poll.index()].pc,
    });
    changed.blocks[block].insts.push(id);
    let ir::Term::Jump(edge) = &mut changed.blocks[block].term else {
        unreachable!()
    };
    edge.args[0] = value;
    let args = [Value::fixnum(511)];
    let expected = reference(&mut ctx, &changed, &args);
    assert_ne!(expected, tier0(&mut ctx, &f, &args));
    let leaf = lower(&f, &changed);
    reset_bytecode_branch_poll_count();
    assert_eq!(native(&mut ctx, &leaf, &args), expected);
    assert_eq!(bytecode_branch_poll_count(), 1);
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn opt_ir_lower_polls_at_tier0_backedge_cadence() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = countdown();
    let original = plan(&f);
    assert!(
        original
            .insts
            .iter()
            .any(|inst| inst.op == ir::Opcode::Poll)
    );
    let mut leaf = None;
    let clif = captured_clif(|| leaf = Some(lower(&f, &original)));
    assert_eq!(clif.len(), 1);
    assert_terminal_poll_target(&f, &original, &clif[0]);
    let leaf = leaf.unwrap();
    for n in [254, 255, 256, 510, 100_000] {
        let args = [Value::fixnum(n)];
        reset_bytecode_branch_poll_count();
        let expected = tier0(&mut ctx, &f, &args);
        let expected_polls = bytecode_branch_poll_count();
        reset_bytecode_branch_poll_count();
        assert_eq!(native(&mut ctx, &leaf, &args), expected, "n={n}");
        assert_eq!(bytecode_branch_poll_count(), expected_polls, "n={n}");
        assert_eq!(expected_polls, (n / 255) as usize, "n={n}");
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

#[test]
fn opt_ir_lower_poll_quit_matches_tier0_flow() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = countdown();
    let leaf = lower(&f, &plan(&f));
    let args = [Value::fixnum(100_000)];
    ctx.set_quit_flag_value(Value::T);
    reset_bytecode_branch_poll_count();
    let expected = {
        let mut vm = Vm::from_context(&mut ctx);
        vm.force_interpreter_only_for_test();
        vm.execute(&f, args.to_vec()).expect_err("Tier-0 quit")
    };
    let expected_polls = bytecode_branch_poll_count();
    ctx.set_quit_flag_value(Value::NIL);
    let expected = expected.as_signal().expect("Tier-0 quit signal");
    assert_eq!(expected.symbol, crate::emacs_core::intern::intern("quit"));

    ctx.set_quit_flag_value(Value::T);
    reset_bytecode_branch_poll_count();
    let actual = leaf.call(&mut ctx as *mut Context as *mut u8, &args);
    ctx.set_quit_flag_value(Value::NIL);
    assert_eq!(actual, NativeRun::Signal);
    let actual = take_pending_flow().expect("native quit flow");
    let actual = actual.as_signal().expect("native quit signal");
    assert_eq!(actual.symbol, expected.symbol);
    assert_eq!(actual.data, expected.data);
    assert_eq!(actual.raw_data, expected.raw_data);
    assert_eq!(bytecode_branch_poll_count(), expected_polls);
    assert_eq!(expected_polls, 1);
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn opt_ir_lower_compiler_only_cons_survives_poll_gc() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = function(
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
    let mut changed = plan(&f);
    let cell = changed.insts[opaque(&changed, &Op::Cons)].result.unwrap();
    for poll in changed
        .insts
        .iter()
        .filter(|inst| inst.op == ir::Opcode::Poll)
    {
        assert!(
            !changed.frames[poll.frame.unwrap().index()]
                .stack
                .contains(&cell),
            "the cons is only an SSA value at the poll"
        );
    }
    changed
        .blocks
        .iter_mut()
        .find(|block| matches!(block.term, ir::Term::Return(_)))
        .unwrap()
        .term = ir::Term::Return(cell);
    let args = [Value::fixnum(255)];
    let original_result = tier0(&mut ctx, &f, &args);
    ctx.gc_stress = true;
    let expected = crate::emacs_core::print::print_value(&reference(&mut ctx, &changed, &args));
    assert_ne!(
        expected,
        crate::emacs_core::print::print_value(&original_result)
    );
    let mut leaf = None;
    let clif = captured_clif(|| leaf = Some(lower(&f, &changed)));
    assert_eq!(clif.len(), 1);
    assert_terminal_poll_target(&f, &changed, &clif[0]);
    let leaf = leaf.unwrap();
    let collections = ctx.tagged_heap.gc_collections();
    reset_bytecode_branch_poll_count();
    let actual = native(&mut ctx, &leaf, &args);
    ctx.gc_stress = false;
    assert_eq!(crate::emacs_core::print::print_value(&actual), expected);
    assert!(ctx.tagged_heap.gc_collections() > collections);
    assert_eq!(bytecode_branch_poll_count(), 1);
    assert_eq!(ctx.jit_root_stack_top, 0);
}
