//! Actual original World/heap transport, without a Context or borrowed owner
//! crossing a channel. GEN0 exercises typed refusal and complete input return.
//!
//! The frame rule (R5) is the shape of `with_prepared`: the source callback
//! returns the reference-free right, and the destination body runs only after
//! that callback, including its protected era argument, has ended.

use super::heap_transfer_adapter;
use super::heap_transfer_adapter::PreparedHeapTransferProductionEra;
use crate::emacs_core::eval::Context;
use neovm_value::tagged::collection_reads::SourceObservationError;
use neovm_value::tagged::gc::heap_transfer::{
    HeapRootDesignator, PendingHeapReturn, PreparedHeapTransfer, RestoredHeapBacking,
    ScopedEraError, TransferError,
};
use neovm_value::tagged::gc::{
    AllStopped, EraWorldLoan, FacadeEpochRetention, FacadeMarkExclusionError, FrozenFunctionRoots,
    FrozenLegacyRoots, Gen1Admission, Gen1AdmissionError, HeapIdentity, PlainSymbolBacking,
    RegistrationError, RendezvousError, SymbolRootDesignators, WorldLoan,
};
use neovm_value::tagged::header::MarkerObj;
use neovm_value::tagged::value::TaggedValue;
use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::sync::Arc;

enum PreparationExpectation {
    Prepared,
    Refuse(SourceObservationError),
}

fn census_words(roots: &FrozenLegacyRoots) -> Vec<TaggedValue> {
    let mut words = Vec::new();
    roots.append_words(&mut words);
    words
}

fn with_prepared(body: impl FnOnce(PendingHeapReturn, PreparedHeapTransfer)) {
    with_prepared_case(PreparationExpectation::Prepared, |_| {}, body);
}

fn with_prepared_case(
    expected: PreparationExpectation,
    before: impl for<'heap> FnOnce(&mut Gen1Admission<'heap>),
    body: impl FnOnce(PendingHeapReturn, PreparedHeapTransfer),
) {
    let _harness = crate::test_utils::unit_harness();
    let census = RefCell::new(Vec::new());
    let prepared = Context::with_test_symbol_functions_init_era(
        PlainSymbolBacking::Unprojected,
        |obarray, ids| {
            let private = TaggedValue::string("retained bootstrap root");
            // The test's source frame captures before the collector exists;
            // the original World mirrors it until preparation publishes roots.
            obarray.seed_value_id(ids[0], private);
            census
                .borrow_mut()
                .extend([private, TaggedValue::fixnum(73), TaggedValue::NIL]);
        },
        |era, _| {
            let (collector, owner) = era.finish().expect("minimal source era finishes");
            let heap = collector.heap_identity();
            // SAFETY: the private test frame above was captured before the
            // collector loan, no Context or activation exists, and World
            // storage still retains every word through this closed fixture.
            let roots = unsafe {
                FrozenLegacyRoots::new(heap, census.take(), FrozenFunctionRoots::Quiescent)
            };
            let root = owner.parallel_heap_root_designator();
            heap_transfer_adapter::detach_source_heap(root.as_non_null());
            assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
            let mut admission = match Gen1Admission::check(collector) {
                Ok(admission) => admission,
                Err(failure) => {
                    let (reason, returned) = failure.into_parts();
                    assert_eq!(reason, Gen1AdmissionError::GenerationalRequired);
                    assert_eq!(returned.heap_identity(), heap);
                    assert_eq!(owner.parallel_heap_identity(), heap);
                    assert!(owner.held_here());
                    assert_eq!(census_words(&roots).len(), 3);
                    drop(returned);
                    drop(roots);
                    drop(owner);
                    eprintln!("PENTRY_E2B_GEN0_TYPED_REFUSAL_OWNERSHIP_RETURNED");
                    return None;
                }
            };
            let observations = admission
                .detach_source_observations()
                .expect("actual source observation detachment");
            // SAFETY: this closed fixture owns the only source frame. Heap TLS
            // was detached and the roots are retained. The reference-free
            // result is returned out of every source loan frame before any
            // destination body executes. Dropping the collector here does not
            // end this era argument's protector; the callback return does.
            // The hook can only change observation metadata for refusal cases.
            before(&mut admission);
            let outcome = unsafe {
                PreparedHeapTransfer::prepare_detached(
                    admission,
                    WorldLoan::from(owner),
                    roots,
                    observations,
                )
            };
            let (pending, transfer) = match (expected, outcome) {
                (PreparationExpectation::Prepared, Ok(prepared)) => prepared,
                (PreparationExpectation::Refuse(expected_error), Err(failure)) => {
                    let (reason, admission, loan, roots, receipt) = failure.into_parts();
                    assert_eq!(reason, TransferError::Observations(expected_error));
                    assert_eq!(admission.into_collector().heap_identity(), heap);
                    assert_eq!(loan.parallel_heap_identity(), heap);
                    assert_eq!(census_words(&roots).len(), 3);
                    drop(receipt);
                    drop(roots);
                    drop(loan);
                    return None;
                }
                (PreparationExpectation::Prepared, Err(failure)) => {
                    panic!("detached preparation refused: {failure:?}")
                }
                (PreparationExpectation::Refuse(_), Ok(_)) => {
                    panic!("invalid source observation state exported")
                }
            };
            assert_eq!(transfer.publication_root_count(), 1);
            Some((pending, transfer))
        },
    );
    // The enclosing source callback, including its protected era argument,
    // has ended. Only the transfer now retains the World.
    if let Some((pending, transfer)) = prepared {
        body(pending, transfer);
    }
}

