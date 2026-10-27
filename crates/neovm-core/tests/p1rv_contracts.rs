//! Compiler contracts for real parallel mutator registration authorities.

#[test]
#[cfg(feature = "contract-fixtures")]
fn p1rv_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/p1rv_ui/*.rs");
}

#[test]
#[cfg(feature = "contract-fixtures")]
fn p3sym_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/p1rv_ui/p3sym_*.rs");
}
