//! Compiler-enforced privacy of test-support retained native leaves.

#[test]
#[cfg(all(feature = "jit", feature = "contract-fixtures"))]
fn retained_leaf_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/retained_leaf_pass/*.rs");
    cases.compile_fail("tests/retained_leaf_ui/*.rs");
}
