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

fn assert_owned_old_hydrated_key(heap: &TaggedHeap, owner: Value, key_address: usize) {
    assert!(heap.owns_heap_value_for_test(owner));
    let key =
        unsafe { Value::from_cons_ptr(key_address as *const crate::tagged::header::ConsCell) };
    assert!(heap.owns_heap_value_for_test(key));
    assert!(heap.value_is_old_for_test(owner));
    assert!(heap.value_is_old_for_test(key));
    {
        let table = owner.as_hash_table().unwrap();
        assert!(!table.needs_hydration());
        assert_eq!(table.data.len(), 1);
        assert_eq!(*table.key_snapshots().next().unwrap(), key);
        let entry = table.data.entries_in_slot_order().next().unwrap();
        assert_eq!(entry.key, key);
        assert_eq!(entry.value, Value::fixnum(11));
    }
    assert_eq!(key.cons_car(), Value::fixnum(7));
    assert_eq!(key.cons_cdr(), Value::NIL);
}

#[test]
fn gc_generational_hydration_old_owner_keeps_key_through_minors_and_majors() {
    crate::test_utils::init_test_tracing();
    let mut heap = heap_with_generational_knob(true);
    set_tagged_heap(&mut heap);
    assert!(heap.generational_enabled());
    assert!(!heap.dump_partition_active());
    let mut table = LispHashTable::new(HashTableTest::Equal);
    table.set_pending_dump_entries(vec![(
        HashKey::EqualCons(Box::new(HashKey::Int(7)), Box::new(HashKey::Nil)),
        Value::fixnum(11),
        None,
    )]);
    let owner = heap.alloc_hash_table(table);
    let owner_root = ScratchRootGuard::root(owner);
    let owner_pointer =
        owner.as_veclike_ptr().unwrap() as *const crate::tagged::header::HashTableObj;

    // Real full tracing reads the parked entries without materializing keys.
    // Unlike pending_permanent_table, this Box stays on the correct collector
    // list and acquires ordinary-old age through a real completed major.
    heap.collect_exact(std::iter::once(owner));
    assert!(heap.owns_heap_value_for_test(owner));
    assert!(heap.value_is_old_for_test(owner));
    {
        let object = unsafe { &*owner_pointer };
        assert!(object.header.gc.tenured);
        assert!(!object.header.gc.generation.permanent());
        assert!(!object.header.gc.is_remembered());
        assert!(object.table.needs_hydration());
    }
    assert_eq!(heap.remembered_log_len_for_test(), 0);

    // Access through the actual API hydrates a fresh structural cons key.
    // Save only its address and end the table borrow before any collection.
    let key_address = {
        let table = owner.as_hash_table().unwrap();
        assert!(!table.needs_hydration());
        let key = *table.key_snapshots().next().unwrap();
        assert!(key.is_cons());
        assert!(heap.owns_heap_value_for_test(key));
        assert!(!heap.value_is_old_for_test(key));
        assert_eq!(key.cons_car(), Value::fixnum(7));
        assert_eq!(key.cons_cdr(), Value::NIL);
        key.as_cons_ptr().unwrap() as usize
    };
    assert!(heap.is_remembered_for_test(owner));
    assert_eq!(heap.remembered_log_len_for_test(), 1);

    heap.begin_minor_collection();
    heap.seed_root(owner);
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
    assert_owned_old_hydrated_key(&heap, owner, key_address);
    assert_eq!(heap.remembered_log_len_for_test(), 0);

    // The owner is the only explicit root; the key has no scratch root and
    // no recent R edge can repair a missed ordinary-old owner trace.
    for _ in 0..3 {
        assert_eq!(heap.remembered_log_len_for_test(), 0);
        heap.collect_exact(std::iter::once(owner));
        assert_owned_old_hydrated_key(&heap, owner, key_address);
        assert_eq!(heap.remembered_log_len_for_test(), 0);
    }

    drop(owner_root);
    heap.collect_exact(std::iter::empty());
    // Ownership queries only: neither dead payload is dereferenced, and no
    // further allocation can reuse either slot before these assertions.
    assert!(!heap.owns_heap_value_for_test(owner));
    let key =
        unsafe { Value::from_cons_ptr(key_address as *const crate::tagged::header::ConsCell) };
    assert!(!heap.owns_heap_value_for_test(key));
}