fn finish(pending: PendingHeapReturn, transfer: PreparedHeapTransfer) {
    let returned = transfer.return_to_source().expect("reference-free return");
    let restored = pending
        .restore(returned)
        .unwrap_or_else(|failure| panic!("source restore: {:?}", failure.reason()));
    // SAFETY: original source thread. This fixture has no Context or
    // activation to reattach. No collector borrow remains, and the restored
    // words stay owned until publication retirement under serial access.
    let (owner, roots, publication) =
        unsafe { heap_transfer_adapter::recover_transferred_source(restored) }
            .unwrap_or_else(|failure| panic!("source materialization: {:?}", failure.reason()));
    assert!(owner.held_here());
    let words = census_words(&roots);
    assert_eq!(words.len(), 3);
    assert_eq!(
        words[0].as_str_owned().as_deref(),
        Some("retained bootstrap root")
    );
    assert_eq!(words[1], TaggedValue::fixnum(73));
    assert_eq!(words[2], TaggedValue::NIL);
    publication
        .try_finish()
        .expect("source roots restored with no readers");
    drop(roots);
    drop(owner);
}

#[test]
fn pentry_e2b_heap_transfer_zero_job_roundtrip() {
    with_prepared(finish);
}

#[test]
fn pentry_e2b_heap_transfer_os_thread_repeated_local_loans() {
    with_prepared(|pending, transfer| {
        let source = std::thread::current().id();
        let transfer = std::thread::spawn(move || {
            assert_ne!(source, std::thread::current().id());
            let (transfer, heap) = transfer
                .with_collector(|admission| {
                    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
                    let collector = admission.into_collector();
                    let heap = collector.heap_identity();
                    (collector, heap)
                })
                .expect("destination-local GEN1 loan");
            let (transfer, again) = transfer
                .with_collector(|admission| (admission.into_collector(), ()))
                .expect("first local reference ended before reconstruction");
            assert_eq!(again, ());
            assert_eq!(transfer.heap_identity(), heap);
            transfer
        })
        .join()
        .expect("service thread");
        assert!(pending.is_prepared());
        finish(pending, transfer);
    });
}

#[test]
fn pentry_e2b_heap_transfer_panic_poison_retains_canonical_debt() {
    with_prepared(|pending, transfer| {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = transfer.with_collector::<()>(|_admission| panic!("TRANSFER_CALLBACK_PANIC"));
        }));
        assert!(result.is_err());
        assert!(pending.is_poisoned());
        drop(pending);
    });
}

#[test]
fn pentry_e2b_heap_transfer_drop_poison_retains_backing() {
    with_prepared(|pending, transfer| {
        drop(transfer);
        assert!(pending.is_poisoned());
    });
}

#[test]
fn pentry_e2b_heap_transfer_forget_leaves_unresolved_reservation() {
    with_prepared(|pending, transfer| {
        std::mem::forget(transfer);
        assert!(pending.is_prepared());
        drop(pending);
    });
}

#[test]
fn pentry_e2b_heap_transfer_return_to_other_source_refuses_and_owns_both() {
    with_prepared(|first, first_transfer| {
        with_prepared(|second, second_transfer| {
            let returned = second_transfer.return_to_source().unwrap();
            let failed = match first.restore(returned) {
                Ok(_) => panic!("foreign source accepted"),
                Err(failure) => failure,
            };
            assert_eq!(failed.reason(), TransferError::WrongReturn);
            assert!(failed.pending_is_poisoned());
            assert!(second.is_poisoned());
            assert!(failed.returned_retains_backing());
            assert!(first_transfer.retains_backing());
        });
    });
}

