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

/// The dispatch loop fetches one `Op` per instruction, so `Op`'s size is the
/// per-instruction load width. Recorded rather than aspirational: it is 8
/// bytes, which is why the `bytecode-call-loop` gap is NOT opcode density.
///
/// Worth stating because the module doc above anticipates "a compact byte
/// stream", and a 16-byte load in the dispatch loop's `perf annotate` output
/// looks like evidence for exactly that. It is not -- with line-tables-only
/// debuginfo and this much inlining, a spill/reload lands on a nearby source
/// line. Measure the type before believing an instruction width.
#[test]
fn op_records_its_dispatch_fetch_width() {
    let size = std::mem::size_of::<Op>();
    let align = std::mem::align_of::<Op>();
    eprintln!("size_of::<Op>() = {size}, align = {align}");
    assert!(
        size <= 16,
        "Op grew past 16 bytes ({size}); the dispatch loop loads this per instruction"
    );
}
