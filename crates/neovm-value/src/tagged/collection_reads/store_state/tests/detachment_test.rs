use super::*;
use crate::tagged::gc::{
    AliasableBox, QuiescentCollector, clear_tagged_heap_root_if_installed, set_tagged_heap_root,
};

fn fixture() -> AliasableBox<TaggedHeap> {
    let heap = AliasableBox::new(TaggedHeap::new());
    unsafe { set_tagged_heap_root(heap.root()) };
    heap
}
fn stopped(heap: &mut AliasableBox<TaggedHeap>) -> QuiescentCollector<'_> {
    let root = heap.root();
    // SAFETY: actual sole fixture owner, no marker, callbacks or live borrowed
    // store views. Only explicit observation handles may exist in refusal tests.
    let collector = unsafe { (*root.as_ptr()).drain_to_quiescent(|_| {}) }.unwrap();
    // SAFETY: standalone `AliasableBox<TaggedHeap>` fixture detached for collector test.
    unsafe { clear_tagged_heap_root_if_installed(root) };
    collector
}
fn detached(collector: &mut QuiescentCollector<'_>) -> SourceObservationsDetached {
    collector
        .detach_source_observations()
        .unwrap_or_else(|e| panic!("detach: {e:?}"))
}
fn error(collector: &mut QuiescentCollector<'_>) -> SourceObservationError {
    match collector.detach_source_observations() {
        Ok(_) => panic!("expected refusal"),
        Err(error) => error,
    }
}
fn current_identity() -> ObservationIdentity {
    STATE.with(|slot| ObservationIdentity::of(&slot.owner.borrow()))
}

