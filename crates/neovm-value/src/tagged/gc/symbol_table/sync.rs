//! The production synchronization backend for symbol-table publication.
//!
//! Isolated Loom tests bind a Loom backend at this same parent seam. The
//! publication algorithm is not copied, and this module has no `cfg(loom)`
//! switch.
pub(super) use std::sync::atomic::{AtomicPtr, Ordering};
pub(super) use std::sync::{Mutex, MutexGuard, TryLockError};
