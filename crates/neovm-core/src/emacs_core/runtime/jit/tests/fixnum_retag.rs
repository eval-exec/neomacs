use super::*;

/// The proof used to omit tag guards and GC stores must account for masked
/// shift counts, and it applies only to the I64 representation of Lisp values.
#[test]
fn fixnum_retag_proof_requires_cleared_tag_bits_in_an_i64() {
    for ty in [types::I8, types::I16, types::I32, types::I64] {
        let mut func = Function::with_name_signature(
            UserFuncName::user(0, 0),
            Signature::new(cranelift_codegen::isa::CallConv::SystemV),
        );
        let mut fbctx = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);
        let block = fb.create_block();
        let raw = fb.append_block_param(block, ty);
        fb.switch_to_block(block);
        fb.seal_block(block);
        for shift in [-1, 0, 1, 2, 3, 63, 64, 65, 66, 127, 128, 129, 130] {
            let shifted = fb.ins().ishl_imm_u(raw, shift);
            for tag in [0, 1, 2, 3, 4, 6] {
                let added = fb.ins().iadd_imm_u(shifted, tag);
                let ored = fb.ins().bor_imm_u(shifted, tag);
                let expected = ty == types::I64 && tag == 2 && (shift as u64 & 63) >= 2;
                for value in [added, ored] {
                    assert_eq!(
                        is_known_fixnum(&fb, value),
                        expected,
                        "type={ty}, shift={shift}, tag={tag}, value={value}"
                    );
                }
            }
        }
        let constant = fb.ins().iconst(ty, Value::make_int(7).bits() as i64);
        assert_eq!(is_known_fixnum(&fb, constant), ty == types::I64);
        let bare_add = fb.ins().iadd_imm_u(raw, 2);
        let variable_shift = fb.ins().ishl(raw, raw);
        let variable_tag = fb.ins().iadd_imm_u(variable_shift, 2);
        for unknown in [raw, bare_add, variable_tag] {
            assert!(!is_known_fixnum(&fb, unknown));
        }
    }
}

#[test]
fn fixnum_retag_emitter_preserves_the_local_guard_and_root_proof() {
    let mut func = Function::with_name_signature(
        UserFuncName::user(0, 0),
        Signature::new(cranelift_codegen::isa::CallConv::SystemV),
    );
    let mut fbctx = FunctionBuilderContext::new();
    let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);
    let block = fb.create_block();
    let raw = fb.append_block_param(block, types::I64);
    fb.switch_to_block(block);
    fb.seal_block(block);
    lowering::imm_pool_reset();
    lowering::imm_pool_define(&mut fb, lowering::POOLED_IMMEDIATES);
    let tagged = lowering::retag_fixnum(&mut fb, raw);
    assert!(is_known_fixnum(&fb, tagged));
    lowering::imm_pool_reset();
}