#[test]
fn pentry_e2b_observation_actual_collector_roundtrip_and_sole_owner() {
    let mut heap = fixture();
    let original = current_identity();
    let mut collector = stopped(&mut heap);
    let receipt = detached(&mut collector);
    assert!(current_identity() != original);
    assert_eq!(collector.validate_source_observation_export(), Ok(()));
    collector
        .restore_source_observations(receipt)
        .expect("matching local effect");
    assert!(current_identity() == original);
    assert_eq!(
        collector.validate_source_observation_export(),
        Err(SourceObservationError::EscapedOwner)
    );
    drop(collector);
    unsafe { set_tagged_heap_root(heap.root()) };
    let value = heap.alloc_cons(TaggedValue::fixnum(73), TaggedValue::NIL);
    assert_eq!(value.cons_car(), TaggedValue::fixnum(73));
}
#[test]
fn pentry_e2b_observation_repeated_scopes_use_fresh_placeholders() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    let first = detached(&mut collector);
    let old_placeholder = first.placeholder.clone();
    let failure = collector
        .restore_source_observations(first)
        .expect_err("escaped placeholder");
    assert_eq!(failure.reason, SourceObservationError::EscapedOwner);
    drop(old_placeholder);
    collector
        .restore_source_observations(failure.receipt)
        .unwrap();
    let second = detached(&mut collector);
    assert_eq!(collector.validate_source_observation_export(), Ok(()));
    collector.restore_source_observations(second).unwrap();
}
#[test]
fn pentry_e2b_observation_active_capture_refuses_without_discarding() {
    let mut heap = fixture();
    let scope = CollectionReadScope::begin();
    let original = current_identity();
    let mut collector = stopped(&mut heap);
    assert_eq!(
        error(&mut collector),
        SourceObservationError::ActiveCaptures
    );
    assert!(current_identity() == original);
    drop(collector);
    drop(scope);
    let mut collector = stopped(&mut heap);
    let receipt = detached(&mut collector);
    collector.restore_source_observations(receipt).unwrap();
}
#[test]
fn pentry_e2b_observation_completed_certificate_refuses_until_retired() {
    let mut heap = fixture();
    let (_, certificate) = capture(|| ());
    let certificate = certificate.expect("real completed certificate");
    let mut collector = stopped(&mut heap);
    assert_eq!(
        error(&mut collector),
        SourceObservationError::LiveCertificates
    );
    drop(collector);
    drop(certificate);
    let mut collector = stopped(&mut heap);
    let receipt = detached(&mut collector);
    collector.restore_source_observations(receipt).unwrap();
}
#[test]
fn pentry_e2b_observation_forgotten_certificate_stays_refused() {
    let mut heap = fixture();
    let (_, certificate) = capture(|| ());
    std::mem::forget(certificate.expect("real certificate"));
    let mut collector = stopped(&mut heap);
    assert_eq!(
        error(&mut collector),
        SourceObservationError::LiveCertificates
    );
}
#[test]
fn pentry_e2b_observation_extra_rc_owner_refuses() {
    let mut heap = fixture();
    let extra = ObservationWorld::current();
    let mut collector = stopped(&mut heap);
    assert_eq!(error(&mut collector), SourceObservationError::EscapedOwner);
    drop(extra);
    let receipt = detached(&mut collector);
    collector.restore_source_observations(receipt).unwrap();
}
#[test]
fn pentry_e2b_observation_weak_owner_refuses() {
    let mut heap = fixture();
    let weak = STATE.with(|slot| Rc::downgrade(&slot.owner.borrow().0));
    let mut collector = stopped(&mut heap);
    assert_eq!(error(&mut collector), SourceObservationError::EscapedOwner);
    drop(weak);
    let receipt = detached(&mut collector);
    collector.restore_source_observations(receipt).unwrap();
}
#[test]
fn pentry_e2b_observation_owner_reader_and_writer_refuse() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    STATE.with(|slot| {
        let reader = slot.owner.borrow();
        assert_eq!(error(&mut collector), SourceObservationError::OwnerBorrowed);
        drop(reader);
        let writer = slot.owner.borrow_mut();
        assert_eq!(error(&mut collector), SourceObservationError::OwnerBorrowed);
        drop(writer);
    });
    let receipt = detached(&mut collector);
    collector.restore_source_observations(receipt).unwrap();
}
#[test]
fn pentry_e2b_observation_suspended_slot_loans_refuse() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    STATE.with(|slot| {
        let reader = slot.suspended.borrow();
        assert_eq!(
            error(&mut collector),
            SourceObservationError::SuspendedBorrowed
        );
        drop(reader);
        let writer = slot.suspended.borrow_mut();
        assert_eq!(
            error(&mut collector),
            SourceObservationError::SuspendedBorrowed
        );
        drop(writer);
    });
}
#[test]
fn pentry_e2b_observation_cold_reader_and_writer_refuse() {
    let mut heap = fixture();
    let current = ObservationWorld::current();
    let mut collector = stopped(&mut heap);
    current.with_state(|cell| {
        let reader = cell.borrow();
        assert_eq!(error(&mut collector), SourceObservationError::ColdBorrowed);
        drop(reader);
        let writer = cell.borrow_mut();
        assert_eq!(error(&mut collector), SourceObservationError::ColdBorrowed);
        drop(writer);
    });
}
#[test]
fn pentry_e2b_observation_suspended_owner_refuses_without_removal() {
    let mut heap = fixture();
    STATE.with(|slot| {
        slot.suspended
            .borrow_mut()
            .push(ObservationWorld::default())
    });
    let mut collector = stopped(&mut heap);
    assert_eq!(
        error(&mut collector),
        SourceObservationError::SuspendedScopes
    );
    STATE.with(|slot| assert_eq!(slot.suspended.borrow().len(), 1));
    STATE.with(|slot| slot.suspended.borrow_mut().clear());
}
#[test]
fn pentry_e2b_observation_wrong_store_root_refuses_without_mutation() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    let other = ObservationWorld::default();
    STATE.with(|slot| {
        let root = slot.store.replace(other.root());
        assert_eq!(
            error(&mut collector),
            SourceObservationError::StoreDisplaced
        );
        assert_eq!(slot.store.get(), other.root());
        slot.store.set(root);
    });
}
#[test]
fn pentry_e2b_observation_displaced_placeholder_returns_owned_receipt() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    let receipt = detached(&mut collector);
    let replacement = ObservationWorld::default();
    let saved = STATE.with(|slot| {
        let previous = std::mem::replace(&mut *slot.owner.borrow_mut(), replacement.clone());
        slot.store.set(replacement.root());
        previous
    });
    let failure = collector.restore_source_observations(receipt).unwrap_err();
    assert_eq!(failure.reason, SourceObservationError::PlaceholderDisplaced);
    assert!(current_identity() == ObservationIdentity::of(&replacement));
    let removed = STATE.with(|slot| {
        let previous = std::mem::replace(&mut *slot.owner.borrow_mut(), saved);
        slot.store.set(failure.receipt.placeholder.root());
        previous
    });
    drop(removed);
    collector
        .restore_source_observations(failure.receipt)
        .unwrap();
}
#[test]
fn pentry_e2b_observation_restore_loan_refusal_preserves_receipt() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    let receipt = detached(&mut collector);
    let failure = STATE.with(|slot| {
        let _reader = slot.owner.borrow();
        collector.restore_source_observations(receipt).unwrap_err()
    });
    assert_eq!(failure.reason, SourceObservationError::OwnerBorrowed);
    collector
        .restore_source_observations(failure.receipt)
        .unwrap();
}
#[test]
fn pentry_e2b_observation_wrong_heap_restore_keeps_both_worlds() {
    let mut first = fixture();
    let original = current_identity();
    let receipt = {
        let mut collector = stopped(&mut first);
        detached(&mut collector)
    };
    let placeholder = ObservationIdentity::of(&receipt.placeholder);
    let mut second = fixture();
    let second_identity = current_identity();
    assert!(
        second_identity != placeholder,
        "new heap must not adopt a reserved source sentinel"
    );
    let failure = {
        let mut collector = stopped(&mut second);
        collector.restore_source_observations(receipt).unwrap_err()
    };
    assert_eq!(failure.reason, SourceObservationError::WrongHeap);
    assert!(current_identity() == second_identity);
    drop(second);
    // Reinstall the exact placeholder, not the original first owner.
    failure.receipt.placeholder.install(BarrierWindow::NONE);
    let mut collector = stopped(&mut first);
    collector
        .restore_source_observations(failure.receipt)
        .unwrap();
    assert!(current_identity() == original);
}
#[test]
fn pentry_e2b_observation_drop_receipt_never_restores_original() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    let receipt = detached(&mut collector);
    let placeholder = current_identity();
    drop(receipt);
    assert!(current_identity() == placeholder);
    assert_eq!(collector.validate_source_observation_export(), Ok(()));
}
#[test]
fn pentry_e2b_observation_forget_receipt_never_restores_original() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    let receipt = detached(&mut collector);
    let placeholder = current_identity();
    std::mem::forget(receipt);
    assert!(current_identity() == placeholder);
    assert_eq!(collector.validate_source_observation_export(), Ok(()));
}
#[test]
fn pentry_e2b_observation_unwind_receipt_never_restores_original() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    let receipt = detached(&mut collector);
    let placeholder = current_identity();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _receipt = receipt;
        panic!("SOURCE_OBSERVATION_EFFECT_UNWIND");
    }));
    assert!(result.is_err());
    assert!(current_identity() == placeholder);
    assert_eq!(collector.validate_source_observation_export(), Ok(()));
}
#[test]
fn pentry_e2b_observation_refuses_installed_heap_before_effect() {
    let heap = fixture();
    let root = heap.root();
    // SAFETY: sole fixture owner as in stopped(); intentionally preserve TLS
    // to exercise refusal before any observation-owner effect.
    let mut collector = unsafe { (*root.as_ptr()).drain_to_quiescent(|_| {}) }.unwrap();
    assert_eq!(error(&mut collector), SourceObservationError::InstalledHeap);
    // SAFETY: standalone `AliasableBox<TaggedHeap>` fixture detached for collector test.
    unsafe { clear_tagged_heap_root_if_installed(root) };
}
#[test]
fn pentry_e2b_observation_export_revalidates_new_alias() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    let receipt = detached(&mut collector);
    assert_eq!(collector.validate_source_observation_export(), Ok(()));
    collector.restore_source_observations(receipt).unwrap();
    assert_eq!(
        collector.validate_source_observation_export(),
        Err(SourceObservationError::EscapedOwner)
    );
}

