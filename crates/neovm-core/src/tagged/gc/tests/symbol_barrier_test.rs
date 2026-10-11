//! Symbol edges must reach the barrier even though bare symbols are not pointers.

use super::*;
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::intern::{intern_uninterned, is_canonical_id};
use crate::emacs_core::value::{HashTableTest, LispHashTable};

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

fn weak_symbol_key_survives_full_collections(mapped: bool, with_position: bool) {
    let mut heap = TaggedHeap::new();
    heap.generational.enabled = false;
    set_tagged_heap(&mut heap);
    let roots = ScratchRoots::new();
    // Register a real image span so the first full cycle blackens the partition.
    let image = fake_image::FakeImage::leak(false);
    let image_vector = image.register_vector(&mut heap);
    roots.keep(image_vector);
    let owner = if mapped {
        image_vector
    } else {
        heap.alloc_vector(vec![TaggedValue::NIL])
    };
    roots.keep(owner);
    heap.collect_exact(std::iter::once(owner));
    assert!(heap.dump_blackened);
    assert!(!heap.mapped_remembered.contains(&owner.bits()));

    let id = intern_uninterned("u34-review-weak-symbol-key");
    assert!(!is_canonical_id(id));
    let symbol = TaggedValue::from_sym_id(id);
    let strong_value = if with_position {
        heap.alloc_symbol_with_pos(symbol, TaggedValue::fixnum(31))
    } else {
        symbol
    };
    assert!(crate::tagged::mutate::set_vector_slot(
        owner,
        0,
        strong_value
    ));
    let table = heap.alloc_hash_table(LispHashTable::new_with_options(
        HashTableTest::Eq,
        4,
        Some(HashTableWeakness::Key),
        1.5,
        0.8,
    ));
    roots.keep(table);
    crate::tagged::mutate::with_hash_table_mut(table, |table| {
        table.insert(
            symbol.to_hash_key(&HashTableTest::Eq),
            symbol,
            TaggedValue::fixnum(37),
        );
    });
    for _ in 0..3 {
        heap.collect_exact([owner, table].into_iter());
        assert_eq!(
            table.as_hash_table().unwrap().data.len(),
            1,
            "the strong owner must keep its uninterned weak key alive"
        );
        assert!(heap.marked_symbols.contains(id));
        assert_eq!(owner.as_vector_data().unwrap().as_slice()[0], strong_value);
        assert!(heap.current_mutator_gc().remset.is_empty());
    }
}

#[test]
fn generational_off_permanent_owner_keeps_uninterned_weak_symbol_key() {
    weak_symbol_key_survives_full_collections(false, false);
}

#[test]
fn generational_off_mapped_owner_keeps_uninterned_weak_symbol_key() {
    weak_symbol_key_survives_full_collections(true, false);
}

#[test]
fn generational_off_symbol_with_pos_keeps_its_bare_weak_symbol_key() {
    for mapped in [false, true] {
        weak_symbol_key_survives_full_collections(mapped, true);
    }
}