#[test]
fn pentry_e2b_heap_transfer_installed_tls_refuses_without_constructing_local_loan() {
    with_prepared(|pending, transfer| {
        let unrelated =
            neovm_value::tagged::gc::AliasableBox::new(neovm_value::tagged::gc::TaggedHeap::new());
        neovm_value::tagged::gc::set_tagged_heap_root(unrelated.root());
        let failure = match transfer.with_collector::<()>(|_| panic!("must refuse before callback"))
        {
            Ok((_, ())) => panic!("installed TLS accepted"),
            Err(failure) => failure,
        };
        assert_eq!(failure.reason(), TransferError::InstalledTls);
        assert!(neovm_value::tagged::gc::tagged_heap_is_current(&unrelated));
        neovm_value::tagged::gc::clear_tagged_heap_if_installed(&unrelated);
        assert!(failure.retains_backing());
        assert!(pending.is_poisoned());
    });
}

#[test]
fn pentry_e2b_heap_transfer_panic_clears_original_tls_after_collector_drop() {
    with_prepared(|pending, transfer| {
        let root = transfer.root_ptr();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = transfer.with_collector::<()>(|admission| {
                // Address-only fault injection. Installing through
                // `set_tagged_heap_root` here would borrow the heap while
                // admission owns its collector.
                neovm_value::tagged::gc::heap_transfer::poke_installed_heap_address(root);
                drop(admission.into_collector());
                panic!("TRANSFER_INSTALLED_TLS_PANIC");
            });
        }));
        assert!(result.is_err());
        assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
        assert!(pending.is_poisoned());
    });
}

#[test]
fn pentry_e2b_heap_transfer_wrong_returned_collector_is_owned_refusal() {
    with_prepared(|pending, transfer| {
        let failure = match transfer.with_collector(|admission| {
            drop(admission.into_collector());
            let foreign = Box::leak(Box::new(neovm_value::tagged::gc::TaggedHeap::new()));
            // SAFETY: independent fresh leaked allocation, sole local owner,
            // no installed TLS, active collection, or external root.
            let collector =
                unsafe { foreign.drain_to_quiescent(|_| {}) }.expect("fresh foreign collector");
            (collector, ())
        }) {
            Ok(_) => panic!("foreign returned collector accepted"),
            Err(failure) => failure,
        };
        assert_eq!(failure.reason(), TransferError::WrongRoot);
        assert!(pending.is_poisoned());
        assert!(failure.retains_backing());
    });
}

#[test]
fn pentry_e2b_heap_transfer_source_abandon_during_local_scope_refuses_return() {
    with_prepared(|pending, transfer| {
        let failure = match transfer.with_collector(|admission| {
            drop(pending);
            (admission.into_collector(), ())
        }) {
            Ok(_) => panic!("abandoned source resumed"),
            Err(failure) => failure,
        };
        assert_eq!(failure.reason(), TransferError::Phase);
        assert!(failure.is_poisoned());
        assert!(failure.retains_backing());
    });
}

#[test]
fn pentry_e2b_heap_transfer_returned_collector_tls_restore_is_detached_before_refusal() {
    with_prepared(|pending, transfer| {
        let root = transfer.root_ptr();
        let failure = match transfer.with_collector(|admission| {
            // Address-only fault injection. Collector Drop restores the store
            // protocol before the owning local scope retires it.
            neovm_value::tagged::gc::heap_transfer::poke_installed_heap_address(root);
            (admission.into_collector(), ())
        }) {
            Ok(_) => panic!("installed callback TLS accepted"),
            Err(failure) => failure,
        };
        assert_eq!(failure.reason(), TransferError::InstalledTls);
        assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
        assert!(pending.is_poisoned());
        assert!(failure.retains_backing());
    });
}

fn finish_restored(restored: RestoredHeapBacking) {
    // SAFETY: original source thread. This fixture has no Context or
    // activation to reattach. No collector borrow remains, and the restored
    // words stay owned until publication retirement under serial access.
    let (owner, roots, publication) =
        unsafe { heap_transfer_adapter::recover_transferred_source(restored) }
            .unwrap_or_else(|failure| panic!("source materialization: {:?}", failure.reason()));
    assert!(owner.held_here());
    let words = census_words(&roots);
    assert_eq!(words.len(), 3);
    assert_eq!(
        words[0].as_str_owned().as_deref(),
        Some("retained bootstrap root")
    );
    assert_eq!(words[1], TaggedValue::fixnum(73));
    assert_eq!(words[2], TaggedValue::NIL);
    publication
        .try_finish()
        .expect("source roots restored with no readers");
    drop(roots);
    drop(owner);
}

#[test]
fn pentry_e2b_heap_transfer_export_refuses_stale_source_placeholder() {
    with_prepared_case(
        PreparationExpectation::Refuse(SourceObservationError::PlaceholderDisplaced),
        |admission| admission.install_canonical_source_observations(),
        |_, _| panic!("invalid source must never reach destination"),
    );
}

