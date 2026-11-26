//! Phase 1c.0 U3: nesting a `HeapMut` bulk mutation closure fails with E0499.
use neovm_value::tagged::gc::TaggedHeap;
use neovm_value::value::Value;

fn reenter_hash_table_mut(heap: &mut TaggedHeap, v: Value) {
    let mut hm = heap.heap_mut();
    let _ = v.with_hash_table_mut_in(&mut hm, |_outer| {
        let _ = v.with_hash_table_mut_in(&mut hm, |_inner| ());
    });
}

fn main() {
    let _ = reenter_hash_table_mut;
}
