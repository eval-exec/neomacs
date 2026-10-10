//! Worker-only batching: executable results, failure atomicity and lifetime.

use super::*;
use crate::emacs_core::jit::compile::lowering::jit_isa_for;
use crate::emacs_core::jit::compile::shim_refs::Shim;
use cranelift_codegen::ir::{
    AbiParam, Function, InstBuilder, Signature, UserExternalName, UserFuncName, types,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::Linkage;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// No Lisp state, globals, runtime shims or process spawning: a tiny CLIF
/// body simply returns its private integer under the host's C convention.
fn constant(value: i64, allocator: RegallocChoice) -> JobPayload {
    let config = jit_isa_for(allocator).expect("host ISA").frontend_config();
    let mut signature = Signature::new(config.default_call_conv);
    signature.returns.push(AbiParam::new(types::I64));
    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), signature);
    let mut context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut func, &mut context);
        let entry = builder.create_block();
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let value = builder.ins().iconst(types::I64, value);
        builder.ins().return_(&[value]);
        builder.finalize(config);
    }
    JobPayload {
        func,
        name: "worker-batch-test".into(),
        linkage: Linkage::Local,
        named: true,
        imports: Box::default(),
        portable: true,
        regalloc: allocator,
        disasm: false,
    }
}

fn selected_constant(value: i64, allocator: RegallocChoice) -> JobPayload {
    let mut payload = constant(value, allocator);
    // Different frontend declaration IDs must be remapped; unused imports
    // still require the corresponding backend groups. We never execute
    // runtime shims or manufacture a Context pointer in this fixture.
    let array = payload
        .func
        .declare_imported_user_function(UserExternalName::new(0, 1000));
    let sink = payload
        .func
        .declare_imported_user_function(UserExternalName::new(0, 1001));
    payload.imports = vec![
        (array, Shim::T2RecordArrayUse),
        (sink, Shim::SqrtBindingValid),
    ]
    .into_boxed_slice();
    payload
}

fn invoke(entry: usize) -> i64 {
    assert_ne!(entry, 0);
    // SAFETY: callers only supply a completed successful SealedBatch (or
    // immediate finalized backend result), and constant's ABI is ()->i64
    // using the host's default C convention. Code mappings live forever.
    unsafe {
        let function: unsafe extern "C" fn() -> i64 = std::mem::transmute(entry);
        function()
    }
}

fn seal_one(backend: &mut WorkerBackend, value: i64) -> usize {
    let mut batch = backend.batch();
    batch
        .prepare(constant(value, RegallocChoice::Full))
        .expect("prepare");
    let sealed = batch.finish().expect("seal");
    assert_eq!(sealed.codes.len(), 1);
    let entry = sealed.codes[0].entry;
    assert_eq!(invoke(entry), value);
    entry
}

fn reset_and_reuse(backend: &mut WorkerBackend, earlier: usize, earlier_value: i64) {
    assert!(
        backend.0.modules.iter().all(Option::is_none),
        "aborted bookkeeping is discarded"
    );
    assert_eq!(
        invoke(earlier),
        earlier_value,
        "old published code remains mapped"
    );
    let next = seal_one(backend, 71);
    assert_eq!(invoke(next), 71);
    assert_eq!(
        invoke(earlier),
        earlier_value,
        "reuse cannot overwrite published code"
    );
}

fn permissions(entry: usize) -> String {
    let maps = std::fs::read_to_string("/proc/self/maps").expect("own mappings");
    maps.lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let (start, end) = fields.next()?.split_once('-')?;
            let start = usize::from_str_radix(start, 16).ok()?;
            let end = usize::from_str_radix(end, 16).ok()?;
            (start..end)
                .contains(&entry)
                .then(|| fields.next().map(str::to_owned))
                .flatten()
        })
        .expect("entry has a live mapping")
}

