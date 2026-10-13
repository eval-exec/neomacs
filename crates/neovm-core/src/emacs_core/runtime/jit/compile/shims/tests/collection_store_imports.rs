//! Import policy is owned frontend data; selecting a shim adds no runtime
//! dispatch and does not consult a backend worker's scalar policy overrides.

use super::*;
use crate::emacs_core::jit::compile::shim_refs::{
    RtRefs, Shim, ShimGroups, ShimIds, force_lazy_shims_for_test,
};
use cranelift_codegen::ir::{Function, Signature, UserFuncName};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Module, default_libcall_names};

struct Settings;

impl Drop for Settings {
    fn drop(&mut self) {
        force_lazy_shims_for_test(true);
        crate::tagged::collection_reads::force_compiled_journal_for_test(None);
    }
}

fn groups(journal: bool) -> ShimGroups {
    ShimGroups {
        subr_spec: false,
        cbsym_spec: false,
        tier2_profile: false,
        direct_shapes: false,
        call_census: false,
        direct_framed: false,
        hof: false,
        collection_journal: journal,
        collection_observation_gate: false,
        throw_flow: crate::emacs_core::jit::compile::shim_refs::ThrowShimSelection::Unavailable,
    }
}

fn selected_imports(journal: bool, lazy: bool) -> Vec<(String, Signature)> {
    force_lazy_shims_for_test(lazy);
    let builder = JITBuilder::new(default_libcall_names()).unwrap();
    let mut module = JITModule::new(builder);
    let config = module.target_config();
    let groups = groups(journal);
    let ids = ShimIds::declare(
        &mut module,
        config.default_call_conv,
        config.pointer_type(),
        groups,
    )
    .unwrap();
    let mut func = Function::with_name_signature(
        UserFuncName::user(0, 0),
        Signature::new(config.default_call_conv),
    );
    let refs = RtRefs::new(
        ids,
        groups,
        &mut func,
        config.default_call_conv,
        config.pointer_type(),
    );
    [
        Shim::Aset,
        Shim::Varset,
        Shim::Varbind,
        Shim::Unbind,
        Shim::Setcar,
        Shim::Setcdr,
    ]
    .into_iter()
    .map(|shim| {
        let reference = refs.get(&mut func, shim);
        let external = &func.dfg.ext_funcs[reference];
        let cranelift_codegen::ir::ExternalName::User(user) = external.name else {
            panic!("a runtime shim uses a module-owned name");
        };
        let name = &func.params.user_named_funcs()[user];
        let declaration = module
            .declarations()
            .get_function_decl(cranelift_module::FuncId::from_u32(name.index));
        (
            declaration.name.clone().expect("named shim"),
            func.dfg.signatures[external.signature].clone(),
        )
    })
    .collect()
}

#[test]
fn collection_store_on_selects_specialized_imports_with_legacy_signatures() {
    let _settings = Settings;
    for lazy in [true, false] {
        let legacy = selected_imports(false, lazy);
        let selected = selected_imports(true, lazy);
        for ((old_name, old_signature), (name, signature)) in legacy.into_iter().zip(selected) {
            assert_eq!(name, format!("{old_name}_collection"));
            assert_eq!(signature, old_signature, "{name} retains the caller ABI");
        }
    }
}

#[test]
fn collection_store_aot_names_are_exported_and_salted() {
    use crate::emacs_core::jit::aot::{JIT_SHIM_NAMES, MIR_SHIM_NAMES};

    for name in [
        "neovm_jit_aset_collection",
        "neovm_jit_varset_collection",
        "neovm_jit_varbind_collection",
        "neovm_jit_unbind_collection",
        "neovm_jit_setcar_collection",
        "neovm_jit_setcdr_collection",
        "neovm_jit_string_collection_write",
    ] {
        assert!(JIT_SHIM_NAMES.contains(&name), "host must export {name}");
        assert!(
            MIR_SHIM_NAMES.contains(&name),
            "AOT must permit and salt {name}"
        );
    }
    for name in [
        "neovm_jit_t2_record_array_use",
        "neovm_jit_sqrt_binding_valid",
        "neovm_jit_unobserved_collection_owner",
    ] {
        assert!(
            !MIR_SHIM_NAMES.contains(&name),
            "JIT-only {name} stays excluded"
        );
    }
}

#[test]
fn collection_store_aot_content_hash_distinguishes_journal_policies() {
    use crate::emacs_core::bytecode::Op;
    use crate::tagged::collection_reads::{CompiledJournalMode, force_compiled_journal_for_test};

    let _settings = Settings;
    let ops = [Op::Constant(0), Op::Return];
    let constants = [Value::make_int(19)];
    let hashes = [
        CompiledJournalMode::Off,
        CompiledJournalMode::Observed,
        CompiledJournalMode::Eager,
    ]
    .map(|mode| {
        force_compiled_journal_for_test(Some(mode));
        crate::emacs_core::jit::aot::leaf_content_hash(&ops, &constants, 0)
            .expect("fixed pointer-free content")
    });
    assert_ne!(hashes[0], hashes[1], "Off cannot load an Observed artifact");
    assert_ne!(
        hashes[1], hashes[2],
        "Observed cannot load an Eager artifact"
    );
    assert_ne!(hashes[0], hashes[2], "Off cannot load an Eager artifact");
}
