//! value-gc B5 regression guard: `HeapPtr::new` requires `unsafe` so safe code
//! in `tagged::gc` cannot send an arbitrary `*mut TaggedHeap` to the collector.
#![allow(dead_code)]

struct TaggedHeap;

#[path = "../../../neovm-value/src/tagged/gc/heap_ptr.rs"]
mod heap_ptr;

fn main() {
    let _ = heap_ptr::HeapPtr::new(std::ptr::null_mut());
}
