//! U1-U3: safe code cannot collect, drive the sweep protocol, escape a
//! place-anchored borrow, hold a `HeapRef` borrow across collection/teardown/
//! bulk replacement, or re-enter a `HeapMut` bulk mutator.
//!
//! Named `*_compile_contracts` so the nextest trybuild override applies.
//! The target's `required-features` keeps it out of the ordinary suite.
//! Keeps a single runner so `scripts/check_compile_contracts.py` `EXPECTED`
//! remains unchanged.

#[test]
#[cfg(feature = "contract-fixtures")]
fn heap_capability_u1_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/heap_capability_pass/*.rs");
    cases.compile_fail("tests/heap_capability_ui/*.rs");
}
