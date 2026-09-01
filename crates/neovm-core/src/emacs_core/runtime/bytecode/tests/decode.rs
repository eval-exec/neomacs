use super::*;
use crate::emacs_core::intern::resolve_sym;
use crate::emacs_core::value::HashTableTest;

#[test]
fn string_value_to_bytes_basic() {
    crate::test_utils::init_test_tracing();
    let bytes = string_value_to_bytes("ABC");
    assert_eq!(bytes, vec![65, 66, 67]);
}

#[test]
fn string_value_to_bytes_octal_escape() {
    crate::test_utils::init_test_tracing();
    // \300 = 0xC0 = char 192
    let s = "\u{00C0}"; // 192 as char
    let bytes = string_value_to_bytes(s);
    assert_eq!(bytes, vec![0xC0]);
}

#[test]
fn decode_simple_constant_return() {
    crate::test_utils::init_test_tracing();
    // bytecodes: constant(0) return
    // byte 192 = constant 0, byte 135 = return
    let bytecodes = vec![192, 135];
    let mut constants = vec![Value::fixnum(42)];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::Constant(0), Op::Return]);
}

#[test]
fn decode_car_cdr() {
    crate::test_utils::init_test_tracing();
    // car=64, cdr=65, return=135
    let bytecodes = vec![64, 65, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::Car, Op::Cdr, Op::Return]);
}

#[test]
fn decode_arithmetic() {
    crate::test_utils::init_test_tracing();
    // add=92, sub=90, mul=95, return=135
    let bytecodes = vec![92, 90, 95, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::Add, Op::Sub, Op::Mul, Op::Return]);
}

#[test]
fn decode_varref_immediate() {
    crate::test_utils::init_test_tracing();
    // varref 0 = byte 8, varref 5 = byte 13
    let bytecodes = vec![8, 13, 135];
    let mut constants = vec![
        Value::symbol("x"),
        Value::symbol("y"),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::symbol("z"),
    ];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::VarRef(0), Op::VarRef(5), Op::Return]);
}

#[test]
fn decode_goto_jump_patching() {
    crate::test_utils::init_test_tracing();
    // constant(0), goto-if-nil to byte 5, constant(1), return, constant(2), return
    // byte 0: 192 → constant(0) [1 byte]
    // byte 1: 131, 5, 0 → goto-if-nil to byte 5 [3 bytes]
    // byte 4: 193 → constant(1) [1 byte]
    // byte 5: 135 → return [1 byte]
    let bytecodes = vec![192, 131, 5, 0, 193, 135];
    let mut constants = vec![Value::NIL, Value::fixnum(1)];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    // Instructions: [0] constant(0), [1] goto-if-nil(4), [2] constant(1), [3] return
    // Wait, byte 5 maps to instruction index... let me trace:
    // byte 0 → instr 0: constant(0)
    // byte 1 → instr 1: goto-if-nil(target=byte 5)
    // byte 4 → instr 2: constant(1)
    // byte 5 → instr 3: return
    // So goto-if-nil should jump to instruction 3
    assert_eq!(
        ops,
        vec![
            Op::Constant(0),
            Op::GotoIfNil(3),
            Op::Constant(1),
            Op::Return,
        ]
    );
}

#[test]
fn decode_call_immediate() {
    crate::test_utils::init_test_tracing();
    // call 0 = byte 32, call 3 = byte 35
    let bytecodes = vec![32, 35, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::Call(0), Op::Call(3), Op::Return]);
}

#[test]
fn decode_list_ops() {
    crate::test_utils::init_test_tracing();
    // list1=67, list2=68, cons=66
    let bytecodes = vec![67, 68, 66, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::List(1), Op::List(2), Op::Cons, Op::Return]);
}

#[test]
fn decode_constant_range() {
    crate::test_utils::init_test_tracing();
    // byte 192 = constant(0), byte 255 = constant(63)
    let bytecodes = vec![192, 255, 135];
    let mut constants = (0..64).map(|i| Value::fixnum(i)).collect();
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::Constant(0), Op::Constant(63), Op::Return]);
}

#[test]
fn decode_rejects_unused_opcode_128() {
    crate::test_utils::init_test_tracing();
    let mut constants = vec![Value::fixnum(42)];
    let err = decode_gnu_bytecode(&[128, 0, 135], &mut constants).unwrap_err();
    assert!(matches!(err, DecodeError::UnknownOpcode(128, 0)));
}

#[test]
fn decode_unwind_protect_pop() {
    crate::test_utils::init_test_tracing();
    // unwind-protect = byte 142
    let bytecodes = vec![142, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::UnwindProtectPop, Op::Return]);
}

#[test]
fn decode_save_excursion_and_restriction() {
    crate::test_utils::init_test_tracing();
    let mut constants = vec![];
    let save_excursion = decode_gnu_bytecode(&[138, 135], &mut constants).unwrap();
    assert_eq!(save_excursion, vec![Op::SaveExcursion, Op::Return]);

    let mut constants = vec![];
    let save_restriction = decode_gnu_bytecode(&[140, 135], &mut constants).unwrap();
    assert_eq!(save_restriction, vec![Op::SaveRestriction, Op::Return]);
}

#[test]
fn decode_discard_n() {
    crate::test_utils::init_test_tracing();
    // discardN = byte 182, operand = 3
    let bytecodes = vec![182, 3, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::DiscardN(3), Op::Return]);
}

