//! Compiler contracts for actual dormant intern read-domain/slot sources.
#[test]
#[cfg(feature = "contract-fixtures")]
fn intern_shared_scope_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/intern_shared_pass/*.rs");
    cases.compile_fail("tests/intern_shared_ui/*.rs");
}
