//! Production synchronization backend for the generic publication owner.
//!
//! The owner imports this parent seam. There is no `cfg(loom)` switch here.
pub(super) use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
pub(super) use std::sync::{Mutex, MutexGuard, TryLockError};
