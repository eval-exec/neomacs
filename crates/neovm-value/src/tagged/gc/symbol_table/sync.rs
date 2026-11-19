//! The production synchronization backend for the N3 publication view.
//!
//! The adapter borrows its atomic root through this parent seam. The metadata
//! lock lives in `gc/publication/sync.rs`, with the generic owner. Isolated
//! Loom tests bind a Loom backend at this same parent seam. This module has
//! no `cfg(loom)` switch.
pub(super) use std::sync::atomic::{AtomicPtr, Ordering};
