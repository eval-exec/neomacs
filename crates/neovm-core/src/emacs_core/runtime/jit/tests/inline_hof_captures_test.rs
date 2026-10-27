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

/// Threading: scalar inline configuration belongs to this fixture's compiler
/// thread; no Lisp state or mutator cache is stored. The returned backend guard
/// restores the enclosing mode while keeping the legacy v2 producer selected.
struct Knobs;

impl Knobs {
    fn enter() -> (Self, impl Drop) {
        // Mounted twice: the `default_profile` mount keeps the production
        // front (Opt backend, INLINE2 off, profit gate on), so these HOF
        // loops are reached through the default profile's HOF-only front.
        let production = module_path!().ends_with("default_profile_tests");
        let backend = compile::opt_mode_scope_for_test(if production {
            compile::OptMode::Opt
        } else {
            compile::OptMode::Legacy
        });
        crate::test_utils::init_test_tracing();
        inline::force_inline_for_test(Some(true));
        compile::force_inline2_for_test(Some(if production {
            Inline2Mode::Off
        } else {
            Inline2Mode::Hof
        }));
        compile::force_profit_gate_for_test(production);
        compile::force_deopt_for_test(false);
        (Self, backend)
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
        // The production mount's front is the HOF-only one.
        let fuse = if module_path!().ends_with("default_profile_tests") {
            inline::fuse_hof_calls_v2
        } else {
            inline::fuse_calls_v2
        };
        let fused = fuse(
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

fn assert_hof_admitted(source: &ByteCodeFunction) {
    let fuse = if module_path!().ends_with("default_profile_tests") {
        inline::fuse_hof_calls_v2
    } else {
        inline::fuse_calls_v2
    };
    let Some(fused) = fuse(
        source.executable_ops(),
        &source.constants,
        None,
        1,
        &vec![crate::emacs_core::jit::NumericFeedback::FixnumOnly; source.executable_ops().len()],
    ) else {
        panic!("callback fixture has consistent entry depths");
    };
    assert!(fused.admitted_hof_at(3).is_some());
}

#[test]
fn inline_hof_flags_keep_conditional_captured_count_and_boundaries() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    let cell = Value::cons(Value::make_int(0), Value::NIL);
    // OR retains the first true result. The shared final conditional is a
    // separate leader, receiving tagged T/NIL from either predecessor.
    let mut ops = vec![
        Op::StackRef(0),
        Op::Constant(1),
        Op::Gtr,
        Op::GotoIfNotNilElsePop(7),
        Op::StackRef(0),
        Op::Constant(2),
        Op::Lss,
        Op::GotoIfNil(14),
    ];
    ops.extend(increment_ops());
    ops.push(Op::Return);
    let prototype = Value::make_bytecode(function(
        ops,
        vec![
            Value::symbol("V0"),
            Value::make_int(750),
            Value::make_int(250),
        ],
    ));
    let Ok(callback) = ctx.apply2(Value::symbol("make-closure"), prototype, cell) else {
        panic!("fixture must create a lexical closure");
    };
    let sequence = Value::list(
        vec![0, 249, 250, 500, 750, 751, 999]
            .into_iter()
            .map(Value::make_int)
            .collect(),
    );
    ctx.push_vm_root_frame();
    ctx.push_vm_frame_root(callback);
    ctx.push_vm_frame_root(sequence);
    let source = caller(callback);
    assert_hof_admitted(&source);
    let Ok(leaf) = compile::compile_bytecode_function_with(&source, Some(&ctx.obarray)) else {
        panic!("admitted HOF fixture must compile");
    };
    let Ok(reference) = ctx.apply2(Value::symbol("mapcar"), callback, sequence) else {
        panic!("reference mapping must succeed");
    };
    ctx.push_vm_frame_root(reference);
    assert_eq!(cell.cons_car().as_int(), Some(4));
    assert!(
        ctx.apply2(Value::symbol("setcar"), cell, Value::make_int(0))
            .is_ok()
    );
    let _ = ctx.debug_on_next_call_is_armed();
    let NativeRun::Ok(bits) = leaf.call(&mut ctx as *mut Context as *mut u8, &[sequence]) else {
        panic!("fixnum conditional capture fixture must run natively");
    };
    assert_eq!(
        print_value(&Value::from_bits(bits)),
        print_value(&reference)
    );
    assert_eq!(cell.cons_car().as_int(), Some(4));
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty() && ctx.bc_buf.is_empty() && ctx.bc_frames.is_empty());
    ctx.pop_vm_root_frame();
}

#[test]
fn inline_hof_flags_keep_all_four_branch_stack_effects() {
    use super::super::CallbackBranch;
    use strum::IntoEnumIterator;
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    for branch in CallbackBranch::iter() {
        let jump = match branch {
            CallbackBranch::PopOnNil => Op::GotoIfNil(6),
            CallbackBranch::PopOnNonNil => Op::GotoIfNotNil(6),
            CallbackBranch::KeepOnNil => Op::GotoIfNilElsePop(6),
            CallbackBranch::KeepOnNonNil => Op::GotoIfNotNilElsePop(6),
        };
        let mut ops = vec![
            Op::StackRef(0),
            Op::Constant(0),
            Op::Gtr,
            jump,
            Op::Constant(1),
            Op::Return,
        ];
        match branch {
            CallbackBranch::PopOnNil | CallbackBranch::PopOnNonNil => ops.push(Op::Constant(2)),
            CallbackBranch::KeepOnNil | CallbackBranch::KeepOnNonNil => {}
        }
        ops.push(Op::Return);
        let callback = Value::make_bytecode(function(
            ops,
            vec![
                Value::make_int(750),
                Value::make_int(111),
                Value::make_int(222),
            ],
        ));
        let sequence = Value::list(vec![Value::make_int(500), Value::make_int(900)]);
        ctx.push_vm_root_frame();
        ctx.push_vm_frame_root(callback);
        ctx.push_vm_frame_root(sequence);
        let source = caller(callback);
        assert_hof_admitted(&source);
        let Ok(leaf) = compile::compile_bytecode_function_with(&source, Some(&ctx.obarray)) else {
            panic!("admitted HOF fixture must compile");
        };
        let Ok(reference) = ctx.apply2(Value::symbol("mapcar"), callback, sequence) else {
            panic!("reference mapping must succeed");
        };
        ctx.push_vm_frame_root(reference);
        let _ = ctx.debug_on_next_call_is_armed();
        let NativeRun::Ok(bits) = leaf.call(&mut ctx as *mut Context as *mut u8, &[sequence])
        else {
            panic!("branch fixture must run natively: {branch:?}");
        };
        // Ordinary mapcar independently exercises the callback stack effects;
        // ElsePop's taken path returns the retained exact Lisp T or NIL.
        assert_eq!(
            print_value(&Value::from_bits(bits)),
            print_value(&reference),
            "{branch:?}",
        );
        assert_eq!(ctx.depth, 0);
        assert!(ctx.specpdl.is_empty() && ctx.bc_buf.is_empty() && ctx.bc_frames.is_empty());
        ctx.pop_vm_root_frame();
    }
}

#[test]
fn inline_hof_flags_resume_nan_then_error_without_repeating_capture() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    let cell = Value::cons(Value::make_int(0), Value::NIL);
    let callback = closure(&mut ctx, cell, true);
    let sequence = Value::list(vec![
        Value::make_int(999),
        Value::make_float(f64::NAN),
        Value::make_int(500),
        Value::symbol("hof-flags-not-a-number"),
    ]);
    ctx.push_vm_root_frame();
    ctx.push_vm_frame_root(callback);
    ctx.push_vm_frame_root(sequence);
    let source = caller(callback);
    assert_hof_admitted(&source);
    let Ok(leaf) = compile::compile_bytecode_function_with(&source, Some(&ctx.obarray)) else {
        panic!("admitted HOF fixture must compile");
    };
    let reference = match ctx
        .apply2(Value::symbol("mapcar"), callback, sequence)
        .kinded()
    {
        Err(FlowKind::Signal(signal)) => (
            signal.symbol,
            signal.data.iter().map(print_value).collect::<Vec<_>>(),
        ),
        other => panic!("reference must reject the final operand: {other:?}"),
    };
    assert_eq!(cell.cons_car().as_int(), Some(1));
    assert!(
        ctx.apply2(Value::symbol("setcar"), cell, Value::make_int(0))
            .is_ok()
    );
    let _ = ctx.debug_on_next_call_is_armed();
    let NativeRun::DeoptAt(resume) = leaf.call(&mut ctx as *mut Context as *mut u8, &[sequence])
    else {
        panic!("cold float/type guard must resume the original callback");
    };
    match compile::resumed_chain::resume_deopt(&mut ctx, &source, Value::NIL, &leaf, *resume)
        .kinded()
    {
        Err(FlowKind::Signal(signal)) => {
            assert_eq!(
                (
                    signal.symbol,
                    signal.data.iter().map(print_value).collect::<Vec<_>>()
                ),
                reference
            );
        }
        other => panic!("resumed callback must preserve its numeric error: {other:?}"),
    }
    assert_eq!(cell.cons_car().as_int(), Some(1));
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty() && ctx.bc_buf.is_empty() && ctx.bc_frames.is_empty());
    ctx.pop_vm_root_frame();
}

