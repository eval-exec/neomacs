//! T-S2: a payload built against one module's shim ids compiles in a module
//! that declared its shims under other ids.

use super::*;
use crate::emacs_core::jit::compile::lowering::{RegallocChoice, jit_isa_for};
use crate::emacs_core::jit::compile::shim_refs::{RtRefs, ShimGroups};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, UserFuncName, types};
use cranelift_frontend::FunctionBuilder;
use cranelift_jit::JITBuilder;
use cranelift_module::default_libcall_names;

const ALL_GROUPS: ShimGroups = ShimGroups {
    subr_spec: true,
    cbsym_spec: true,
    tier2_profile: true,
    direct_shapes: true,
    call_census: true,
};

/// A JIT module whose first `padding` declarations are unrelated functions,
/// so its shims land on different `FuncId`s than an unpadded module's.
fn module_with_padding(padding: usize) -> (JITModule, ShimIds) {
    let mut builder = JITBuilder::with_isa(
        jit_isa_for(RegallocChoice::Full).expect("host isa"),
        default_libcall_names(),
    );
    crate::emacs_core::jit::compile::register_shims(&mut builder);
    let mut module = JITModule::new(builder);
    let config = module.target_config();
    let mut sig = Signature::new(config.default_call_conv);
    sig.returns.push(AbiParam::new(types::I64));
    for i in 0..padding {
        module
            .declare_function(&format!("jit_bg_pad{i}"), Linkage::Local, &sig)
            .expect("declare padding");
    }
    let shims = ShimIds::declare(
        &mut module,
        config.default_call_conv,
        config.pointer_type(),
        ALL_GROUPS,
    )
    .expect("declare shims");
    (module, shims)
}

/// `() -> i64` calling each of `calls` once (zero arguments of its types).
fn function_calling(module: &JITModule, shims: ShimIds, calls: &[Shim]) -> Function {
    let config = module.target_config();
    let (call_conv, ptr_ty) = (config.default_call_conv, config.pointer_type());
    let mut sig = Signature::new(call_conv);
    sig.returns.push(AbiParam::new(types::I64));
    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig);
    let mut fbctx = FunctionBuilderContext::new();
    {
        let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);
        let refs = RtRefs::new(shims, ALL_GROUPS, fb.func, call_conv, ptr_ty);
        let block = fb.create_block();
        fb.switch_to_block(block);
        fb.seal_block(block);
        for &shim in calls {
            let callee = refs.try_get(fb.func, shim).expect("declared");
            let args: Vec<_> = shim
                .signature(call_conv, ptr_ty)
                .params
                .iter()
                .map(|p| {
                    if p.value_type == types::F64 {
                        fb.ins().f64const(0.0)
                    } else {
                        fb.ins().iconst(p.value_type, 0)
                    }
                })
                .collect();
            fb.ins().call(callee, &args);
        }
        let zero = fb.ins().iconst(types::I64, 0);
        fb.ins().return_(&[zero]);
        fb.finalize(config);
    }
    func
}

#[test]
fn jit_bg_payload_names_imports_by_shim_and_remaps_by_name() {
    let calls = [Shim::Varref, Shim::Cons, Shim::ArithSpec];
    let (front, front_ids) = module_with_padding(0);
    let func = function_calling(&front, front_ids, &calls);
    let captured = Captured {
        func,
        name: "__neovm_jit_leaf".into(),
        linkage: Linkage::Local,
        disasm: false,
    };
    let mut payload = JobPayload::package(captured, &front_ids, RegallocChoice::Full);
    assert!(payload.portable);
    let named: Vec<Shim> = payload.imports.iter().map(|&(_, shim)| shim).collect();
    assert_eq!(named, calls, "imports are named in first-use order");

    // A backend module that declared its shims after three other functions.
    let (mut backend, backend_ids) = module_with_padding(3);
    for &shim in &calls {
        assert_ne!(
            front_ids.get(shim),
            backend_ids.get(shim),
            "{shim:?}: the two modules must disagree for the remap to matter"
        );
    }
    remap_imports(&mut payload.func, &payload.imports, &backend_ids).expect("remap");
    let config = backend.target_config();
    for &(reference, shim) in payload.imports.iter() {
        let name = &payload.func.params.user_named_funcs()[reference];
        let id = FuncId::from_u32(name.index);
        assert_eq!(Some(id), backend_ids.get(shim));
        let decl = backend.declarations().get_function_decl(id);
        assert_eq!(decl.name.as_deref(), Some(shim.symbol()), "{shim:?}");
        assert_eq!(
            decl.signature,
            shim.signature(config.default_call_conv, config.pointer_type()),
            "{shim:?}"
        );
    }
    // And the remapped function defines and links in the backend module.
    let fid = backend
        .declare_anonymous_function(&payload.func.signature)
        .expect("declare");
    let mut ctx = backend.make_context();
    ctx.func = payload.func;
    backend.define_function(fid, &mut ctx).expect("define");
    backend.finalize_definitions().expect("finalize");
    assert!(!backend.get_finalized_function(fid).is_null());
}

/// An import that names no declared shim ties the payload to its front's
/// module: it is packaged without a remap table.
#[test]
fn jit_bg_payload_with_a_foreign_import_is_not_portable() {
    let (front, front_ids) = module_with_padding(0);
    let mut func = function_calling(&front, front_ids, &[Shim::Cons]);
    func.declare_imported_user_function(UserExternalName::new(0, 9_999));
    let captured = Captured {
        func,
        name: "__neovm_jit_leaf".into(),
        linkage: Linkage::Local,
        disasm: false,
    };
    let payload = JobPayload::package(captured, &front_ids, RegallocChoice::Full);
    assert!(!payload.portable);
    assert!(payload.imports.is_empty());
}
