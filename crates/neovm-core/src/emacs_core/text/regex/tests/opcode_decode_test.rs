use super::*;
use strum::VariantArray;

#[test]
fn opcode_35_is_rejected_by_derived_decoder() {
    assert_eq!(RegexOp::from_repr(35), None);
    assert_eq!(RegexOp::from_byte(35), None);
}

#[test]
fn every_opcode_byte_matches_the_declared_variants() {
    for byte in u8::MIN..=u8::MAX {
        let declared = RegexOp::VARIANTS
            .iter()
            .copied()
            .find(|op| *op as u8 == byte);
        assert_eq!(RegexOp::from_repr(byte), declared, "opcode byte {byte}");
        assert_eq!(RegexOp::from_byte(byte), declared, "opcode byte {byte}");
    }
}

#[test]
fn invalid_opcode_cannot_be_sealed_and_fails_checked_matching() {
    let mut pattern = CompiledPattern::new();
    pattern.buffer = vec![35, RegexOp::Succeed as u8];
    assert!(!validate_sealed_buffer(&pattern));
    assert_eq!(opcode_len(&pattern.buffer, 0), None);
    assert!(re_match(&pattern, b"", 0, 0, &DefaultSyntaxLookup, 0).is_none());
}

#[test]
fn every_declared_opcode_is_accepted_at_a_validated_boundary() {
    for &op in RegexOp::VARIANTS {
        let mut pattern = CompiledPattern::new();
        // Zero operands give minimal literals/charsets and in-bounds
        // register, syntax and counter fields. No execution is requested.
        // Nastyloop requires a preceding NoOp, as GNU's compiler emits.
        pattern.buffer = vec![RegexOp::NoOp as u8, op as u8, 0, 0, 0, 0];
        let len = opcode_len(&pattern.buffer, 1).expect("declared opcode");
        pattern.buffer.truncate(len + 1);
        if matches!(
            op,
            RegexOp::Jump
                | RegexOp::OnFailureJump
                | RegexOp::OnFailureKeepStringJump
                | RegexOp::OnFailureJumpLoop
                | RegexOp::OnFailureJumpNastyloop
                | RegexOp::OnFailureJumpSmart
                | RegexOp::SucceedN
                | RegexOp::JumpN
        ) {
            store_number(&mut pattern.buffer, 2, (len - 3) as i16);
        }
        pattern.buffer.push(RegexOp::Succeed as u8);
        assert!(validate_sealed_buffer(&pattern), "opcode {op:?}");
    }
}

#[test]
fn an_invalid_opcode_byte_in_a_literal_operand_remains_valid_data() {
    let mut pattern = CompiledPattern::new();
    pattern.buffer = vec![RegexOp::Exactn as u8, 1, 35, RegexOp::Succeed as u8];
    pattern.buffer_sealed = validate_sealed_buffer(&pattern);
    assert!(pattern.buffer_sealed);
    let (end, _) = re_match(&pattern, b"#", 0, 1, &DefaultSyntaxLookup, 0)
        .expect("literal byte 35 is data, not an opcode");
    assert_eq!(end, 1);
}
