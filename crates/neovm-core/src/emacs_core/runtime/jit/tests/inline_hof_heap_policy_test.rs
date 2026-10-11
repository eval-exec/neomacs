//! A HOF callback is a loop heap site even in a straight-line pure caller.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::compile::{self, Inline2Mode};
use crate::emacs_core::value::{LambdaParams, Value};

fn function(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("hof-heap-policy-argument")],
        optional: vec![],
        rest: None,
    });
    function.lexical = true;
    function.ops = ops;
    function.constants = constants.into();
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    function.seal_hand_assembled_ops_for_test();
    function.jit_runtime().set_hot_for_test();
    function
}

#[test]
fn inline_hof_heap_policy_detects_hidden_loop_stores_and_releases_scope() {
    let _ctx = Context::new();
    crate::test_utils::init_test_tracing();
    inline::force_inline_for_test(Some(true));
    compile::force_inline2_for_test(Some(Inline2Mode::Hof));
    assert!(!hoist_callback_heap_ptr());
    for name in ["mapc", "mapcar"] {
        for store in [None, Some(Op::Setcar), Some(Op::Setcdr)] {
            let has_store = store.is_some();
            let callback = Value::make_bytecode(match store {
                Some(op) => function(
                    vec![Op::Constant(0), Op::StackRef(1), op, Op::Return],
                    vec![Value::cons(Value::NIL, Value::NIL)],
                ),
                None => function(vec![Op::StackRef(0), Op::Return], vec![]),
            });
            let caller = function(
                vec![
                    Op::Constant(0),
                    Op::Constant(1),
                    Op::StackRef(2),
                    Op::Call(2),
                    Op::Return,
                ],
                vec![Value::symbol(name), callback],
            );
            let fused = inline::fuse_calls_v2(
                caller.executable_ops(),
                &caller.constants,
                None,
                1,
                &vec![NumericFeedback::FixnumOnly; caller.executable_ops().len()],
            )
            .expect("admitted list callback");
            assert_eq!(fused.ops, caller.executable_ops());
            {
                let _scope = inline::FusedScope::enter(std::rc::Rc::new(fused));
                assert_eq!(hoist_callback_heap_ptr(), has_store, "{name}");
            }
            assert!(!hoist_callback_heap_ptr());
        }
    }
    compile::force_inline2_for_test(None);
    inline::force_inline_for_test(None);
}
