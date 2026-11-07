//! Cross-heap identity rows for the heap registry: these need a real
//! `Context` per heap, so they stayed with the evaluator when the module
//! moved to `neovm_value::heap_registry` (peer ruling STAGE3B T3).  The
//! Context-free guard rows moved with the module.

use crate::emacs_core::eval::Context;
use crate::emacs_core::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};

#[test]
fn gc_tls_ownership_heap_registry_same_heap_reset_preserves_context_handle() {
    let _ctx = crate::test_utils::new_context();
    let slot = HeapRegistrySlot::new(String::from("first"));
    let owned = slot.current();
    slot.reset(String::from("reset"));
    assert!(HeapRegistryHandle::registry_ptr_eq_for_test(
        &owned,
        &slot.current()
    ));
    assert_eq!(owned.borrow().as_str(), "reset");
}

#[test]
fn gc_tls_ownership_heap_registry_new_heap_reset_preserves_old_context_state() {
    let first = crate::test_utils::new_context();
    let slot = HeapRegistrySlot::new(String::from("first"));
    let owned = slot.current();
    let second = crate::test_utils::new_context();
    slot.reset(String::from("second"));
    assert_eq!(
        owned.heap_identity(),
        first.tagged_heap().tagged_heap_identity()
    );
    assert_eq!(
        slot.current().heap_identity(),
        second.tagged_heap().tagged_heap_identity()
    );
    assert!(!HeapRegistryHandle::registry_ptr_eq_for_test(
        &owned,
        &slot.current()
    ));
    assert_eq!(owned.borrow().as_str(), "first");
    assert_eq!(slot.borrow().as_str(), "second");
}
