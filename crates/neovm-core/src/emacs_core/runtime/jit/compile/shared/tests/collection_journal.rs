//! Compiler-owned import ordering across scalar policy overrides. These tests
//! execute no Lisp or native leaf and retain no mutator state in the backend.

use super::*;
use crate::emacs_core::jit::compile::shim_refs::{
    RtRefs, SelectedShimGroups, force_lazy_shims_for_test,
};
use crate::tagged::collection_reads::{CompiledJournalMode, force_compiled_journal_for_test};
use cranelift_codegen::ir::{InstBuilder, Signature, UserFuncName, types};
use cranelift_frontend::FunctionBuilder;

struct Settings;

impl Settings {
    fn off() -> Self {
        force_compiled_journal_for_test(Some(CompiledJournalMode::Off));
        Self
    }
}

impl Drop for Settings {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
        force_lazy_shims_for_test(true);
    }
}

fn groups(collection_journal: bool, tier2_profile: bool) -> ShimGroups {
    ShimGroups {
        subr_spec: true,
        cbsym_spec: true,
        tier2_profile,
        direct_shapes: true,
        call_census: true,
        direct_framed: true,
        hof: true,
        collection_journal,
        collection_observation_gate: collection_journal,
    }
}

fn selected(groups: ShimGroups) -> SelectedShimGroups {
    SelectedShimGroups {
        main: groups,
        array_profile: true,
        sink_versions: true,
    }
}

fn reference_ids(choice: RegallocChoice) -> ShimIds {
    let builder = JITBuilder::with_isa(jit_isa_for(choice).unwrap(), default_libcall_names());
    let mut module = JITModule::new(builder);
    let config = module.target_config();
    ShimIds::declare_selected(
        &mut module,
        config.default_call_conv,
        config.pointer_type(),
        selected(groups(false, false)),
    )
    .unwrap()
}

#[test]
fn shared_journal_off_preserves_selected_ids_and_on_appends_without_reordering() {
    let _settings = Settings::off();
    let choice = RegallocChoice::Full;
    let expected = reference_ids(choice);
    let mut backend = SharedJit::fresh();
    backend
        .ensure_module_selected(choice, false, true, true)
        .unwrap();
    let shared = backend.modules[choice.index()].as_ref().unwrap();
    for shim in [
        Shim::Cons,
        Shim::Aset,
        Shim::Setcar,
        Shim::Setcdr,
        Shim::Varset,
        Shim::DirectFramed,
        Shim::HofStore,
        Shim::T2RecordArrayUse,
        Shim::SqrtBindingValid,
    ] {
        assert_eq!(
            shared.shims.get(shim),
            expected.get(shim),
            "collection OFF preserves main and selected {shim:?} IDs"
        );
    }
    assert!(shared.shims.get(Shim::StringCollectionWrite).is_none());
    assert!(shared.shims.get(Shim::UnobservedCollectionOwner).is_none());
    let original = shared.shims;

    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    backend.ensure_module(choice, false).unwrap();
    let journal = backend.modules[choice.index()]
        .as_ref()
        .unwrap()
        .shims
        .get(Shim::StringCollectionWrite)
        .expect("an ON test override appends the missing journal import");
    assert!(journal.as_u32() > original.get(Shim::SqrtBindingValid).unwrap().as_u32());
    let gate = backend.modules[choice.index()]
        .as_ref()
        .unwrap()
        .shims
        .get(Shim::UnobservedCollectionOwner)
        .expect("an Observed override appends the missing gate import");
    assert!(gate.as_u32() > journal.as_u32());

    // A main-only profiling redeclaration can hide optional IDs in its returned
    // table. The selected backend must recover both IDs by their existing names.
    backend.ensure_module(choice, true).unwrap();
    backend
        .ensure_module_selected(choice, true, true, true)
        .unwrap();
    let after_profile = backend.modules[choice.index()].as_ref().unwrap();
    assert!(after_profile.shims.get(Shim::TierRequest).is_some());
    assert_eq!(
        after_profile.shims.get(Shim::StringCollectionWrite),
        Some(journal)
    );
    assert_eq!(
        after_profile.shims.get(Shim::UnobservedCollectionOwner),
        Some(gate)
    );
    for shim in [
        Shim::Cons,
        Shim::DirectFramed,
        Shim::HofStore,
        Shim::T2RecordArrayUse,
        Shim::SqrtBindingValid,
    ] {
        assert_eq!(
            after_profile.shims.get(shim),
            original.get(shim),
            "{shim:?}"
        );
    }

    force_compiled_journal_for_test(Some(CompiledJournalMode::Off));
    backend
        .ensure_module_selected(choice, true, true, true)
        .unwrap();
    let shared = backend.modules[choice.index()].as_ref().unwrap();
    let config = shared.module.target_config();
    for lazy in [true, false] {
        force_lazy_shims_for_test(lazy);
        let mut func = Function::with_name_signature(
            UserFuncName::user(0, 0),
            Signature::new(config.default_call_conv),
        );
        let refs = RtRefs::new_selected(
            shared.shims,
            selected(groups(false, true)),
            &mut func,
            config.default_call_conv,
            config.pointer_type(),
        );
        assert!(
            refs.try_get(&mut func, Shim::StringCollectionWrite)
                .is_none()
        );
        assert!(
            refs.try_get(&mut func, Shim::UnobservedCollectionOwner)
                .is_none()
        );
        assert!(refs.try_get(&mut func, Shim::T2RecordArrayUse).is_some());
        assert!(refs.try_get(&mut func, Shim::SqrtBindingValid).is_some());
    }

    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    backend
        .ensure_module_selected(choice, true, true, true)
        .unwrap();
    let shared = backend.modules[choice.index()].as_ref().unwrap();
    assert_eq!(shared.shims.get(Shim::StringCollectionWrite), Some(journal));
    assert_eq!(
        shared.shims.get(Shim::UnobservedCollectionOwner),
        Some(gate)
    );
    assert_eq!(
        shared.shims.get(Shim::T2RecordArrayUse),
        original.get(Shim::T2RecordArrayUse)
    );
    assert_eq!(
        shared.shims.get(Shim::SqrtBindingValid),
        original.get(Shim::SqrtBindingValid)
    );
}

