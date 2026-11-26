use super::super::{
    IN_FLIGHT_ROOTS, InFlightRegistryHandle, PinnedWord, collect_in_flight_registry_gc_roots,
    install_in_flight_registry_handle,
};
use super::{
    DetachedInFlightRegistry, InFlightDetachError, InFlightDetachFailure,
    detach_in_flight_registry_handle,
};
use crate::value::Value;
use std::sync::Arc;

fn detach(
    expected: &InFlightRegistryHandle,
) -> Result<DetachedInFlightRegistry, InFlightDetachFailure> {
    detach_in_flight_registry_handle(expected, InFlightRegistryHandle::new(None))
}

fn installed() -> InFlightRegistryHandle {
    let handle = InFlightRegistryHandle::new(None);
    install_in_flight_registry_handle(&handle);
    handle
}

fn active_is(expected: &InFlightRegistryHandle) -> bool {
    IN_FLIGHT_ROOTS.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(&current.table, &expected.table))
    })
}

fn push_fixnum(handle: &InFlightRegistryHandle, n: i64) {
    let mut table = handle.table.lock().expect("table");
    table
        .slots
        .push(Some(vec![PinnedWord::of(Value::fixnum(n))]));
}

fn collected(handle: &InFlightRegistryHandle) -> Vec<Value> {
    let mut out = Vec::new();
    collect_in_flight_registry_gc_roots(&mut out, handle, 0);
    out
}

fn poison(handle: &InFlightRegistryHandle) {
    let worker = handle.clone();
    let result = std::thread::spawn(move || {
        let _guard = worker.table.lock().expect("initial table not poisoned");
        panic!("IN_FLIGHT_DETACH_POISON_FAULT");
    })
    .join();
    assert!(result.is_err());
    assert!(handle.table.is_poisoned());
}

#[test]
fn pentry_e2b_in_flight_detach_preserves_real_pin_and_original_roots() {
    let original = installed();
    push_fixnum(&original, 47);
    assert_eq!(collected(&original), vec![Value::fixnum(47)]);
    let receipt = detach(&original).expect("detach actual slot");
    assert!(!active_is(&original));
    assert!(active_is(&receipt.placeholder));
    assert!(receipt.placeholder.table.lock().unwrap().slots.is_empty());
    assert_eq!(
        collected(&original),
        vec![Value::fixnum(47)],
        "detachment changed live pin contents"
    );
    super::restore_in_flight_registry_handle(receipt).expect("matched restore");
    assert!(active_is(&original));
}

#[test]
fn pentry_e2b_in_flight_detach_preserves_absent_slot() {
    let original = installed();
    let departing = IN_FLIGHT_ROOTS.with(|slot| slot.borrow_mut().take());
    drop(departing);
    let receipt = detach(&original).expect("inactive source");
    assert!(IN_FLIGHT_ROOTS.with(|slot| slot.borrow().is_none()));
    super::restore_in_flight_registry_handle(receipt).expect("inactive no-op");
    assert!(IN_FLIGHT_ROOTS.with(|slot| slot.borrow().is_none()));
}

#[test]
fn pentry_e2b_in_flight_detach_preserves_foreign_same_heap_table() {
    let original = installed();
    let newer = InFlightRegistryHandle::new(original.heap_identity);
    install_in_flight_registry_handle(&newer);
    let receipt = detach(&original).expect("foreign table preserved");
    assert!(active_is(&newer));
    super::restore_in_flight_registry_handle(receipt).expect("inactive no-op");
    assert!(active_is(&newer));
}

#[test]
fn pentry_e2b_in_flight_detach_refuses_slot_reader_without_mutation() {
    let original = installed();
    IN_FLIGHT_ROOTS.with(|slot| {
        let view = slot.borrow();
        let failure = detach(&original).err().expect("slot borrow refusal");
        let (reason, placeholder) = failure.into_parts();
        assert_eq!(reason, InFlightDetachError::SlotBorrowed);
        assert!(placeholder.table.lock().unwrap().slots.is_empty());
        assert!(!Arc::ptr_eq(&placeholder.table, &original.table));
        assert!(Arc::ptr_eq(&view.as_ref().unwrap().table, &original.table));
    });
    assert!(active_is(&original));
}

#[test]
fn pentry_e2b_in_flight_detach_refuses_original_table_loan_without_waiting() {
    let original = installed();
    let guard = original.table.lock().unwrap();
    let failure = detach(&original).err().expect("table loan refusal");
    assert_eq!(failure.reason, InFlightDetachError::OriginalBusy);
    assert!(active_is(&original));
    drop(guard);
}

#[test]
fn pentry_e2b_in_flight_detach_refuses_concurrent_table_loan() {
    let original = installed();
    let table = original.clone();
    let (entered, ready) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let guard = table.table.lock().unwrap();
        entered.send(()).unwrap();
        released.recv().unwrap();
        drop(guard);
    });
    ready.recv().unwrap();
    let failure = detach(&original).err().expect("concurrent loan refusal");
    assert_eq!(failure.reason, InFlightDetachError::OriginalBusy);
    assert!(active_is(&original));
    release.send(()).unwrap();
    worker.join().unwrap();
}