fn assert_flag_mapping_native(
    ctx: &mut Context,
    callback: Value,
    sequence: Value,
    expected: &[Value],
) {
    ctx.push_vm_root_frame();
    ctx.push_vm_frame_root(callback);
    ctx.push_vm_frame_root(sequence);
    let roots = ctx.save_vm_frame_roots();
    let source = caller(callback);
    assert_hof_admitted(&source);
    let leaf = compile::compile_bytecode_function_with(&source, Some(&ctx.obarray))
        .expect("the admitted flag fixture must compile");
    let _ = ctx.debug_on_next_call_is_armed();
    let NativeRun::Ok(bits) = leaf.call(ctx as *mut Context as *mut u8, &[sequence]) else {
        panic!("the successful flag fixture must execute its native HOF path");
    };
    let actual = crate::emacs_core::value::list_to_vec(&Value::from_bits(bits))
        .expect("mapcar returns a proper list");
    assert_eq!(actual.len(), expected.len());
    for (&actual, &expected) in actual.iter().zip(expected) {
        if let Some(expected_float) = expected.as_float() {
            assert_eq!(
                actual.as_float().map(f64::to_bits),
                Some(expected_float.to_bits())
            );
        } else {
            assert_eq!(actual, expected);
        }
    }
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty() && ctx.bc_buf.is_empty() && ctx.bc_frames.is_empty());
    assert!(ctx.jit_bind_stack.is_empty());
    assert_eq!(ctx.save_vm_frame_roots(), roots);
    ctx.pop_vm_root_frame();
}