#[test]
fn pentry_e2b_heap_transfer_export_refuses_forgotten_original_rc() {
    with_prepared_case(
        PreparationExpectation::Refuse(SourceObservationError::EscapedOwner),
        |admission| admission.leak_canonical_source_observation_owner(),
        |_, _| panic!("escaped original owner must not be exported"),
    );
}

#[test]
fn pentry_e2b_heap_transfer_callback_observation_install_is_retired_before_reexport() {
    with_prepared(|pending, transfer| {
        // A destination thread cannot overwrite the source-local placeholder.
        // TLS retirement must finish before the transfer is sent back.
        let transfer = std::thread::spawn(move || {
            let (transfer, ()) = transfer
                .with_collector(|mut admission| {
                    admission.install_canonical_source_observations();
                    (admission.into_collector(), ())
                })
                .expect("canonical membership retired after callback frame");
            assert!(transfer.is_prepared());
            let (transfer, ()) = transfer
                .with_collector(|mut admission| {
                    assert_eq!(admission.validate_source_observation_export(), Ok(()));
                    (admission.into_collector(), ())
                })
                .unwrap();
            transfer
        })
        .join()
        .expect("destination worker");
        finish(pending, transfer);
    });
}

#[test]
fn pentry_e2b_heap_transfer_escaped_certificate_poison_prevents_reexport() {
    with_prepared(|pending, transfer| {
        let escaped = RefCell::new(None);
        let failure = match transfer.with_collector(|mut admission| {
            admission.install_canonical_source_observations();
            let (_, certificate) = neovm_value::tagged::collection_reads::capture(|| ());
            *escaped.borrow_mut() = Some(certificate.expect("real original certificate"));
            (admission.into_collector(), ())
        }) {
            Ok(_) => panic!("live original certificate reexported"),
            Err(failure) => failure,
        };
        assert_eq!(
            failure.reason(),
            TransferError::Observations(SourceObservationError::LiveCertificates)
        );
        assert!(pending.is_poisoned());
        assert!(pending.retains_observations());
        assert!(failure.retains_backing());
        drop(escaped.take());
    });
}

#[test]
fn pentry_e2b_heap_transfer_forgotten_certificate_poison_prevents_reexport() {
    with_prepared(|pending, transfer| {
        let failure = match transfer.with_collector(|mut admission| {
            admission.install_canonical_source_observations();
            let (_, certificate) = neovm_value::tagged::collection_reads::capture(|| ());
            std::mem::forget(certificate.expect("real original certificate"));
            (admission.into_collector(), ())
        }) {
            Ok(_) => panic!("forgotten certificate reexported"),
            Err(failure) => failure,
        };
        assert_eq!(
            failure.reason(),
            TransferError::Observations(SourceObservationError::LiveCertificates)
        );
        assert!(pending.is_poisoned());
        assert!(failure.retains_backing());
    });
}

#[test]
fn pentry_e2b_heap_transfer_panic_retires_original_observation_rc() {
    with_prepared(|pending, transfer| {
        let original = std::cell::Cell::new(std::ptr::null());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = transfer.with_collector::<()>(|mut admission| {
                admission.install_canonical_source_observations();
                original.set(neovm_value::tagged::collection_reads::installed_store_address());
                panic!("ORIGINAL_OBSERVATION_INSTALL_PANIC");
            });
        }));
        assert!(result.is_err());
        assert!(pending.is_poisoned());
        assert!(!original.get().is_null());
        assert_ne!(
            neovm_value::tagged::collection_reads::installed_store_address(),
            original.get()
        );
        assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    });
}

#[test]
fn pentry_e2b_heap_transfer_foreign_observation_owner_preserved_and_source_restore_refuses() {
    with_prepared(|pending, transfer| {
        // SAFETY: this fixture owns the thread's observation TLS. The guard
        // drops at the end of the test, after the refusal is observed.
        let foreign = unsafe {
            neovm_value::tagged::collection_reads::ObservationPlaceholderGuard::install()
        };
        let expected = foreign.root();
        let (transfer, ()) = transfer
            .with_collector(|admission| (admission.into_collector(), ()))
            .unwrap();
        assert_eq!(
            neovm_value::tagged::collection_reads::installed_store_address(),
            expected
        );
        let returned = transfer.return_to_source().unwrap();
        let restored = pending
            .restore(returned)
            .unwrap_or_else(|_| panic!("matching returned control"));
        let failure = match unsafe { heap_transfer_adapter::recover_transferred_source(restored) } {
            Ok(_) => panic!("displaced source placeholder accepted"),
            Err(failure) => failure,
        };
        assert_eq!(
            failure.reason(),
            TransferError::Observations(SourceObservationError::PlaceholderDisplaced)
        );
        assert!(failure.retains_backing());
        assert!(failure.retains_observations());
        assert_eq!(
            neovm_value::tagged::collection_reads::installed_store_address(),
            expected
        );
    });
}

