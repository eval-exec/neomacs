//! Test-only surfaces for core-owned tests of the moved text fixtures.
//!
//! Writes and resets stay in the signed `authority` module: the originals are
//! plain `pub(crate)` with no visibility widening, and the fence is their only
//! cross-crate route. Reads use the TEST-SUPPORT-VIS rows instead.

#[cfg(feature = "test-support")]
pub mod authority;
