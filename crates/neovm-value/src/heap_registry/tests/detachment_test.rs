use super::super::{HeapRegistryHandle, HeapRegistrySlot};
use super::{RegistryDetachError, Restoration};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

#[test]
fn pentry_e2b_registry_detach_roundtrip_retains_original_contents() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let original = slot.current();
    let receipt = unsafe { slot.detach(&original, String::new()) }.expect("detach");
    assert_eq!(original.borrow().as_str(), "original");
    assert_eq!(slot.borrow().as_str(), "");
    assert!(!Arc::ptr_eq(&original.registry, &slot.current().registry));
    assert_eq!(slot.current().heap_identity(), 0);
    receipt.restore(&slot).expect("matched restore");
    assert!(Arc::ptr_eq(&original.registry, &slot.current().registry));
    assert_eq!(slot.borrow().as_str(), "original");
}

#[test]
fn pentry_e2b_registry_detach_parked_none_stays_empty() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    slot.park();
    let expected = HeapRegistryHandle::new(String::from("expected"));
    let receipt = unsafe { slot.detach(&expected, String::new()) }.expect("inactive");
    assert!(matches!(receipt.restoration, Restoration::AlreadyInactive));
    assert!(slot.active.borrow().is_none());
    receipt
        .restore(&slot)
        .expect("inactive restore leaves the park");
    assert!(slot.active.borrow().is_none());
    assert_eq!(expected.borrow().as_str(), "expected");
}

#[test]
fn pentry_e2b_registry_detach_preserves_newer_same_heap_installation() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let original = slot.current();
    let newer = HeapRegistryHandle::new(String::from("newer"));
    assert_eq!(original.heap_identity(), newer.heap_identity());
    slot.install(&newer);
    let receipt = unsafe { slot.detach(&original, String::new()) }.expect("already inactive");
    assert!(matches!(receipt.restoration, Restoration::AlreadyInactive));
    assert!(Arc::ptr_eq(&newer.registry, &slot.current().registry));
    receipt
        .restore(&slot)
        .expect("inactive restore preserves newer");
    assert!(Arc::ptr_eq(&newer.registry, &slot.current().registry));
    assert_eq!(original.borrow().as_str(), "original");
    assert_eq!(slot.borrow().as_str(), "newer");
}

#[test]
fn pentry_e2b_registry_detach_refuses_outer_reader_and_owns_placeholder() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let original = slot.current();
    let view = slot.borrow();
    let failure = unsafe { slot.detach(&original, String::from("placeholder")) }
        .err()
        .expect("borrow refusal");
    assert_eq!(failure.reason, RegistryDetachError::SlotBorrowed);
    let (_, placeholder) = failure.into_parts();
    assert_eq!(placeholder.borrow().as_str(), "placeholder");
    assert_eq!(view.as_str(), "original");
    drop(view);
    assert!(Arc::ptr_eq(&original.registry, &slot.current().registry));
}

#[test]
fn pentry_e2b_registry_detach_refuses_outer_writer() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let original = slot.current();
    let mut view = slot.borrow_mut();
    let failure = unsafe { slot.detach(&original, String::new()) }
        .err()
        .expect("borrow refusal");
    assert_eq!(failure.reason, RegistryDetachError::SlotBorrowed);
    view.push_str(" unchanged");
    drop(view);
    assert_eq!(original.borrow().as_str(), "original unchanged");
}

#[test]
fn pentry_e2b_registry_detach_refuses_direct_handle_reader() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let original = slot.current();
    let view = original.borrow();
    let failure = unsafe { slot.detach(&original, String::new()) }
        .err()
        .expect("borrow refusal");
    assert_eq!(failure.reason, RegistryDetachError::RegistryBorrowed);
    assert_eq!(view.as_str(), "original");
    assert!(Arc::ptr_eq(&original.registry, &slot.current().registry));
}