#[test]
fn decode_non_switch_does_not_retain_byte_offset_map() {
    crate::test_utils::init_test_tracing();
    let mut constants = vec![];
    let (ops, offset_map) =
        decode_gnu_bytecode_with_offset_map(&[182, 3, 135], &mut constants).unwrap();

    assert_eq!(ops, vec![Op::DiscardN(3), Op::Return]);
    assert!(
        offset_map.is_empty(),
        "only Bswitch needs byte-offset translation after decoding"
    );
}

#[test]
fn decode_switch_preserves_hash_table_byte_targets() {
    crate::test_utils::init_test_tracing();
    let table = Value::hash_table(HashTableTest::Eq);
    if !table.is_hash_table() {
        panic!("expected hash table constant");
    };
    let _ = table.with_hash_table_mut(|ht| {
        let key = Value::symbol("foo").to_hash_key(&ht.test);
        ht.insert(key, Value::symbol("foo"), Value::fixnum(8));
    });

    // byte 0: constant key
    // byte 1: constant switch-table
    // byte 2: switch
    // byte 3: goto byte 8
    // byte 6: constant default
    // byte 7: return
    // byte 8: constant target
    // byte 9: return
    let bytecodes = vec![193, 192, 183, 130, 8, 0, 194, 135, 195, 135];
    let mut constants = vec![
        table,
        Value::symbol("foo"),
        Value::fixnum(10),
        Value::fixnum(20),
    ];
    let (ops, offset_map) =
        decode_gnu_bytecode_with_offset_map(&bytecodes, &mut constants).unwrap();

    assert_eq!(
        ops,
        vec![
            Op::Constant(1),
            Op::Constant(0),
            Op::Switch,
            Op::Goto(6),
            Op::Constant(2),
            Op::Return,
            Op::Constant(3),
            Op::Return,
        ]
    );

    let raw_target = {
        table
            .as_hash_table()
            .unwrap()
            .data
            .values()
            .next()
            .copied()
            .expect("switch table target")
    };
    assert_eq!(raw_target, Value::fixnum(8));
    assert_eq!(
        offset_map
            .binary_search_by_key(&8, |entry| entry.byte_offset)
            .map(|index| offset_map[index].instruction_index),
        Ok(6)
    );
}

#[test]
fn decode_buffer_op_point() {
    crate::test_utils::init_test_tracing();
    // point = byte 96
    let bytecodes = vec![96, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    // Upstream 762188a5d moved buffer-op dispatch inline: the decoder
    // emits Op::CallBuiltinSym(intern("point"), 0) and does NOT inject
    // the symbol into the constants pool. Verify the op shape.
    match &ops[0] {
        Op::CallBuiltinSym(sym, 0) => {
            assert_eq!(
                crate::emacs_core::intern::resolve_sym(*sym),
                "point",
                "buffer-op byte 96 should dispatch to `point`"
            );
        }
        other => panic!("expected CallBuiltinSym(point, 0), got {:?}", other),
    }
}

#[test]
fn decode_buffer_op_save_current_buffer() {
    crate::test_utils::init_test_tracing();
    let bytecodes = vec![114, 135];
    let mut constants = vec![];
    let ops = decode_gnu_bytecode(&bytecodes, &mut constants).unwrap();
    assert_eq!(ops, vec![Op::SaveCurrentBuffer, Op::Return]);
    assert!(constants.is_empty());
}

#[test]
fn parse_arglist_descriptor_no_rest() {
    crate::test_utils::init_test_tracing();
    // 2 mandatory, 3 total → 1 optional
    let params = parse_arglist_descriptor(2 | (3 << 8));
    assert_eq!(params.required.len(), 2);
    assert_eq!(params.optional.len(), 1);
    assert!(params.rest.is_none());
}

#[test]
fn parse_arglist_descriptor_with_rest() {
    crate::test_utils::init_test_tracing();
    // 1 mandatory + &rest, with 1 non-rest slot total.
    let params = parse_arglist_descriptor(1 | (1 << 8) | 128);
    assert_eq!(params.required.len(), 1);
    assert_eq!(params.optional.len(), 0);
    assert!(params.rest.is_some());
}

#[test]
fn parse_arglist_descriptor_with_optional_and_rest_slot() {
    crate::test_utils::init_test_tracing();
    // GNU lexical bytecode can carry both optional args and a hidden rest slot.
    let params = parse_arglist_descriptor(3 | (4 << 8) | 128);
    assert_eq!(params.required.len(), 3);
    assert_eq!(params.optional.len(), 1);
    assert!(params.rest.is_some());
}

#[test]
fn parse_arglist_descriptor_zero_args() {
    crate::test_utils::init_test_tracing();
    let params = parse_arglist_descriptor(0);
    assert_eq!(params.required.len(), 0);
    assert_eq!(params.optional.len(), 0);
    assert!(params.rest.is_none());
}

#[test]
fn parse_arglist_value_from_list() {
    crate::test_utils::init_test_tracing();

    let arglist = Value::list(vec![
        Value::symbol("x"),
        Value::symbol("&optional"),
        Value::symbol("y"),
        Value::symbol("&rest"),
        Value::symbol("z"),
    ]);
    let params = parse_arglist_value(&arglist);
    assert_eq!(params.required.len(), 1);
    assert_eq!(resolve_sym(params.required[0]), "x");
    assert_eq!(params.optional.len(), 1);
    assert_eq!(resolve_sym(params.optional[0]), "y");
    assert!(params.rest.is_some());
    assert_eq!(resolve_sym(params.rest.unwrap()), "z");
}

#[test]
fn parse_arglist_value_int() {
    crate::test_utils::init_test_tracing();
    let params = parse_arglist_value(&Value::fixnum(1 | (2 << 8)));
    assert_eq!(params.required.len(), 1);
    assert_eq!(params.optional.len(), 1);
    assert!(params.rest.is_none());
}
