//! Test-only read surface for core-owned tests of the moved value modules.
//!
//! Peer ruling A1/A3: every item here is written from the signed item table
//! (oracleb + nmjit), named explicitly, and is a read-only accessor or a
//! re-export.  Nothing here widens the product API: the module exists only
//! under the `test-support` feature, which only `neovm-core`'s
//! `[dev-dependencies]` may enable.
//!
//! The owning lanes fill this module after the peer signs the item table.

/// Signed Prep-D R6 TA2 authority fence (peer 05:5xZ-b / 06:0xZ). Nameable
/// only from `crates/neovm-core/core_tests/**`; every item is one
/// AUTHORITY-ITEMS.tsv row signed before code.
pub mod authority;

use crate::heap_types::LispString;

// VALUE-A3-COLLAPSE (peer 00:36:00Z): the A3 v2/R8 payload rows collapsed onto
// their already-signed field / A-prime / production routes (the consumer now
// reads `data`, `size`, `owned_capacity()`, `has_trailing_nul()`,
// `borrowed_alias()`, `character_count_kind()` and `collection_observed()`
// directly). Only the owned-storage predicate has no other route, so it stays
// here as a copied read. `from_borrowed_bytes` moved to the AUTHORITY fence
// because it mints over caller-supplied raw memory.
pub fn heap_types__LispString__has_owned_storage(s: &LispString) -> bool {
    crate::heap_types::__test_support::lisp_string_has_owned_storage(s)
}
