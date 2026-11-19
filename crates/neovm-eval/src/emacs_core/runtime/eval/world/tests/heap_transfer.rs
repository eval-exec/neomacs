//! Actual original World/heap transport, without a Context or borrowed owner
//! crossing a channel. GEN0 exercises typed refusal and complete input return.
//!
//! The frame rule (R5) is the shape of `with_prepared`: the source callback
//! returns the reference-free right, and the destination body runs only after
//! that callback, including its protected era argument, has ended.

use super::heap_transfer_adapter;
use crate::emacs_core::eval::Context;
use neovm_value::tagged::gc::heap_transfer::{
    PendingHeapReturn, PreparedHeapTransfer, TransferError,
};
use neovm_value::tagged::gc::{
    FrozenFunctionRoots, FrozenLegacyRoots, Gen1Admission, Gen1AdmissionError, PlainSymbolBacking,
};
use neovm_value::tagged::value::TaggedValue;
use std::cell::RefCell;

fn census_words(roots: &FrozenLegacyRoots) -> Vec<TaggedValue> {
    let mut words = Vec::new();
    roots.append_words(&mut words);
    words
}

fn with_prepared(body: impl FnOnce(PendingHeapReturn, PreparedHeapTransfer)) {
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
            let admission = match Gen1Admission::check(collector) {
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
            // SAFETY: this closed fixture owns the only source frame. Heap TLS
            // was detached and the roots are retained. The reference-free
            // result is returned out of every source loan frame before any
            // destination body executes. Dropping the collector here does not
            // end this era argument's protector; the callback return does.
            let (pending, transfer) = unsafe {
                heap_transfer_adapter::prepare_reference_free_transfer(admission, owner, roots)
            }
            .unwrap_or_else(|failure| panic!("detached preparation refused: {failure:?}"));
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
