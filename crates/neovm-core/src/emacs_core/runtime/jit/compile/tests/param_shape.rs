use super::*;
use crate::emacs_core::bytecode::{FunctionParams, StackDepth};
use crate::emacs_core::value::Value;

fn stack_function(nonrest: usize, declared_depth: usize) -> ByteCodeFunction {
    let descriptor = i64::try_from(nonrest).expect("test host shape") << 8;
    let mut function = ByteCodeFunction::new(
        FunctionParams::try_from(Value::fixnum(descriptor)).expect("integer parameter slot"),
    );
    function.max_stack = StackDepth::for_test(declared_depth);
    function
}

#[test]
fn native_parameter_slot_indices_accept_last_u16_index_and_reject_the_next() {
    let count = usize::from(u16::MAX) + 1;
    let accepted = stack_function(count, count);
    assert_eq!(
        JitParamShape::try_from(&accepted).unwrap().entry_depth(),
        count
    );
    let declined = stack_function(count + 1, count + 1);
    assert!(matches!(
        JitParamShape::try_from(&declined),
        Err(CompileError::BadOperand)
    ));
    let empty = stack_function(0, 0);
    assert_eq!(JitParamShape::try_from(&empty).unwrap().entry_depth(), 0);
}

#[test]
fn huge_parameter_and_declared_depth_decline_without_building_a_slot_vector() {
    let count = usize::try_from(1_u64 << 32).expect("64-bit test host");
    let function = stack_function(count, count);
    assert!(matches!(
        JitParamShape::try_from(&function),
        Err(CompileError::BadOperand)
    ));
}
