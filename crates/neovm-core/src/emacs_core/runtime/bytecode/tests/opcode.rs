use super::*;
use crate::emacs_core::Value;

#[test]
fn op_disasm_constant() {
    crate::test_utils::init_test_tracing();
    let constants = vec![Value::fixnum(42)];
    assert_eq!(Op::Constant(0).disasm(&constants), "constant 0 ; 42");
}

#[test]
fn op_disasm_varref() {
    crate::test_utils::init_test_tracing();
    let constants = vec![Value::symbol("x")];
    assert_eq!(Op::VarRef(0).disasm(&constants), "varref 0 ; x");
}

#[test]
fn op_disasm_simple() {
    crate::test_utils::init_test_tracing();
    let c: Vec<Value> = vec![];
    assert_eq!(Op::Add.disasm(&c), "add");
    assert_eq!(Op::Return.disasm(&c), "return");
    assert_eq!(Op::Goto(10).disasm(&c), "goto 10");
}
