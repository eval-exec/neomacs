//! The register leaf ABI (S2.1a): T1, Rust and Cranelift agree on
//! `(vmctx, aux, a0..) -> (value, status)` for every arity; and a compiled
//! body answers the same through either ABI, on every exit.

use super::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::default_libcall_names;

/// `fn(vmctx, aux, a0..a{k-1}) -> (vmctx ^ 3*aux + Σ (i+2)*a_i, 10k + 7)`,
/// built from [`LeafAbi::signature`] with the JIT's own ISA.
fn build_probe(module: &mut JITModule, k: u8) -> cranelift_module::FuncId {
    let cfg = module.target_config();
    let sig = LeafAbi::Register { arity: k }.signature(cfg.default_call_conv, cfg.pointer_type());
    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig.clone());
    let mut fbctx = cranelift_frontend::FunctionBuilderContext::new();
    {
        let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);
        let block = fb.create_block();
        fb.append_block_params_for_function_params(block);
        fb.switch_to_block(block);
        fb.seal_block(block);
        let p = fb.block_params(block).to_vec();
        let three = fb.ins().iconst(types::I64, 3);
        let aux3 = fb.ins().imul(p[1], three);
        let mut acc = fb.ins().bxor(p[0], aux3);
        for i in 0..k as usize {
            let w = fb.ins().iconst(types::I64, i as i64 + 2);
            let term = fb.ins().imul(p[2 + i], w);
            acc = fb.ins().iadd(acc, term);
        }
        let status = fb.ins().iconst(types::I64, 10 * i64::from(k) + 7);
        fb.ins().return_(&[acc, status]);
        fb.finalize(cfg);
    }
    let fid = module
        .declare_function(&format!("neovm_reg_abi_probe_{k}"), Linkage::Local, &sig)
        .expect("declare");
    let mut ctx = module.make_context();
    ctx.func = func;
    module.define_function(fid, &mut ctx).expect("define");
    module.clear_context(&mut ctx);
    fid
}

/// T1: both return words, the two register parameters and every argument
/// (the fifth and sixth on the stack) arrive where each side puts them,
/// called directly and through the arity's thunk.
#[test]
fn rust_and_cranelift_agree_on_the_register_abi_for_every_arity() {
    let isa = super::lowering::jit_isa().expect("isa");
    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let ids: Vec<_> = (0..=MAX_REG_ARGS as u8)
        .map(|k| build_probe(&mut module, k))
        .collect();
    module.finalize_definitions().expect("finalize");
    let args: [i64; MAX_REG_ARGS] = [
        0x1111,
        -0x2222,
        0x3333_0000_0000,
        -7,
        0x0bad_cafe,
        0x7fff_ffff_ffff,
    ];
    let vmctx = 0x5a5a_5a5a_0000usize as *mut u8;
    let aux = 0x0123_4567_89a0usize as *const u8;
    for (k, &id) in ids.iter().enumerate() {
        let entry = module.get_finalized_function(id);
        // SAFETY: `entry` has the register ABI for exactly `k` words and
        // never dereferences its pointer parameters.
        let ret = unsafe { call_register_entry(entry, k as u8, vmctx, aux, args.as_ptr()) };
        let mut want = (vmctx as i64) ^ (aux as i64).wrapping_mul(3);
        for (i, &a) in args[..k].iter().enumerate() {
            want = want.wrapping_add(a.wrapping_mul(i as i64 + 2));
        }
        assert_eq!(
            ret,
            NativeRet {
                value: want,
                status: 10 * k as i64 + 7
            },
            "arity {k}"
        );
        // The leaf's thunk for the arity answers the same, and strips the
        // spec slot's key flags from `aux`.
        let thunk = register_thunk_for(LeafAbi::Register { arity: k as u8 });
        for flagged in [
            aux,
            (aux as usize | SpecSlot::KEY_REGISTER as usize) as *const u8,
        ] {
            // SAFETY: as above.
            let via_thunk = unsafe { thunk(entry, vmctx, flagged, args.as_ptr()) };
            assert_eq!(via_thunk, ret, "arity {k} through its thunk");
        }
    }
}

#[test]
fn only_bodies_a_direct_call_can_enter_take_the_register_abi() {
    // This pin covers the original global implication. The self policy
    // requires an actual selected self site, covered by direct_self_tests.
    force_direct_sites_for_test(Some(DirectSitesMode::All));
    force_direct_memory_for_test(Some(false));
    // Direct calls imply the register ABI: pin them off, so the ABI knob
    // alone decides (the suite also runs with `NEOVM_JIT_DIRECT_CALL=on`).
    force_direct_call_for_test(Some(false));
    force_register_abi_for_test(Some(true));
    let build = |aot, osr, arity, frameless, prefix| {
        LeafAbi::for_build(aot, osr, arity, frameless, prefix, false)
    };
    assert_eq!(
        build(false, false, 0, true, 0),
        LeafAbi::Register { arity: 0 }
    );
    assert_eq!(
        build(false, false, MAX_REG_ARGS, true, 0),
        LeafAbi::Register {
            arity: MAX_REG_ARGS as u8
        }
    );
    assert_eq!(
        build(false, false, MAX_REG_ARGS + 1, true, 0),
        LeafAbi::Memory
    );
    assert_eq!(build(true, false, 1, true, 0), LeafAbi::Memory, "AOT");
    assert_eq!(build(false, true, 1, true, 0), LeafAbi::Memory, "OSR");
    assert_eq!(build(false, false, 1, false, 0), LeafAbi::Memory, "framed");
    assert_eq!(build(false, false, 1, true, 1), LeafAbi::Memory, "patched");
    {
        let _optional = LambdaListScope::enter(LambdaList::Optional);
        assert_eq!(
            build(false, false, 1, true, 0),
            LeafAbi::Memory,
            "&optional"
        );
    }
    assert_eq!(
        build(false, false, 1, true, 0),
        LeafAbi::Register { arity: 1 },
        "the scope restores"
    );
    force_register_abi_for_test(Some(false));
    assert_eq!(build(false, false, 1, true, 0), LeafAbi::Memory);
    force_direct_call_for_test(Some(true));
    assert_eq!(
        build(false, false, 1, true, 0),
        LeafAbi::Register { arity: 1 },
        "direct calls imply the register ABI"
    );
    force_register_abi_for_test(None);
    force_direct_call_for_test(None);
    force_direct_sites_for_test(None);
    force_direct_memory_for_test(None);
}