#[test]
fn inline_hof_flags_keep_truthy_zero_on_materialized_branches() {
    use super::super::CallbackBranch;
    use strum::IntoEnumIterator;
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    let inputs = [
        Value::NIL,
        Value::make_int(0),
        Value::make_float(0.0),
        Value::make_float(-0.0),
        Value::make_int(-1),
        Value::T,
    ];
    let sequence = Value::list(inputs.to_vec());
    ctx.push_vm_root_frame();
    ctx.push_vm_frame_root(sequence);
    for branch in CallbackBranch::iter() {
        let jump = match branch {
            CallbackBranch::PopOnNil => Op::GotoIfNil(4),
            CallbackBranch::PopOnNonNil => Op::GotoIfNotNil(4),
            CallbackBranch::KeepOnNil => Op::GotoIfNilElsePop(4),
            CallbackBranch::KeepOnNonNil => Op::GotoIfNotNilElsePop(4),
        };
        let mut ops = vec![Op::StackRef(0), jump, Op::Constant(0), Op::Return];
        match branch {
            CallbackBranch::PopOnNil | CallbackBranch::PopOnNonNil => ops.push(Op::Constant(1)),
            CallbackBranch::KeepOnNil | CallbackBranch::KeepOnNonNil => {}
        }
        ops.push(Op::Return);
        let callback = Value::make_bytecode(function(
            ops,
            vec![Value::make_int(111), Value::make_int(222)],
        ));
        let expected: Vec<_> = inputs
            .iter()
            .map(|&input| match branch {
                CallbackBranch::PopOnNil => {
                    if input.is_nil() {
                        Value::make_int(222)
                    } else {
                        Value::make_int(111)
                    }
                }
                CallbackBranch::PopOnNonNil => {
                    if input.is_nil() {
                        Value::make_int(111)
                    } else {
                        Value::make_int(222)
                    }
                }
                CallbackBranch::KeepOnNil => {
                    if input.is_nil() {
                        Value::NIL
                    } else {
                        Value::make_int(111)
                    }
                }
                CallbackBranch::KeepOnNonNil => {
                    if input.is_nil() {
                        Value::make_int(111)
                    } else {
                        input
                    }
                }
            })
            .collect();
        assert_flag_mapping_native(&mut ctx, callback, sequence, &expected);
        // Add1 produces RawFixnum(0), so the materialized path must retag it
        // before testing Lisp NIL instead of treating machine zero as false.
        let jump = match branch {
            CallbackBranch::PopOnNil => Op::GotoIfNil(5),
            CallbackBranch::PopOnNonNil => Op::GotoIfNotNil(5),
            CallbackBranch::KeepOnNil => Op::GotoIfNilElsePop(5),
            CallbackBranch::KeepOnNonNil => Op::GotoIfNotNilElsePop(5),
        };
        let mut ops = vec![Op::StackRef(0), Op::Add1, jump, Op::Constant(0), Op::Return];
        match branch {
            CallbackBranch::PopOnNil | CallbackBranch::PopOnNonNil => ops.push(Op::Constant(1)),
            CallbackBranch::KeepOnNil | CallbackBranch::KeepOnNonNil => {}
        }
        ops.push(Op::Return);
        let callback = Value::make_bytecode(function(
            ops,
            vec![Value::make_int(111), Value::make_int(222)],
        ));
        let expected = match branch {
            CallbackBranch::PopOnNil | CallbackBranch::KeepOnNil => Value::make_int(111),
            CallbackBranch::PopOnNonNil => Value::make_int(222),
            CallbackBranch::KeepOnNonNil => Value::make_int(0),
        };
        assert_flag_mapping_native(
            &mut ctx,
            callback,
            Value::list(vec![Value::make_int(-1)]),
            &[expected],
        );
    }
    ctx.pop_vm_root_frame();
}