#[test]
fn pentry_e2b_heap_transfer_source_certificate_refusal_owns_recovery_inputs() {
    with_prepared(|pending, transfer| {
        let (_, certificate) = neovm_value::tagged::collection_reads::capture(|| ());
        let certificate = certificate.expect("real source placeholder certificate");
        let returned = transfer.return_to_source().unwrap();
        let restored = pending
            .restore(returned)
            .unwrap_or_else(|_| panic!("matching returned control"));
        let failure = match unsafe { heap_transfer_adapter::recover_transferred_source(restored) } {
            Ok(_) => panic!("borrowed source effect restored"),
            Err(failure) => failure,
        };
        assert_eq!(
            failure.reason(),
            TransferError::Observations(SourceObservationError::LiveCertificates)
        );
        assert!(failure.retains_backing());
        assert!(failure.retains_observations());
        drop(certificate);
        let (_reason, restored) = failure.into_restored();
        finish_restored(restored);
    });
}

/// Return every collector and era argument frame before reconstructing the
/// closed source. The caller holds `unit_harness` for the whole loan.
fn closed_source_inputs() -> (
    WorldLoan,
    FrozenLegacyRoots,
    neovm_value::tagged::collection_reads::SourceObservationsDetached,
) {
    let census = RefCell::new(Vec::new());
    Context::with_test_symbol_functions_init_era(
        PlainSymbolBacking::Unprojected,
        |obarray, ids| {
            let private = TaggedValue::string("closed-source-transfer-root");
            obarray.seed_value_id(ids[0], private);
            census
                .borrow_mut()
                .extend([private, TaggedValue::fixnum(73), TaggedValue::NIL]);
        },
        |era, _| {
            let (mut collector, owner) = era.finish().expect("closed source fixture finishes");
            let heap = collector.heap_identity();
            // SAFETY: the private test frame above was captured before the
            // collector loan, no Context or activation remains, and World
            // storage still retains every word through this closed fixture.
            let roots = unsafe {
                FrozenLegacyRoots::new(heap, census.take(), FrozenFunctionRoots::Quiescent)
            };
            let root = owner.parallel_heap_root_designator();
            heap_transfer_adapter::detach_source_heap(root.as_non_null());
            assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
            let observations = collector
                .detach_source_observations()
                .expect("actual source observation detachment");
            drop(collector);
            (WorldLoan::from(owner), roots, observations)
        },
    )
}

fn prepare_closed_fixture() -> Option<(PendingHeapReturn, PreparedHeapTransfer)> {
    let (owner, roots, observations) = closed_source_inputs();
    // SAFETY: every original source collector and era argument frame returned
    // from `closed_source_inputs` first. No source view, TLS slot, or heap
    // mutation remains. The owner, census, and receipt retain that same
    // closed quiescent allocation.
    match unsafe { PreparedHeapTransfer::prepare_closed_source(owner, roots, observations) } {
        Ok(prepared) => Some(prepared),
        Err(failure) => {
            assert_eq!(failure.reason(), TransferError::GenerationalRequired);
            None
        }
    }
}

fn finish_closed_source(pending: PendingHeapReturn, transfer: PreparedHeapTransfer) {
    let returned = transfer.return_to_source().expect("reference-free return");
    let restored = pending
        .restore(returned)
        .unwrap_or_else(|failure| panic!("source restore: {:?}", failure.reason()));
    // SAFETY: original source thread. This fixture has no Context or
    // activation to reattach. No collector borrow remains, and the restored
    // words stay owned until publication retirement under serial access.
    let (owner, roots, publication) =
        unsafe { heap_transfer_adapter::recover_transferred_source(restored) }
            .unwrap_or_else(|failure| panic!("source materialization: {:?}", failure.reason()));
    assert!(owner.held_here());
    let words = census_words(&roots);
    assert_eq!(words.len(), 3);
    assert_eq!(
        words[0].as_str_owned().as_deref(),
        Some("closed-source-transfer-root")
    );
    assert_eq!(words[1], TaggedValue::fixnum(73));
    assert_eq!(words[2], TaggedValue::NIL);
    publication
        .try_finish()
        .expect("closed source publication has no readers");
    drop(roots);
    drop(owner);
}

#[test]
fn pentry_e2b_heap_transfer_closed_source_roundtrip() {
    let _harness = crate::test_utils::unit_harness();
    let Some((pending, transfer)) = prepare_closed_fixture() else {
        return;
    };
    finish_closed_source(pending, transfer);
}

