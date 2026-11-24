//! OSR's physical frame stays in the existing driver while a deopt finishes
//! its inner calls. These tests exercise that ownership boundary directly;
//! the production entry and readback tests separately prove native transfer.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::eval::{BcFrame, ConditionFrame, Context, ResumeTarget};
use neovm_value::error::FlowRef;
use neovm_value::value::LambdaParams;

fn code(nargs: usize, ops: Vec<Op>, constants: Vec<Value>) -> Value {
    let mut code = ByteCodeFunction::new(LambdaParams::simple(
        (0..nargs)
            .map(|n| intern(&format!("osr-chain-arg-{n}")))
            .collect(),
    ));
    code.set_lexical(true);
    code.set_ops(ops);
    code.set_constants(constants.into());
    code.set_max_stack(crate::emacs_core::bytecode::StackDepth::for_test(16));
    code.seal_hand_assembled_ops_for_test();
    Value::make_bytecode(code)
}

fn context() -> Context {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    ctx.push_vm_root_frame();
    ctx.depth = 1;
    ctx
}

fn park_physical(ctx: &mut Context, function: Value, base: usize) {
    ctx.bc_frames.push(BcFrame {
        base,
        fun: function,
    });
}

#[test]
fn osr_chain_in_place_returns_the_call_result_without_running_the_physical_tail() {
    let mut ctx = context();
    let inner = code(1, vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]);
    let physical = code(
        0,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Call(1),
            Op::Constant(2),
            Op::Add,
            Op::Return,
        ],
        vec![inner, Value::fixnum(5), Value::fixnum(100)],
    );
    let outer = Value::fixnum(700);
    ctx.bc_buf.extend_from_slice(&[outer, Value::NIL]);
    park_physical(&mut ctx, physical, 1);
    let physical_stack = [inner, Value::fixnum(5)];
    let inner_stack = [Value::fixnum(5)];
    let frames = [InlinedChainFrame {
        frame: ChainFrame {
            function: inner,
            pc: 0,
            stack: &inner_stack,
            handlers: 0,
            binds: &[],
        },
        link: ChainLink::Bcall { nargs: 1 },
        backtrace: ChainBacktrace::Virtual,
    }];
    let mut vm = Vm::from_context(&mut ctx);
    let next = vm
        .run_resumed_chain_in_place(
            physical.get_bytecode_data().unwrap(),
            ChainFrame {
                function: physical,
                pc: 2,
                stack: &physical_stack,
                handlers: 0,
                binds: &[],
            },
            &frames,
            1,
            0,
            0,
        )
        .unwrap();
    assert_eq!(next, 3);
    assert_eq!(vm.ctx.bc_buf, [outer, Value::fixnum(6)]);
    assert_eq!(vm.ctx.depth, 1);
    assert_eq!(vm.ctx.bc_frames.len(), 1, "the physical frame stays parked");
    assert!(vm.ctx.specpdl.is_empty());
    let mut pc = next;
    let result = vm.run_loop(
        physical.get_bytecode_data().unwrap(),
        1,
        &mut pc,
        &mut HandlerStack::new(),
        &mut BindStack::new(),
        true,
        true,
    );
    let result = vm.cleanup_bytecode_frame(result, 0, 0, 1).unwrap();
    assert_eq!(result, Value::fixnum(106));
    assert_eq!(vm.ctx.bc_buf, [outer]);
    assert!(vm.ctx.bc_frames.is_empty());
}

