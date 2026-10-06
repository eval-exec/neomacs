use super::*;

#[test]
fn vm_unbind_zero_preserves_caller_binding_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();
    let symbol = intern("vm-unbind-zero-caller-binding");
    eval.obarray.set_symbol_value_id(symbol, Value::fixnum(7));
    eval.try_specbind(symbol, Value::fixnum(31)).unwrap();
    let caller_depth = eval.specpdl.len();
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let answer = function.add_constant(Value::fixnum(1));
    function.ops = vec![Op::Unbind(0), Op::Constant(answer), Op::Return];
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(1);
    // GNU bytecode.c's Bunbind0 calls unbind_to(SPECPDL_INDEX(), Qnil).
    let result = new_vm(&mut eval).execute(&function, vec![]).unwrap();
    assert_eq!(result, Value::fixnum(1));
    assert_eq!(eval.specpdl.len(), caller_depth);
    assert_eq!(
        eval.eval_str("vm-unbind-zero-caller-binding").unwrap(),
        Value::fixnum(31)
    );
    eval.unbind_to(caller_depth - 1);
}

#[test]
fn vm_unbind_excess_rejects_without_unwinding_caller() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();
    let symbol = intern("vm-unbind-excess-caller-binding");
    eval.obarray.set_symbol_value_id(symbol, Value::fixnum(7));
    eval.try_specbind(symbol, Value::fixnum(31)).unwrap();
    let caller_depth = eval.specpdl.len();
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let answer = function.add_constant(Value::fixnum(1));
    function.ops = vec![Op::Unbind(1), Op::Constant(answer), Op::Return];
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(1);
    // This is a Neo safety boundary test, not a GNU malformed-bytecode probe:
    // the function owns no bind entries and must never unwind its caller.
    let result = new_vm(&mut eval).execute(&function, vec![]);
    assert_eq!(
        eval.specpdl.len(),
        caller_depth,
        "callee must preserve caller prefix"
    );
    assert_eq!(
        eval.eval_str("vm-unbind-excess-caller-binding").unwrap(),
        Value::fixnum(31)
    );
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(resolve_sym(sig.symbol), "error");
            assert_eq!(sig.data.len(), 1);
            assert_eq!(sig.data[0].as_utf8_str(), Some("Invalid byte-code"));
        }
        other => panic!("expected invalid byte-code error, got {other:?}"),
    }
    eval.unbind_to(caller_depth - 1);
}

#[test]
fn vm_unbind_excess_signal_is_caught_by_the_same_frame_handler() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new_minimal_vm_harness();
    let symbol = intern("vm-unbind-excess-handled-binding");
    eval.obarray.set_symbol_value_id(symbol, Value::fixnum(7));
    eval.try_specbind(symbol, Value::fixnum(31)).unwrap();
    let caller_depth = eval.specpdl.len();
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: vec![],
        optional: vec![],
        rest: None,
    });
    let unreached = function.add_constant(Value::fixnum(1));
    let handled = function.add_constant(Value::fixnum(2));
    // (condition-case nil <Bunbind 1> (error 2)): the rejected count must
    // publish the live operand stack before the frame's own handler resumes.
    let handler = 5;
    function.ops = vec![
        Op::PushConditionCase(handler),
        Op::Unbind(1),
        Op::Constant(unreached),
        Op::PopHandler,
        Op::Return,
        Op::Pop,
        Op::Constant(handled),
        Op::Return,
    ];
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(2);
    let result = new_vm(&mut eval).execute(&function, vec![]).unwrap();
    assert_eq!(result, Value::fixnum(2));
    assert_eq!(eval.specpdl.len(), caller_depth);
    assert_eq!(
        eval.eval_str("vm-unbind-excess-handled-binding").unwrap(),
        Value::fixnum(31)
    );
    eval.unbind_to(caller_depth - 1);
}
