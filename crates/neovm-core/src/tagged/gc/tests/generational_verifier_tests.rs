//! Minor termination detects missed barriers before promotion or reclamation.

use super::*;
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::intern::intern_uninterned;
use crate::emacs_core::value::{HashTableTest, LispHashTable};
use crate::heap_types::LispString;
use std::ffi::OsString;
use std::panic::{AssertUnwindSafe, catch_unwind};

struct ScratchRoots(usize);

impl ScratchRoots {
    fn new() -> Self {
        Self(save_scratch_gc_roots())
    }

    fn keep(&self, value: TaggedValue) {
        push_scratch_gc_root(value);
    }
}

impl Drop for ScratchRoots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn verified_heap() -> TaggedHeap {
    let mut heap = TaggedHeap::new();
    heap.generational.enabled = true;
    heap.generational.verify = true;
    // The independent verifier must work without a blackened dump partition.
    heap.partition_dump = false;
    heap.dump_blackened = false;
    heap.publish_barrier_window();
    heap
}

fn minor(heap: &mut TaggedHeap, roots: &[TaggedValue]) {
    heap.begin_minor_collection();
    for &root in roots {
        heap.seed_root(root);
    }
    heap.complete_minor_collection();
    heap.finish_incremental_sweep_now();
}

#[derive(Clone, Copy, Debug)]
enum OwnerClass {
    Cons,
    ArenaVector,
    BoxHashTable,
}

fn missed_barrier_is_detected_before_sweep(class: OwnerClass) {
    let mut heap = verified_heap();
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let owner = match class {
        OwnerClass::Cons => heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL),
        OwnerClass::ArenaVector => heap.alloc_vector(vec![TaggedValue::NIL]),
        OwnerClass::BoxHashTable => heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq)),
    };
    roots.keep(owner);
    minor(&mut heap, &[owner]);
    assert!(heap.value_is_old_for_test(owner));
    if matches!(class, OwnerClass::BoxHashTable) {
        assert_eq!(
            heap.generational.old_objects as usize,
            TaggedHeap::value_heap_addr(owner).unwrap(),
            "the verifier must walk the Box old list through gc_link"
        );
    }
    let child = heap.alloc_string(LispString::from_utf8("missed-barrier-young-child"));
    let child_ptr = child.as_string_ptr().unwrap().cast();
    heap.begin_minor_collection();
    heap.seed_root(owner);
    assert!(!heap.is_value_marked(child));
    // Inject after begin: the current-child backstop at cycle start must not
    // repair the deliberately missing barrier before the verifier sees it.
    // No Lisp allocation or safepoint occurs after this unrooted child exists.
    unsafe {
        match class {
            OwnerClass::Cons => (*(owner.xcons_ptr() as *mut ConsCell)).set_car(child),
            OwnerClass::ArenaVector => {
                let ptr = owner.as_veclike_ptr().unwrap() as *mut VectorObj;
                (*ptr).data.store_atomic(0, child);
            }
            OwnerClass::BoxHashTable => {
                let ptr = owner.as_veclike_ptr().unwrap() as *mut HashTableObj;
                (*ptr).table.insert(
                    TaggedValue::T.to_hash_key(&HashTableTest::Eq),
                    TaggedValue::T,
                    child,
                );
            }
        }
    }
    assert_eq!(heap.remembered_log_len_for_test(), 0);
    assert!(!heap.partition_dump);
    let panic = catch_unwind(AssertUnwindSafe(|| heap.complete_minor_collection()))
        .expect_err("a missed old-to-young barrier must fail at minor termination");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("verification panic has a string message");
    assert!(message.contains("verification"), "{class:?}: {message}");
    assert!(
        message.contains("1 unmarked heap children"),
        "{class:?}: {message}"
    );
    assert!(
        heap.owns_string_object(child_ptr),
        "the detector must fire before freeing its young child"
    );
    assert!(!heap.value_is_old_for_test(child));
    assert!(!heap.is_value_marked(child));
    assert!(!heap.sweep_in_progress());
}

#[test]
fn generational_verifier_automatically_rejects_unlogged_old_cons_edge() {
    missed_barrier_is_detected_before_sweep(OwnerClass::Cons);
}

#[test]
fn generational_verifier_automatically_rejects_unlogged_old_arena_edge() {
    missed_barrier_is_detected_before_sweep(OwnerClass::ArenaVector);
}

#[test]
fn generational_verifier_automatically_rejects_unlogged_old_box_edge() {
    missed_barrier_is_detected_before_sweep(OwnerClass::BoxHashTable);
}

