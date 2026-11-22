use super::super::{
    DEFER_SCOPE, DEFERRED, DeferScope, EPOCH_DISCARDS, JobCell, JobClass, PENDING_COUNT,
    PENDING_IDS, pending_count, reset_carrier_tls, stash_deferred, take_deferred,
};
use super::DetachedBgMetadata;
use crate::emacs_core::jit::source_suspension::{SourceScopeRefusal, SourceScopeSlot};
use std::cell::Cell;
use std::sync::Arc;
use std::time::Instant;

fn isolate() -> Isolate {
    reset_carrier_tls();
    Isolate
}

struct Isolate;

impl Drop for Isolate {
    fn drop(&mut self) {
        reset_carrier_tls();
    }
}

#[test]
fn pentry_e2b_source_compile_background_stale_ids_and_epochs_move_without_loss() {
    let _guard = isolate();
    assert_eq!(pending_count(), 0);
    PENDING_IDS.with(|slot| slot.borrow_mut().extend([17, 29]));
    EPOCH_DISCARDS.with(|slot| {
        slot.borrow_mut().insert(17, 2);
    });
    let receipt = DetachedBgMetadata::withdraw().unwrap();
    assert_eq!(receipt.pending_ids.as_slice(), &[17, 29]);
    assert_eq!(receipt.epoch_discards.get(&17), Some(&2));
    assert!(PENDING_IDS.with(|slot| slot.borrow().is_empty()));
    assert!(EPOCH_DISCARDS.with(|slot| slot.borrow().is_empty()));
    drop(receipt);
}

#[test]
fn pentry_e2b_source_compile_background_every_slot_loan_preserves_metadata() {
    let _guard = isolate();
    PENDING_IDS.with(|slot| slot.borrow_mut().push(41));
    EPOCH_DISCARDS.with(|slot| {
        slot.borrow_mut().insert(41, 1);
    });
    macro_rules! loan {
        ($slot:ident, $kind:ident) => {
            $slot.with(|slot| {
                let reader = slot.borrow();
                assert_eq!(
                    DetachedBgMetadata::withdraw().unwrap_err(),
                    SourceScopeRefusal::Borrowed(SourceScopeSlot::$kind)
                );
                drop(reader);
            });
        };
    }
    loan!(DEFERRED, DeferredCode);
    loan!(PENDING_IDS, PendingIds);
    loan!(EPOCH_DISCARDS, EpochDiscards);
    assert_eq!(PENDING_IDS.with(|slot| slot.borrow().clone()), vec![41]);
    assert_eq!(
        EPOCH_DISCARDS.with(|slot| slot.borrow().get(&41).copied()),
        Some(1)
    );
}

#[test]
fn pentry_e2b_source_compile_background_live_defer_scope_is_not_consumed() {
    let _guard = isolate();
    let scope = DeferScope::enter(Some(JobClass::Entry));
    assert_eq!(
        DetachedBgMetadata::withdraw().unwrap_err(),
        SourceScopeRefusal::DeferredScope
    );
    assert_eq!(DEFER_SCOPE.with(Cell::get), Some(JobClass::Entry));
    drop(scope);
    assert!(DetachedBgMetadata::withdraw().is_ok());
}

#[test]
fn pentry_e2b_source_compile_background_pending_count_is_not_cancelled() {
    let _guard = isolate();
    let scope = neovm_value::tls_scope::TlsScope::new(&PENDING_COUNT, 1);
    assert_eq!(
        DetachedBgMetadata::withdraw().unwrap_err(),
        SourceScopeRefusal::PendingJobs
    );
    assert_eq!(pending_count(), 1);
    drop(scope);
    assert_eq!(pending_count(), 0);
}

#[test]
fn pentry_e2b_source_compile_background_real_deferred_precedes_pending_count() {
    let _guard = isolate();
    let cell = JobCell::new();
    stash_deferred(
        Arc::clone(&cell),
        JobClass::Entry,
        Instant::now(),
        43,
        false,
    );
    assert_eq!(
        pending_count(),
        0,
        "deferred work precedes PendingJob construction"
    );
    let owners = Arc::strong_count(&cell);
    assert_eq!(
        DetachedBgMetadata::withdraw().unwrap_err(),
        SourceScopeRefusal::DeferredCode
    );
    assert_eq!(Arc::strong_count(&cell), owners);
    assert!(!cell.is_cancelled());
    let code = take_deferred().unwrap();
    assert!(Arc::ptr_eq(&code.cell, &cell));
    assert!(DetachedBgMetadata::withdraw().is_ok());
}
