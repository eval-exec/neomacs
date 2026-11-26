//! value-gc B5 regression guard: `HeapPtr`'s inner `*mut TaggedHeap` field must
//! remain private so safe code in `tagged::gc` cannot construct `HeapPtr(ptr)`.
#![allow(dead_code)]

struct TaggedHeap;

#[path = "../../../neovm-value/src/tagged/gc/heap_ptr.rs"]
mod heap_ptr;

fn main() {
    let _ = heap_ptr::HeapPtr(std::ptr::null_mut());
}
