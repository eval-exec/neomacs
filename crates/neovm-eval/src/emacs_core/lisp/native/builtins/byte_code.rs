//! GNU Fbyte_code's checked function slots and shared compiled constructor.

use crate::emacs_core::bytecode::function_slots::BytecodeSlotErrorExt as _;
use crate::emacs_core::bytecode::function_slots::StackDepthErrorExt as _;
use crate::emacs_core::bytecode::function_slots::{
    BytecodeSlotOrigin, BytecodeString, ConstantsVector, StackDepth,
};
use crate::emacs_core::eval::Context;
use neovm_value::error::{EvalResult, expect_args};
use neovm_value::value::Value;

/// Arguments have been evaluated by ordinary subr dispatch. GNU accepts legacy
/// multibyte code strings here, normalizes them with string-as-unibyte, and
/// delegates construction to Fmake_byte_code (src/bytecode.c:298–323).
#[doc(hidden)]
pub fn builtin_byte_code(context: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("byte-code", &args, 3)?;
    let origin = BytecodeSlotOrigin::Direct;
    let code = BytecodeString::try_from(args[0])
        .map_err(|error| error.into_flow(origin))?
        .into_unibyte(origin)?;
    let constants = ConstantsVector::try_from(args[1]).map_err(|error| error.into_flow(origin))?;
    let depth = StackDepth::try_from(args[2]).map_err(|error| error.into_flow(origin))?;
    let function =
        super::byte_code_for_immediate_call(&code.value(), &constants.value(), &depth.value())
            .map_err(|_| origin.invalid_flow())?;
    // The direct VM entry roots the owned function's constants through Lisp
    // calls and collections, and its instructions die with this call.
    let mut vm = crate::emacs_core::bytecode::Vm::from_context(context);
    vm.execute(&function, Vec::new())
}