#[derive(Debug)]
enum FlagPredicateInputs {
    Unary {
        op: Op,
        true_input: Value,
        false_input: Value,
    },
    Binary {
        op: Op,
        rhs: Value,
        true_input: Value,
        false_input: Value,
    },
}

#[test]
fn inline_hof_flags_all_boolean_producers_keep_exact_results() {
    use super::super::CallbackBranch;
    use strum::IntoEnumIterator;
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    let zero = Value::make_int(0);
    let one = Value::make_int(1);
    let cons = Value::cons(zero, Value::NIL);
    let string = Value::string("");
    let float = Value::make_float(0.0);
    ctx.push_vm_root_frame();
    for value in [cons, string, float] {
        ctx.push_vm_frame_root(value);
    }
    let cases = [
        FlagPredicateInputs::Unary {
            op: Op::Null,
            true_input: Value::NIL,
            false_input: zero,
        },
        FlagPredicateInputs::Unary {
            op: Op::Not,
            true_input: Value::NIL,
            false_input: zero,
        },
        FlagPredicateInputs::Unary {
            op: Op::Consp,
            true_input: cons,
            false_input: Value::NIL,
        },
        FlagPredicateInputs::Unary {
            op: Op::Stringp,
            true_input: string,
            false_input: Value::NIL,
        },
        FlagPredicateInputs::Unary {
            op: Op::Listp,
            true_input: Value::NIL,
            false_input: zero,
        },
        FlagPredicateInputs::Unary {
            op: Op::Symbolp,
            true_input: Value::NIL,
            false_input: zero,
        },
        FlagPredicateInputs::Unary {
            op: Op::Integerp,
            true_input: zero,
            false_input: float,
        },
        FlagPredicateInputs::Unary {
            op: Op::Numberp,
            true_input: float,
            false_input: Value::symbol("hof-flag-nonnumeric"),
        },
        FlagPredicateInputs::Binary {
            op: Op::Eq,
            rhs: zero,
            true_input: zero,
            false_input: one,
        },
        FlagPredicateInputs::Binary {
            op: Op::Eqlsign,
            rhs: zero,
            true_input: zero,
            false_input: one,
        },
        FlagPredicateInputs::Binary {
            op: Op::Lss,
            rhs: one,
            true_input: zero,
            false_input: one,
        },
        FlagPredicateInputs::Binary {
            op: Op::Gtr,
            rhs: zero,
            true_input: one,
            false_input: zero,
        },
        FlagPredicateInputs::Binary {
            op: Op::Leq,
            rhs: zero,
            true_input: zero,
            false_input: one,
        },
        FlagPredicateInputs::Binary {
            op: Op::Geq,
            rhs: zero,
            true_input: zero,
            false_input: Value::make_int(-1),
        },
    ];
    for case in cases {
        let (producer, constants, inputs) = match case {
            FlagPredicateInputs::Unary {
                op,
                true_input,
                false_input,
            } => (
                vec![Op::StackRef(0), op],
                Vec::new(),
                [true_input, false_input],
            ),
            FlagPredicateInputs::Binary {
                op,
                rhs,
                true_input,
                false_input,
            } => (
                vec![Op::StackRef(0), Op::Constant(0), op],
                vec![rhs],
                [true_input, false_input],
            ),
        };
        for branch in CallbackBranch::iter() {
            let mut ops = producer.clone();
            let mut constants = constants.clone();
            let fallthrough = u16::try_from(constants.len()).unwrap();
            constants.push(Value::make_int(111));
            let taken = u16::try_from(constants.len()).unwrap();
            constants.push(Value::make_int(222));
            let target = u32::try_from(ops.len() + 3).unwrap();
            ops.push(match branch {
                CallbackBranch::PopOnNil => Op::GotoIfNil(target),
                CallbackBranch::PopOnNonNil => Op::GotoIfNotNil(target),
                CallbackBranch::KeepOnNil => Op::GotoIfNilElsePop(target),
                CallbackBranch::KeepOnNonNil => Op::GotoIfNotNilElsePop(target),
            });
            ops.extend([Op::Constant(fallthrough), Op::Return]);
            match branch {
                CallbackBranch::PopOnNil | CallbackBranch::PopOnNonNil => {
                    ops.push(Op::Constant(taken))
                }
                CallbackBranch::KeepOnNil | CallbackBranch::KeepOnNonNil => {}
            }
            ops.push(Op::Return);
            let callback = Value::make_bytecode(function(ops, constants));
            let sequence = Value::list(inputs.to_vec());
            let expected = match branch {
                CallbackBranch::PopOnNil => [Value::make_int(111), Value::make_int(222)],
                CallbackBranch::PopOnNonNil => [Value::make_int(222), Value::make_int(111)],
                CallbackBranch::KeepOnNil => [Value::make_int(111), Value::NIL],
                CallbackBranch::KeepOnNonNil => [Value::T, Value::make_int(111)],
            };
            assert_flag_mapping_native(&mut ctx, callback, sequence, &expected);
        }
    }
    ctx.pop_vm_root_frame();
}