#[test]
fn osr_chain_in_place_nested_calls_honor_materialized_exit_debugger_replacement() {
    let mut ctx = context();
    ctx.eval_str("(setq debugger (lambda (&rest args) 70))")
        .unwrap();
    let inner = code(1, vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]);
    let middle = code(
        1,
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Add1,
            Op::Return,
        ],
        vec![inner],
    );
    let physical = code(
        0,
        vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        vec![middle, Value::fixnum(5)],
    );
    let physical_stack = [middle, Value::fixnum(5)];
    let middle_stack = [Value::fixnum(5), inner, Value::fixnum(5)];
    let inner_stack = [Value::fixnum(5)];
    ctx.bc_buf.extend_from_slice(&physical_stack);
    park_physical(&mut ctx, physical, 0);
    ctx.push_backtrace_frame(middle, &[Value::fixnum(5)]);
    assert!(ctx.set_backtrace_debug_on_exit(0, true));
    ctx.depth += 1;
    let frames = [
        InlinedChainFrame {
            frame: ChainFrame {
                function: middle,
                pc: 2,
                stack: &middle_stack,
                handlers: 0,
                binds: &[],
            },
            link: ChainLink::Bcall { nargs: 1 },
            backtrace: ChainBacktrace::Materialized { index: 0 },
        },
        InlinedChainFrame {
            frame: ChainFrame {
                function: inner,
                pc: 0,
                stack: &inner_stack,
                handlers: 0,
                binds: &[],
            },
            link: ChainLink::Bcall { nargs: 1 },
            backtrace: ChainBacktrace::Virtual,
        },
    ];
    let next = Vm::from_context(&mut ctx)
        .run_resumed_chain_in_place(
            physical.get_bytecode_data().unwrap(),
            ChainFrame {
                function: physical,
                pc: 2,
                stack: &physical_stack,
                handlers: 0,
                binds: &[],
            },
            &frames,
            0,
            0,
            0,
        )
        .unwrap();
    assert_eq!(next, 3);
    assert_eq!(ctx.bc_buf, [Value::fixnum(70)]);
    assert_eq!(ctx.depth, 1);
    assert_eq!(ctx.bc_frames.len(), 1);
    assert!(ctx.specpdl.is_empty());
}

fn throwing_call() -> (Value, Value) {
    let inner = code(
        0,
        vec![Op::Constant(0), Op::Constant(1), Op::Throw, Op::Return],
        vec![Value::symbol("osr-chain-tag"), Value::fixnum(17)],
    );
    let physical = code(
        0,
        vec![Op::Constant(0), Op::Call(0), Op::Return],
        vec![inner],
    );
    (physical, inner)
}

#[test]
fn osr_chain_in_place_error_leaves_physical_bindings_for_its_owner_to_unwind() {
    let mut ctx = context();
    let (physical, inner) = throwing_call();
    let dynamic = intern("osr-chain-dynamic");
    ctx.obarray.set_symbol_value_id(dynamic, Value::fixnum(9));
    ctx.try_specbind(dynamic, Value::fixnum(99)).unwrap();
    ctx.bc_buf.push(Value::NIL);
    park_physical(&mut ctx, physical, 0);
    let physical_stack = [inner];
    let frames = [InlinedChainFrame {
        frame: ChainFrame {
            function: inner,
            pc: 0,
            stack: &[],
            handlers: 0,
            binds: &[],
        },
        link: ChainLink::Bcall { nargs: 0 },
        backtrace: ChainBacktrace::Virtual,
    }];
    let mut vm = Vm::from_context(&mut ctx);
    let flow = vm
        .run_resumed_chain_in_place(
            physical.get_bytecode_data().unwrap(),
            ChainFrame {
                function: physical,
                pc: 1,
                stack: &physical_stack,
                handlers: 0,
                binds: &[0],
            },
            &frames,
            0,
            1,
            0,
        )
        .unwrap_err();
    // With no registered catcher, Tier-0 turns the throw into GNU's
    // no-catch signal; its physical owner still owns this binding.
    assert!(matches!(flow.kind(), FlowRef::Signal(_)));
    assert_eq!(vm.ctx.depth, 1);
    assert_eq!(vm.ctx.specpdl.len(), 1);
    assert_eq!(vm.ctx.bc_frames.len(), 1);
    assert_eq!(
        vm.ctx.obarray.symbol_value_id_copied(dynamic),
        Some(Value::fixnum(99))
    );
    vm.cleanup_bytecode_frame(Err(flow), 0, 0, 0).unwrap_err();
    assert_eq!(
        vm.ctx.obarray.symbol_value_id_copied(dynamic),
        Some(Value::fixnum(9))
    );
    assert!(vm.ctx.bc_buf.is_empty());
    assert!(vm.ctx.bc_frames.is_empty());
    assert!(vm.ctx.specpdl.is_empty());
}

