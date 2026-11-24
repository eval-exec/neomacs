//! U1: safe code cannot collect or drive the sweep protocol.
//!
//! Named `*_compile_contracts` so the nextest trybuild override applies.
//! The target's `required-features` keeps it out of the ordinary suite.

#[test]
#[cfg(feature = "contract-fixtures")]
fn heap_capability_u1_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/heap_capability_ui/*.rs");
}
