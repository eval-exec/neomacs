//! Production synchronization seam for the L6 rendezvous additions.
//!
//! There is no `cfg(loom)` switch here, matching `gc/publication/sync.rs` and
//! `gc/symbol_table/sync.rs`. The L6.0 ring control itself uses no
//! synchronization (it runs under the canonical registry gate); this seam
//! currently carries the atomic module the keeper/resume paths read. The
//! product park/wake primitives are added here when L6.7 copies
//! `gate.rs`/`mod.rs`; L6.0's Loom model does not bind this seam (it drives the
//! product state under the test's own Loom Mutex/Condvar).
#![forbid(unsafe_code)]

pub(in crate::tagged::gc) use std::sync::atomic;