#[test]
fn pentry_e2b_observation_missing_canonical_owner_refuses() {
    let mut heap = AliasableBox::new(TaggedHeap::new());
    let mut collector = stopped(&mut heap);
    assert_eq!(error(&mut collector), SourceObservationError::MissingOwner);
}
#[test]
fn pentry_e2b_observation_second_detach_refuses_while_receipt_out() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    let receipt = detached(&mut collector);
    let placeholder = current_identity();
    assert_eq!(error(&mut collector), SourceObservationError::WrongOwner);
    assert!(current_identity() == placeholder);
    collector.restore_source_observations(receipt).unwrap();
}
#[test]
fn pentry_e2b_observation_restore_wrong_thread_metadata_refuses() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    let mut receipt = detached(&mut collector);
    let original = receipt.thread;
    // Fault only metadata. No receipt, heap or Rc crosses this OS thread.
    receipt.thread = std::thread::spawn(|| std::thread::current().id())
        .join()
        .unwrap();
    let mut failure = collector.restore_source_observations(receipt).unwrap_err();
    assert_eq!(failure.reason, SourceObservationError::WrongThread);
    failure.receipt.thread = original;
    collector
        .restore_source_observations(failure.receipt)
        .unwrap();
}
#[test]
fn pentry_e2b_observation_restore_wrong_allocation_metadata_refuses() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    let mut receipt = detached(&mut collector);
    let original = receipt.original;
    receipt.original = ObservationIdentity::of(&receipt.placeholder);
    let mut failure = collector.restore_source_observations(receipt).unwrap_err();
    assert_eq!(failure.reason, SourceObservationError::WrongOwner);
    failure.receipt.original = original;
    collector
        .restore_source_observations(failure.receipt)
        .unwrap();
}
#[test]
fn pentry_e2b_observation_placeholder_capture_refuses_restore_until_retired() {
    let mut heap = fixture();
    let mut collector = stopped(&mut heap);
    let receipt = detached(&mut collector);
    let scope = CollectionReadScope::begin();
    let failure = collector.restore_source_observations(receipt).unwrap_err();
    assert_eq!(failure.reason, SourceObservationError::ActiveCaptures);
    drop(scope);
    collector
        .restore_source_observations(failure.receipt)
        .unwrap();
}
#[test]
fn pentry_e2b_observation_detach_clears_existing_recent_read_metadata() {
    let mut heap = fixture();
    let bits = 0x1200; // Journal identity key only; never a dereferenced object.
    note_recent_miss(bits);
    assert!(!recently_observed(bits));
    let mut collector = stopped(&mut heap);
    // Address retirement itself may also clear the existing cache. Exercise
    // actual dirty recent metadata specifically at this effect's boundary.
    note_recent_miss(bits);
    assert!(!recently_observed(bits));
    let receipt = detached(&mut collector);
    assert!(!ACTIVE.with(Cell::get));
    RECENT_READS.with(|recent| assert!(recent.words.iter().all(|word| word.get() == 0)));
    collector.restore_source_observations(receipt).unwrap();
}
