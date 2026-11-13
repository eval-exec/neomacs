//! P6 parameter modes: a byte-code function's mapped and descriptor
//! parameter forms survive a pdump round trip (the file's other three tests
//! are CONTEXT-FREE header validators and stay in eval).
//
// API-drift shims (post-split re-home): the pre-split test reached
// `Context`, `dump_to_file` and `load_from_dump` through the pdump test
// module. The evaluator is now built through the core test facade,
// `load_from_dump` lives in core's load_boot, the seal goes through the
// signed AUTHORITY fence (the upper-crate `ByteCodeFunctionExt` trait stays
// crate-private), and the obarray write uses its public accessor.
use crate::emacs_core::bytecode::StackDepth;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use neovm_value::value::Value;

#[test]
fn p6_pdump_round_trip_preserves_mapped_and_descriptor_parameter_modes() {
    use crate::emacs_core::bytecode::{ByteCodeFunction, FunctionParams, Op};
    use crate::emacs_core::load_boot::load_from_dump;
    use crate::emacs_core::pdump::dump_to_file;
    use crate::emacs_core::print::print_value;
    use neovm_value::intern::intern;
    use neovm_value::value::LambdaParams;

    let mut context = crate::test_utils::new_context();
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
        neovm_eval::test_support::authority::seal_hand_assembled_ops_for_test(&mut code);
        context
            .obarray_mut()
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
