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
#[cfg(not(feature = "test-support"))]
mod root_batch;
#[cfg(feature = "test-support")]
pub mod root_batch;
#[cfg(feature = "test-support")]
pub mod root_table;
#[cfg(not(feature = "test-support"))]
mod root_table;
mod shared_root;

pub use immediate::{ImmediateValue, NotImmediate};
pub use root_batch::{
    FinalizedRootBatch, PreparedRootBatch, RetiredRootBatch, RootBatchError,
    RootBatchFinalizeFailure, RootBatchFinishFailure, RootBatchLease, RootBatchPublishFailure,
    RootBatchReader,
};
pub(crate) use root_batch::{
    ParallelRootBatchError, ParallelRootBatchLease, ParallelRootBatchReader, ParallelRootIdentity,
    PreparedParallelRootBatch,
};
/// TEST-SUPPORT-VIS: the moved transport oracle reads the root-table census;
/// root_table itself stays private and cell_census keeps pub(super) in product
/// builds.
#[cfg(feature = "test-support")]
pub use root_table::cell_census;
#[doc(hidden)]
pub use root_table::collect_shared_root_gc_roots;
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) use root_table::root_table_is_alive_for_test;
#[cfg(any(test, feature = "test-support"))]
pub(crate) use root_table::with_root_table_lock_for_test;
pub(crate) use shared_root::{ConsRootSeal, RootSlotOps, RootedConsInputs, cons_into};
pub use shared_root::{LocalRoot, RootedSlot, SharedRoot, SharedRootError};
