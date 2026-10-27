//! Compiler contracts for the Part A root-batch and collector authorities.
//!
//! Kept separate from the inherited thread-safety fixtures so both flows can
//! run these contracts explicitly without changing the existing goldens.

#[test]
#[cfg(feature = "contract-fixtures")]
fn p0s3_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/p0s3_ui/*.rs");
}
