use super::*;

#[test]
fn constant_dedup() {
    crate::test_utils::init_test_tracing();
    let mut func = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    let i1 = func.add_constant(Value::fixnum(42));
    let i2 = func.add_constant(Value::fixnum(42));
    assert_eq!(i1, i2);
    assert_eq!(func.constants.len(), 1);
}

#[test]
fn symbol_dedup() {
    crate::test_utils::init_test_tracing();
    let mut func = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    let i1 = func.add_symbol("x");
    let i2 = func.add_symbol("x");
    let i3 = func.add_symbol("y");
    assert_eq!(i1, i2);
    assert_ne!(i1, i3);
    assert_eq!(func.constants.len(), 2);
}

#[test]
fn patch_jump() {
    crate::test_utils::init_test_tracing();
    let mut func = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    func.emit(Op::GotoIfNil(0)); // placeholder
    func.emit(Op::Constant(0));
    func.emit(Op::Return);
    let target = func.current_offset();
    func.patch_jump(0, target);
    assert_eq!(func.ops[0], Op::GotoIfNil(3));
}

#[test]
fn disassemble_output() {
    crate::test_utils::init_test_tracing();
    let mut func = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    func.add_constant(Value::fixnum(42));
    func.emit(Op::Constant(0));
    func.emit(Op::Return);
    let dis = func.disassemble();
    assert!(dis.contains("constant 0 ; 42"));
    assert!(dis.contains("return"));
}

#[test]
fn gnu_ir_is_decoded_only_on_first_access() {
    crate::test_utils::init_test_tracing();
    let raw = vec![182, 3, 135]; // discardN 3; return
    let mut constants = Vec::new();
    let (ops, offset_map) =
        super::super::decode::decode_gnu_bytecode_with_offset_map(&raw, &mut constants).unwrap();
    let mut func = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    func.ops = ops.clone();
    func.gnu_byte_offset_map = Some(offset_map);
    func.gnu_bytecode_bytes = Some(crate::tagged::header::LispByteVec::owned(raw));

    func.defer_gnu_decode();
    assert!(func.resident_ops().is_empty());
    assert_eq!(func.resident_ops_capacity(), 0);

    assert_eq!(func.executable_ops(), ops);
    assert_eq!(func.resident_ops(), ops);
    assert!(func.executable_gnu_byte_offset_map().is_none());
}

/// Cite-and-overturn of the former `cloning_deferred_gnu_code_does_not_copy_
/// decoded_ir` pin: a clone (a `make-closure` instance) neither COPIES nor
/// RE-DECODES the deferred IR — it shares the prototype's decode cell, so the
/// IR is resident in the clone the moment it is resident in the prototype and
/// no second decode ever runs.
#[test]
fn cloning_deferred_gnu_code_shares_the_decoded_ir() {
    let raw = vec![135]; // return
    let mut func = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    func.ops = vec![Op::Return];
    func.gnu_byte_offset_map = Some(Vec::new());
    func.gnu_bytecode_bytes = Some(crate::tagged::header::LispByteVec::owned(raw));
    func.defer_gnu_decode();
    let decodes_before = super::lazy_gnu_decode_count_for_test();
    assert_eq!(func.executable_ops(), &[Op::Return]);
    assert_eq!(super::lazy_gnu_decode_count_for_test(), decodes_before + 1);

    let cloned = func.clone();
    assert_eq!(
        cloned.resident_ops(),
        &[Op::Return],
        "shared: already resident in the clone"
    );
    assert_eq!(cloned.executable_ops(), &[Op::Return]);
    assert_eq!(
        super::lazy_gnu_decode_count_for_test(),
        decodes_before + 1,
        "the clone must not decode again"
    );
    assert!(
        std::sync::Arc::ptr_eq(
            func.lazy_gnu_code.as_ref().unwrap(),
            cloned.lazy_gnu_code.as_ref().unwrap()
        ),
        "one decode cell per source"
    );
}
