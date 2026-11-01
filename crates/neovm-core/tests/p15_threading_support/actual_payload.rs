// Compile the actual lower payload source, not a second identity/schema sketch.
// The header back-reference and crate-private string storage methods need
// compile-only vocabulary glue. This fixture does not validate collector
// tracing or native storage accounting; the production unit tests do that.
// External API visibility cases separately use the real public crate facade.
#![allow(dead_code)]

/// Retain the real string's auto traits and public root/byte APIs. The external
/// crate deliberately keeps storage-origin and capacity queries private, so
/// these compile contracts must not make production methods public to reach
/// them. No fixture constructs this wrapper or executes its storage queries.
#[derive(Debug)]
pub(crate) struct LispString(neovm_core::heap_types::LispString);

impl LispString {
    pub(crate) fn intervals(&self) -> StringRoots<'_> {
        StringRoots(&self.0)
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.0.as_bytes().len()
    }

    pub(crate) fn has_owned_storage(&self) -> bool {
        unreachable!("compile-only string storage-origin glue")
    }

    pub(crate) fn owned_capacity(&self) -> usize {
        unreachable!("compile-only string storage-capacity glue")
    }
}

pub(crate) struct StringRoots<'a>(&'a neovm_core::heap_types::LispString);

impl StringRoots<'_> {
    pub(crate) fn for_each_root(self, mut visit: impl FnMut(tagged::value::TaggedValue)) {
        neovm_core::gc_trace::GcTrace::trace_roots_with(self.0, &mut visit);
    }
}

pub(crate) mod buffer {
    pub use neovm_core::buffer::BufferId;
}

pub(crate) mod tagged {
    pub mod gc {
        pub use neovm_core::tagged::gc::HeapIdentity;
    }
    pub mod value {
        pub use neovm_core::tagged::value::TaggedValue;
    }
    pub mod header {
        pub use neovm_core::tagged::header::{VecLikeHeader, VecLikeType};

        pub(crate) struct MutexObj {
            pub(crate) header: VecLikeHeader,
            pub(crate) payload: crate::threading::MutexPayload,
        }
    }
}

#[path = "../../src/heap_types/threading.rs"]
pub(crate) mod threading;
