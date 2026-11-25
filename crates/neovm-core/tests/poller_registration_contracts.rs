//! Compile-fail contracts for B5 poller registration and `LiveProcessIo`
//! descriptor ownership boundaries.
//!
//! Named `*_compile_contracts` so `.config/nextest.toml`'s trybuild overrides
//! (compiler memory wrapper, exclusive test threads, 900s slow-timeout) apply.
//! Included from `thread_safety_contracts` so the CI contract job runs it.

#[test]
#[cfg(feature = "contract-fixtures")]
fn poller_registration_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("../neovm-system/tests/poller_registration_pass/*.rs");
    cases.compile_fail("../neovm-system/tests/poller_registration_ui/*.rs");
}
