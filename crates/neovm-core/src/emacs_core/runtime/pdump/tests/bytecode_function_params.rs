use crate::emacs_core::bytecode::StackDepth;
use crate::emacs_core::pdump::DumpError;
use crate::emacs_core::pdump::mapped_heap::{BC_FLAG_HAS_ARGLIST, BytecodeExtras};
use crate::emacs_core::pdump::types::DumpFunctionParamsKind;
use crate::emacs_core::value::Value;

fn empty_header() -> BytecodeExtras {
    bytemuck::Zeroable::zeroed()
}

#[test]
fn p6_pdump_mapped_parameter_header_rejects_unknown_kind() {
    let mut header = empty_header();
    header.params_kind = u8::MAX;
    assert!(matches!(
        header.unnamed_params(),
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
        header.unnamed_params(),
        Err(DumpError::BytecodeParameterShape)
    ));
    header.arglist_word = Value::fixnum(-1).bits() as u64;
    assert!(header.unnamed_params().unwrap().is_some());
    header.n_required = 1;
    assert!(matches!(
        header.unnamed_params(),
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

#[test]
fn p6_pdump_round_trip_preserves_mapped_and_descriptor_parameter_modes() {
    use crate::emacs_core::bytecode::{ByteCodeFunction, FunctionParams, Op};
    use crate::emacs_core::eval::Context;
    use crate::emacs_core::intern::intern;
    use crate::emacs_core::pdump::{dump_to_file, load_from_dump};
    use crate::emacs_core::print::print_value;
    use crate::emacs_core::value::LambdaParams;

    let mut context = Context::new();
    context.eval_str(r#"(progn
        (defvar p6-dump-stack (make-byte-code 0 "\300\207" [42] 65537))
        (defvar p6-dump-negative (make-byte-code -1 "\300\207" [42] 1))
        (defvar p6-dump-large (make-byte-code (ash 1 40) "\300\207" [42] 1))
        (defvar p6-dump-dynamic (make-byte-code '(p6-dump-x &optional p6-dump-y &rest p6-dump-z) "\10\207" [p6-dump-x] 1)))"#).unwrap();
    let symbol = intern("p6-dump-native-x");
    for (name, params) in [
        (
            "p6-dump-named",
            FunctionParams::Named(LambdaParams::simple(vec![symbol])),
        ),
        (
            "p6-dump-dynamic-decoded",
            FunctionParams::try_from(Value::list(vec![Value::from_sym_id(symbol)])).unwrap(),
        ),
    ] {
        let mut code = ByteCodeFunction::new(params);
        code.constants = vec![Value::from_sym_id(symbol)].into();
        code.ops = vec![Op::VarRef(0), Op::Return];
        code.max_stack = StackDepth::for_test(1);
        code.seal_hand_assembled_ops();
        context
            .obarray
            .set_symbol_value(name, Value::make_bytecode(code));
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("parameter-modes.pdump");
    dump_to_file(&context, &path).unwrap();
    let mut loaded = load_from_dump(&path).unwrap();
    let result = loaded
        .eval_str(
            r#"(list
        (aref p6-dump-stack 3) (funcall p6-dump-stack)
        (aref p6-dump-negative 0) (func-arity p6-dump-large)
        (funcall p6-dump-dynamic 7 8 9)
        (funcall p6-dump-named 22)
        (funcall p6-dump-dynamic-decoded 33))"#,
        )
        .unwrap();
    assert_eq!(
        print_value(&result),
        "(65537 42 -1 (0 . 4294967296) 7 22 33)"
    );
}
