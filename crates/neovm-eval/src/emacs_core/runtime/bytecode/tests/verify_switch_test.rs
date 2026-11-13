//! What admits an [`Op::Switch`] into the operand-stack proof, and what the
//! admitted target set is. Every refusal here keeps the function in the
//! checked driver; every admission puts it in the verified one, whose
//! `Switch` arm resolves only through the recorded set.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use neovm_value::value::{HashTableTest, HashTableWeakness, LambdaParams};

/// `constant key; constant table; switch; goto 8; constant 10; return;
/// constant 20; return` (the decode test's shape). Instruction starts:
/// bytes 0,1,2,3,6,7,8,9 are instructions 0..=7; byte 8 is instruction 6.
const SWITCH_BYTES: [u8; 10] = [193, 192, 183, 130, 8, 0, 194, 135, 195, 135];

fn jump_table(entries: &[(&str, Value)]) -> Value {
    let table = Value::hash_table(HashTableTest::Eq);
    let _ = table.with_hash_table_mut(|ht| {
        for (name, target) in entries {
            let key = Value::symbol(name);
            ht.insert(key.to_hash_key(&ht.test), key, *target);
        }
    });
    table
}

/// The pool `SWITCH_BYTES` indexes: 0 = table, 1 = key, 2/3 = answers.
fn pool(table: Value) -> Vec<Value> {
    vec![
        table,
        Value::symbol("vsw-foo"),
        Value::fixnum(10),
        Value::fixnum(20),
    ]
}

fn verify(bytes: &[u8], constants: &[Value], max_stack: usize) -> Option<StackProof> {
    let mut constants = constants.to_vec();
    let (ops, offset_map) = decode_gnu_bytecode_with_offset_map(bytes, &mut constants).unwrap();
    verify_stack_effects_with_tables(
        &ops,
        0,
        max_stack,
        Some(SwitchTables {
            constants: &constants,
            offset_map: Some(&offset_map),
        }),
    )
}

#[test]
fn switch_with_instruction_start_targets_is_admitted() {
    crate::test_utils::init_test_tracing();
    let constants = pool(jump_table(&[("vsw-foo", Value::fixnum(8))]));
    let proof = verify(&SWITCH_BYTES, &constants, 2).expect("admitted");
    // The per-instruction depths are kept for a switch-bearing stream: the
    // switch itself is entered at 2, its target and the fall-through at 0;
    // instruction 4 is reachable from nothing.
    assert_eq!(proof.depths.len(), 8);
    assert_eq!(proof.depths[2], 2);
    assert_eq!(proof.depths[3], 0);
    assert_eq!(proof.depths[4], StackProof::UNREACHABLE);
    assert_eq!(proof.depths[6], 0);
    assert!(proof.proves_depth(6, 0));
    assert!(!proof.proves_depth(6, 1));
    assert!(!proof.proves_depth(4, 0));
    assert!(!proof.proves_depth(8, 0), "past the end");
    // A stream without a switch keeps no depth table.
    let (plain_ops, _) =
        decode_gnu_bytecode_with_offset_map(&[192, 135], &mut vec![Value::NIL]).unwrap();
    assert_eq!(
        verify_stack_effects_with_tables(&plain_ops, 0, 1, None),
        Some(StackProof::default())
    );
    // Without its tables the proof still refuses every switch (the eager
    // installers and hand-assembled chunks take this form).
    let mut scratch = constants.clone();
    let (ops, _) = decode_gnu_bytecode_with_offset_map(&SWITCH_BYTES, &mut scratch).unwrap();
    assert!(!verify_stack_effects(&ops, 0, 2));
}

#[test]
fn every_table_target_is_entered_at_the_switch_depth() {
    crate::test_utils::init_test_tracing();
    // Byte 6 (instruction 4, `constant 10`) is reachable only through the
    // switch, so the proof enters it at the switch's post-pop depth; a
    // duplicate value (two keys to byte 8) is one target.
    let constants = pool(jump_table(&[
        ("vsw-foo", Value::fixnum(8)),
        ("vsw-bar", Value::fixnum(6)),
        ("vsw-baz", Value::fixnum(8)),
    ]));
    let proof = verify(&SWITCH_BYTES, &constants, 2).expect("admitted");
    assert!(proof.proves_depth(4, 0));
    assert!(proof.proves_depth(6, 0));
    let mut scratch = constants.clone();
    let (ops, map) = decode_gnu_bytecode_with_offset_map(&SWITCH_BYTES, &mut scratch).unwrap();
    let mut targets = switch_static_targets(&ops, &constants, Some(&map), 2).expect("static set");
    targets.sort_unstable();
    assert_eq!(targets, vec![(6, 4), (8, 6)]);
}