fn flag_shared_consumer_ops() -> Vec<Op> {
    vec![
        Op::StackRef(0),
        Op::Constant(0),
        Op::Eq,
        Op::GotoIfNil(6),
        Op::Constant(0),
        Op::Goto(9),
        Op::StackRef(0),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNotNilElsePop(11),
        Op::Constant(1),
        Op::Return,
    ]
}

#[test]
fn inline_hof_flags_shared_consumer_keeps_zero_and_booleans_tagged() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    let callback = Value::make_bytecode(function(
        flag_shared_consumer_ops(),
        vec![Value::make_int(0), Value::NIL],
    ));
    let sequence = Value::list(vec![
        Value::make_int(0),
        Value::make_int(1),
        Value::make_int(-1),
    ]);
    assert_flag_mapping_native(
        &mut ctx,
        callback,
        sequence,
        &[Value::make_int(0), Value::T, Value::NIL],
    );
}

#[test]
fn inline_hof_flags_step_plan_owns_only_adjacent_same_block_pairs() {
    use super::super::{CallbackBlock, CallbackBlockIndex, CallbackStep};
    let constants = [
        Value::make_int(0),
        Value::make_int(111),
        Value::make_int(222),
    ];
    let adjacent = vec![
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(5),
        Op::Constant(1),
        Op::Return,
        Op::Constant(2),
        Op::Return,
    ];
    let cfg = analyze_cfg(&adjacent, &constants, None, 1).unwrap();
    let steps: Vec<_> = CallbackBlock::new(&adjacent, &cfg, CallbackBlockIndex(0))
        .unwrap()
        .into_steps()
        .collect();
    assert_eq!(steps.len(), 2);
    assert!(matches!(&steps[0], CallbackStep::Materialized { pc, .. } if pc.0 == 0));
    assert!(matches!(&steps[1], CallbackStep::FusedBranch(fused)
        if fused.producer_pc.0 == 1 && fused.fallthrough.0 == 3 && fused.target.0 == 5));
    let unsupported = vec![
        Op::Constant(0),
        Op::GotoIfNil(4),
        Op::Constant(1),
        Op::Return,
        Op::Constant(2),
        Op::Return,
    ];
    let nonadjacent = vec![
        Op::Constant(0),
        Op::Gtr,
        Op::Dup,
        Op::GotoIfNil(6),
        Op::Constant(1),
        Op::Return,
        Op::Constant(2),
        Op::Return,
    ];
    for ops in [unsupported, nonadjacent] {
        let cfg = analyze_cfg(&ops, &constants, None, 1).unwrap();
        for step in CallbackBlock::new(&ops, &cfg, CallbackBlockIndex(0))
            .unwrap()
            .into_steps()
        {
            match step {
                CallbackStep::Materialized { .. } => {}
                CallbackStep::FusedBranch(_) => {
                    panic!("unsupported or nonadjacent pair must stay tagged")
                }
            }
        }
    }
    let shared = flag_shared_consumer_ops();
    let cfg = analyze_cfg(&shared, &[Value::make_int(0), Value::NIL], None, 1).unwrap();
    for entry in [6, 9] {
        let block_index = cfg
            .leaders
            .iter()
            .position(|&leader| leader == entry)
            .unwrap();
        let steps: Vec<_> = CallbackBlock::new(&shared, &cfg, CallbackBlockIndex(block_index))
            .unwrap()
            .into_steps()
            .collect();
        for step in &steps {
            match step {
                CallbackStep::Materialized { .. } => {}
                CallbackStep::FusedBranch(_) => {
                    panic!("a leader splits producer from shared consumer")
                }
            }
        }
        if entry == 6 {
            assert!(
                matches!(steps.last(), Some(CallbackStep::Materialized { pc, op: Op::Gtr }) if pc.0 == 8)
            );
        } else {
            assert!(
                matches!(steps.first(), Some(CallbackStep::Materialized { pc, op: Op::GotoIfNotNilElsePop(11) }) if pc.0 == 9)
            );
        }
    }
}

