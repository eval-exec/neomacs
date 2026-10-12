//! Call classification completes its short cell loan before callee entry.

use super::*;
use crate::emacs_core::eval::Context;

fn bytecode(value: i64) -> Value {
    let mut function = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    function.ops = vec![Op::Constant(0), Op::Return];
    function.constants = vec![Value::fixnum(value)].into();
    function.max_stack = super::super::function_slots::StackDepth::try_from(4usize)
        .expect("valid fixture stack depth");
    Value::make_bytecode(function)
}

fn copied_bytecode(target: ResolvedStackCallTarget) -> Value {
    match target {
        ResolvedStackCallTarget::ByteCode { callee } => callee.value(),
        ResolvedStackCallTarget::Interpreter { .. } => {
            panic!("generic classifier cannot prepare an iterative call")
        }
        ResolvedStackCallTarget::Builtin { .. }
        | ResolvedStackCallTarget::BuiltinLeaf { .. }
        | ResolvedStackCallTarget::Generic => panic!("the test cell must hold bytecode"),
    }
}

#[test]
fn world_bases_call_reader_reacquires_after_epoch_retirement() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let symbol = intern("world-bases-pure-call-reader");
    let called = Value::from_sym_id(symbol);
    let first = bytecode(11);
    let second = bytecode(22);
    context.obarray.set_symbol_function_id(symbol, first);
    let mut vm = Vm::from_context(&mut context);

    let cold = vm.resolve_stack_call_target(called);
    assert_eq!(copied_bytecode(cold), first);
    let warm = vm.resolve_stack_call_target(called);
    assert_eq!(copied_bytecode(warm), first);

    let old_epoch = vm.ctx.dispatch_projection().function_epoch();
    vm.ctx.obarray.set_symbol_function_id(symbol, second);
    assert!(vm.ctx.dispatch_projection().function_epoch() > old_epoch);
    let changed = vm.resolve_stack_call_target(called);
    assert_eq!(copied_bytecode(changed), second);
}

#[test]
fn world_bases_call_reader_warm_hit_keeps_current_callee() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let symbol = intern("world-bases-pure-call-reader-count");
    let called = Value::from_sym_id(symbol);
    let callee = bytecode(33);
    context.obarray.set_symbol_function_id(symbol, callee);
    let mut vm = Vm::from_context(&mut context);
    for _ in 0..2 {
        assert_eq!(
            copied_bytecode(vm.resolve_stack_call_target(called)),
            callee
        );
    }
    assert!(
        vm.ctx
            .symbol_bytecode_call_cache
            .get(symbol, vm.ctx.obarray.function_epoch())
            .is_some()
    );
}
