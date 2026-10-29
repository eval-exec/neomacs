//! Compiler contracts for real parallel mutator registration authorities.

// Compile the actual positive client as part of this external integration
// crate. Mixing a pass case into trybuild's negative runner changes every
// dependency/client from a metadata check into an executable build.
#[cfg(feature = "contract-fixtures")]
#[path = "p1rv_pass/nmjit_n2_admission.rs"]
mod nmjit_n2_admission;

#[test]
#[cfg(feature = "contract-fixtures")]
fn nmjit_n2_actual_api_compile_contracts() {
    nmjit_n2_admission::main();
}

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
