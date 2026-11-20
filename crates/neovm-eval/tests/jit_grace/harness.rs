//! Compile-fail contracts for the serial JIT grace receipt.
//!
//! Named `*_compile_contracts` so the nextest overrides for trybuild
//! runners (compiler memory wrapper, all test threads, 900s slow-timeout)
//! apply to it.

#[test]
fn jit_grace_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/jit_grace_ui/*.rs");
}