#[test]
fn jit_worker_batch_eight_leaves_pack_and_seal_before_execution() {
    let mut immediate = WorkerBackend::new();
    for value in 0..8 {
        let code = immediate
            .define(constant(value, RegallocChoice::Full))
            .expect("immediate compile");
        assert_eq!(invoke(code.entry), value);
    }
    let immediate_memory = immediate.0.arena.stats();
    assert_eq!(immediate_memory.seals, 8);

    let mut backend = WorkerBackend::new();
    let mut batch = backend.batch();
    for value in 0..8 {
        assert_eq!(
            batch.capacity(RegallocChoice::Full),
            BatchCapacity::Available
        );
        batch
            .prepare(constant(value, RegallocChoice::Full))
            .expect("prepare");
        assert_eq!(batch.arena_seals(), 0, "prepare must not seal a member");
    }
    assert_eq!(
        batch.capacity(RegallocChoice::Full),
        BatchCapacity::SealFirst
    );
    let sealed = batch.finish().expect("all eight seal together");
    assert_eq!(sealed.modules_finalized, 1);
    assert_eq!(sealed.codes.len(), 8);
    assert_eq!(sealed.arena_seals, 1);
    let packed_memory = backend.0.arena.stats();
    assert!(packed_memory.seals < immediate_memory.seals);
    assert!(
        packed_memory.page_bytes < immediate_memory.page_bytes,
        "tiny leaves share pages"
    );
    let entries: std::collections::HashSet<_> =
        sealed.codes.iter().map(|code| code.entry).collect();
    assert_eq!(
        entries.len(),
        8,
        "same labels must define distinct functions"
    );
    for (value, code) in sealed.codes.iter().enumerate() {
        assert!(
            permissions(code.entry).starts_with("r-x"),
            "published entry must be RX"
        );
        assert!(code.code_bytes > 0);
        assert_eq!(invoke(code.entry), value as i64);
    }
}

#[test]
fn jit_worker_batch_prepared_function_has_no_finalized_entry() {
    let mut backend = WorkerBackend::new();
    let mut batch = backend.batch();
    batch
        .prepare(constant(31, RegallocChoice::Full))
        .expect("prepare");
    let code = &batch.definitions[0];
    let module = &batch.backend.0.modules[code.allocator.index()]
        .as_ref()
        .unwrap()
        .module;
    // Do not execute unsealed code. Cranelift itself rejects obtaining a
    // finalized address until its outstanding definitions are finalized.
    assert!(
        catch_unwind(AssertUnwindSafe(
            || module.get_finalized_function(code.function)
        ))
        .is_err()
    );
    assert_eq!(batch.arena_seals(), 0);
    let sealed = batch.finish().expect("finish");
    assert_eq!(invoke(sealed.codes[0].entry), 31);
}

#[test]
fn jit_worker_batch_abandonment_retains_prior_code_and_reuses_backend() {
    let mut backend = WorkerBackend::new();
    let earlier = seal_one(&mut backend, 19);
    {
        let mut batch = backend.batch();
        batch
            .prepare(constant(23, RegallocChoice::Full))
            .expect("unsealed new member");
        // Drop without publication or finalization.
    }
    reset_and_reuse(&mut backend, earlier, 19);
}

#[test]
fn jit_worker_batch_prepare_error_permanently_aborts_every_member() {
    let mut backend = WorkerBackend::new();
    let earlier = seal_one(&mut backend, 19);
    let mut batch = backend.batch();
    batch.prepare(constant(23, RegallocChoice::Full)).unwrap();
    batch.test_fault = Some(TestFault::PrepareError);
    assert!(batch.prepare(constant(29, RegallocChoice::Full)).is_err());
    assert_eq!(batch.capacity(RegallocChoice::Full), BatchCapacity::Aborted);
    assert!(batch.prepare(constant(37, RegallocChoice::Full)).is_err());
    assert!(
        batch.finish().is_err(),
        "no earlier successful member may escape"
    );
    reset_and_reuse(&mut backend, earlier, 19);
}

#[test]
fn jit_worker_batch_real_prepare_rejection_cannot_finish_earlier_members() {
    let mut backend = WorkerBackend::new();
    let earlier = seal_one(&mut backend, 19);
    let mut batch = backend.batch();
    batch.prepare(constant(23, RegallocChoice::Full)).unwrap();
    let mut invalid = constant(29, RegallocChoice::Full);
    invalid.portable = false;
    assert!(batch.prepare(invalid).is_err());
    assert_eq!(batch.capacity(RegallocChoice::Full), BatchCapacity::Aborted);
    assert!(batch.finish().is_err());
    reset_and_reuse(&mut backend, earlier, 19);
}

#[test]
fn jit_worker_batch_prepare_panic_permanently_aborts_every_member() {
    let mut backend = WorkerBackend::new();
    let earlier = seal_one(&mut backend, 19);
    let mut batch = backend.batch();
    batch.prepare(constant(23, RegallocChoice::Full)).unwrap();
    batch.test_fault = Some(TestFault::PreparePanic);
    assert!(
        catch_unwind(AssertUnwindSafe(
            || batch.prepare(constant(29, RegallocChoice::Full))
        ))
        .is_err()
    );
    assert_eq!(batch.capacity(RegallocChoice::Full), BatchCapacity::Aborted);
    assert!(batch.finish().is_err());
    reset_and_reuse(&mut backend, earlier, 19);
}

