//! A fake pdump image for tests: one contiguous allocation holding an image
//! cons and an image vector, registered the way the pdump loader registers
//! image objects (extending the dump span, which turns the partition on).
//!
//! One allocation matters: the dump span is `[lowest, highest)` over every
//! registered object, and the collector treats anything inside it as an
//! image object. Two separately boxed fakes can lie far apart in the
//! malloc heap, and the span between them then swallows ordinary heap
//! objects, which are never marked and get swept while live.

use super::*;
use crate::tagged::header::{ConsCdrOrNext, LispValueVec, VecLikeHeader, VecLikeType, VectorObj};

/// The image's objects, adjacent in memory.
#[repr(C)]
pub(crate) struct FakeImage {
    cons: [ConsCell; 1],
    vector: VectorObj,
    /// Backing for a vector whose slots are mapped storage (a store must
    /// copy them first); `vector`'s own slots are owned.
    mapped_slots: [TaggedValue; 3],
}

impl FakeImage {
    /// A fresh image, leaked for the life of the process (image memory is
    /// never freed).
    pub(crate) fn leak(mapped_storage: bool) -> &'static mut FakeImage {
        let image = Box::leak(Box::new(FakeImage {
            cons: [ConsCell {
                car: TaggedValue::fixnum(1),
                cdr_or_next: ConsCdrOrNext {
                    cdr: TaggedValue::NIL,
                },
            }],
            vector: VectorObj {
                header: VecLikeHeader::new(VecLikeType::Vector),
                data: LispValueVec::owned(vec![TaggedValue::NIL; 2]),
            },
            mapped_slots: [TaggedValue::fixnum(9); 3],
        }));
        if mapped_storage {
            let slots = image.mapped_slots.as_ptr();
            // SAFETY: the slots live as long as the (leaked) image.
            image.vector.data = unsafe { LispValueVec::mapped(slots, 3) };
        }
        image
    }

    /// Register the image cons with `heap` and answer it.
    pub(crate) fn register_cons(&mut self, heap: &mut TaggedHeap) -> TaggedValue {
        let cell = self.cons.as_mut_ptr();
        unsafe { heap.register_mapped_cons_range(cell, 1) };
        unsafe { TaggedValue::from_cons_ptr(cell) }
    }

    /// Register the image vector with `heap` and answer it.
    pub(crate) fn register_vector(&mut self, heap: &mut TaggedHeap) -> TaggedValue {
        let obj = &mut self.vector as *mut VectorObj;
        unsafe {
            heap.register_mapped_veclike_object(obj as *mut VecLikeHeader, size_of::<VectorObj>())
        };
        unsafe { TaggedValue::from_veclike_ptr(obj as *const VecLikeHeader) }
    }
}
