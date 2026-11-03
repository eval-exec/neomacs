//! Lower NeoVM runtime utilities shared with `neovm-core`.
//!
//! This crate is the extraction target for the value runtime. It must never
//! depend on `neovm-core` or on a UI/display crate; it starts with the
//! thread-local restoration guard that lower value code and upper consumers
//! both use.

pub mod logging;
pub mod tls_scope;
