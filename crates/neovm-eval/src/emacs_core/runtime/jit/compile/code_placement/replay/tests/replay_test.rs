use super::*;

#[test]
fn diagnostic_replay_refuses_missing_or_metadata_only_vcode() {
    for text in [
        None,
        Some(""),
        Some(" \n"),
        Some("block0:\n  uninit %rax\n"),
    ] {
        assert!(matches!(
            validate_vcode(text),
            Err(ReplayError::MissingAssembly)
        ));
    }
}

#[test]
fn diagnostic_replay_rejects_aligned_simd_unknown_and_hidden_instruction_lines() {
    for text in [
        "block0:\n  movaps (%rip), %xmm0\n",
        "block0:\n  movdqa (%rip), %xmm0\n",
        "block0:\n  vmovdqa (%rip), %xmm0\n",
        "block0:\n  unsupported_opcode %rax\n",
        "block0:\n  jz label1; movaps (%rip), %xmm0\n",
        "block0:\n  retq\nmovaps (%rip), %xmm0\n",
        "block0:\n  unwind UnknownFrameRecord\n  retq\n",
        "block0:\n  lock cmpxchgq (%rip), %rax\n",
        "block0:\n  .byte 0x90\n",
        "block0:\n  leaq 8(%rip), %rax\n  retq\n",
        "block0:\n  movq %rax, %rcx; j label2\n",
        "block0:\n  jmp label1; j label2\n",
        "block0:\n  j label2\n",
        "block0:\n  imulq %rax ;; implicit: movaps (%rip), %xmm0\n",
    ] {
        assert!(
            matches!(
                validate_vcode(Some(text)),
                Err(ReplayError::UnsupportedInstruction { .. })
            ),
            "{text}"
        );
    }
}

#[test]
fn diagnostic_replay_accepts_reviewed_scalar_and_unaligned_vector_operations() {
    validate_vcode(Some(
        "  pushq %rbp\n\
         \x20 unwind PushFrameRegs { offset_upward_to_caller_sp: 16 }\n\
         block0:\n\
         \x20 movq (%rip), %rax\n\
         \x20 cmovneq (%rip), %rax\n\
         \x20 vmovq %rax, %xmm0\n\
         \x20 vpinsrq $0x1, (%rip), %xmm0, %xmm0\n\
         \x20 vmovdqu %xmm0, (%rsp)\n\
         \x20 load_ext_name userextname0+0, %rax\n\
         \x20 imulq %rcx ;; implicit: %rax, %rdx\n\
         \x20 cqto  ;; implicit: %rdx, %rax\n\
         \x20 idivq %r15 ;; implicit: %rax, %rdx, trap=252\n\
         \x20 jz label1; j label2\n\
         block1:\n\
         \x20 retq\n",
    ))
    .unwrap();
}

#[test]
fn diagnostic_replay_executes_integer_constant_pool_at_every_byte_offset() {
    use crate::emacs_core::jit::compile::code_arena::CodeArena;
    use crate::emacs_core::jit::compile::code_placement::{
        PlacementConfig, PlacementOffset, define_placed_function,
    };
    use crate::emacs_core::jit::compile::lowering::{RegallocChoice, build_jit_isa};
    use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::{Module, default_libcall_names};

    const CONSTANT: u64 = 0x1234_5678_9abc_def0;
    let isa = build_jit_isa(RegallocChoice::Full).unwrap();
    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    let arena = CodeArena::new();
    builder.memory_provider(Box::new(arena.handle()));
    let mut module = JITModule::new(builder);
    let mut reference = None;
    for offset in 0..64 {
        let mut ctx = module.make_context();
        ctx.func.signature.params.push(AbiParam::new(types::I64));
        ctx.func.signature.returns.push(AbiParam::new(types::I64));
        let mut scratch = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut scratch);
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        let condition = fb.block_params(entry)[0];
        let constant = fb.ins().iconst(types::I64, CONSTANT as i64);
        let zero = fb.ins().iconst(types::I64, 0);
        // The first select arm is a GprMem operand in Cranelift's x64
        // lowering, which sinks this integer constant into its 8-byte pool.
        let selected = fb.ins().select(condition, constant, zero);
        fb.ins().return_(&[selected]);
        fb.seal_all_blocks();
        fb.finalize(module.target_config());
        let id = module
            .declare_anonymous_function(&ctx.func.signature)
            .unwrap();
        define_placed_function(
            &mut module,
            id,
            &mut ctx,
            PlacementConfig::Offset(PlacementOffset::new(offset).unwrap()),
        )
        .unwrap();
        let code = ctx.compiled_code().unwrap();
        assert_eq!(
            code.buffer.alignment, 8,
            "fixture must exercise pool alignment"
        );
        assert!(code.vcode.as_ref().unwrap().contains("(%rip)"));
        if let Some(bytes) = &reference {
            assert_eq!(code.code_buffer(), bytes);
        } else {
            reference = Some(code.code_buffer().to_vec());
        }
        module.finalize_definitions().unwrap();
        let entry = module.get_finalized_function(id);
        assert_eq!(entry as usize % 64, offset);
        // SAFETY: this fixture defines exactly extern C (u64) -> u64;
        // the owning module finalized the code and stays alive during calls.
        let run: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(entry) };
        assert_eq!(run(0), 0);
        assert_eq!(run(1), CONSTANT);
        assert_eq!(run(u64::MAX), CONSTANT);
    }
}