#[test]
fn switch_value_that_is_not_an_instruction_start_is_refused() {
    crate::test_utils::init_test_tracing();
    // Byte 4 is inside the `goto`'s operand.
    assert_eq!(
        verify(
            &SWITCH_BYTES,
            &pool(jump_table(&[("vsw-foo", Value::fixnum(4))])),
            2
        ),
        None
    );
    // Past the end of the stream, and negative.
    assert_eq!(
        verify(
            &SWITCH_BYTES,
            &pool(jump_table(&[("vsw-foo", Value::fixnum(100))])),
            2
        ),
        None
    );
    assert_eq!(
        verify(
            &SWITCH_BYTES,
            &pool(jump_table(&[("vsw-foo", Value::fixnum(-1))])),
            2
        ),
        None
    );
}

#[test]
fn switch_value_that_is_not_a_fixnum_is_refused() {
    crate::test_utils::init_test_tracing();
    let constants = pool(jump_table(&[("vsw-foo", Value::symbol("vsw-elsewhere"))]));
    assert_eq!(verify(&SWITCH_BYTES, &constants, 2), None);
}

#[test]
fn switch_whose_table_is_not_the_preceding_constant_is_refused() {
    crate::test_utils::init_test_tracing();
    // constant key; constant table; dup; switch; return -- the table reaches
    // the switch through a `dup`, which the resolver does not follow.
    let bytes = [193, 192, 137, 183, 135];
    let constants = pool(jump_table(&[("vsw-foo", Value::fixnum(4))]));
    assert_eq!(verify(&bytes, &constants, 3), None);
}

/// The collector only REMOVES entries of a weak table, and a removed entry
/// is a miss -- the proven fall-through -- so weakness needs no refusal (the
/// JIT compiles weak tables the same way, keeping its lookup shim).
#[test]
fn weak_jump_table_is_admitted() {
    crate::test_utils::init_test_tracing();
    let table = jump_table(&[("vsw-foo", Value::fixnum(8))]);
    let _ = table.with_hash_table_mut(|ht| ht.weakness = Some(HashTableWeakness::Key));
    assert!(verify(&SWITCH_BYTES, &pool(table), 2).is_some());
}

#[test]
fn switch_whose_constant_is_not_a_hash_table_is_refused() {
    crate::test_utils::init_test_tracing();
    let constants = pool(Value::vector(vec![Value::fixnum(8)]));
    assert_eq!(verify(&SWITCH_BYTES, &constants, 2), None);
}

#[test]
fn switch_target_must_be_entered_at_the_switch_depth() {
    crate::test_utils::init_test_tracing();
    // constant key; constant table; switch; constant 10; constant 20; return.
    // Instruction 3 (byte 3) is the fall-through at depth 0: admitted as a
    // target. Instruction 4 (byte 4) is reached by fall-through at depth 1,
    // so a switch edge entering it at depth 0 is a depth conflict.
    let bytes = [193, 192, 183, 194, 195, 135];
    assert!(
        verify(
            &bytes,
            &pool(jump_table(&[("vsw-foo", Value::fixnum(3))])),
            2
        )
        .is_some()
    );
    assert_eq!(
        verify(
            &bytes,
            &pool(jump_table(&[("vsw-foo", Value::fixnum(4))])),
            2
        ),
        None
    );
}

#[test]
fn switch_needs_its_two_operands() {
    crate::test_utils::init_test_tracing();
    // constant table; switch; return -- one operand on the stack.
    let bytes = [192, 183, 135];
    assert_eq!(
        verify(
            &bytes,
            &pool(jump_table(&[("vsw-foo", Value::fixnum(2))])),
            2
        ),
        None
    );
}

