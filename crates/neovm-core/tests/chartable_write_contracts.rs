//! The slot capability must not expose a plain store or a growable backing.
#[test]
#[ignore = "compiles separate fixtures; run explicitly with the light gates"]
fn chartable_write_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/chartable_write_ui/*.rs");
}
