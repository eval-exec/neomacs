//! Production synchronization backend for the L6 serial ring and its park/wake.
//!
//! There is no `cfg(loom)` switch here, matching `gc/publication/sync.rs` and
//! `gc/symbol_table/sync.rs`. A Loom target copies the ring control (and this
//! parent seam) and binds the seam to Loom at the same path; production builds
//! see exactly the std types.
#![forbid(unsafe_code)]

pub(in crate::tagged::gc) use std::sync::atomic;
pub(in crate::tagged::gc) use std::sync::{Condvar, Mutex, MutexGuard, TryLockError};