#[test]
fn osr_chain_in_place_error_is_offered_to_the_existing_physical_handler() {
    let mut ctx = context();
    let (physical, inner) = throwing_call();
    ctx.bc_buf.push(Value::NIL);
    park_physical(&mut ctx, physical, 0);
    ctx.push_condition_frame(ConditionFrame::Catch {
        tag: Value::symbol("osr-chain-tag"),
        resume: ResumeTarget::VmCatch {
            resume_id: 1,
            target: 2,
            stack_len: 0,
            spec_depth: 0,
            bind_stack_len: 0,
        },
    });
    let physical_stack = [inner];
    let frames = [InlinedChainFrame {
        frame: ChainFrame {
            function: inner,
            pc: 0,
            stack: &[],
            handlers: 0,
            binds: &[],
        },
        link: ChainLink::Bcall { nargs: 0 },
        backtrace: ChainBacktrace::Virtual,
    }];
    let mut vm = Vm::from_context(&mut ctx);
    let flow = vm
        .run_resumed_chain_in_place(
            physical.get_bytecode_data().unwrap(),
            ChainFrame {
                function: physical,
                pc: 1,
                stack: &physical_stack,
                handlers: 1,
                binds: &[],
            },
            &frames,
            0,
            0,
            0,
        )
        .unwrap_err();
    assert_eq!(vm.ctx.condition_stack_len(), 1);
    let mut pc = 1;
    let mut handlers = HandlerStack::new();
    handlers.push(Handler::Condition);
    let mut binds = BindStack::new();
    vm.resume_nonlocal(
        physical.get_bytecode_data().unwrap(),
        &mut pc,
        &mut handlers,
        &mut binds,
        flow,
    )
    .unwrap();
    assert_eq!(pc, 2);
    assert_eq!(vm.ctx.bc_buf, [Value::fixnum(17)]);
    assert!(handlers.is_empty());
    assert_eq!(vm.ctx.condition_stack_len(), 0);
    let result = vm.run_loop(
        physical.get_bytecode_data().unwrap(),
        0,
        &mut pc,
        &mut handlers,
        &mut binds,
        true,
        true,
    );
    assert_eq!(
        vm.cleanup_bytecode_frame(result, 0, 0, 0).unwrap(),
        Value::fixnum(17)
    );
    assert!(vm.ctx.bc_frames.is_empty());
}

#[test]
fn osr_chain_in_place_roots_the_physical_residual_across_inner_collection() {
    let mut ctx = context();
    let inner = code(
        1,
        vec![
            Op::Constant(0),
            Op::Call(0),
            Op::Pop,
            Op::StackRef(0),
            Op::Return,
        ],
        vec![Value::symbol("garbage-collect")],
    );
    let physical = code(
        1,
        vec![
            Op::Constant(0),
            Op::Nil,
            Op::Call(1),
            Op::List(2),
            Op::Return,
        ],
        vec![inner],
    );
    let residual = Value::cons(Value::fixnum(123), Value::fixnum(456));
    let physical_stack = [residual, inner, Value::NIL];
    let inner_stack = [Value::NIL];
    ctx.bc_buf.push(Value::NIL);
    park_physical(&mut ctx, physical, 0);
    let frames = [InlinedChainFrame {
        frame: ChainFrame {
            function: inner,
            pc: 0,
            stack: &inner_stack,
            handlers: 0,
            binds: &[],
        },
        link: ChainLink::Bcall { nargs: 1 },
        backtrace: ChainBacktrace::Virtual,
    }];
    let next = Vm::from_context(&mut ctx)
        .run_resumed_chain_in_place(
            physical.get_bytecode_data().unwrap(),
            ChainFrame {
                function: physical,
                pc: 2,
                stack: &physical_stack,
                handlers: 0,
                binds: &[],
            },
            &frames,
            0,
            0,
            0,
        )
        .unwrap();
    assert_eq!(next, 3);
    assert_eq!(ctx.bc_buf[0], residual);
    assert_eq!(ctx.bc_buf[0].cons_car(), Value::fixnum(123));
    assert_eq!(ctx.bc_buf[0].cons_cdr(), Value::fixnum(456));
    assert_eq!(ctx.bc_buf[1], Value::NIL);
    assert_eq!(ctx.depth, 1);
    assert!(ctx.specpdl.is_empty());
}

