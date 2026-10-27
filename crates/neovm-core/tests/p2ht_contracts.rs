//! Compiler contracts for the proof-2 sealed hash-table authority.

#[test]
#[cfg(feature = "contract-fixtures")]
fn p2ht_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/p2ht_ui/*.rs");
}
