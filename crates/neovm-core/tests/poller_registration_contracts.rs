//! Compile-fail contracts for B5 poller registration, `LiveProcessIo`, and
//! `ProcessManager` descriptor ownership boundaries.
//!
//! Named `*_compile_contracts` so `.config/nextest.toml`'s trybuild overrides
//! (compiler memory wrapper, exclusive test threads, 900s slow-timeout) apply.
//! Included from `thread_safety_contracts` so the CI contract job runs it.

#[test]
#[cfg(feature = "contract-fixtures")]
fn poller_registration_compile_contracts() {
    use super::fixture_target::FixtureTarget;

    let cases = trybuild::TestCases::new();
    cases.pass("../neovm-system/tests/poller_registration_pass/*.rs");
    for fixture in FixtureTarget::current().fixtures(
        "../neovm-system/tests/poller_registration_ui",
        "../neovm-system/tests/poller_registration_ui/arm64_macos",
    ) {
        cases.compile_fail(fixture);
    }
}