#[test]
fn generational_verifier_accepts_barriered_old_cons_arena_and_box_edges() {
    let mut heap = verified_heap();
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let cons = heap.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    roots.keep(cons);
    let vector = heap.alloc_vector(vec![TaggedValue::NIL; 2]);
    roots.keep(vector);
    let table = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
    roots.keep(table);
    minor(&mut heap, &[cons, vector, table]);

    {
        let child = heap.alloc_string(LispString::from_utf8("old-cons-child"));
        assert!(crate::tagged::mutate::set_cons_car(cons, child));
    }
    assert!(crate::tagged::mutate::set_cons_cdr(
        cons,
        TaggedValue::fixnum(211)
    ));
    let symbol = TaggedValue::from_sym_id(intern_uninterned("u34-verifier-old-symbol"));
    assert!(crate::tagged::mutate::set_vector_slot(vector, 0, symbol));
    {
        let child = heap.alloc_symbol_with_pos(symbol, TaggedValue::fixnum(223));
        assert!(crate::tagged::mutate::set_vector_slot(vector, 1, child));
    }
    {
        let child = heap.alloc_cons(TaggedValue::fixnum(227), TaggedValue::NIL);
        crate::tagged::mutate::with_hash_table_mut(table, |table| {
            table.insert(
                TaggedValue::T.to_hash_key(&HashTableTest::Eq),
                TaggedValue::T,
                child,
            );
        })
        .expect("the old Box owner is a hash table");
    }
    assert_eq!(heap.remembered_log_len_for_test(), 3);
    minor(&mut heap, &[cons, vector, table]);
    assert!(!heap.partition_dump);
    assert!(heap.value_is_old_for_test(cons.cons_car()));
    assert_eq!(cons.cons_cdr(), TaggedValue::fixnum(211));
    let slots = vector.as_vector_data().unwrap().as_slice();
    assert_eq!(slots[0], symbol);
    assert!(heap.value_is_old_for_test(slots[1]));
    assert_eq!(slots[1].as_symbol_with_pos().unwrap().sym, symbol);
    let table = table.as_hash_table().unwrap();
    let entry = table.data.entries_in_slot_order().next().unwrap();
    assert!(heap.value_is_old_for_test(entry.value));
    assert_eq!(entry.value.cons_car(), TaggedValue::fixnum(227));
}

#[test]
fn generational_verifier_accepts_mapped_and_permanent_owners() {
    let mut heap = verified_heap();
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    let image = fake_image::FakeImage::leak(false);
    let mapped = image.register_vector(&mut heap);
    roots.keep(mapped);
    let permanent = heap.alloc_vector(vec![TaggedValue::NIL; 2]);
    roots.keep(permanent);
    heap.collect_exact([mapped, permanent].into_iter());
    let header = TaggedHeap::value_heap_addr(permanent).unwrap() as *const GcHeader;
    assert!(unsafe { (*header).generation.permanent() });
    assert!(heap.dump_blackened);

    {
        let child = heap.alloc_cons(TaggedValue::fixnum(229), TaggedValue::NIL);
        assert!(crate::tagged::mutate::set_vector_slot(mapped, 0, child));
    }
    let symbol = TaggedValue::from_sym_id(intern_uninterned("u34-verifier-mapped-symbol"));
    assert!(crate::tagged::mutate::set_vector_slot(mapped, 1, symbol));
    {
        let child = heap.alloc_symbol_with_pos(symbol, TaggedValue::fixnum(233));
        assert!(crate::tagged::mutate::set_vector_slot(permanent, 0, child));
    }
    assert!(crate::tagged::mutate::set_vector_slot(permanent, 1, mapped));
    // R must suffice for the minor even with the persistent full-cycle seeds
    // cleared. Neither young child is independently rooted by this test.
    heap.mapped_remembered.clear();
    minor(&mut heap, &[]);
    let mapped_slots = mapped.as_vector_data().unwrap().as_slice();
    assert!(heap.value_is_old_for_test(mapped_slots[0]));
    assert_eq!(mapped_slots[0].cons_car(), TaggedValue::fixnum(229));
    assert_eq!(mapped_slots[1], symbol);
    let permanent_slots = permanent.as_vector_data().unwrap().as_slice();
    assert!(heap.value_is_old_for_test(permanent_slots[0]));
    assert_eq!(permanent_slots[0].as_symbol_with_pos().unwrap().sym, symbol);
    assert_eq!(permanent_slots[1], mapped);
}

struct RestoreEnvironment(Vec<(&'static str, Option<OsString>)>);

impl Drop for RestoreEnvironment {
    fn drop(&mut self) {
        for (name, value) in &self.0 {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}

#[test]
fn generational_verifier_knob_is_constructor_read_and_disabled_with_generations_off() {
    // Nextest isolates each test in its own process. No collector thread is
    // started here; restore both variables even if an assertion panics.
    let _restore = RestoreEnvironment(
        ["NEOVM_GC_GENERATIONAL", "NEOVM_GC_VERIFY_GENERATIONAL"]
            .into_iter()
            .map(|name| (name, std::env::var_os(name)))
            .collect(),
    );
    unsafe {
        std::env::set_var("NEOVM_GC_GENERATIONAL", "1");
        std::env::set_var("NEOVM_GC_VERIFY_GENERATIONAL", "1");
    }
    let enabled = TaggedHeap::new();
    assert!(enabled.generational.enabled);
    assert!(enabled.generational.verify);
    unsafe { std::env::set_var("NEOVM_GC_VERIFY_GENERATIONAL", "0") };
    assert!(
        enabled.generational.verify,
        "a live heap keeps its constructor setting"
    );
    assert!(!TaggedHeap::new().generational.verify);
    unsafe {
        std::env::set_var("NEOVM_GC_GENERATIONAL", "0");
        std::env::set_var("NEOVM_GC_VERIFY_GENERATIONAL", "1");
    }
    let disabled = TaggedHeap::new();
    assert!(!disabled.generational.enabled);
    assert!(!disabled.generational.verify);
}
