//! Compile-fail contracts and paired compiling controls for E4-P2 C1
//! (`OwnerSlabHandle` and `OwnerSlabLease`).
//!
//! Every negative compile-fail fixture has a compiling control beside it in
//! `tests/e4p2_slab_lease_pass/` so each failure is proven to come from the
//! intended borrow-checker or thread-confinement contract rather than a broken
//! import or missing trait bound.

#[test]
#[cfg(feature = "contract-fixtures")]
fn e4p2_slab_lease_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/e4p2_slab_lease_pass/*.rs");
    cases.compile_fail("tests/e4p2_slab_lease_ui/*.rs");
}
