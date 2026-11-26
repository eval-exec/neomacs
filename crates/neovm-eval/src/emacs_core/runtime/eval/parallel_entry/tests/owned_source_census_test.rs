use super::super::source_entry::install_inactive_source_module_probe_for_test;
use super::{
    Effect, HasKeeper, Keeper, OwnedSourceHeapDrained, OwnedSourceRegistries, SourceEntryRefusal,
    SourceFrame, SourceWithdrawalFailure, SourceWithdrawalStep,
};
use neovm_value::value::Value;
use std::mem::ManuallyDrop;

fn ready() -> Box<super::super::super::Context> {
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

#[test]
fn pentry_e2b_source_census_actual_keeper_and_original_box_stay_local() {
    let source = ready();
    let address = std::ptr::from_ref(&*source);
    let heap = source.tagged_heap.heap_identity();
    let alive = source.world_lifetime_probe_for_test();
    let drained = capture(source).unwrap().prepare_census().unwrap();
    assert!(alive());
    assert_eq!(std::ptr::from_ref(&**drained._frame.source), address);
    assert_eq!(drained._frame.effects._receipt.heap_identity(), heap);
    assert!(drained._frame.effects.keeper().held_here());
    assert_eq!(
        drained._frame.effects.keeper().parallel_heap_identity(),
        heap
    );
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    assert!(drained._frame.source.tagged_heap.facade_mark_is_excluded());
    finish_unexported_fixture(drained);
    assert!(
        !alive(),
        "the fixture joins its actual GC service after the retained keeper returns"
    );
}

#[test]
fn pentry_e2b_source_census_private_frame_and_detached_registry_roots_are_captured() {
    let mut source = ready();
    let private = Value::vector(vec![Value::fixnum(67)]);
    source.lexenv = private;
    let program = Value::vector(vec![Value::fixnum(0); 3]);
    neovm_text::emacs_core::ccl::builtin_register_ccl_program_impl(vec![
        Value::symbol("pentry-e2b-source-census-program"),
        program,
    ])
    .unwrap();
    let drained = capture(source).unwrap().prepare_census().unwrap();
    let roots = &drained._frame.effects._receipt;
    assert!(
        roots.contains_for_test(private),
        "original private shell must be rooted"
    );
    assert!(
        roots.contains_for_test(program),
        "withdrawn original registry must be rooted"
    );
}

#[test]
fn pentry_e2b_source_census_disposable_cache_word_is_retired_before_capture() {
    let source = ready();
    let ephemeral = neovm_value::chartab::make_char_table_value(Value::NIL, Value::NIL);
    neovm_text::emacs_core::casetab::activate_casetab_thread_locals(Some(ephemeral));
    let mut roots = Vec::new();
    neovm_text::emacs_core::casetab::collect_casetab_gc_roots(
        &mut roots,
        neovm_value::heap_registry::text_retirement::heap_identity_word(
            source.tagged_heap.heap_identity(),
        ),
    );
    assert!(
        roots.contains(&ephemeral),
        "actual disposable root exists first"
    );
    let drained = capture(source).unwrap().prepare_census().unwrap();
    assert!(
        !drained._frame.effects._receipt.contains_for_test(ephemeral),
        "no stale cache-only word may enter the final census"
    );
}

#[test]
fn pentry_e2b_source_census_drop_and_forget_retain_backing_without_tls_recovery() {
    let source = ready();
    let alive = source.world_lifetime_probe_for_test();
    drop(capture(source).unwrap().prepare_census().unwrap());
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    let source = ready();
    let second = source.world_lifetime_probe_for_test();
    std::mem::forget(capture(source).unwrap().prepare_census().unwrap());
    assert!(alive() && second());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_source_census_catch_unwind_never_recovers_source_activation() {
    let source = ready();
    let alive = source.world_lifetime_probe_for_test();
    let drained = capture(source).unwrap().prepare_census().unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _retained = drained;
        panic!("OWNED_SOURCE_CENSUS_UNWIND");
    }));
    assert!(result.is_err());
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_source_census_rechecks_entry_before_the_actual_drain() {
    let source = ready();
    let alive = source.world_lifetime_probe_for_test();
    let retired = capture(source).unwrap().retire_text().unwrap();
    // Clear the retired source's heap slot so the foreign Context can install
    // cleanly; the retired source is not entered again, only refused on TLS.
    // SAFETY: `retired._frame.source` is retired and never evaluated again.
    unsafe {
        neovm_value::tagged::gc::clear_tagged_heap_if_installed(&retired._frame.source.tagged_heap);
    }
    let foreign = ready();
    // Production prepare_census has no gap between retirement and the drain.
    // This reaches the private recheck with another heap already current.
    let error = retired
        .close_heap_tls()
        .err()
        .expect("new heap owner refuses");
    assert!(format!("{error:?}").contains("HeapTlsNotCurrent"));
    match &error {
        SourceWithdrawalFailure::Retained(failure) => {
            assert_eq!(failure.step, SourceWithdrawalStep::HeapPreparation);
        }
        other => panic!("closed source cannot return an ordinary Context: {other:?}"),
    }
    drop(error);
    assert!(alive());
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &foreign.tagged_heap
    ));
}

