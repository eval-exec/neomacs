//! Test-only read surface for core-owned tests of the moved value modules.
//!
//! Peer ruling A1/A3: every item here is written from the signed item table
//! (oracleb + nmjit), named explicitly, and is a read-only accessor or a
//! re-export.  Nothing here widens the product API: the module exists only
//! under the `test-support` feature, which only `neovm-core`'s
//! `[dev-dependencies]` may enable.
//!
//! The owning lanes fill this module after the peer signs the item table.