#[cfg(feature = "jit")]
#[test]
fn osr_hof_chain_in_place_resumes_the_current_callback_and_mapping_once() {
    use crate::emacs_core::jit::compile::{self, Inline2Mode};
    // One `osr_transfer` call must transfer: the OSR leaf is compiled in
    // line, whatever `NEOVM_JIT_BG` the suite runs under.
    let _mode =
        crate::emacs_core::jit::bg::TestModeScope::enter(crate::emacs_core::jit::bg::BgMode::Sync);
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    compile::force_inline2_for_test(Some(Inline2Mode::Hof));
    compile::force_deopt_for_test(false);
    let mut ctx = context();
    crate::emacs_core::jit::force_jit_off_for_test(false);
    let inner = code(1, vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]);
    inner
        .get_bytecode_data()
        .unwrap()
        .jit_runtime()
        .set_hot_for_test();
    let physical = code(
        1,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![Value::symbol("mapcar"), inner],
    );
    let maximum = Value::fixnum(Value::MOST_POSITIVE_FIXNUM);
    let sequence = Value::list_from_slice(&[maximum, Value::fixnum(5)]);
    ctx.bc_buf.push(sequence);
    park_physical(&mut ctx, physical, 0);
    let before = crate::emacs_core::jit::cache::OSR_TRANSFER_COUNT
        .load(std::sync::atomic::Ordering::Relaxed);
    let mut vm = Vm::from_context(&mut ctx);
    let mut aux = InterpreterFrameAuxStack::new(HandlerStack::new(), BindStack::new());
    let outcome = vm.osr_transfer::<false>(physical.get_bytecode_data().unwrap(), 0, 0, &mut aux);
    let OsrOutcome::Interpret { pc: next, .. } = outcome else {
        panic!("a callback overflow must hand the physical frame back in place");
    };
    assert_eq!(next, 4);
    assert_eq!(
        crate::emacs_core::jit::cache::OSR_TRANSFER_COUNT
            .load(std::sync::atomic::Ordering::Relaxed),
        before + 1,
    );
    assert_eq!(vm.ctx.depth, 1);
    assert!(vm.ctx.specpdl.is_empty());
    assert_eq!(vm.ctx.bc_frames.len(), 1);
    let mapped = vm.ctx.bc_buf[1];
    assert!(mapped.cons_car().is_bignum());
    assert_eq!(mapped.cons_cdr().cons_car(), Value::fixnum(6));
    assert!(mapped.cons_cdr().cons_cdr().is_nil());
    let mut pc = next;
    let result = vm.run_loop(
        physical.get_bytecode_data().unwrap(),
        0,
        &mut pc,
        &mut HandlerStack::new(),
        &mut BindStack::new(),
        true,
        true,
    );
    assert_eq!(vm.cleanup_bytecode_frame(result, 0, 0, 0).unwrap(), mapped);
    assert!(vm.ctx.bc_buf.is_empty());
    assert!(vm.ctx.bc_frames.is_empty());
    compile::force_inline2_for_test(None);
    crate::emacs_core::jit::inline::force_inline_for_test(None);
}

/// A verified driver has no frame-depth check. The same precise deopt that
/// the checked driver resumes in place must finish on the checked driver
/// instead of continuing inside the verified monomorph.
#[cfg(feature = "jit")]
#[test]
fn verified_osr_precise_deopt_finishes_on_the_checked_driver() {
    use crate::emacs_core::jit::compile::{self, Inline2Mode};
    let _mode =
        crate::emacs_core::jit::bg::TestModeScope::enter(crate::emacs_core::jit::bg::BgMode::Sync);
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    compile::force_inline2_for_test(Some(Inline2Mode::Hof));
    compile::force_deopt_for_test(false);
    let mut ctx = context();
    crate::emacs_core::jit::force_jit_off_for_test(false);
    let inner = code(1, vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]);
    inner
        .get_bytecode_data()
        .unwrap()
        .jit_runtime()
        .set_hot_for_test();
    let physical = code(
        1,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![Value::symbol("mapcar"), inner],
    );
    let maximum = Value::fixnum(Value::MOST_POSITIVE_FIXNUM);
    let sequence = Value::list_from_slice(&[maximum, Value::fixnum(5)]);
    ctx.bc_buf.push(sequence);
    park_physical(&mut ctx, physical, 0);
    let mut vm = Vm::from_context(&mut ctx);
    let mut aux = InterpreterFrameAuxStack::new(HandlerStack::new(), BindStack::new());
    let outcome = vm.osr_transfer::<true>(physical.get_bytecode_data().unwrap(), 0, 0, &mut aux);
    let mapped = match outcome {
        OsrOutcome::Returned(mapped) => mapped,
        OsrOutcome::Interpret { pc, .. } => {
            panic!("verified precise deopt resumed in place at pc {pc}")
        }
        OsrOutcome::Exited => panic!("verified precise deopt exited instead of returning"),
    };
    assert!(mapped.cons_car().is_bignum());
    assert_eq!(mapped.cons_cdr().cons_car(), Value::fixnum(6));
    assert!(mapped.cons_cdr().cons_cdr().is_nil());
    compile::force_inline2_for_test(None);
    crate::emacs_core::jit::inline::force_inline_for_test(None);
}