#[test]
fn pentry_e2b_source_census_existing_keeper_from_another_world_refuses() {
    let source = ready();
    let mut first = capture(source).unwrap();
    // SAFETY: `first._frame.source` is captured/suspended and never evaluated again.
    unsafe {
        neovm_value::tagged::gc::clear_tagged_heap_if_installed(&first._frame.source.tagged_heap);
    }
    let foreign_source = ready();
    let foreign = capture(foreign_source).unwrap();
    // The shared preparation check rejects before it clears the installed heap.
    // SAFETY: `first._frame.source` is captured/suspended and never evaluated again.
    let error = unsafe {
        first
            ._frame
            .source
            .close_owned_source_heap_tls(foreign._frame.effects.keeper())
    }
    .err()
    .expect("another World's retained keeper is not admission");
    assert_eq!(
        error,
        super::super::super::world::OwnedParallelError::Identity
    );
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &foreign._frame.source.tagged_heap
    ));
    assert!(first._frame.effects.keeper().held_here());
}

#[test]
fn pentry_e2b_source_census_preserves_unsupported_weak_table_without_preclean() {
    use neovm_value::value::{HashTableTest, HashTableWeakness, LispHashTable};
    let mut source = ready();
    let table = source
        .tagged_heap
        .alloc_hash_table(LispHashTable::new_with_options(
            HashTableTest::Eql,
            0,
            Some(HashTableWeakness::Value),
            1.5,
            0.8125,
        ));
    source.set_variable("window-dead-windows-table", table);
    let before = source.tagged_heap.gc_collections();
    let drained = capture(source).unwrap().prepare_census().unwrap();
    assert_eq!(
        drained
            ._frame
            .source
            .builtin_var_value_by_name("window-dead-windows-table"),
        Some(table)
    );
    assert_eq!(
        drained._frame.source.tagged_heap.gc_collections(),
        before,
        "closed preparation cannot run serial weak-table preclean"
    );
}

/// Teardown for the one positive fixture. It has not exported a transport,
/// created a participant, or admitted an era. Receipts are released here so
/// the Context can join its GC service. This is not a product resume path.
fn finish_unexported_fixture(drained: OwnedSourceHeapDrained) {
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    assert!(drained._frame.effects.keeper().held_here());
    let SourceFrame {
        mut source,
        effects,
    } = drained._frame;
    source.tagged_heap.install_tagged_heap();
    effects.release(&mut source);
    drop(ManuallyDrop::into_inner(source));
}

/// Same teardown after a matched return. Publication retires only once the
/// source heap is installed again. This is still not a product resume path.
pub(super) fn finish_returned_fixture(
    drained: OwnedSourceHeapDrained,
    publication: neovm_value::tagged::transport::RootBatchLease,
) {
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    assert!(drained._frame.effects.keeper().held_here());
    let SourceFrame {
        mut source,
        effects,
    } = drained._frame;
    source.tagged_heap.install_tagged_heap();
    effects.release(&mut source);
    publication
        .try_finish()
        .expect("returned source is active before publication retirement");
    assert!(
        !source.tagged_heap.facade_mark_is_excluded(),
        "F4: every RootBatchLease and FacadeEpochRetention must retire before World teardown so TaggedHeap::drop reclaims backing"
    );
    drop(ManuallyDrop::into_inner(source));
}

trait Release {
    fn release(self, source: &mut super::super::super::Context);
}

impl Release for Keeper {
    fn release(self, _source: &mut super::super::super::Context) {
        drop(ManuallyDrop::into_inner(self.0));
    }
}

impl<R: Release, T: ReleaseReceipt> Release for Effect<R, T> {
    fn release(self, source: &mut super::super::super::Context) {
        ManuallyDrop::into_inner(self._receipt).release_receipt(source);
        self._previous.release(source);
    }
}

trait ReleaseReceipt {
    fn release_receipt(self, source: &mut super::super::super::Context);
}

impl ReleaseReceipt for super::super::super::owner_mirrors::ParkedOwner {
    fn release_receipt(self, source: &mut super::super::super::Context) {
        source.resume_owner(self);
    }
}

macro_rules! drop_receipt {
    ($($ty:ty),+ $(,)?) => {$(
        impl ReleaseReceipt for $ty {
            fn release_receipt(self, _source: &mut super::super::super::Context) {
                drop(self);
            }
        }
    )+};
}

drop_receipt!(
    neovm_value::tagged::gc::FrozenLegacyRoots,
    super::TextEntryChecked,
    super::CaseRetired,
    super::SyntaxRetired,
    super::CategoryRetired,
    super::RegexRetired,
    super::StringPositionRetired,
    neovm_value::quit::quit_detach::DetachedQuitActivation,
    crate::emacs_core::builtins::arithmetic::DetachedIntegerWidthActivation,
    neovm_value::error::SourceShellInFlightReceipt,
    neovm_value::heap_registry::detachment::DetachedRegistry<
        Vec<Option<crate::emacs_core::eval::data::dynamic_module_registry::GlobalRefEntry>>,
    >,
    neovm_value::heap_registry::detachment::DetachedRegistry<
        std::collections::HashMap<
            i64,
            crate::emacs_core::builtins::hooks::WindowConfigurationSnapshot,
        >,
    >,
    neovm_value::heap_registry::detachment::DetachedRegistry<
        crate::emacs_core::builtins::file_notify::FileNotifyState,
    >,
    neovm_value::heap_registry::detachment::DetachedRegistry<
        crate::emacs_core::builtins::collections::HashTableTestRegistry,
    >,
    neovm_value::heap_registry::detachment::DetachedRegistry<
        crate::emacs_core::eval::data::terminal_state::TerminalLispRegistry,
    >,
    neovm_value::heap_registry::detachment::DetachedRegistry<
        crate::emacs_core::eval::data::face_registry::FaceAttrState,
    >,
    neovm_value::heap_registry::detachment::DetachedRegistry<
        neovm_text::emacs_core::charset::CharsetRegistry,
    >,
    neovm_value::heap_registry::detachment::DetachedRegistry<
        neovm_text::emacs_core::ccl::CclRegistry,
    >,
);