/// An owned frontend payload declares a string journal or observation gate
/// while its backend's scalar policy is OFF. No generated function is called:
/// dummy bits verify declaration/remapping and linking without Lisp mutation.
fn journal_payload(array_profile: bool, observation_gate: bool) -> split::JobPayload {
    let choice = RegallocChoice::Full;
    let builder = JITBuilder::with_isa(jit_isa_for(choice).unwrap(), default_libcall_names());
    let mut module = JITModule::new(builder);
    let config = module.target_config();
    let mut main = groups(!observation_gate, false);
    main.collection_observation_gate = observation_gate;
    let selected = SelectedShimGroups {
        main,
        array_profile,
        sink_versions: false,
    };
    let ids = ShimIds::declare_selected(
        &mut module,
        config.default_call_conv,
        config.pointer_type(),
        selected,
    )
    .unwrap();
    let mut signature = Signature::new(config.default_call_conv);
    signature
        .returns
        .push(cranelift_codegen::ir::AbiParam::new(types::I64));
    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), signature);
    let refs = RtRefs::new_selected(
        ids,
        selected,
        &mut func,
        config.default_call_conv,
        config.pointer_type(),
    );
    let mut context = FunctionBuilderContext::new();
    {
        let mut fb = FunctionBuilder::new(&mut func, &mut context);
        let block = fb.create_block();
        fb.switch_to_block(block);
        fb.seal_block(block);
        let shim = if observation_gate {
            Shim::UnobservedCollectionOwner
        } else {
            Shim::StringCollectionWrite
        };
        let record = refs.try_get(fb.func, shim).unwrap();
        let dummy = fb.ins().iconst(types::I64, 0);
        fb.ins().call(record, &[dummy]);
        let result = fb.ins().iconst(types::I64, 1);
        fb.ins().return_(&[result]);
        fb.finalize(config);
    }
    let imports = func
        .params
        .user_named_funcs()
        .iter()
        .map(|(reference, name)| {
            let shim = [
                Shim::StringCollectionWrite,
                Shim::UnobservedCollectionOwner,
                Shim::T2RecordArrayUse,
            ]
            .into_iter()
            .find(|&shim| ids.get(shim).is_some_and(|id| id.as_u32() == name.index))
            .expect("every payload import is owned and named");
            (reference, shim)
        })
        .collect::<Vec<_>>();
    split::JobPayload {
        func,
        name: "journal_worker_declaration_test".into(),
        linkage: Linkage::Local,
        named: false,
        imports: imports.into_boxed_slice(),
        portable: true,
        regalloc: choice,
        disasm: false,
    }
}

#[test]
fn shared_journal_worker_declares_owned_payload_imports_under_off_policy() {
    let _settings = Settings::off();
    force_lazy_shims_for_test(true);
    for (array_profile, observation_gate) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let mut backend = WorkerBackend::new();
        let payload = journal_payload(array_profile, observation_gate);
        backend.define_with_groups(payload).unwrap();
        let shared = backend.0.modules[RegallocChoice::Full.index()]
            .as_ref()
            .unwrap();
        assert_eq!(
            shared.shims.get(Shim::StringCollectionWrite).is_some(),
            !observation_gate
        );
        assert_eq!(
            shared.shims.get(Shim::UnobservedCollectionOwner).is_some(),
            observation_gate
        );
        assert_eq!(
            shared.shims.get(Shim::T2RecordArrayUse).is_some(),
            array_profile
        );
        assert!(!super::super::jit_gen0_collection_journal_on());
    }
}
