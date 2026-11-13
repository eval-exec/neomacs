use super::*;

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
