//! Compiler contracts for the Part A root-batch and collector authorities.
//!
//! Kept separate from the inherited thread-safety fixtures so both flows can
//! run these contracts explicitly without changing the existing goldens.

#[cfg(feature = "contract-fixtures")]
#[path = "thread_safety_ui/support/fixture_target.rs"]
mod fixture_target;

#[test]
#[cfg(feature = "contract-fixtures")]
fn p0s3_compile_contracts() {
    let cases = trybuild::TestCases::new();
    // A target whose diagnostic differs supplies `arm64_macos/<name>.rs`.
    for fixture in fixture_target::FixtureTarget::current()
        .fixtures("tests/p0s3_ui", "tests/p0s3_ui/arm64_macos")
    {
        cases.compile_fail(fixture);
    }
}
