//! Lower NeoVM runtime utilities shared with `neovm-core`.
//!
//! This crate is the extraction target for the value runtime. It must never
//! depend on `neovm-core` or on a UI/display crate; it starts with the
//! thread-local restoration guard that lower value code and upper consumers
//! both use.

pub mod logging;
pub mod tls_scope;

/// Test-only read surface for core-owned tests of the moved modules.
///
/// The contents are written by the owning lanes from the peer-signed item
/// table; this skeleton declares the module and the feature so the manifest
/// and profile gates exist before the items land.  Additive only: no product
/// item's visibility changes under this feature.
#[cfg(feature = "test-support")]
pub mod test_support;