/// Lazy mapcar body whose `Dup` is proven at the same depth as `Call(2)`.
///
/// Bytes are constant, constant, stack-ref 2, dup, pop, call 2, return.
/// Entry depth is the one required argument (fixnum descriptor 257). Dup
/// and the call are both depth 4; the pop between them is depth 5, so the
/// instruction after Dup is not the post-call depth.
#[cfg(feature = "jit")]
fn lazy_hof_physical(inner: Value) -> Value {
    let params = crate::emacs_core::bytecode::FunctionParams::try_from(Value::fixnum(257))
        .expect("fixnum arglist 257");
    let mut func = ByteCodeFunction::new(params);
    // OSR compiles only a lexical body. The fixnum descriptor already puts
    // the argument on the stack, so this flag does not change entry depth.
    func.set_lexical(true);
    func.set_max_stack(crate::emacs_core::bytecode::StackDepth::for_test(8));
    func.set_constants(vec![Value::symbol("mapcar"), inner].into());
    func.gnu_bytecode_bytes = Some(neovm_value::tagged::header::LispByteVec::owned(vec![
        192, 193, 2, 137, 136, 34, 135,
    ]));
    func.defer_gnu_decode();
    let value = Value::make_bytecode(func);
    let data = value.get_bytecode_data().expect("bytecode");
    assert!(
        data.lazy_gnu_code().is_some(),
        "the physical body must keep a lazy GNU cell"
    );
    assert!(data.executes_verified_ops(), "the lazy body must verify");
    assert_eq!(
        data.executable_ops(),
        &[
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Dup,
            Op::Pop,
            Op::Call(2),
            Op::Return,
        ]
    );
    let proof = data
        .lazy_gnu_code()
        .as_ref()
        .and_then(|lazy| lazy.proof())
        .expect("published stack proof");
    assert!(proof.proves_depth(3, 4), "Dup is proven at the call depth");
    assert!(proof.proves_depth(5, 4), "Call(2) is proven at depth 4");
    assert!(
        !proof.proves_depth(4, 2),
        "the instruction after Dup is not the post-call depth"
    );
    assert!(proof.proves_depth(6, 2), "Return is the post-call depth");
    value
}

#[cfg(feature = "jit")]
fn arm_hof_osr() {
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    crate::emacs_core::jit::compile::force_inline2_for_test(Some(
        crate::emacs_core::jit::compile::Inline2Mode::Hof,
    ));
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    crate::emacs_core::jit::force_jit_off_for_test(false);
}

#[cfg(feature = "jit")]
fn disarm_hof_osr() {
    crate::emacs_core::jit::compile::force_inline2_for_test(None);
    crate::emacs_core::jit::inline::force_inline_for_test(None);
}

