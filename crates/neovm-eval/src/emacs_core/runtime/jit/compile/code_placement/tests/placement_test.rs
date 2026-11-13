//! Placement contracts, including real relocation and execution at every offset.
//! Each fixture owns its module, arena and compiler state on the current thread.

use super::*;

#[test]
fn placement_configuration_rejects_ambiguous_or_invalid_offsets() {
    assert_eq!(
        parse_config(None, None).unwrap(),
        PlacementConfig::Policy(PlacementPolicy::Switch64),
        "unset placement is the graduated switch64 default"
    );
    assert_eq!(
        parse_config(Some("off"), None).unwrap(),
        PlacementConfig::Policy(PlacementPolicy::Off),
        "off remains the explicit opt-out"
    );
    assert_eq!(
        parse_config(Some("switch64"), None).unwrap(),
        PlacementConfig::Policy(PlacementPolicy::Switch64)
    );
    for value in ["-1", "64", "256", "garbage", ""] {
        assert!(parse_config(None, Some(value)).is_err(), "{value}");
    }
    assert!(parse_config(Some("loop64"), Some("0")).is_err());
    assert!(parse_config(Some("unknown"), None).is_err());
    for offset in 0..64 {
        let value = PlacementOffset::new(offset).unwrap();
        assert_eq!(value.bytes(), offset);
        assert_eq!(value.validate_alignment(8).is_ok(), offset % 8 == 0);
    }
}

#[test]
fn placement_diagnostic_selector_matches_exact_lisp_names() {
    assert!(selector_matches(None, None));
    assert!(selector_matches(
        Some("elb-pcase"),
        Some("lisp:elb-pcase#123:opt")
    ));
    assert!(!selector_matches(
        Some("elb-pcase"),
        Some("lisp:elb-pcase-entry#123:opt")
    ));
    assert!(!selector_matches(
        Some("elb-pcase"),
        Some("__neovm_jit_leaf")
    ));
}

#[test]
fn placement_aligns_shortest_backward_target_with_required_buffer_alignment() {
    let edges = [(0, 80), (900, 80), (300, 257), (290, 257), (600, 580)];
    let config = PlacementConfig::Policy(PlacementPolicy::Loop64);
    let offset = placement_for(config, 1, &edges).unwrap().unwrap();
    assert_eq!((offset.bytes() + 580) % 64, 0);
    let aligned = placement_for(config, 8, &edges).unwrap().unwrap();
    assert_eq!(aligned.bytes() % 8, 0);
    assert!(offset.bytes() - aligned.bytes() < 8);
    assert_eq!(
        placement_for(config, 1, &[(0, 10)])
            .unwrap()
            .unwrap()
            .bytes(),
        0
    );
    assert!(placement_for(config, 3, &edges).is_err());
}

