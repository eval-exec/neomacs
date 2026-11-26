use super::super::source_entry::install_inactive_source_module_probe_for_test;
use super::{
    HasKeeper, OwnedSourceCacheClosed, OwnedSourceCarriersClosed, OwnedSourceRegistries,
    SourceEntryRefusal, SourceWithdrawalFailure, SourceWithdrawalStep,
};

fn ready() -> Box<super::super::super::Context> {
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::cache::reset_carrier_tls();
    install_inactive_source_module_probe_for_test();
    let mut source = Box::new(super::super::super::Context::new_minimal_vm_harness(
        crate::test_utils::unit_harness(),
    ));
    source.setup_thread_locals();
    source.gc_collect_exact();
    if source.tagged_heap.tagged_heap_generational_enabled() {
        return source;
    }
    match capture(source) {
        Err(SourceWithdrawalFailure::BeforeEffects(failure)) => {
            assert_eq!(failure.reason, SourceEntryRefusal::GenerationalRequired);
            panic!("these rows need a generational heap; NEOVM_GC_GENERATIONAL=0 disables it")
        }
        other => panic!("GEN0 must refuse before any effect: {:?}", other.err()),
    }
}

fn capture(
    source: Box<super::super::super::Context>,
) -> Result<OwnedSourceRegistries, SourceWithdrawalFailure> {
    // SAFETY: fixtures own fresh serial Contexts with no collector, running
    // mutator, store callback, or borrowed heap view.
    unsafe { OwnedSourceRegistries::capture(source) }
}

fn carriers(
    source: Box<super::super::super::Context>,
) -> (impl Fn() -> bool, OwnedSourceCarriersClosed) {
    let alive = source.world_lifetime_probe_for_test();
    let closed = capture(source)
        .unwrap()
        .withdraw_observations()
        .unwrap()
        .withdraw_carriers()
        .unwrap();
    (alive, closed)
}

#[test]
fn pentry_e2b_source_jit_cache_actual_source_accepts_feature_static_empty_cache() {
    let source = ready();
    let (alive, closed) = carriers(source);
    let closed: OwnedSourceCacheClosed = closed.withdraw_jit_cache().unwrap();
    assert!(closed._frame.effects.keeper().held_here());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    assert!(alive());
}

#[test]
fn pentry_e2b_source_jit_cache_actual_source_drop_forget_and_unwind_retain_backing() {
    let source = ready();
    let (first, closed) = carriers(source);
    drop(closed.withdraw_jit_cache().unwrap());
    assert!(first());
    let source = ready();
    let (second, closed) = carriers(source);
    std::mem::forget(closed.withdraw_jit_cache().unwrap());
    assert!(second());
    let source = ready();
    let (third, closed) = carriers(source);
    let closed = closed.withdraw_jit_cache().unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _closed = closed;
        panic!("OWNED_SOURCE_JIT_CACHE_UNWIND");
    }));
    assert!(result.is_err());
    assert!(first() && second() && third());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[cfg(feature = "jit")]
#[test]
fn pentry_e2b_source_jit_cache_actual_source_preserves_new_heap_on_refusal() {
    let source = ready();
    let (alive, closed) = carriers(source);
    let foreign = ready();
    let error = closed
        .withdraw_jit_cache()
        .err()
        .expect("a newer heap is installed");
    assert!(format!("{error:?}").contains("HeapInstalled"));
    match &error {
        SourceWithdrawalFailure::Retained(failure) => {
            assert_eq!(failure.step, SourceWithdrawalStep::JitCache);
        }
        SourceWithdrawalFailure::BeforeEffects(_) => panic!("source already closed"),
    }
    drop(error);
    assert!(alive());
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &foreign.tagged_heap
    ));
}