#[test]
fn pentry_e2b_registry_detach_refuses_direct_handle_writer() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let original = slot.current();
    let mut view = original.borrow_mut();
    let failure = unsafe { slot.detach(&original, String::new()) }
        .err()
        .expect("borrow refusal");
    assert_eq!(failure.reason, RegistryDetachError::RegistryBorrowed);
    view.push_str(" retained");
    drop(view);
    assert_eq!(slot.borrow().as_str(), "original retained");
}

#[test]
fn pentry_e2b_registry_detach_restore_refuses_outer_view_and_retries_owned_receipt() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let original = slot.current();
    let receipt = unsafe { slot.detach(&original, String::new()) }.expect("detach");
    let view = slot.borrow();
    let failure = receipt.restore(&slot).expect_err("borrow refusal");
    assert_eq!(failure.reason, RegistryDetachError::SlotBorrowed);
    drop(view);
    let (_, receipt) = failure.into_parts();
    receipt
        .restore(&slot)
        .expect("same retained receipt after borrow ends");
    assert_eq!(slot.borrow().as_str(), "original");
}

#[test]
fn pentry_e2b_registry_detach_restore_refuses_direct_placeholder_view() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let original = slot.current();
    let receipt = unsafe { slot.detach(&original, String::new()) }.expect("detach");
    let placeholder = slot.current();
    let view = placeholder.borrow();
    let failure = receipt.restore(&slot).expect_err("placeholder refusal");
    assert_eq!(failure.reason, RegistryDetachError::PlaceholderBorrowed);
    drop(view);
    let (_, receipt) = failure.into_parts();
    receipt.restore(&slot).expect("matched restore");
}

#[test]
fn pentry_e2b_registry_detach_restore_refuses_direct_original_view() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let original = slot.current();
    let receipt = unsafe { slot.detach(&original, String::new()) }.expect("detach");
    let view = original.borrow();
    let failure = receipt.restore(&slot).expect_err("original refusal");
    assert_eq!(failure.reason, RegistryDetachError::RegistryBorrowed);
    drop(view);
    let (_, receipt) = failure.into_parts();
    receipt.restore(&slot).expect("matched restore");
}

#[test]
fn pentry_e2b_registry_detach_restore_refuses_displaced_placeholder() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let original = slot.current();
    let receipt = unsafe { slot.detach(&original, String::new()) }.expect("detach");
    let placeholder = slot.current();
    let newer = HeapRegistryHandle::new(String::from("newer"));
    slot.install(&newer);
    let failure = receipt.restore(&slot).expect_err("displacement refusal");
    assert_eq!(failure.reason, RegistryDetachError::Displaced);
    assert_eq!(slot.borrow().as_str(), "newer");
    let (_, receipt) = failure.into_parts();
    assert!(Arc::ptr_eq(&original.registry, &receipt.original.registry));
    slot.install(&placeholder);
    receipt
        .restore(&slot)
        .expect("original slot and exact placeholder");
    assert_eq!(slot.borrow().as_str(), "original");
}

#[test]
fn pentry_e2b_registry_detach_restore_refuses_wrong_slot() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let other = HeapRegistrySlot::new(String::from("other"));
    let receipt = unsafe { slot.detach(&slot.current(), String::new()) }.expect("detach");
    let failure = receipt.restore(&other).expect_err("wrong slot refusal");
    assert_eq!(failure.reason, RegistryDetachError::WrongSlot);
    assert_eq!(other.borrow().as_str(), "other");
    assert_eq!(slot.borrow().as_str(), "");
    let (_, receipt) = failure.into_parts();
    receipt.restore(&slot).expect("original slot");
}

#[test]
fn pentry_e2b_registry_detach_drop_never_reinstalls() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let original = slot.current();
    let receipt = unsafe { slot.detach(&original, String::new()) }.expect("detach");
    drop(receipt);
    assert_eq!(original.borrow().as_str(), "original");
    assert_eq!(slot.borrow().as_str(), "");
    assert!(!Arc::ptr_eq(&original.registry, &slot.current().registry));
}