fn flag_capture_before_comparison(ctx: &mut Context, cell: Value) -> Value {
    let prototype = Value::make_bytecode(function(
        vec![
            Op::Constant(0),
            Op::Dup,
            Op::CarSafe,
            Op::Add1,
            Op::Setcar,
            Op::Pop,
            Op::StackRef(0),
            Op::Constant(1),
            Op::Gtr,
            Op::GotoIfNil(12),
            Op::Constant(2),
            Op::Return,
            Op::Constant(3),
            Op::Return,
        ],
        vec![
            Value::symbol("V0"),
            Value::make_int(0),
            Value::make_int(777),
            Value::make_int(111),
        ],
    ));
    ctx.apply2(Value::symbol("make-closure"), prototype, cell)
        .unwrap()
}

#[test]
fn inline_hof_flags_producer_deopt_keeps_resume_frontier() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    let cell = Value::cons(Value::make_int(0), Value::NIL);
    let callback = flag_capture_before_comparison(&mut ctx, cell);
    let sequence = Value::list(vec![
        Value::make_int(1),
        Value::make_float(1.5),
        Value::make_int(-1),
    ]);
    ctx.push_vm_root_frame();
    ctx.push_vm_frame_root(callback);
    ctx.push_vm_frame_root(sequence);
    let roots = ctx.save_vm_frame_roots();
    let source = caller(callback);
    assert_hof_admitted(&source);
    let leaf = compile::compile_bytecode_function_with(&source, Some(&ctx.obarray)).unwrap();
    let _ = ctx.debug_on_next_call_is_armed();
    let NativeRun::DeoptAt(resume) = leaf.call(&mut ctx as *mut Context as *mut u8, &[sequence])
    else {
        panic!("cold mixed numeric producer must transfer at its own opcode");
    };
    let chain = resume.inlined.as_ref().expect("precise HOF chain");
    let state = chain.hof.as_ref().unwrap();
    assert!(state.callback_entered);
    assert_eq!(state.index, 1);
    assert_eq!(state.item.as_float(), Some(1.5));
    assert_eq!(chain.frames[0].pc, 8);
    assert_eq!(chain.frames[0].stack.len(), 3);
    assert_eq!(chain.frames[0].stack[0].as_float(), Some(1.5));
    assert_eq!(chain.frames[0].stack[1].as_float(), Some(1.5));
    assert_eq!(chain.frames[0].stack[2], Value::make_int(0));
    assert_eq!(
        cell.cons_car().as_int(),
        Some(2),
        "both committed stores precede the transfer"
    );
    assert_eq!(
        ctx.vm_frame_root_slots(state.sink_base, 1),
        &[Value::make_int(777)]
    );
    let result =
        compile::resumed_chain::resume_deopt(&mut ctx, &source, Value::NIL, &leaf, *resume)
            .unwrap();
    assert_eq!(print_value(&result), "(777 777 111)");
    assert_eq!(
        cell.cons_car().as_int(),
        Some(3),
        "the interrupted callback store is not replayed"
    );
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty() && ctx.bc_buf.is_empty() && ctx.bc_frames.is_empty());
    assert!(ctx.jit_bind_stack.is_empty());
    assert_eq!(ctx.save_vm_frame_roots(), roots);
    ctx.pop_vm_root_frame();
}