fn mixed_batch(backend: &mut WorkerBackend) -> WorkerBatch<'_> {
    let mut batch = backend.batch();
    batch.prepare(constant(23, RegallocChoice::Fast)).unwrap();
    batch.prepare(constant(29, RegallocChoice::Full)).unwrap();
    batch
}

#[test]
fn jit_worker_batch_partial_finalization_error_publishes_no_member() {
    let mut backend = WorkerBackend::new();
    let earlier = seal_one(&mut backend, 19);
    let mut batch = mixed_batch(&mut backend);
    batch.test_fault = Some(TestFault::FinalizeErrorAfter(1));
    assert!(
        batch.finish().is_err(),
        "even an already sealed first module must not escape"
    );
    reset_and_reuse(&mut backend, earlier, 19);
}

#[test]
fn jit_worker_batch_partial_finalization_panic_publishes_no_member() {
    let mut backend = WorkerBackend::new();
    let earlier = seal_one(&mut backend, 19);
    let mut batch = mixed_batch(&mut backend);
    batch.test_fault = Some(TestFault::FinalizePanicAfter(1));
    assert!(catch_unwind(AssertUnwindSafe(|| batch.finish())).is_err());
    reset_and_reuse(&mut backend, earlier, 19);
}

#[test]
fn jit_worker_batch_module_rollover_waits_for_opaque_ids_to_seal() {
    let mut backend = WorkerBackend::new();
    let earlier = seal_one(&mut backend, 19);
    // Simulate a generation near its real limit without compiling thousands
    // of filler bodies or changing a thread/process-global test knob.
    backend.0.modules[RegallocChoice::Full.index()]
        .as_mut()
        .unwrap()
        .leaves = module_leaf_limit() - 1;
    let mut batch = backend.batch();
    batch.prepare(constant(23, RegallocChoice::Full)).unwrap();
    assert_eq!(
        batch.capacity(RegallocChoice::Full),
        BatchCapacity::SealFirst
    );
    let last = batch
        .finish()
        .expect("last ID in old generation seals")
        .codes[0]
        .entry;
    let next = seal_one(&mut backend, 29);
    assert_eq!(
        backend.0.modules[RegallocChoice::Full.index()]
            .as_ref()
            .unwrap()
            .leaves,
        1
    );
    assert_eq!(invoke(earlier), 19);
    assert_eq!(invoke(last), 23);
    assert_eq!(invoke(next), 29);
}

#[test]
fn jit_worker_batch_mixed_allocators_and_late_selected_imports_link() {
    let mut backend = WorkerBackend::new();
    let mut batch = backend.batch();
    batch.prepare(constant(11, RegallocChoice::Fast)).unwrap();
    batch.prepare(constant(22, RegallocChoice::Full)).unwrap();
    batch
        .prepare(selected_constant(33, RegallocChoice::Fast))
        .unwrap();
    batch
        .prepare(selected_constant(44, RegallocChoice::Full))
        .unwrap();
    batch.prepare(constant(55, RegallocChoice::Fast)).unwrap();
    let sealed = batch
        .finish()
        .expect("owned selected imports link in both allocators");
    assert_eq!(sealed.modules_finalized, 2);
    for (code, expected) in sealed.codes.iter().zip([11, 22, 33, 44, 55]) {
        assert_eq!(invoke(code.entry), expected);
    }
}

#[test]
fn jit_worker_batch_capacity_misuse_poisoning_is_not_recoverable() {
    let mut backend = WorkerBackend::new();
    let earlier = seal_one(&mut backend, 19);
    let mut batch = backend.batch();
    for value in 0..WORKER_BATCH_LIMIT {
        batch
            .prepare(constant(value as i64, RegallocChoice::Full))
            .unwrap();
    }
    assert_eq!(
        batch.capacity(RegallocChoice::Full),
        BatchCapacity::SealFirst
    );
    assert!(batch.prepare(constant(99, RegallocChoice::Full)).is_err());
    assert_eq!(batch.capacity(RegallocChoice::Full), BatchCapacity::Aborted);
    assert!(
        batch.finish().is_err(),
        "capacity error must not enable partial publication"
    );
    reset_and_reuse(&mut backend, earlier, 19);
}