#[test]
fn pentry_e2b_registry_detach_forget_retains_original_without_reinstall() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let original = slot.current();
    let weak = original.downgrade();
    let receipt = unsafe { slot.detach(&original, String::new()) }.expect("detach");
    drop(original);
    std::mem::forget(receipt);
    assert_eq!(
        weak.upgrade().expect("retained original").borrow().as_str(),
        "original"
    );
    assert_eq!(slot.borrow().as_str(), "");
}

struct DropProbe {
    slot: std::rc::Weak<HeapRegistrySlot<DropProbe>>,
    drops: Rc<Cell<usize>>,
}

impl Drop for DropProbe {
    fn drop(&mut self) {
        if let Some(slot) = self.slot.upgrade() {
            assert!(
                slot.active.try_borrow_mut().is_ok(),
                "REGISTRY_PAYLOAD_DROPPED_UNDER_SLOT_GUARD"
            );
        }
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn pentry_e2b_registry_detach_payload_cleanup_runs_after_slot_guards() {
    let drops = Rc::new(Cell::new(0));
    let slot = Rc::new(HeapRegistrySlot::new(DropProbe {
        slot: std::rc::Weak::new(),
        drops: Rc::clone(&drops),
    }));
    let original = slot.current();
    let placeholder = || DropProbe {
        slot: Rc::downgrade(&slot),
        drops: Rc::clone(&drops),
    };
    let receipt = unsafe { slot.detach(&original, placeholder()) }.expect("detach");
    receipt
        .restore(&slot)
        .expect("restore drops placeholder outside guards");
    assert_eq!(drops.get(), 1);
    let view = original.borrow();
    let failure = unsafe { slot.detach(&original, placeholder()) }
        .err()
        .expect("direct view refusal");
    assert_eq!(failure.reason, RegistryDetachError::RegistryBorrowed);
    drop(failure);
    assert_eq!(drops.get(), 2);
    drop(view);
}

#[test]
fn pentry_e2b_registry_detach_nested_receipts_restore_in_actual_installation_order() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let outer = unsafe { slot.detach(&slot.current(), String::from("outer placeholder")) }
        .expect("outer detach");
    let inner = unsafe { slot.detach(&slot.current(), String::from("inner placeholder")) }
        .expect("inner detach");
    let failure = outer
        .restore(&slot)
        .expect_err("outer cannot overwrite inner installation");
    assert_eq!(failure.reason, RegistryDetachError::Displaced);
    assert_eq!(slot.borrow().as_str(), "inner placeholder");
    inner
        .restore(&slot)
        .expect("inner restores actual outer placeholder");
    assert_eq!(slot.borrow().as_str(), "outer placeholder");
    let (_, outer) = failure.into_parts();
    outer.restore(&slot).expect("outer restores original");
    assert_eq!(slot.borrow().as_str(), "original");
}

#[test]
fn pentry_e2b_registry_detach_inactive_restore_refuses_live_original_view() {
    let slot = HeapRegistrySlot::new(String::from("original"));
    let original = slot.current();
    let newer = HeapRegistryHandle::new(String::from("newer"));
    slot.install(&newer);
    let receipt = unsafe { slot.detach(&original, String::new()) }.expect("inactive detach");
    let view = original.borrow_mut();
    let failure = receipt
        .restore(&slot)
        .expect_err("inactive receipt still checks original loan");
    assert_eq!(failure.reason, RegistryDetachError::RegistryBorrowed);
    assert_eq!(slot.borrow().as_str(), "newer");
    drop(view);
    let (_, receipt) = failure.into_parts();
    receipt
        .restore(&slot)
        .expect("inactive no-op after original view ends");
    assert!(Arc::ptr_eq(&newer.registry, &slot.current().registry));
}
