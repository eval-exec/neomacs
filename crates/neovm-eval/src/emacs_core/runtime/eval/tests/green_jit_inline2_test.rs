//! Inline2's existing guard exits materialize virtual frames before Lisp runs.
//!
//! Forward-only virtual bodies cannot call Lisp, park, or collect. These
//! fixtures prove both sides of that boundary: physical compiled callers admit
//! and yield around real inline regions, and a failing virtual guard rebuilds
//! its exact callee before a signal hook admits/yields and another owner GCs.

use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op, StackDepth};
use crate::emacs_core::eval::{Context, SpecBinding, ThreadingMode};
use crate::emacs_core::jit::RuntimeStateTieringExt;
use crate::emacs_core::jit::{
    cache,
    compile::{self, Inline2Mode, NativeRun, ProfitGateTestPolicy},
    inline,
};
#[doc(hidden)]
pub use crate::emacs_core::subr::{FixedMin2, SubrSpec};
use neovm_value::error::{EvalResult, FlowRef};
use neovm_value::intern::intern;
use neovm_value::value::{LambdaParams, Value};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

struct Knobs {
    jit_off: bool,
    profit: ProfitGateTestPolicy,
}

impl Knobs {
    #[doc(hidden)]
    pub fn enter() -> Self {
        let jit_off = crate::emacs_core::jit::jit_forced_off_for_test();
        crate::emacs_core::jit::force_jit_off_for_test(false);
        inline::force_inline_for_test(Some(true));
        compile::force_inline2_for_test(Some(Inline2Mode::All));
        compile::force_deopt_for_test(false);
        Self {
            jit_off,
            profit: compile::replace_profit_gate_for_test(ProfitGateTestPolicy::Bypass),
        }
    }
}

impl Drop for Knobs {
    fn drop(&mut self) {
        inline::force_inline_for_test(None);
        compile::force_inline2_for_test(None);
        compile::replace_profit_gate_for_test(self.profit);
        crate::emacs_core::jit::force_jit_off_for_test(self.jit_off);
    }
}

fn fixture(action: impl FnOnce(&mut Context)) {
    crate::test_utils::init_test_tracing();
    let _knobs = Knobs::enter();
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Legacy);
    let _background =
        crate::emacs_core::jit::bg::TestModeScope::enter(crate::emacs_core::jit::bg::BgMode::Sync);
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    context.set_lexical_binding(true);
    context.register_subr(SubrSpec::fixed0("g2-inline-witness", witness));
    context.register_subr(SubrSpec::fixed0("g2-inline-admit", admit_native));
    context.register_subr(SubrSpec::fixed0("g2-inline-yield", yield_native));
    context.register_subr(SubrSpec::fixed2(
        "g2-inline-signal-hook",
        signal_hook,
        FixedMin2::Two,
    ));
    context.set_variable("g2-inline-witnesses", Value::fixnum(0));
    let result = catch_unwind(AssertUnwindSafe(|| action(&mut context)));
    context.set_variable("signal-hook-function", Value::NIL);
    context.green_teardown();
    assert!(
        context.specpdl.is_empty(),
        "all reconstructed frames retire"
    );
    assert_eq!(context.jit_root_stack_top, 0);
    assert_eq!(cache::native_depth(), 0);
    if let Err(payload) = result {
        resume_unwind(payload);
    }
}

fn bytecode(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut function =
        ByteCodeFunction::new(LambdaParams::simple(vec![intern("g2-inline-argument")]));
    function.lexical = true;
    function.ops = ops;
    function.constants = constants.into();
    function.max_stack = StackDepth::for_test(32);
    function.seal_hand_assembled_ops_for_test();
    function.jit_runtime().set_hot_for_test();
    function
}

fn install(context: &mut Context, name: &str, function: ByteCodeFunction) -> Value {
    let function = Value::make_bytecode(function);
    context.set_function(name, function);
    function
}

fn witness(context: &mut Context) -> EvalResult {
    assert!(
        cache::native_depth() > 0,
        "fixture must execute native code"
    );
    let seen = context
        .visible_variable_value_or_nil("g2-inline-witnesses")
        .as_fixnum()
        .unwrap();
    context.set_variable("g2-inline-witnesses", Value::fixnum(seen + 1));
    Ok(Value::T)
}

