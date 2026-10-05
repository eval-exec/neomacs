//! Compile-time contracts for the thread-safety ownership boundaries.

#[test]
#[ignore = "compiles separate fixtures; run explicitly with the light gates"]
fn thread_safety_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/thread_safety_ui/*.rs");
    // Context's host closure diagnostics name the selected Flow representation.
    // Keep the same transfer contracts with each feature's actual diagnostics.
    #[cfg(feature = "flow-word")]
    cases.compile_fail("tests/thread_safety_ui/flow_word/*.rs");
    #[cfg(not(feature = "flow-word"))]
    cases.compile_fail("tests/thread_safety_ui/plain_flow/*.rs");
}
