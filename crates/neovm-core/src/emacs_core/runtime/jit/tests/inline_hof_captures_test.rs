//! Captured-cell witnesses preserve conditional entry and precise arithmetic.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::error::{FlowKind, FlowResultExt as _};
use crate::emacs_core::eval::{
    Context, bytecode_branch_poll_count, reset_bytecode_branch_poll_count,
};
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::compile::{self, Inline2Mode, NativeRun};
use crate::emacs_core::jit::inline;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

struct Knobs;

impl Knobs {
    fn enter() -> Self {
        crate::test_utils::init_test_tracing();
        inline::force_inline_for_test(Some(true));
        compile::force_inline2_for_test(Some(Inline2Mode::Hof));
        compile::force_profit_gate_for_test(false);
        compile::force_deopt_for_test(false);
        Self
    }
}

impl Drop for Knobs {
    fn drop(&mut self) {
        inline::force_inline_for_test(None);
        compile::force_inline2_for_test(None);
        compile::force_deopt_for_test(false);
    }
}

fn function(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams::simple(vec![intern("hof-capture-arg")]));
    function.lexical = true;
    function.ops = ops;
    function.constants = constants.into();
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    function.seal_hand_assembled_ops_for_test();
    function.jit_runtime().set_hot_for_test();
    function
}

fn increment_ops() -> Vec<Op> {
    vec![
        Op::Constant(0),
        Op::Dup,
        Op::CarSafe,
        Op::Add1,
        Op::Setcar,
        Op::Return,
    ]
}

fn closure(ctx: &mut Context, capture: Value, conditional: bool) -> Value {
    let (ops, constants) = if conditional {
        let mut ops = vec![Op::StackRef(0), Op::Constant(1), Op::Gtr, Op::GotoIfNil(10)];
        ops.extend(increment_ops());
        ops.push(Op::Return);
        (ops, vec![Value::symbol("V0"), Value::make_int(750)])
    } else {
        (increment_ops(), vec![Value::symbol("V0")])
    };
    let prototype = Value::make_bytecode(function(ops, constants));
    ctx.apply2(Value::symbol("make-closure"), prototype, capture)
        .unwrap()
}

fn caller(callback: Value) -> ByteCodeFunction {
    function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![Value::symbol("mapcar"), callback],
    )
}

#[test]
fn inline_hof_captures_reject_internal_entries_and_other_store_shapes() {
    let mut ops = increment_ops();
    assert_eq!(increments(&ops, 1, &[0]).len(), 1);
    assert!(increments(&ops, 0, &[0]).is_empty());
    for leader in 1..=4 {
        assert!(increments(&ops, 1, &[0, leader]).is_empty());
    }
    ops[4] = Op::Setcdr;
    assert!(increments(&ops, 1, &[0]).is_empty());
    ops[4] = Op::Setcar;
    ops[3] = Op::Sub1;
    assert!(increments(&ops, 1, &[0]).is_empty());
}

#[test]
fn inline_hof_captures_force_deopt_keeps_original_lowering() {
    let _knobs = Knobs::enter();
    compile::force_deopt_for_test(true);
    let mut function = cranelift_codegen::ir::Function::new();
    let mut context = cranelift_frontend::FunctionBuilderContext::new();
    let mut fb = FunctionBuilder::new(&mut function, &mut context);
    let plan = Captures::build(
        &mut fb,
        HofSite {
            callback: Value::NIL,
            closure: true,
            prefix: 1,
            kind: HofKind::Mapc,
            call_site_pc: 0,
            const_base: 0,
        },
    )
    .unwrap();
    assert!(plan.increments.is_empty() && plan.values.is_empty());
}

