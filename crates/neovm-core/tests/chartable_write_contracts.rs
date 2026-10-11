//! The slot capability must not expose a plain store or a growable backing.
#[test]
fn chartable_write_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/chartable_write_ui/*.rs");
}
