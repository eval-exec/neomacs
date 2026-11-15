use super::*;
use neovm_value::value::Value;

#[test]
fn op_disasm_constant() {
    // The disassembler prints constants through the Lisp printer, whose
    // terminal lookup queries the lifecycle hook table: arm the unit runtime
    // (production arms it at Context construction).
    let _harness = crate::test_utils::unit_harness();
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

/// GNU disass.el:290 binds `print-escape-newlines` around the constant
/// `prin1`, so a string constant's newline is escaped on one line.
#[test]
fn op_disasm_constant_escapes_newlines_like_gnu_disass() {
    let _harness = crate::test_utils::unit_harness();
    let constants = vec![Value::string("a\nb")];
    let text = Op::Constant(0).disasm(&constants);
    assert_eq!(text, "constant 0 ; \"a\\nb\"");
    assert!(!text.contains('\n'));
}

/// GNU disass.el:260-275 prints a compiled-code constant as
/// `<byte-code-function>` and disassembles it recursively.
#[test]
fn op_disasm_byte_code_function_constant_recurses() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt as _;
    use neovm_value::value::LambdaParams;
    let _harness = crate::test_utils::unit_harness();
    let mut child = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    child.add_constant(Value::fixnum(7));
    child.emit(Op::Constant(0));
    child.emit(Op::Return);
    let constants = vec![Value::make_bytecode(child)];
    let text = Op::Constant(0).disasm(&constants);
    assert!(
        text.starts_with("constant 0 ; <byte-code-function>\n"),
        "{text}"
    );
    assert!(text.contains("constant 0 ; 7"), "{text}");
}

/// A `(macro . <byte-code-function>)` constant is `<compiled macro>`.
#[test]
fn op_disasm_compiled_macro_constant_recurses() {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt as _;
    use neovm_value::value::LambdaParams;
    let _harness = crate::test_utils::unit_harness();
    let mut child = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    child.emit(Op::Nil);
    child.emit(Op::Return);
    let constants = vec![Value::cons(
        Value::symbol("macro"),
        Value::make_bytecode(child),
    )];
    let text = Op::Constant(0).disasm(&constants);
    assert!(
        text.starts_with("constant 0 ; <compiled macro>\n"),
        "{text}"
    );
}

/// GNU disass.el:249-258: the constant feeding a `byte-switch` prints the
/// jump table (`<jump-table-TEST (V TAG ...)>`), not the table object.
#[test]
fn op_disasm_constant_before_switch_prints_jump_table() {
    use neovm_value::value::HashTableTest;
    let _harness = crate::test_utils::unit_harness();
    let table = Value::hash_table(HashTableTest::Eq);
    let _ = table.with_hash_table_mut(|ht| {
        let key = Value::symbol("vsw-foo");
        ht.insert(key.to_hash_key(&ht.test), key, Value::fixnum(8));
    });
    let constants = vec![table];
    let text = Op::Constant(0).disasm_in(&constants, Some(&Op::Switch));
    assert_eq!(text, "constant 0 ; <jump-table-eq (vsw-foo 8)>");
}