#[test]
fn inline_hof_captures_noncons_declines_without_an_early_branch_error() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    for argument in [500, 900] {
        let callback = closure(&mut ctx, Value::make_int(42), true);
        let sequence = Value::list(vec![Value::make_int(argument)]);
        ctx.push_vm_root_frame();
        ctx.push_vm_frame_root(callback);
        ctx.push_vm_frame_root(sequence);
        // Compile while the callback's Add1 is still fixnum-only. The active
        // reference branch may widen its feedback, but must keep the already
        // compiled witness's refusal equivalent to the original callback.
        let source = caller(callback);
        let fused = inline::fuse_calls_v2(
            source.executable_ops(),
            &source.constants,
            None,
            1,
            &vec![
                crate::emacs_core::jit::NumericFeedback::FixnumOnly;
                source.executable_ops().len()
            ],
        )
        .expect("conditional callback has consistent entry depths");
        assert!(fused.admitted_hof_at(3).is_some());
        let leaf = compile::compile_bytecode_function_with(&source, Some(&ctx.obarray)).unwrap();
        let reference = ctx.apply2(Value::symbol("mapcar"), callback, sequence);
        if let Ok(value) = reference.as_ref() {
            ctx.push_vm_frame_root(*value);
        }
        let _ = ctx.debug_on_next_call_is_armed();
        let NativeRun::DeoptAt(resume) =
            leaf.call(&mut ctx as *mut Context as *mut u8, &[sequence])
        else {
            panic!("non-cons capture declines before the original callback")
        };
        let readback = resume.inlined.as_ref().expect("HOF witness chain");
        let state = readback.hof.as_ref().unwrap();
        assert!(!state.callback_entered);
        assert_eq!(state.index, 0);
        assert_eq!(state.item, Value::make_int(argument));
        assert_eq!(readback.frames[0].pc, 0);
        assert_eq!(readback.frames[0].stack, vec![Value::make_int(argument)]);
        let result =
            compile::resumed_chain::resume_deopt(&mut ctx, &source, Value::NIL, &leaf, *resume);
        match (result.kinded(), reference.kinded()) {
            (Ok(value), Ok(reference)) => assert_eq!(print_value(&value), print_value(&reference)),
            (Err(FlowKind::Signal(signal)), Err(FlowKind::Signal(reference))) => {
                assert_eq!(signal.symbol, reference.symbol);
                assert_eq!(signal.data, reference.data);
            }
            other => panic!("capture refusal changes callback behavior: {other:?}"),
        }
        assert_eq!(ctx.depth, 0);
        assert!(ctx.specpdl.is_empty() && ctx.bc_buf.is_empty() && ctx.bc_frames.is_empty());
        ctx.pop_vm_root_frame();
    }
}

#[test]
fn inline_hof_captures_empty_mapping_skips_the_witness() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    let callback = closure(&mut ctx, Value::make_int(42), false);
    let source = caller(callback);
    let leaf = compile::compile_bytecode_function_with(&source, Some(&ctx.obarray)).unwrap();
    let _ = ctx.debug_on_next_call_is_armed();
    let NativeRun::Ok(bits) = leaf.call(&mut ctx as *mut Context as *mut u8, &[Value::NIL]) else {
        panic!("empty mapping has no callback or capture witness")
    };
    assert_eq!(Value::from_bits(bits), Value::NIL);
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty());
}

#[test]
fn inline_hof_captures_float_car_after_poll_deopts_inside_add1_once() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    let cell = Value::cons(Value::make_int(0), Value::NIL);
    ctx.set_variable("hof-capture-cell", cell);
    let callback = closure(&mut ctx, cell, false);
    let sequence = Value::list((0..300).map(Value::make_int).collect());
    ctx.push_vm_root_frame();
    ctx.push_vm_frame_root(callback);
    ctx.push_vm_frame_root(sequence);
    ctx.eval_str(
        "(setq post-gc-hook (list (lambda () (setq post-gc-hook nil) (setcar hof-capture-cell 1.5))))",
    ).unwrap();
    let source = caller(callback);
    let leaf = compile::compile_bytecode_function_with(&source, Some(&ctx.obarray)).unwrap();
    let _ = ctx.debug_on_next_call_is_armed();
    ctx.gc_stress = true;
    reset_bytecode_branch_poll_count();
    let NativeRun::DeoptAt(resume) = leaf.call(&mut ctx as *mut Context as *mut u8, &[sequence])
    else {
        panic!("fresh captured car transfers at the callback's Add1")
    };
    let readback = resume.inlined.as_ref().expect("precise HOF chain");
    let state = readback.hof.as_ref().unwrap();
    assert!(state.callback_entered);
    assert_eq!(state.index, 255);
    assert_eq!(state.item, Value::make_int(255));
    assert_eq!(readback.frames[0].pc, 3);
    assert_eq!(readback.frames[0].stack[1], cell);
    assert_eq!(readback.frames[0].stack[2].as_float(), Some(1.5));
    assert_eq!(bytecode_branch_poll_count(), 1);
    ctx.gc_stress = false;
    let result =
        compile::resumed_chain::resume_deopt(&mut ctx, &source, Value::NIL, &leaf, *resume)
            .unwrap();
    assert_eq!(cell.cons_car().as_float(), Some(46.5));
    let mapped = crate::emacs_core::value::list_to_vec(&result).unwrap();
    assert_eq!(mapped.len(), 300);
    assert_eq!(mapped[254], Value::make_int(255));
    assert_eq!(mapped[255].as_float(), Some(2.5));
    assert_eq!(mapped[299].as_float(), Some(46.5));
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty() && ctx.bc_buf.is_empty() && ctx.bc_frames.is_empty());
    assert_eq!(ctx.save_vm_frame_roots(), 2);
    ctx.pop_vm_root_frame();
}