#[test]
fn pentry_e2b_in_flight_detach_refuses_sticky_table_poison() {
    let original = installed();
    poison(&original);
    let failure = detach(&original).err().expect("poison refusal");
    assert_eq!(failure.reason, InFlightDetachError::OriginalPoisoned);
    assert!(original.table.is_poisoned());
    assert!(active_is(&original));
}

#[test]
fn pentry_e2b_in_flight_detach_restore_refuses_slot_reader_and_owns_receipt() {
    let original = installed();
    let receipt = detach(&original).unwrap();
    let failure = IN_FLIGHT_ROOTS.with(|slot| {
        let _view = slot.borrow();
        super::restore_in_flight_registry_handle(receipt).expect_err("slot borrow refusal")
    });
    assert_eq!(failure.reason, InFlightDetachError::SlotBorrowed);
    let (_, receipt) = failure.into_parts();
    super::restore_in_flight_registry_handle(receipt).expect("same receipt after borrow ends");
    assert!(active_is(&original));
}

#[test]
fn pentry_e2b_in_flight_detach_restore_refuses_original_loan_and_retries() {
    let original = installed();
    let receipt = detach(&original).unwrap();
    let guard = original.table.lock().unwrap();
    let failure =
        super::restore_in_flight_registry_handle(receipt).expect_err("original loan refusal");
    assert_eq!(failure.reason, InFlightDetachError::OriginalBusy);
    drop(guard);
    let (_, receipt) = failure.into_parts();
    super::restore_in_flight_registry_handle(receipt)
        .expect("same receipt after original loan ends");
    assert!(active_is(&original));
}

#[test]
fn pentry_e2b_in_flight_detach_restore_refuses_placeholder_loan_and_retries() {
    let original = installed();
    let receipt = detach(&original).unwrap();
    let placeholder = receipt.placeholder.clone();
    let guard = placeholder.table.lock().unwrap();
    let failure =
        super::restore_in_flight_registry_handle(receipt).expect_err("placeholder loan refusal");
    assert_eq!(failure.reason, InFlightDetachError::PlaceholderBusy);
    drop(guard);
    let (_, receipt) = failure.into_parts();
    super::restore_in_flight_registry_handle(receipt)
        .expect("same receipt after placeholder loan ends");
}

#[test]
fn pentry_e2b_in_flight_detach_restore_refuses_original_poison() {
    let original = installed();
    let receipt = detach(&original).unwrap();
    poison(&original);
    let failure =
        super::restore_in_flight_registry_handle(receipt).expect_err("original poison refusal");
    assert_eq!(failure.reason, InFlightDetachError::OriginalPoisoned);
    assert!(active_is(&failure.receipt.placeholder));
    assert!(original.table.is_poisoned());
}

#[test]
fn pentry_e2b_in_flight_detach_restore_refuses_placeholder_poison() {
    let original = installed();
    let receipt = detach(&original).unwrap();
    poison(&receipt.placeholder);
    let failure =
        super::restore_in_flight_registry_handle(receipt).expect_err("placeholder poison refusal");
    assert_eq!(failure.reason, InFlightDetachError::PlaceholderPoisoned);
    assert!(active_is(&failure.receipt.placeholder));
    assert!(failure.receipt.placeholder.table.is_poisoned());
}

#[test]
fn pentry_e2b_in_flight_detach_displaced_restore_keeps_new_table() {
    let original = installed();
    let receipt = detach(&original).unwrap();
    let newer = InFlightRegistryHandle::new(original.heap_identity);
    install_in_flight_registry_handle(&newer);
    let failure =
        super::restore_in_flight_registry_handle(receipt).expect_err("displacement refusal");
    assert_eq!(failure.reason, InFlightDetachError::Displaced);
    drop(failure);
    assert!(active_is(&newer));
}

#[test]
fn pentry_e2b_in_flight_detach_unique_sentinel_refuses_nested_aba() {
    let original = installed();
    let outer = detach(&original).unwrap();
    let outer_placeholder = outer.placeholder.clone();
    let inner = detach(&outer_placeholder).unwrap();
    let failure =
        super::restore_in_flight_registry_handle(outer).expect_err("IN_FLIGHT_DETACH_ABA_REFUSED");
    assert_eq!(failure.reason, InFlightDetachError::Displaced);
    assert!(active_is(&inner.placeholder));
    super::restore_in_flight_registry_handle(inner)
        .expect("nested source restores exact outer sentinel");
    let (_, outer) = failure.into_parts();
    super::restore_in_flight_registry_handle(outer)
        .expect("outer restores after exact sentinel returns");
    assert!(active_is(&original));
}

#[test]
fn pentry_e2b_in_flight_detach_drop_never_reinstalls() {
    let original = installed();
    let receipt = detach(&original).unwrap();
    let placeholder = receipt.placeholder.clone();
    drop(receipt);
    assert!(active_is(&placeholder));
    assert!(!active_is(&original));
}

#[test]
fn pentry_e2b_in_flight_detach_forget_retains_metadata_without_reinstall() {
    let original = installed();
    let receipt = detach(&original).unwrap();
    let weak = Arc::downgrade(&original.table);
    let placeholder = receipt.placeholder.clone();
    drop(original);
    std::mem::forget(receipt);
    assert!(weak.upgrade().is_some());
    assert!(active_is(&placeholder));
}