#[test]
fn inline_hof_flags_signal_cleanup_allows_next_mapping() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    ctx.gc_stress = false;
    let cell = Value::cons(Value::make_int(0), Value::NIL);
    let callback = flag_capture_before_comparison(&mut ctx, cell);
    let bad = Value::symbol("hof-flag-producer-not-a-number");
    let sequence = Value::list(vec![Value::make_int(1), bad, Value::make_int(2)]);
    ctx.push_vm_root_frame();
    ctx.push_vm_frame_root(callback);
    ctx.push_vm_frame_root(sequence);
    let roots = ctx.save_vm_frame_roots();
    let source = caller(callback);
    assert_hof_admitted(&source);
    let leaf = compile::compile_bytecode_function_with(&source, Some(&ctx.obarray)).unwrap();
    let _ = ctx.debug_on_next_call_is_armed();
    let NativeRun::DeoptAt(resume) = leaf.call(&mut ctx as *mut Context as *mut u8, &[sequence])
    else {
        panic!("the bad numeric operand must leave native code before signaling");
    };
    let chain = resume.inlined.as_ref().unwrap();
    assert_eq!(chain.frames[0].pc, 8);
    assert_eq!(chain.hof.as_ref().unwrap().index, 1);
    assert_eq!(cell.cons_car().as_int(), Some(2));
    let flow = compile::resumed_chain::resume_deopt(&mut ctx, &source, Value::NIL, &leaf, *resume);
    match flow.kinded() {
        Err(FlowKind::Signal(signal)) => {
            assert_eq!(signal.symbol, intern("wrong-type-argument"));
            assert_eq!(signal.data.len(), 2);
            assert_eq!(signal.data[0], Value::symbol("number-or-marker-p"));
            assert_eq!(signal.data[1], bad);
        }
        other => panic!("the callback must signal its original operand error: {other:?}"),
    }
    assert_eq!(
        cell.cons_car().as_int(),
        Some(2),
        "the failed mapping never reaches its third element"
    );
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty() && ctx.bc_buf.is_empty() && ctx.bc_frames.is_empty());
    assert!(ctx.jit_bind_stack.is_empty());
    assert_eq!(ctx.save_vm_frame_roots(), roots);
    let valid = Value::list(vec![Value::make_int(2)]);
    ctx.push_vm_frame_root(valid);
    let next_roots = ctx.save_vm_frame_roots();
    let _ = ctx.debug_on_next_call_is_armed();
    let NativeRun::Ok(bits) = leaf.call(&mut ctx as *mut Context as *mut u8, &[valid]) else {
        panic!("a handled callback signal must leave the next valid mapping usable");
    };
    assert_eq!(print_value(&Value::from_bits(bits)), "(777)");
    assert_eq!(cell.cons_car().as_int(), Some(3));
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty() && ctx.bc_buf.is_empty() && ctx.bc_frames.is_empty());
    assert!(ctx.jit_bind_stack.is_empty());
    assert_eq!(ctx.save_vm_frame_roots(), next_roots);
    ctx.pop_vm_root_frame();
}
