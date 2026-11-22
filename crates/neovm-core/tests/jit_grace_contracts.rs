//! Compile-fail contracts for the serial JIT grace receipt.
//!
//! Named `*_compile_contracts` so the nextest overrides for trybuild
//! runners (compiler memory wrapper, all test threads, 900s slow-timeout)
//! apply. The fixtures stay beside the receipt in neovm-eval; this runner
//! is `#[path]`-included from `thread_safety_contracts` so the six-target
//! CI command already counts it.

#[test]
#[cfg(feature = "contract-fixtures")]
fn jit_grace_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("../neovm-eval/tests/jit_grace_ui/*.rs");
}