#[test]
fn pentry_e2b_heap_transfer_closed_source_actual_os_thread_loans() {
    let _harness = crate::test_utils::unit_harness();
    let Some((pending, transfer)) = prepare_closed_fixture() else {
        return;
    };
    let transfer = std::thread::spawn(move || {
        let (transfer, heap) = transfer
            .with_collector(|admission| {
                let collector = admission.into_collector();
                let heap = collector.heap_identity();
                (collector, heap)
            })
            .expect("destination-local GEN1 loan");
        assert_eq!(heap, transfer.heap_identity());
        transfer
    })
    .join()
    .expect("service thread");
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    finish_closed_source(pending, transfer);
}

#[test]
fn pentry_e2b_heap_transfer_closed_source_drop_poison_retains_publication() {
    let _harness = crate::test_utils::unit_harness();
    let Some((pending, transfer)) = prepare_closed_fixture() else {
        return;
    };
    drop(transfer);
    assert!(pending.is_poisoned());
    drop(pending);
}

#[test]
fn pentry_e2b_heap_transfer_closed_source_foreign_tls_refusal_keeps_original_inputs() {
    let _harness = crate::test_utils::unit_harness();
    let (owner, roots, observations) = closed_source_inputs();
    let expected = owner.parallel_heap_identity();
    let unrelated =
        neovm_value::tagged::gc::AliasableBox::new(neovm_value::tagged::gc::TaggedHeap::new());
    neovm_value::tagged::gc::set_tagged_heap_root(unrelated.root());
    // SAFETY: the original source frames remain closed and retained. A
    // different TLS activation is introduced only to prove the refusal runs
    // before any heap dereference.
    let outcome =
        unsafe { PreparedHeapTransfer::prepare_closed_source(owner, roots, observations) };
    let current = neovm_value::tagged::gc::tagged_heap_is_current(&unrelated);
    neovm_value::tagged::gc::clear_tagged_heap_if_installed(&unrelated);
    let failure = match outcome {
        Err(failure) => failure,
        Ok(_) => panic!("foreign TLS was accepted"),
    };
    assert_eq!(failure.reason(), TransferError::InstalledTls);
    assert_eq!(failure.retained_owner_heap_identity(), expected);
    assert_eq!(failure.retained_root_word_count(), 3);
    assert!(current);
}

struct PanickingRetention {
    inner: WorldLoan,
    _probe: Arc<()>,
}

// SAFETY: every identity and backing operation delegates to the same actual
// owner. The retention hook panics before producing a new right. Preparation
// must retain this original adapter during unwinding.
unsafe impl EraWorldLoan for PanickingRetention {
    fn held_here(&self) -> bool {
        self.inner.held_here()
    }

    fn parallel_heap_identity(&self) -> HeapIdentity {
        self.inner.parallel_heap_identity()
    }

    fn parallel_epoch_retention(&self) -> Result<FacadeEpochRetention, FacadeMarkExclusionError> {
        panic!("CLOSED_SOURCE_RETENTION_HOOK_UNWIND")
    }

    fn symbol_root_designators(&self) -> SymbolRootDesignators {
        self.inner.symbol_root_designators()
    }

    fn check_symbol_root_designators(
        &self,
        roots: &SymbolRootDesignators,
    ) -> Result<(), RegistrationError> {
        self.inner.check_symbol_root_designators(roots)
    }

    fn collect_parallel_world_roots(
        &self,
        stopped: &mut AllStopped<'_>,
        out: &mut Vec<TaggedValue>,
    ) -> Result<Vec<*mut *mut MarkerObj>, RendezvousError> {
        self.inner.collect_parallel_world_roots(stopped, out)
    }

    fn into_retained_owner(self: Box<Self>) -> Box<dyn std::any::Any + Send> {
        Box::new(self.inner).into_retained_owner()
    }

    fn adopt_current_thread(&self) {
        self.inner.adopt_current_thread()
    }

    fn heap_root_designator(&self) -> HeapRootDesignator {
        self.inner.heap_root_designator()
    }
}

#[test]
fn pentry_e2b_heap_transfer_closed_source_unwind_retains_actual_owner() {
    let _harness = crate::test_utils::unit_harness();
    let (owner, roots, observations) = closed_source_inputs();
    let probe = Arc::new(());
    let weak = Arc::downgrade(&probe);
    let owner = WorldLoan::new(PanickingRetention {
        inner: owner,
        _probe: probe,
    });
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        // SAFETY: every actual source frame ended, on the same original
        // backing, with no intervening heap operation. The fault changes only
        // the retention hook.
        unsafe { PreparedHeapTransfer::prepare_closed_source(owner, roots, observations) }
    }));
    match outcome {
        Err(payload) => assert_eq!(
            payload.downcast_ref::<&str>().copied(),
            Some("CLOSED_SOURCE_RETENTION_HOOK_UNWIND")
        ),
        Ok(Err(failure)) => assert_eq!(failure.reason(), TransferError::GenerationalRequired),
        Ok(Ok(_)) => panic!("retention hook did not execute"),
    }
    assert!(
        weak.upgrade().is_some(),
        "unwind or refusal must retain the original owner"
    );
}

