use super::*;
use crate::emacs_core::bytecode::{FunctionParams, StackDepth};
use crate::emacs_core::value::Value;

fn constant_function(arglist: Value) -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(
        FunctionParams::try_from(arglist).expect("accepted GNU parameter slot"),
    );
    function.max_stack = StackDepth::for_test(4);
    function.ops = vec![Op::Nil, Op::Return];
    function
}

#[test]
fn jit_rejects_unrepresentable_and_inconsistent_stack_shapes() {
    for descriptor in [-1, 1, 383] {
        let mut function = constant_function(Value::fixnum(descriptor));
        function.lexical = true;
        let error = compile_bytecode_function(&function).expect_err("no valid native shape");
        assert!(
            matches!(error, CompileError::TakesArguments),
            "{descriptor}: {error:?}"
        );
    }
}

#[test]
fn jit_preserves_dynamic_nil_as_a_zero_argument_function() {
    let function = constant_function(Value::NIL);
    assert_eq!(function.params.fixed_arity(), Some(0));
    let leaf = compile_bytecode_function(&function).expect("dynamic nil has no named bindings");
    assert_eq!(leaf.call_for_test(&[]), Some(Value::NIL.bits()));
}

#[test]
fn jit_declines_dynamic_named_binding_even_with_lexical_metadata() {
    let arglist = Value::cons(
        Value::from_sym_id(crate::emacs_core::intern::intern("jit-dynamic-parameter")),
        Value::NIL,
    );
    let mut function = constant_function(arglist);
    function.lexical = true;
    let error =
        compile_bytecode_function(&function).expect_err("dynamic names need invocation binding");
    assert!(matches!(error, CompileError::TakesArguments));
}

#[test]
fn jit_declines_huge_entry_shape_before_proportional_allocation() {
    for descriptor in [1_i64 << 36, 1_i64 << 40] {
        let mut function = constant_function(Value::fixnum(descriptor));
        function.lexical = true;
        function.max_stack = StackDepth::for_test(1);
        let error = compile_bytecode_function(&function).expect_err("entry exceeds declared depth");
        assert!(matches!(error, CompileError::TakesArguments));
    }
}
