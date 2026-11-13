//! Backtrace integration test requiring the real subr.el runtime macros.

use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::RuntimeStateTieringExt;
use neovm_eval::test_support::authority::{
    prepare_entry_for_test, spec_shim_policy_scope_for_test,
};
use neovm_value::error::FlowResultExt as _;
use neovm_value::intern::SymId;
use neovm_value::value::{LambdaParams, Value};

fn lambda(nargs: usize, hot: bool, ops: Vec<Op>, constants: Vec<Value>) -> Value {
    lambda_stack(nargs, hot, 16, ops, constants)
}

fn lambda_stack(
    nargs: usize,
    hot: bool,
    max_stack: usize,
    ops: Vec<Op>,
    constants: Vec<Value>,
) -> Value {
    let mut f = ByteCodeFunction::new(LambdaParams {
        // SAFETY: test placeholder id, never resolved (peer seal N8).
        required: (1..=nargs)
            .map(|i| unsafe { SymId::from_raw(i as u32) })
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(max_stack);
    f.seal_hand_assembled_ops();
    if hot {
        f.jit_runtime().set_hot_for_test();
    }
    let v = Value::make_bytecode(f);
    crate::emacs_core::eval::push_scratch_gc_root(v);
    v
}

fn shim_fast() -> u64 {
    neovm_eval::test_support::spec_shim_fast_count_for_test()
}

struct SpecShimPolicy;

impl SpecShimPolicy {
    fn enter(slow: bool) -> impl Drop {
        spec_shim_policy_scope_for_test(slow)
    }
}

fn signal_text(flow: &neovm_value::error::Flow) -> String {
    let Some(sig) = flow.as_signal() else {
        return format!("not-a-signal {flow:?}");
    };
    let data = sig
        .data
        .iter()
        .map(crate::emacs_core::print::print_value)
        .collect::<Vec<_>>()
        .join(" ");
    format!("({} {data})", sig.symbol_name())
}

fn calling(name: &str, max_stack: usize) -> Value {
    lambda_stack(
        1,
        true,
        max_stack,
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![Value::symbol(name)],
    )
}

fn bind_prepared(ev: &mut Context, name: &str, target: Value, caller: Value) {
    ev.obarray_mut()
        .set_symbol_function_id(neovm_value::intern::intern(name), target);
    prepare_entry_for_test(ev, target.get_bytecode_data().expect("callee bytecode"));
    prepare_entry_for_test(ev, caller.get_bytecode_data().expect("caller bytecode"));
}

fn debug_on_error_backtrace(slow: bool) -> (String, u64) {
    let _policy = SpecShimPolicy::enter(slow);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(
        "(progn
           (defvar neo-eieio-arm nil)
           (defvar neo-eieio-log nil)
           (setq debugger
                 (lambda (&rest args)
                   (let (frames)
                     (mapbacktrace
                      (lambda (_evald func args _flags)
                        (unless (memq func '(mapbacktrace debugger))
                          (push (list (if (symbolp func) func (type-of func)) args)
                                frames))))
                     (setq neo-eieio-log (list args (nreverse frames))))
                   nil)))",
    )
    .unwrap();
    let name = "neo-eieio-dbg";
    // Two blocks, so the caller speculates instead of inlining. Nil returns
    // the argument; non-nil is `(signal 'error '("shim-boom"))`. `signal` is
    // the subr, so this does not depend on the Lisp `error` definition.
    let target = lambda(
        1,
        false,
        vec![
            Op::VarRef(0),
            Op::GotoIfNil(7),
            Op::Constant(1),
            Op::Constant(2),
            Op::Constant(3),
            Op::Call(2),
            Op::Return,
            Op::StackRef(0),
            Op::Return,
        ],
        vec![
            Value::symbol("neo-eieio-arm"),
            Value::symbol("signal"),
            Value::symbol("error"),
            Value::cons(Value::string("shim-boom"), Value::NIL),
        ],
    );
    let caller = calling(name, 16);
    bind_prepared(&mut ev, name, target, caller);
    let arg = Value::make_int(3);
    let warmed = shim_fast();
    for _ in 0..4 {
        assert_eq!(
            ev.funcall_general_untraced(caller, vec![arg]).unwrap(),
            arg,
            "warmup returns the argument while the flag is nil"
        );
    }
    if slow {
        assert_eq!(shim_fast(), warmed, "slow spec does not arm the fast path");
    } else {
        assert!(shim_fast() >= warmed + 3, "warmup armed the spec shim");
    }
    ev.eval_str("(setq neo-eieio-arm t debug-on-error t)")
        .unwrap();
    let before = shim_fast();
    let refused = ev
        .funcall_general_untraced(caller, vec![arg])
        .expect_err("the callee signals");
    assert_eq!(signal_text(&refused), "(error \"shim-boom\")");
    let delta = shim_fast() - before;
    let logged = ev.eval_str("neo-eieio-log").unwrap();
    (crate::emacs_core::print::print_value(&logged), delta)
}

#[test]
fn debug_on_error_backtrace_matches_the_slow_spec_path() {
    crate::test_utils::init_test_tracing();
    let fast = debug_on_error_backtrace(false);
    let slow = debug_on_error_backtrace(true);
    assert!(
        fast.0.contains("neo-eieio-dbg"),
        "the callee frame is on the armed backtrace: {}",
        fast.0
    );
    assert_eq!(
        fast.0, slow.0,
        "debug-on-error backtrace changed on the shim"
    );
    assert!(fast.1 >= 1, "the erroring call took the armed fast path");
    assert_eq!(slow.1, 0, "the slow path never counts a shim fast run");
}