#[test]
fn pentry_e2b_retained_era_owner_actual_thread_roundtrip_preserves_backing() {
    use neovm_value::tagged::gc::mutator_registration::RetainedEraOwner;
    let _harness = crate::test_utils::unit_harness();
    let (owner, roots, observations) = closed_source_inputs();
    let identity = owner.parallel_heap_identity();
    let root = owner.heap_root_designator().as_non_null().as_ptr();
    let (retainer, proxy) = RetainedEraOwner::new(owner);
    let proxy = WorldLoan::new(proxy);
    let proxy = std::thread::spawn(move || {
        proxy.adopt_current_thread();
        assert!(proxy.held_here());
        proxy
    })
    .join()
    .unwrap();
    let owner = retainer.recover(proxy).unwrap();
    owner.adopt_current_thread();
    assert!(owner.held_here());
    assert_eq!(owner.parallel_heap_identity(), identity);
    assert_eq!(owner.heap_root_designator().as_non_null().as_ptr(), root);
    // SAFETY: the same closed source. No collector, store frame, or heap
    // mutation occurred during the reference-free owner handoff.
    match unsafe { PreparedHeapTransfer::prepare_closed_source(owner, roots, observations) } {
        Ok((pending, transfer)) => finish_closed_source(pending, transfer),
        Err(failure) => assert_eq!(failure.reason(), TransferError::GenerationalRequired),
    }
}

#[test]
fn pentry_e2b_retained_era_owner_foreign_proxy_is_owned_failure() {
    use neovm_value::tagged::gc::mutator_registration::{OwnerRecoveryError, RetainedEraOwner};
    let _harness = crate::test_utils::unit_harness();
    let (first, first_roots, first_observations) = closed_source_inputs();
    let (second, second_roots, second_observations) = closed_source_inputs();
    let _roots = ManuallyDrop::new((
        first_roots,
        second_roots,
        first_observations,
        second_observations,
    ));
    let (first, first_proxy) = RetainedEraOwner::new(first);
    let first_proxy = WorldLoan::new(first_proxy);
    let (second, second_proxy) = RetainedEraOwner::new(second);
    let second_proxy = WorldLoan::new(second_proxy);
    let failure = match first.recover(second_proxy) {
        Err(failure) => failure,
        Ok(_) => panic!("foreign proxy recovered"),
    };
    assert_eq!(failure.reason(), OwnerRecoveryError::Foreign);
    drop(failure);
    drop(first_proxy);
    drop(second);
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_retained_era_owner_unwind_keeps_original_adapter() {
    use neovm_value::tagged::gc::mutator_registration::RetainedEraOwner;
    let _harness = crate::test_utils::unit_harness();
    let (owner, roots, observations) = closed_source_inputs();
    let _roots = ManuallyDrop::new((roots, observations));
    let probe = Arc::new(());
    let weak = Arc::downgrade(&probe);
    let owner = WorldLoan::new(PanickingRetention {
        inner: owner,
        _probe: probe,
    });
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let (_retainer, proxy) = RetainedEraOwner::new(owner);
        let proxy = WorldLoan::new(proxy);
        let _ = proxy.parallel_epoch_retention();
    }));
    assert!(outcome.is_err());
    assert!(
        weak.upgrade().is_some(),
        "unwind must retain the original owner"
    );
}

#[test]
fn pentry_e2b_retained_era_owner_poison_refuses_recovery() {
    use neovm_value::tagged::gc::mutator_registration::{OwnerRecoveryError, RetainedEraOwner};
    let _harness = crate::test_utils::unit_harness();
    let (owner, roots, observations) = closed_source_inputs();
    let _roots = ManuallyDrop::new((roots, observations));
    let probe = Arc::new(());
    let weak = Arc::downgrade(&probe);
    let owner = WorldLoan::new(PanickingRetention {
        inner: owner,
        _probe: probe,
    });
    let (retainer, proxy) = RetainedEraOwner::new(owner);
    let proxy = WorldLoan::new(proxy);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = proxy.parallel_epoch_retention();
    }));
    assert!(outcome.is_err());
    let failure = match retainer.recover(proxy) {
        Err(failure) => failure,
        Ok(_) => panic!("poisoned era owner recovered"),
    };
    assert_eq!(failure.reason(), OwnerRecoveryError::Poisoned);
    drop(failure);
    assert!(weak.upgrade().is_some());
}

