//! `throw-on-input` interrupts a compiled green loop whose only inline sites
//! are intrinsics. HostInputPublisher lives in the system crate, so this test
//! cannot stay in eval. The assertions are unchanged.

use neovm_eval::test_support::authority::{
    TestOptMode, enter_test_mode_for_test, green_teardown_for_test, inline_arith_scope_for_test,
    jit_root_stack_top_for_test, opt_mode_token_scope_for_test, prepare_osr_for_test,
    seal_hand_assembled_ops_for_test, set_heat_for_test, try_run_osr_for_test,
};
use neovm_value::error::FlowRef;
use neovm_value::intern::SymId;
use neovm_value::value::{LambdaParams, Value};

use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op, StackDepth};
use crate::emacs_core::eval::keyboard::{InputEvent, KeyEvent};
use crate::emacs_core::jit::compile::{NativeRun, take_pending_flow};
use crate::emacs_core::jit::{BgMode, hot_threshold};
use crate::host_input::HostInputPublisher;

fn function(ops: Vec<Op>, constants: Vec<Value>, arity: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        // SAFETY: test placeholder id, never resolved (peer seal N8).
        required: (0..arity)
            .map(|i| unsafe { SymId::from_raw(i as u32 + 1) })
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = StackDepth::for_test(16);
    seal_hand_assembled_ops_for_test(&mut f);
    f
}

/// (lambda (n) (let ((i 0) (a 7)) (while (< i n)
///    (setq a (logand a (mod i 7)) i (1+ i))) a))
fn intrinsic_loop() -> ByteCodeFunction {
    let mut f = function(
        vec![
            Op::Constant(1),
            Op::Constant(2),
            Op::StackRef(1),
            Op::StackRef(3),
            Op::Lss,
            Op::GotoIfNil(18),
            Op::Constant(4),
            Op::StackRef(1),
            Op::Constant(3),
            Op::StackRef(4),
            Op::Constant(0),
            Op::Call(2),
            Op::Call(2),
            Op::StackSet(1),
            Op::StackRef(1),
            Op::Add1,
            Op::StackSet(2),
            Op::Goto(2),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![
            Value::make_int(7),
            Value::make_int(0),
            Value::make_int(7),
            Value::symbol("mod"),
            Value::symbol("logand"),
        ],
        1,
    );
    set_heat_for_test(&f, hot_threshold().saturating_mul(64));
    f
}

/// Enter the intrinsic loop's OSR leaf at the loop head with N bound large.
fn run_osr_loop_head(context: &mut crate::Context, source: Value, n: i64) -> Option<NativeRun> {
    let source_data = source.get_bytecode_data().unwrap();
    prepare_osr_for_test(context, source_data, 2);
    // SAFETY: `context` is the live `Context` this test owns; the call runs
    // in this thread under the dormant-Context contract.
    unsafe {
        try_run_osr_for_test(
            context,
            source_data,
            2,
            &[Value::make_int(n), Value::make_int(0), Value::make_int(7)],
            &[],
        )
    }
}

/// `throw-on-input` interrupts the same loop: the poll promotes queued
/// frontend input into the throw.
#[test]
fn throw_on_input_interrupts_compiled_green_intrinsic_loop() {
    let _backend = opt_mode_token_scope_for_test(TestOptMode::Off);
    let _background = enter_test_mode_for_test(BgMode::Sync);
    let _arith = inline_arith_scope_for_test(true);
    let mut context = crate::test_utils::new_context();
    context.set_green_threading_mode_at_construction_for_test();
    context.set_lexical_binding(true);
    let source = Value::make_bytecode(intrinsic_loop());
    let _root = context.retain_gc_roots([source]);
    let tx = HostInputPublisher::install(&mut context);
    tx.send(InputEvent::key_press(KeyEvent::char('l')))
        .expect("queue ordinary frontend input");
    let sentinel = Value::symbol("green-inline-throw-tag");
    context.set_variable("throw-on-input", sentinel);
    let run = run_osr_loop_head(&mut context, source, 2_000_000)
        .expect("the exact snapshot enters native OSR");
    let NativeRun::Signal = run else {
        panic!("the back-edge poll must serve throw-on-input: {run:?}")
    };
    match take_pending_flow()
        .expect("throw-on-input flow stashed")
        .kind()
    {
        FlowRef::Throw(thrown) => {
            assert_eq!(thrown.tag, sentinel);
            assert_eq!(thrown.value, Value::T);
        }
        other => panic!("expected a throw, got {other:?}"),
    }
    assert_eq!(jit_root_stack_top_for_test(&context), 0);
    green_teardown_for_test(&mut context);
}
