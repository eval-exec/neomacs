use crate::emacs_core::bytecode::StackDepth;
use crate::emacs_core::pdump::DumpError;
use crate::emacs_core::pdump::mapped_heap::unnamed_params;
use crate::emacs_core::pdump::types::DumpFunctionParamsKind;
use neovm_value::tagged::pdump_image::{BC_FLAG_HAS_ARGLIST, BytecodeExtras};
use neovm_value::value::Value;

fn empty_header() -> BytecodeExtras {
    bytemuck::Zeroable::zeroed()
}

#[test]
fn p6_pdump_mapped_parameter_header_rejects_unknown_kind() {
    let mut header = empty_header();
    header.params_kind = u8::MAX;
    assert!(matches!(
        unnamed_params(&header),
        Err(DumpError::BytecodeParameterKind(_))
    ));
}

#[test]
fn p6_pdump_mapped_parameter_header_rejects_inconsistent_arglist() {
    let mut header = empty_header();
    header.params_kind = DumpFunctionParamsKind::Stack.into();
    header.flags = BC_FLAG_HAS_ARGLIST;
    header.arglist_word = Value::NIL.bits() as u64;
    assert!(matches!(
        unnamed_params(&header),
        Err(DumpError::BytecodeParameterShape)
    ));
    header.arglist_word = Value::fixnum(-1).bits() as u64;
    assert!(unnamed_params(&header).unwrap().is_some());
    header.n_required = 1;
    assert!(matches!(
        unnamed_params(&header),
        Err(DumpError::BytecodeParameterShape)
    ));
}

#[test]
fn p6_pdump_mapped_stack_depth_rejects_out_of_range_value() {
    let mut header = empty_header();
    header.max_stack = u64::MAX;
    let result = StackDepth::try_from(header.max_stack).map_err(DumpError::BytecodeStackDepth);
    assert!(matches!(result, Err(DumpError::BytecodeStackDepth(_))));
}
