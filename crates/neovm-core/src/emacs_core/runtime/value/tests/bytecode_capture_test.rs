use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::value::{LambdaParams, Value, bytecode_data_access_count};
use crate::tagged::collection_reads::capture;
use crate::tagged::mutate::LispCollectionRevision;

#[test]
fn bytecode_getter_capture_keeps_identity_and_first_read_revision() {
    crate::test_utils::init_test_tracing();
    let mut code = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    code.ops = vec![Op::Constant(0), Op::Return];
    code.constants = vec![Value::fixnum(41)].into();
    code.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(1);
    let function = Value::make_bytecode(code);
    let accesses = bytecode_data_access_count();
    let (constant, reads) = capture(|| {
        function
            .get_bytecode_data()
            .expect("a normal bytecode getter projects its data")
            .constants[0]
    });
    assert_eq!(constant, Value::fixnum(41));
    assert_eq!(bytecode_data_access_count(), accesses + 1);
    let reads = reads.expect("the bytecode getter retains a coherent capture");
    assert!(reads.unchanged());

    let unrelated = Value::cons(Value::NIL, Value::NIL);
    LispCollectionRevision::changed(unrelated);
    assert!(
        reads.unchanged(),
        "an unrelated write preserves the capture"
    );
    LispCollectionRevision::changed(function);
    assert!(!reads.unchanged(), "the bytecode identity was observed");

    let (_, reads) = capture(|| {
        let first = function.get_bytecode_data().expect("bytecode").constants[0];
        LispCollectionRevision::changed(function);
        let second = function.get_bytecode_data().expect("bytecode").constants[0];
        assert_eq!(first, second);
    });
    assert!(
        reads.is_none(),
        "a repeated getter must not replace the revision of its first read"
    );
}
