//! GNU Fbyte_code's checked function slots and shared compiled constructor.

use crate::emacs_core::bytecode::function_slots::{
    BytecodeSlotOrigin, BytecodeString, ConstantsVector, StackDepth,
};
use crate::emacs_core::error::{EvalResult, expect_args};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

/// Arguments have been evaluated by ordinary subr dispatch. GNU accepts legacy
/// multibyte code strings here, normalizes them with string-as-unibyte, and
/// delegates construction to Fmake_byte_code (src/bytecode.c:298–323).
pub(crate) fn builtin_byte_code(context: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("byte-code", &args, 3)?;
    let origin = BytecodeSlotOrigin::Direct;
    let code = BytecodeString::try_from(args[0])
        .map_err(|error| error.into_flow(origin))?
        .into_unibyte(origin)?;
    let constants = ConstantsVector::try_from(args[1]).map_err(|error| error.into_flow(origin))?;
    let depth = StackDepth::try_from(args[2]).map_err(|error| error.into_flow(origin))?;
    let function = super::make_byte_code_from_parts(
        &Value::NIL,
        &code.value(),
        &constants.value(),
        &depth.value(),
        None,
        None,
    )
    .map_err(|_| origin.invalid_flow())?;
    let bytecode = function
        .get_bytecode_data()
        .ok_or_else(|| origin.invalid_flow())?;
    // Carry the actual function into the VM: its frame roots the new object
    // and its constants through Lisp calls and collections.
    let mut vm = crate::emacs_core::bytecode::Vm::from_context(context);
    vm.execute_with_func_value(bytecode, Vec::new(), function)
}
