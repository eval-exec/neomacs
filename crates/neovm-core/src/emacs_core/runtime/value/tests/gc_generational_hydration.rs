//! Lazy hash-table hydration can install freshly reconstructed heap keys.

use super::*;
use crate::tagged::gc::{TaggedHeap, set_tagged_heap};

struct ScratchRootGuard(usize);

impl ScratchRootGuard {
    fn root(value: Value) -> Self {
        let saved = crate::emacs_core::eval::save_scratch_gc_roots();
        crate::emacs_core::eval::push_scratch_gc_root(value);
        Self(saved)
    }
}

impl Drop for ScratchRootGuard {
    fn drop(&mut self) {
        crate::emacs_core::eval::restore_scratch_gc_roots(self.0);
    }
}

fn heap_with_generational_knob(enabled: bool) -> Box<TaggedHeap> {
    let previous = std::env::var_os("NEOVM_GC_GENERATIONAL");
    // Nextest isolates each test in its own process. Restore the environment
    // immediately after constructing the heap; the knob is read per heap.
    unsafe { std::env::set_var("NEOVM_GC_GENERATIONAL", if enabled { "1" } else { "0" }) };
    let heap = Box::new(TaggedHeap::new());
    unsafe {
        match previous {
            Some(value) => std::env::set_var("NEOVM_GC_GENERATIONAL", value),
            None => std::env::remove_var("NEOVM_GC_GENERATIONAL"),
        }
    }
    heap
}

fn pending_permanent_table(heap: &mut TaggedHeap) -> Value {
    let mut table = LispHashTable::new(HashTableTest::Equal);
    table.set_pending_dump_entries(vec![(
        HashKey::EqualCons(Box::new(HashKey::Int(7)), Box::new(HashKey::Nil)),
        Value::fixnum(11),
        None,
    )]);
    let value = heap.alloc_hash_table(table);
    let header = value.as_veclike_ptr().unwrap() as *mut crate::tagged::header::GcHeader;
    // Model a permanent table loaded before its parked entries are touched.
    // No allocation or safe point occurs between publication and this flag.
    unsafe { (*header).make_permanent() };
    value
}

#[test]
fn gc_generational_hydration_remembers_the_permanent_owner() {
    crate::test_utils::init_test_tracing();
    let mut heap = heap_with_generational_knob(true);
    set_tagged_heap(&mut heap);
    let owner = pending_permanent_table(&mut heap);
    let _owner_root = ScratchRootGuard::root(owner);
    let header = owner.as_veclike_ptr().unwrap() as *const crate::tagged::header::GcHeader;
    assert!(!unsafe { (*header).is_remembered() });
    let table = owner.as_hash_table().unwrap();
    let key = *table.key_snapshots().next().unwrap();
    let _key_root = ScratchRootGuard::root(key);
    assert!(key.is_cons());
    assert_eq!(key.cons_car(), Value::fixnum(7));
    assert!(unsafe { (*header).is_remembered() });
    assert!(heap.is_remembered_for_test(owner));
}

#[test]
fn gc_generational_hydration_knob_off_keeps_the_original_path() {
    crate::test_utils::init_test_tracing();
    let mut heap = heap_with_generational_knob(false);
    set_tagged_heap(&mut heap);
    let owner = pending_permanent_table(&mut heap);
    let _owner_root = ScratchRootGuard::root(owner);
    let header = owner.as_veclike_ptr().unwrap() as *const crate::tagged::header::GcHeader;
    let table = owner.as_hash_table().unwrap();
    assert!(table.key_snapshots().next().unwrap().is_cons());
    assert!(!unsafe { (*header).is_remembered() });
    assert!(!heap.is_remembered_for_test(owner));
}