fn handoff(context: &mut Context) -> EvalResult {
    context
        .eval_str(
            "(setq g2-inline-peer
                   (make-thread (lambda () (garbage-collect) 'inline-peer)))",
        )
        .map_err(neovm_value::error::flow_from_eval_error)?;
    context.apply(Value::symbol("thread-yield"), Vec::new())?;
    let peer = context.visible_variable_value_or_nil("g2-inline-peer");
    let answer = context.apply(Value::symbol("thread-join"), vec![peer])?;
    assert_eq!(answer, Value::symbol("inline-peer"));
    context.set_variable("g2-inline-handoff-done", Value::T);
    Ok(Value::NIL)
}

fn admit_native(context: &mut Context) -> EvalResult {
    assert!(
        cache::native_depth() > 0,
        "admission begins in a live compiled caller"
    );
    assert!(!context.threads_admitted());
    handoff(context)
}

fn yield_native(context: &mut Context) -> EvalResult {
    assert!(
        cache::native_depth() > 0,
        "switch begins in a live compiled caller"
    );
    assert!(context.threads_admitted());
    handoff(context)
}

fn frame_function(entry: &SpecBinding) -> Option<Value> {
    match entry {
        SpecBinding::Backtrace { function, .. }
        | SpecBinding::Backtrace1 { function, .. }
        | SpecBinding::Backtrace2 { function, .. }
        | SpecBinding::BacktraceNative { function, .. } => Some(*function),
        _ => None,
    }
}

fn signal_hook(context: &mut Context, symbol: Value, _data: Value) -> EvalResult {
    assert_eq!(symbol, Value::symbol("wrong-type-argument"));
    assert_eq!(
        cache::native_depth(),
        0,
        "the guard drained native frames before Lisp"
    );
    let callee = context.visible_variable_value_or_nil("g2-inline-callee-source");
    assert!(
        context
            .specpdl
            .iter()
            .filter_map(frame_function)
            .any(|function| function == callee),
        "the inlined callee is already a real backtrace frame"
    );
    let argument = context.visible_variable_value_or_nil("g2-inline-kept");
    assert_eq!(
        argument.cons_car(),
        Value::fixnum(91),
        "the virtual side effect ran once before deopt"
    );
    assert_eq!(argument.cons_cdr(), Value::fixnum(17));
    let admit = context
        .visible_variable_value_or_nil("g2-inline-admit-in-hook")
        .is_truthy();
    assert_eq!(!context.threads_admitted(), admit);
    handoff(context)?;
    assert!(context.tagged_heap.owns_heap_value_for_test(argument));
    assert_eq!(argument.cons_car(), Value::fixnum(91));
    context.set_variable("g2-inline-hook-done", Value::T);
    Ok(Value::NIL)
}

fn start_before_native(context: &mut Context) {
    context
        .eval_str("(thread-join (make-thread (lambda () nil)))")
        .unwrap();
    assert!(context.threads_admitted());
}

fn physical_caller_handoff(context: &mut Context, admit: bool) {
    let callee = install(
        context,
        "g2-inline-callee",
        bytecode(vec![Op::StackRef(0), Op::Car, Op::Return], vec![]),
    );
    let caller = install(
        context,
        "g2-inline-physical",
        bytecode(
            vec![
                Op::Constant(0),
                Op::Call(0),
                Op::Pop,
                Op::Constant(1),
                Op::Call(0),
                Op::Pop,
                Op::Constant(2),
                Op::StackRef(1),
                Op::Call(1),
                Op::Constant(0),
                Op::Call(0),
                Op::Pop,
                Op::Return,
            ],
            vec![
                Value::symbol("g2-inline-witness"),
                Value::symbol(if admit {
                    "g2-inline-admit"
                } else {
                    "g2-inline-yield"
                }),
                callee,
            ],
        ),
    );
    if !admit {
        start_before_native(context);
    }
    let source = caller.get_bytecode_data().unwrap();
    let leaf = compile::compile_bytecode_function_with(source, Some(&context.obarray)).unwrap();
    assert!(
        !leaf.chains.is_empty(),
        "actual inline2 frame metadata was compiled"
    );
    let argument = Value::cons(Value::fixnum(456), Value::string("inline2-root"));
    context.set_variable("g2-inline-kept", argument);
    assert_eq!(
        leaf.call(context as *mut Context as *mut u8, &[argument]),
        NativeRun::Ok(Value::fixnum(456).bits()),
        "the original inline2-capable native caller continues after handoff"
    );
    assert!(context.tagged_heap.owns_heap_value_for_test(argument));
    assert_eq!(
        context.visible_variable_value_or_nil("g2-inline-witnesses"),
        Value::fixnum(2)
    );
    assert_eq!(
        context.visible_variable_value_or_nil("g2-inline-handoff-done"),
        Value::T
    );
}

fn materialized_callee_handoff(context: &mut Context, admit: bool) {
    let callee = install(
        context,
        "g2-inline-callee",
        bytecode(
            vec![
                Op::StackRef(0),
                Op::Constant(0),
                Op::Setcar,
                Op::Pop,
                Op::StackRef(0),
                Op::Cdr,
                Op::Car,
                Op::Return,
            ],
            vec![Value::fixnum(91)],
        ),
    );
    context.set_variable("g2-inline-callee-source", callee);
    let caller = install(
        context,
        "g2-inline-physical",
        bytecode(
            vec![
                Op::Constant(0),
                Op::Call(0),
                Op::Pop,
                Op::Constant(1),
                Op::StackRef(1),
                Op::Call(1),
                Op::Return,
            ],
            vec![Value::symbol("g2-inline-witness"), callee],
        ),
    );
    if !admit {
        start_before_native(context);
    }
    let source = caller.get_bytecode_data().unwrap();
    let leaf = compile::compile_bytecode_function_with(source, Some(&context.obarray)).unwrap();
    assert!(
        !leaf.chains.is_empty(),
        "fixture uses inline2 virtual frames"
    );
    let argument = Value::cons(Value::fixnum(7), Value::fixnum(17));
    context.set_variable("g2-inline-kept", argument);
    context.set_variable(
        "g2-inline-admit-in-hook",
        if admit { Value::T } else { Value::NIL },
    );
    let run = leaf.call(context as *mut Context as *mut u8, &[argument]);
    let NativeRun::DeoptAt(resume) = run else {
        panic!("the virtual Car guard must precisely exit: {run:?}")
    };
    let frames = &resume
        .inlined
        .as_ref()
        .expect("real virtual-frame readback")
        .frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].function, callee);
    assert_eq!(
        frames[0].pc, 6,
        "resume at the failing callee Car, without replaying Setcar"
    );
    assert_eq!(frames[0].stack, vec![argument, Value::fixnum(17)]);
    assert_eq!(argument.cons_car(), Value::fixnum(91));
    assert_eq!(
        context.visible_variable_value_or_nil("g2-inline-witnesses"),
        Value::fixnum(1)
    );
    context.set_variable(
        "signal-hook-function",
        Value::symbol("g2-inline-signal-hook"),
    );
    let flow = compile::resumed_chain::resume_deopt(context, source, caller, &leaf, *resume)
        .expect_err("the original wrong-type error survives the hook's handoff");
    assert!(
        matches!(flow.kind(), FlowRef::Signal(signal) if signal.symbol_name() == "wrong-type-argument")
    );
    drop(flow);
    context.set_variable("signal-hook-function", Value::NIL);
    assert_eq!(
        context.visible_variable_value_or_nil("g2-inline-hook-done"),
        Value::T
    );
    assert_eq!(
        context.visible_variable_value_or_nil("g2-inline-handoff-done"),
        Value::T
    );
}

#[test]
fn green_inline2_admission_preserves_live_compiled_caller_and_inline_region() {
    fixture(|context| physical_caller_handoff(context, true));
}

#[test]
fn green_inline2_switch_preserves_live_compiled_caller_and_inline_region() {
    fixture(|context| physical_caller_handoff(context, false));
}

#[test]
fn green_inline2_virtual_guard_materializes_before_signal_hook_admission() {
    fixture(|context| materialized_callee_handoff(context, true));
}

#[test]
fn green_inline2_virtual_guard_materializes_before_signal_hook_switch_and_gc() {
    fixture(|context| materialized_callee_handoff(context, false));
}
