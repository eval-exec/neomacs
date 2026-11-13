//! The real cons-trie emitter adds only per-instruction source metadata.

use super::*;
use crate::emacs_core::jit::compile::code_placement::anchors::{
    PlacementAnchor, first_anchor_offset,
};
use crate::emacs_core::jit::compile::code_placement::{
    PlacementConfig, PlacementPolicy, config_scope_for_test,
};
use cranelift_codegen::ir::{AbiParam, Function, Opcode};
use cranelift_frontend::FunctionBuilderContext;

struct SingleLanding(Block);

impl SwitchLandings for SingleLanding {
    fn landing(&mut self, _fb: &mut FunctionBuilder, _key: usize) -> Block {
        self.0
    }

    fn fill_pending(&mut self, _fb: &mut FunctionBuilder) {}
}

fn cons_trie(policy: PlacementPolicy) -> Function {
    let _scope = config_scope_for_test(PlacementConfig::Policy(policy));
    let mut function = Function::new();
    function.signature.params.push(AbiParam::new(types::I64));
    function.signature.returns.push(AbiParam::new(types::I64));
    let mut scratch = FunctionBuilderContext::new();
    let mut fb = FunctionBuilder::new(&mut function, &mut scratch);
    let entry = fb.create_block();
    let miss = fb.create_block();
    let hit = fb.create_block();
    fb.append_block_params_for_function_params(entry);
    fb.switch_to_block(entry);
    let value = fb.block_params(entry)[0];
    let mut trie = KeyTrie::default();
    trie.insert(
        &[
            InlineKeyNode::Cons,
            InlineKeyNode::Imm(170),
            InlineKeyNode::Imm(0),
        ],
        0,
    );
    emit_trie(
        &mut fb,
        &trie,
        value,
        &mut Vec::new(),
        miss,
        &mut SingleLanding(hit),
    );
    fb.switch_to_block(miss);
    let zero = fb.ins().iconst(types::I64, 0);
    fb.ins().return_(&[zero]);
    fb.switch_to_block(hit);
    let one = fb.ins().iconst(types::I64, 1);
    fb.ins().return_(&[one]);
    fb.seal_all_blocks();
    fb.finalize(
        lowering::build_jit_isa(lowering::RegallocChoice::Full)
            .unwrap()
            .frontend_config(),
    );
    function
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn switch64_marks_only_the_actual_cons_guard_and_preserves_native_bytes() {
    let marked = cons_trie(PlacementPolicy::Switch64);
    let unmarked = cons_trie(PlacementPolicy::Off);
    let marker = PlacementAnchor::SwitchConsGuard.location();
    let marked_instructions: Vec<_> = marked
        .layout
        .blocks()
        .flat_map(|block| marked.layout.block_insts(block))
        .filter(|&inst| marked.srcloc(inst) == marker)
        .collect();
    assert_eq!(marked_instructions.len(), 1);
    assert_eq!(
        marked.dfg.insts[marked_instructions[0]].opcode(),
        Opcode::Brif
    );
    assert!(
        unmarked
            .layout
            .blocks()
            .flat_map(|block| unmarked.layout.block_insts(block))
            .all(|inst| unmarked.srcloc(inst).is_default())
    );

    let _scope = config_scope_for_test(PlacementConfig::Policy(PlacementPolicy::Off));
    let isa = lowering::build_jit_isa(lowering::RegallocChoice::Full).unwrap();
    let mut marked_ctx = cranelift_codegen::Context::for_function(marked);
    let mut unmarked_ctx = cranelift_codegen::Context::for_function(unmarked);
    let marked_code = marked_ctx.compile(&*isa, &mut Default::default()).unwrap();
    let unmarked_code = unmarked_ctx
        .compile(&*isa, &mut Default::default())
        .unwrap();
    assert_eq!(marked_code.code_buffer(), unmarked_code.code_buffer());
    assert_eq!(marked_code.buffer.alignment, unmarked_code.buffer.alignment);
    assert!(first_anchor_offset(marked_code, PlacementAnchor::SwitchConsGuard).is_some());
    assert_eq!(
        first_anchor_offset(unmarked_code, PlacementAnchor::SwitchConsGuard),
        None
    );
}
