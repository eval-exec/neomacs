//! Typed transport of Lisp values across threads.
//!
//! A raw `TaggedValue` is valid only on the mutator that keeps it reachable.
//! Values leave a mutator in one of two validated forms:
//! - [`ImmediateValue`]: fixnums, `nil` and `t`, which need no root and
//!   mean the same thing in every heap;
//! - [`SharedRoot`]: any value, rooted in its heap's root table until the last
//!   clone drops, and materialized back only on a mutator of that heap.
//!
//! Collector-internal work queues use their own collector word instead.

mod immediate;
mod root_batch;
mod root_table;
mod shared_root;

pub use immediate::{ImmediateValue, NotImmediate};
pub use root_batch::{
    FinalizedRootBatch, PreparedRootBatch, RetiredRootBatch, RootBatchError,
    RootBatchFinalizeFailure, RootBatchFinishFailure, RootBatchLease, RootBatchPublishFailure,
    RootBatchReader,
};
pub(crate) use root_table::collect_shared_root_gc_roots;
pub use shared_root::{LocalRoot, SharedRoot, SharedRootError};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
