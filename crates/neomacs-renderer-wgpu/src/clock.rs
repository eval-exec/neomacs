//! Renderer clock policy.
//!
//! The renderer uses the same compile-target-selected monotonic clock as the
//! editor runtime. Native builds get `crate::clock::Instant`; browser Wasm gets
//! the embedding's installed `performance.now` clock. No renderer API carries
//! or branches on a runtime host identity.

pub(crate) use neovm_core::host::time::Instant;
