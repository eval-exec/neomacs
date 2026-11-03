//! Test-only read surface for core-owned tests of the moved value modules.
//!
//! Peer ruling A1/A3: every item here is written from the signed item table
//! (oracleb + nmjit), named explicitly, and is a read-only accessor or a
//! re-export.  Nothing here widens the product API: the module exists only
//! under the `test-support` feature, which only `neovm-core`'s
//! `[dev-dependencies]` may enable.
//!
//! The owning lanes fill this module after the peer signs the item table.

use crate::heap_types::LispString;
use crate::tagged::header::GcHeader;

// A3 v2 / R8 payload rows (kept under R8 + S3): copied scalars, copied pointer,
// owned alias snapshot, and the canonical borrowed-bytes delegation.
pub fn heap_types__LispString__data_ptr(s: &LispString) -> *const u8 {
    crate::heap_types::__test_support::lisp_string_data_ptr(s)
}

pub fn heap_types__LispString__size(s: &LispString) -> usize {
    crate::heap_types::__test_support::lisp_string_size(s)
}

pub fn heap_types__LispString__owned_capacity(s: &LispString) -> usize {
    crate::heap_types::__test_support::lisp_string_owned_capacity(s)
}

pub fn heap_types__LispString__has_owned_storage(s: &LispString) -> bool {
    crate::heap_types::__test_support::lisp_string_has_owned_storage(s)
}

pub fn heap_types__LispString__has_trailing_nul(s: &LispString) -> bool {
    crate::heap_types::__test_support::lisp_string_has_trailing_nul(s)
}

pub fn heap_types__LispString__character_count_kind(
    s: &LispString,
) -> crate::heap_types::StringCharacterCountKind {
    crate::heap_types::__test_support::lisp_string_character_count_kind(s)
}

pub fn heap_types__LispString__borrowed_alias(s: &LispString) -> Option<LispString> {
    crate::heap_types::__test_support::lisp_string_borrowed_alias(s)
}

/// # Safety
/// Same pointer/length/validation contract as [`LispString::from_borrowed_bytes`].
pub unsafe fn heap_types__LispString__from_borrowed_bytes(
    ptr: *const u8,
    len: usize,
    size: usize,
    size_byte: i64,
    static_rodata: bool,
) -> LispString {
    unsafe {
        crate::heap_types::__test_support::lisp_string_from_borrowed_bytes(
            ptr, len, size, size_byte, static_rodata,
        )
    }
}

// A3 v2: copied read-only GcHeader observation for the relocated clear_string test.
pub fn tagged__header__GcHeader__collection_observed(header: &GcHeader) -> bool {
    crate::tagged::header::__test_support::gc_header_collection_observed(header)
}