#[test]
fn deferred_gnu_function_with_a_switch_is_verified_and_resolves_through_its_proof() {
    crate::test_utils::init_test_tracing();
    let mut func = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    func.constants = pool(jump_table(&[("vsw-foo", Value::fixnum(8))])).into();
    func.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(2);
    func.gnu_bytecode_bytes = Some(neovm_value::tagged::header::LispByteVec::owned(
        SWITCH_BYTES.to_vec(),
    ));
    func.defer_gnu_decode();

    assert!(
        func.executes_verified_ops(),
        "the deferred proof admits the switch"
    );
    // The admitted target: byte 8 is instruction 6, proven at the switch's
    // post-pop depth 0.
    assert_eq!(func.verified_switch_pc(8), Some(Some(6)));
    // Any other byte offset is resolved and then held to its proven depth:
    // byte 3 (instruction 3, the fall-through `goto`) is proven at 0; byte 9
    // (instruction 7, `return`) at 1, not 0.
    assert_eq!(func.verified_switch_pc(3), Some(Some(3)));
    assert_eq!(func.verified_switch_pc(9), Some(None));
    // Byte 6 (instruction 4) is unreachable with this table: no proof, no entry.
    assert_eq!(func.verified_switch_pc(6), Some(None));
    // Not an instruction start; negative; past the end.
    assert_eq!(func.verified_switch_pc(4), Some(None));
    assert_eq!(func.verified_switch_pc(-3), Some(None));
    assert_eq!(func.verified_switch_pc(100), Some(None));
}

#[test]
fn switches_at_different_depths_keep_the_checked_driver() {
    crate::test_utils::init_test_tracing();
    // First switch exits at 0; an extra value under the second switch's
    // operands makes it exit at 1. Its target must not be usable by the
    // first switch through a table mutated after verification.
    let bytes = [193, 192, 183, 195, 193, 194, 183, 135];
    let constants = vec![
        jump_table(&[("vsw-foo", Value::fixnum(3))]),
        Value::symbol("vsw-foo"),
        jump_table(&[("vsw-foo", Value::fixnum(7))]),
        Value::fixnum(10),
    ];
    assert_eq!(verify(&bytes, &constants, 3), None);
    let mut func = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    func.constants = constants.into();
    func.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(3);
    func.gnu_bytecode_bytes = Some(neovm_value::tagged::header::LispByteVec::owned(
        bytes.to_vec(),
    ));
    func.defer_gnu_decode();
    assert!(!func.executes_verified_ops());
    assert_eq!(func.verified_switch_pc(7), None, "use the checked resolver");
}

#[test]
fn admitted_targets_are_pre_resolved_sorted_and_deduplicated() {
    crate::test_utils::init_test_tracing();
    let constants = pool(jump_table(&[
        ("vsw-foo", Value::fixnum(8)),
        ("vsw-bar", Value::fixnum(6)),
        ("vsw-baz", Value::fixnum(8)),
    ]));
    let proof = verify(&SWITCH_BYTES, &constants, 2).expect("admitted");
    assert_eq!(&*proof.switch_targets, &[(6, 4), (8, 6)]);
    assert_eq!(proof.switch_target(6), Some(4));
    assert_eq!(proof.switch_target(8), Some(6));
    assert_eq!(proof.switch_target(7), None, "not a table value");
    // A stream without a switch pre-resolves nothing.
    let (plain_ops, _) =
        decode_gnu_bytecode_with_offset_map(&[192, 135], &mut vec![Value::NIL]).unwrap();
    let plain = verify_stack_effects_with_tables(&plain_ops, 0, 1, None).expect("proof");
    assert!(plain.switch_targets.is_empty());
}

#[test]
fn switches_sharing_targets_share_one_pre_resolved_entry() {
    crate::test_utils::init_test_tracing();
    // constant key; constant table; switch; constant key; constant table;
    // switch; constant 10; return. Both switches name byte 6 (instruction
    // 6), entered at depth 0 from both.
    let bytes = [193, 192, 183, 193, 192, 183, 194, 135];
    let constants = pool(jump_table(&[("vsw-foo", Value::fixnum(6))]));
    let proof = verify(&bytes, &constants, 2).expect("admitted");
    assert_eq!(&*proof.switch_targets, &[(6, 6)]);
}

#[test]
fn pre_resolved_switch_hit_uses_the_fixed_proof_depth() {
    crate::test_utils::init_test_tracing();
    let mut func = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    func.constants = pool(jump_table(&[("vsw-foo", Value::fixnum(8))])).into();
    func.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(2);
    func.gnu_bytecode_bytes = Some(neovm_value::tagged::header::LispByteVec::owned(
        SWITCH_BYTES.to_vec(),
    ));
    func.defer_gnu_decode();
    assert!(func.executes_verified_ops());
    // Byte 8 is pre-resolved at the fixed switch depth. Byte 9 is proven
    // one deeper, and cannot be entered by this switch after a mutation.
    assert_eq!(func.verified_switch_pc(8), Some(Some(6)));
    assert_eq!(func.verified_switch_pc(9), Some(None));
}
