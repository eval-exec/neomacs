//! Compile-time contracts for the thread-safety ownership boundaries.

#[test]
#[ignore = "compiles separate fixtures; run explicitly with the light gates"]
fn thread_safety_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/thread_safety_ui/*.rs");
}