#[test]
fn pentry_e2b_heap_transfer_production_era_roundtrip() {
    let _harness = crate::test_utils::unit_harness();
    let Some((pending, transfer)) = prepare_closed_fixture() else {
        return;
    };
    let (transfer, identity) = transfer
        .with_production_era(|era| era.heap_identity())
        .expect("scoped production era");
    assert_eq!(identity, transfer.heap_identity());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    finish_closed_source(pending, transfer);
}

#[test]
fn pentry_e2b_heap_transfer_production_era_thread_gc_roundtrip() {
    use super::ParallelHeapEraWorldCollectExt;
    let _harness = crate::test_utils::unit_harness();
    let Some((pending, transfer)) = prepare_closed_fixture() else {
        return;
    };
    let transfer = std::thread::spawn(move || {
        let mut transfer = transfer;
        // Two separate eras. Each ends its loan frame before this
        // reference-free right is reused.
        for _ in 0..2 {
            let (returned, identity) = transfer
                .with_production_era(|era| {
                    let mut stopped = era.stop().expect("no peer stop");
                    era.collect_stopped(&mut stopped)
                        .expect("actual rooted collection");
                    stopped.complete().expect("actual collection completion");
                    era.heap_identity()
                })
                .expect("scoped production era");
            assert_eq!(identity, returned.heap_identity());
            assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
            transfer = returned;
        }
        transfer
    })
    .join()
    .expect("service thread");
    finish_closed_source(pending, transfer);
}

#[test]
fn pentry_e2b_heap_transfer_production_era_panic_poison_no_resume() {
    let _harness = crate::test_utils::unit_harness();
    let Some((pending, transfer)) = prepare_closed_fixture() else {
        return;
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        transfer.with_production_era::<()>(|_| panic!("SCOPED_ACTUAL_ERA_UNWIND"))
    }));
    let payload = match result {
        Err(payload) => payload,
        Ok(_) => panic!("callback must unwind"),
    };
    assert_eq!(
        payload.downcast_ref::<&str>().copied(),
        Some("SCOPED_ACTUAL_ERA_UNWIND")
    );
    assert!(pending.is_poisoned());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    drop(pending);
}

#[test]
fn pentry_e2b_heap_transfer_production_era_forgotten_permit_refuses_send() {
    let _harness = crate::test_utils::unit_harness();
    let Some((pending, transfer)) = prepare_closed_fixture() else {
        return;
    };
    let failure = match transfer.with_production_era(|era| {
        let permit = era.register(1, 4).expect("actual permit");
        std::mem::forget(permit);
    }) {
        Err(failure) => failure,
        Ok(_) => panic!("unfinished participant must prevent re-export"),
    };
    assert_eq!(
        failure.reason(),
        ScopedEraError::Finish(RegistrationError::Busy)
    );
    assert!(failure.is_poisoned());
    assert!(pending.is_poisoned());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    drop(failure);
    drop(pending);
}

#[test]
fn pentry_e2b_heap_transfer_production_era_source_drop_prevents_reexport() {
    let _harness = crate::test_utils::unit_harness();
    let Some((pending, transfer)) = prepare_closed_fixture() else {
        return;
    };
    let failure = match transfer.with_production_era(|_| drop(pending)) {
        Err(failure) => failure,
        Ok(_) => panic!("abandoned actual source cannot return to Prepared"),
    };
    assert_eq!(
        failure.reason(),
        ScopedEraError::Transfer(TransferError::Phase)
    );
    assert!(failure.is_poisoned());
    drop(failure);
}

#[test]
fn pentry_e2b_heap_transfer_production_era_foreign_tls_entry_refusal() {
    let _harness = crate::test_utils::unit_harness();
    let Some((pending, transfer)) = prepare_closed_fixture() else {
        return;
    };
    let mut foreign = Context::new_minimal_vm_harness(crate::test_utils::unit_harness());
    foreign.setup_thread_locals();
    let failure =
        match transfer.with_production_era::<()>(|_| panic!("must refuse before callback")) {
            Err(failure) => failure,
            Ok(_) => panic!("foreign activation must be preserved"),
        };
    assert_eq!(
        failure.reason(),
        ScopedEraError::Transfer(TransferError::InstalledTls)
    );
    assert!(failure.is_poisoned());
    assert!(pending.is_poisoned());
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &foreign.tagged_heap
    ));
    neovm_value::tagged::gc::clear_tagged_heap_if_installed(&foreign.tagged_heap);
    drop(failure);
    drop(pending);
}