/// The verified driver installs a real lazy HOF deopt in place. The op at
/// the recorded pc is `Call(2)`, and the proof at `pc + 1` is `len - 2`.
#[cfg(feature = "jit")]
#[test]
fn verified_osr_hof_lazy_body_installs_in_place() {
    let _mode =
        crate::emacs_core::jit::bg::TestModeScope::enter(crate::emacs_core::jit::bg::BgMode::Sync);
    let mut ctx = context();
    arm_hof_osr();
    let inner = code(1, vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]);
    inner
        .get_bytecode_data()
        .unwrap()
        .jit_runtime()
        .set_hot_for_test();
    let physical = lazy_hof_physical(inner);
    let maximum = Value::fixnum(Value::MOST_POSITIVE_FIXNUM);
    let sequence = Value::list_from_slice(&[maximum, Value::fixnum(5)]);
    ctx.bc_buf.push(sequence);
    park_physical(&mut ctx, physical, 0);
    let before = super::super::VERIFIED_RESUME_ENTRIES.load(std::sync::atomic::Ordering::Relaxed);
    let mut vm = Vm::from_context(&mut ctx);
    let mut aux = InterpreterFrameAuxStack::new(HandlerStack::new(), BindStack::new());
    let outcome = vm.osr_transfer::<true>(physical.get_bytecode_data().unwrap(), 0, 0, &mut aux);
    let OsrOutcome::Interpret { pc: next, .. } = outcome else {
        let kind = match outcome {
            OsrOutcome::Returned(_) => "returned",
            OsrOutcome::Exited => "exited",
            OsrOutcome::Interpret { .. } => "interpret",
        };
        panic!("a verified lazy HOF deopt must install in place ({kind})");
    };
    assert_eq!(
        next, 6,
        "in place continues at the instruction after Call(2)"
    );
    assert_eq!(
        super::super::VERIFIED_RESUME_ENTRIES.load(std::sync::atomic::Ordering::Relaxed),
        before + 1,
        "the in-place verified resume must count"
    );
    assert_eq!(vm.ctx.depth, 1);
    assert!(vm.ctx.specpdl.is_empty());
    assert_eq!(vm.ctx.bc_frames.len(), 1);
    let mapped = vm.ctx.bc_buf[1];
    assert!(mapped.cons_car().is_bignum());
    assert_eq!(mapped.cons_cdr().cons_car(), Value::fixnum(6));
    assert!(mapped.cons_cdr().cons_cdr().is_nil());
    disarm_hof_osr();
}

/// A deopt map that names `Dup` at the call's proven depth must not install
/// the verified driver at `pc + 1` with `len - 2`. That point is a `Pop`
/// proven at depth 5. The resume stays off the verified counter.
#[cfg(feature = "jit")]
#[test]
fn verified_osr_hof_non_call_pc_does_not_install_in_place() {
    let _mode =
        crate::emacs_core::jit::bg::TestModeScope::enter(crate::emacs_core::jit::bg::BgMode::Sync);
    let mut ctx = context();
    arm_hof_osr();
    let inner = code(1, vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]);
    inner
        .get_bytecode_data()
        .unwrap()
        .jit_runtime()
        .set_hot_for_test();
    let physical = lazy_hof_physical(inner);
    let maximum = Value::fixnum(Value::MOST_POSITIVE_FIXNUM);
    let sequence = Value::list_from_slice(&[maximum, Value::fixnum(5)]);
    ctx.bc_buf.push(sequence);
    park_physical(&mut ctx, physical, 0);
    let before = super::super::VERIFIED_RESUME_ENTRIES.load(std::sync::atomic::Ordering::Relaxed);
    // Dup is proven at depth 4, the same depth the call's deopt stack has.
    super::super::HOF_DEOPT_PC_OVERRIDE.with(|slot| slot.set(Some(3)));
    let mut vm = Vm::from_context(&mut ctx);
    let mut aux = InterpreterFrameAuxStack::new(HandlerStack::new(), BindStack::new());
    let outcome = vm.osr_transfer::<true>(physical.get_bytecode_data().unwrap(), 0, 0, &mut aux);
    if let OsrOutcome::Interpret { pc, .. } = outcome {
        panic!("a non-Call HOF pc must not install in place at pc {pc}");
    }
    assert_eq!(
        super::super::VERIFIED_RESUME_ENTRIES.load(std::sync::atomic::Ordering::Relaxed),
        before,
        "a rejected HOF in-place resume must not count as verified"
    );
    assert_eq!(
        vm.ctx.bc_buf.as_slice(),
        &[sequence],
        "a rejected HOF resume must not replace the parked frame in place"
    );
    disarm_hof_osr();
}