#[test]
fn placement_hint_is_one_shot_and_nested_scope_restores_on_unwind() {
    assert_eq!(take_allocation_hint(), None);
    let outer = PlacementScope::stage(PlacementOffset::new(17).unwrap());
    let result = std::panic::catch_unwind(|| {
        let _inner = PlacementScope::stage(PlacementOffset::new(31).unwrap());
        assert_eq!(take_allocation_hint().unwrap().bytes(), 31);
        assert_eq!(take_allocation_hint(), None);
        panic!("fixture compiler abort");
    });
    assert!(result.is_err());
    assert_eq!(take_allocation_hint().unwrap().bytes(), 17);
    drop(outer);
    assert_eq!(take_allocation_hint(), None);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod native {
    use super::*;
    use cranelift_codegen::ir::{AbiParam, InstBuilder, types};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::{Linkage, default_libcall_names};

    extern "C" fn finish(value: u64) -> u64 {
        value + 7
    }

    fn module() -> JITModule {
        let isa = crate::emacs_core::jit::compile::lowering::build_jit_isa(
            crate::emacs_core::jit::compile::lowering::RegallocChoice::Full,
        )
        .unwrap();
        let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
        builder.symbol("placement_fixture_finish", finish as *const u8);
        let arena = crate::emacs_core::jit::compile::code_arena::CodeArena::new();
        builder.memory_provider(Box::new(arena.handle()));
        JITModule::new(builder)
    }

    fn system_module() -> JITModule {
        let isa = crate::emacs_core::jit::compile::lowering::build_jit_isa(
            crate::emacs_core::jit::compile::lowering::RegallocChoice::Full,
        )
        .unwrap();
        let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
        builder.symbol("placement_fixture_finish", finish as *const u8);
        JITModule::new(builder)
    }

    fn fixture(module: &mut JITModule) -> cranelift_codegen::Context {
        let mut ctx = module.make_context();
        ctx.func.signature.params.push(AbiParam::new(types::I64));
        ctx.func.signature.returns.push(AbiParam::new(types::I64));
        let helper = module
            .declare_function(
                "placement_fixture_finish",
                Linkage::Import,
                &ctx.func.signature,
            )
            .unwrap();
        let helper = module.declare_func_in_func(helper, &mut ctx.func);
        let mut scratch = FunctionBuilderContext::new();
        let mut fb = FunctionBuilder::new(&mut ctx.func, &mut scratch);
        let entry = fb.create_block();
        let header = fb.create_block();
        let body = fb.create_block();
        let exit = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.append_block_param(header, types::I64);
        fb.append_block_param(header, types::I64);
        fb.switch_to_block(entry);
        let n = fb.block_params(entry)[0];
        let zero = fb.ins().iconst(types::I64, 0);
        fb.ins().jump(header, &[n.into(), zero.into()]);
        fb.switch_to_block(header);
        let n = fb.block_params(header)[0];
        let sum = fb.block_params(header)[1];
        fb.ins().brif(n, body, &[], exit, &[]);
        fb.switch_to_block(body);
        let next_sum = fb.ins().iadd(sum, n);
        let next_n = fb.ins().iadd_imm_s(n, -1);
        fb.ins().jump(header, &[next_n.into(), next_sum.into()]);
        fb.switch_to_block(exit);
        let call = fb.ins().call(helper, &[sum]);
        let answer = fb.inst_results(call)[0];
        fb.ins().return_(&[answer]);
        fb.seal_all_blocks();
        fb.finalize(module.target_config());
        ctx
    }

    #[test]
    fn placement_arena_reserves_shifted_pages_and_preserves_larger_alignment() {
        use cranelift_jit::{BranchProtection, JITMemoryKind, JITMemoryProvider};
        let arena = crate::emacs_core::jit::compile::code_arena::CodeArena::new();
        let mut handle = arena.handle();
        // SAFETY: sysconf has no preconditions; Linux exposes a page size.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
        for (offset, size, alignment) in [(63, page, 1), (17, page - 17, 1), (0, 1, 256)] {
            let _scope = PlacementScope::stage(PlacementOffset::new(offset).unwrap());
            let entry = handle
                .allocate(size, alignment, JITMemoryKind::Executable)
                .unwrap();
            assert_eq!(entry as usize % 64, offset);
            assert_eq!(entry as usize % alignment as usize, 0);
            // SAFETY: allocation owns this writable range until finalization;
            // touching both endpoints exercises shifted page reservations.
            unsafe {
                entry.write(0xc3);
                entry.add(size - 1).write(0xc3);
            }
        }
        handle.finalize(BranchProtection::None).unwrap();
        let _scope = PlacementScope::stage(PlacementOffset::new(1).unwrap());
        assert!(handle.allocate(1, 256, JITMemoryKind::Executable).is_err());
    }

    #[test]
    fn placement_replays_identical_loop_and_external_relocation_at_all_64_offsets() {
        let mut module = module();
        let mut baseline = fixture(&mut module);
        let baseline_id = module
            .declare_anonymous_function(&baseline.func.signature)
            .unwrap();
        module.define_function(baseline_id, &mut baseline).unwrap();
        let reference = baseline.compiled_code().unwrap().code_buffer().to_vec();
        module.finalize_definitions().unwrap();
        for offset in 0..64 {
            let mut ctx = fixture(&mut module);
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
            assert!(
                !code.buffer.relocs().is_empty(),
                "external call is relocated"
            );
            assert_eq!(code.code_buffer(), &reference);
            module.finalize_definitions().unwrap();
            let entry = module.get_finalized_function(id);
            assert_eq!(entry as usize % 64, offset);
            // SAFETY: the fixture declares and defines exactly this C ABI;
            // finalization applied its helper relocation and sealed its code.
            let run: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(entry) };
            assert_eq!(run(100), 5057);
            assert_eq!(run(0), 7);
        }
        assert_eq!(take_allocation_hint(), None);
    }

    #[test]
    fn placement_loop64_metadata_preserves_machine_bytes_and_aligns_a_header() {
        let reference = {
            let _scope = config_scope_for_test(PlacementConfig::Policy(PlacementPolicy::Off));
            let mut module = module();
            let mut ctx = fixture(&mut module);
            let id = module
                .declare_anonymous_function(&ctx.func.signature)
                .unwrap();
            module.define_function(id, &mut ctx).unwrap();
            ctx.compiled_code().unwrap().code_buffer().to_vec()
        };
        let _scope = config_scope_for_test(PlacementConfig::Policy(PlacementPolicy::Loop64));
        let mut module = module();
        let mut ctx = fixture(&mut module);
        let id = module
            .declare_anonymous_function(&ctx.func.signature)
            .unwrap();
        define_placed_function(
            &mut module,
            id,
            &mut ctx,
            PlacementConfig::Policy(PlacementPolicy::Loop64),
        )
        .unwrap();
        let code = ctx.compiled_code().unwrap();
        assert_eq!(code.code_buffer(), &reference);
        let (_, header) = code
            .bb_edges
            .iter()
            .filter(|(from, to)| to <= from)
            .min_by_key(|(from, to)| (from - to, *to))
            .expect("fixture contains a backward edge");
        module.finalize_definitions().unwrap();
        let entry = module.get_finalized_function(id);
        assert_eq!((entry as usize + *header as usize) % 64, 0);
    }

    #[test]
    fn placement_switch64_keeps_marked_guards_aligned_after_different_packed_predecessors() {
        use crate::emacs_core::jit::compile::code_placement::anchors::{
            PlacementAnchor, first_anchor_offset,
        };
        use cranelift_codegen::ir::Opcode;

        let _scope = config_scope_for_test(PlacementConfig::Policy(PlacementPolicy::Off));
        let mut module = module();
        let mut reference_ctx = fixture(&mut module);
        let reference_id = module
            .declare_anonymous_function(&reference_ctx.func.signature)
            .unwrap();
        module
            .define_function(reference_id, &mut reference_ctx)
            .unwrap();
        let reference = reference_ctx
            .compiled_code()
            .unwrap()
            .code_buffer()
            .to_vec();
        assert_eq!(reference_ctx.compiled_code().unwrap().buffer.alignment, 1);
        let mut predecessor_lengths = Vec::new();
        let mut marked_entries = Vec::new();
        for additions in [0, 1, 7, 31] {
            let mut predecessor = module.make_context();
            predecessor
                .func
                .signature
                .params
                .push(AbiParam::new(types::I64));
            predecessor
                .func
                .signature
                .returns
                .push(AbiParam::new(types::I64));
            let mut scratch = FunctionBuilderContext::new();
            let mut fb = FunctionBuilder::new(&mut predecessor.func, &mut scratch);
            let entry = fb.create_block();
            fb.append_block_params_for_function_params(entry);
            fb.switch_to_block(entry);
            let argument = fb.block_params(entry)[0];
            let mut result = argument;
            for _ in 0..additions {
                result = fb.ins().iadd(result, argument);
            }
            fb.ins().return_(&[result]);
            fb.seal_all_blocks();
            fb.finalize(module.target_config());
            let predecessor_id = module
                .declare_anonymous_function(&predecessor.func.signature)
                .unwrap();
            module
                .define_function(predecessor_id, &mut predecessor)
                .unwrap();
            predecessor_lengths.push(predecessor.compiled_code().unwrap().code_buffer().len());

            let mut marked = fixture(&mut module);
            let branch = marked
                .func
                .layout
                .blocks()
                .flat_map(|block| marked.func.layout.block_insts(block))
                .find(|&inst| marked.func.dfg.insts[inst].opcode() == Opcode::Brif)
                .expect("fixture has an actual loop condition");
            marked
                .func
                .set_srcloc(branch, PlacementAnchor::SwitchConsGuard.location());
            let id = module
                .declare_anonymous_function(&marked.func.signature)
                .unwrap();
            define_placed_function(
                &mut module,
                id,
                &mut marked,
                PlacementConfig::Policy(PlacementPolicy::Switch64),
            )
            .unwrap();
            let code = marked.compiled_code().unwrap();
            assert_eq!(code.code_buffer(), &reference);
            assert_eq!(code.buffer.alignment, 1);
            let anchor = first_anchor_offset(code, PlacementAnchor::SwitchConsGuard)
                .expect("marked condition survives into native metadata");
            marked_entries.push((id, anchor, code.buffer.alignment));
        }
        assert!(
            predecessor_lengths
                .windows(2)
                .all(|pair| pair[0] != pair[1])
        );
        // Seal only once: every predecessor and marked function was packed
        // into writable arena capacity without page-boundary resets between it.
        module.finalize_definitions().unwrap();
        let mut previous_residue = None;
        for (id, anchor, alignment) in marked_entries {
            let entry = module.get_finalized_function(id);
            let residue = (entry as usize + anchor as usize) % 64;
            assert!(residue < alignment as usize);
            if let Some(previous) = previous_residue {
                assert_eq!(residue, previous);
            }
            previous_residue = Some(residue);
            // SAFETY: fixture emits exactly this C ABI and its helper symbol
            // is relocated; the owning module remains alive after sealing.
            let run: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(entry) };
            assert_eq!(run(100), 5057);
            assert_eq!(run(0), 7);
        }
    }

    #[test]
    fn placement_switch64_unmarked_packed_functions_preserve_bytes_relocations_and_execution() {
        let _scope = config_scope_for_test(PlacementConfig::Policy(PlacementPolicy::Off));
        let mut module = module();
        let mut reference_ctx = fixture(&mut module);
        let reference_id = module
            .declare_anonymous_function(&reference_ctx.func.signature)
            .unwrap();
        module
            .define_function(reference_id, &mut reference_ctx)
            .unwrap();
        let reference = reference_ctx
            .compiled_code()
            .unwrap()
            .code_buffer()
            .to_vec();
        let mut entries = Vec::new();
        for _ in 0..8 {
            let mut ctx = fixture(&mut module);
            assert!(
                ctx.func.srclocs.is_empty(),
                "fixture takes the unmarked fast path"
            );
            let id = module
                .declare_anonymous_function(&ctx.func.signature)
                .unwrap();
            define_placed_function(
                &mut module,
                id,
                &mut ctx,
                PlacementConfig::Policy(PlacementPolicy::Switch64),
            )
            .unwrap();
            let code = ctx.compiled_code().unwrap();
            assert_eq!(code.code_buffer(), &reference);
            assert!(!code.buffer.relocs().is_empty());
            entries.push(id);
        }
        module.finalize_definitions().unwrap();
        for id in entries {
            let entry = module.get_finalized_function(id);
            assert_eq!(entry as usize % 64, 0);
            // SAFETY: fixture has this exact C ABI and the live owning module
            // finalized its external helper relocation and executable pages.
            let run: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(entry) };
            assert_eq!(run(100), 5057);
            assert_eq!(run(0), 7);
        }
        assert_eq!(take_allocation_hint(), None);
    }

    #[test]
    fn placement_function64_aligns_every_function_in_an_unsealed_batch() {
        let mut module = module();
        let mut ids = Vec::new();
        for _ in 0..8 {
            let mut ctx = fixture(&mut module);
            let id = module
                .declare_anonymous_function(&ctx.func.signature)
                .unwrap();
            define_placed_function(
                &mut module,
                id,
                &mut ctx,
                PlacementConfig::Policy(PlacementPolicy::Function64),
            )
            .unwrap();
            ids.push(id);
        }
        module.finalize_definitions().unwrap();
        for id in ids {
            assert_eq!(module.get_finalized_function(id) as usize % 64, 0);
        }
    }

    #[test]
    fn placement_policy_falls_back_on_system_memory_but_exact_diagnostics_refuse_it() {
        let mut module = system_module();
        let mut ctx = fixture(&mut module);
        let id = module
            .declare_anonymous_function(&ctx.func.signature)
            .unwrap();
        define_placed_function(
            &mut module,
            id,
            &mut ctx,
            PlacementConfig::Policy(PlacementPolicy::Loop64),
        )
        .unwrap();
        module.finalize_definitions().unwrap();
        let entry = module.get_finalized_function(id);
        // SAFETY: this module owns the finalized fixture and its helper symbol;
        // the declared and emitted signature is exactly extern C fn(u64)->u64.
        let run: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(entry) };
        assert_eq!(run(100), 5057);

        let mut ctx = fixture(&mut module);
        let id = module
            .declare_anonymous_function(&ctx.func.signature)
            .unwrap();
        assert!(
            define_placed_function(
                &mut module,
                id,
                &mut ctx,
                PlacementConfig::Offset(PlacementOffset::new(0).unwrap()),
            )
            .is_err()
        );
        assert_eq!(take_allocation_hint(), None);
    }
}
