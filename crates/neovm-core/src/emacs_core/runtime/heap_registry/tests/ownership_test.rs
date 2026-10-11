use super::*;
use crate::emacs_core::eval::Context;

#[test]
fn gc_tls_ownership_heap_registry_read_guard_prevents_slot_replacement() {
    let slot = HeapRegistrySlot::new(String::from("first"));
    let replacement = HeapRegistryHandle::new(String::from("second"));
    let guard = slot.borrow();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        slot.install(&replacement);
    }));
    assert!(
        result.is_err(),
        "registry installation displaced a borrowed active handle"
    );
    assert_eq!(guard.as_str(), "first");
    drop(guard);
    slot.install(&replacement);
    assert_eq!(slot.borrow().as_str(), "second");
}

#[test]
fn gc_tls_ownership_heap_registry_write_guard_prevents_slot_replacement() {
    let slot = HeapRegistrySlot::new(String::from("first"));
    let replacement = HeapRegistryHandle::new(String::from("second"));
    let mut guard = slot.borrow_mut();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        slot.install(&replacement);
    }));
    assert!(
        result.is_err(),
        "registry installation displaced a mutable active guard"
    );
    guard.push_str(" changed");
    assert_eq!(guard.as_str(), "first changed");
    drop(guard);
    slot.install(&replacement);
    assert_eq!(slot.borrow().as_str(), "second");
}

#[test]
fn gc_tls_ownership_heap_registry_same_heap_reset_preserves_context_handle() {
    let _ctx = Context::new();
    let slot = HeapRegistrySlot::new(String::from("first"));
    let owned = slot.current();
    slot.reset(String::from("reset"));
    assert!(Arc::ptr_eq(&owned.registry, &slot.current().registry));
    assert_eq!(owned.borrow().as_str(), "reset");
}

#[test]
fn gc_tls_ownership_heap_registry_new_heap_reset_preserves_old_context_state() {
    let first = Context::new();
    let slot = HeapRegistrySlot::new(String::from("first"));
    let owned = slot.current();
    let second = Context::new();
    slot.reset(String::from("second"));
    assert_eq!(owned.heap_identity(), first.tagged_heap.identity());
    assert_eq!(
        slot.current().heap_identity(),
        second.tagged_heap.identity()
    );
    assert!(!Arc::ptr_eq(&owned.registry, &slot.current().registry));
    assert_eq!(owned.borrow().as_str(), "first");
    assert_eq!(slot.borrow().as_str(), "second");
}
