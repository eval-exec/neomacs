//! The slot capability must not expose a plain store or a growable backing.

#[cfg(feature = "contract-fixtures")]
#[path = "thread_safety_ui/support/fixture_target.rs"]
mod fixture_target;

#[test]
#[cfg(feature = "contract-fixtures")]
fn chartable_write_compile_contracts() {
    let cases = trybuild::TestCases::new();
    // A target whose diagnostic differs supplies `arm64_macos/<name>.rs`.
    for fixture in fixture_target::FixtureTarget::current().fixtures(
        "tests/chartable_write_ui",
        "tests/chartable_write_ui/arm64_macos",
    ) {
        cases.compile_fail(fixture);
    }
}
