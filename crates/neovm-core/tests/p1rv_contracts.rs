//! Compiler contracts for real parallel mutator registration authorities.

#[cfg(feature = "contract-fixtures")]
#[path = "thread_safety_ui/support/fixture_target.rs"]
mod fixture_target;

// Compile the actual positive client as part of this external integration
// crate. Mixing a pass case into trybuild's negative runner changes every
// dependency/client from a metadata check into an executable build.
#[cfg(feature = "contract-fixtures")]
#[path = "p1rv_pass/nmjit_n2_admission.rs"]
mod nmjit_n2_admission;

#[test]
#[cfg(feature = "contract-fixtures")]
fn nmjit_n2_actual_api_compile_contracts() {
    nmjit_n2_admission::main();
}

/// The p1rv fixtures for this target; a target whose diagnostic differs
/// supplies `arm64_macos/<name>.rs`.
#[cfg(feature = "contract-fixtures")]
fn p1rv_fixtures() -> Vec<std::path::PathBuf> {
    fixture_target::FixtureTarget::current().fixtures("tests/p1rv_ui", "tests/p1rv_ui/arm64_macos")
}

#[test]
#[cfg(feature = "contract-fixtures")]
fn p1rv_compile_contracts() {
    let cases = trybuild::TestCases::new();
    for fixture in p1rv_fixtures() {
        cases.compile_fail(fixture);
    }
}

#[test]
#[cfg(feature = "contract-fixtures")]
fn p3sym_compile_contracts() {
    let cases = trybuild::TestCases::new();
    let p3sym: Vec<_> = p1rv_fixtures()
        .into_iter()
        .filter(|fixture| {
            fixture
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("p3sym_"))
        })
        .collect();
    assert!(!p3sym.is_empty(), "no p3sym_* fixtures in tests/p1rv_ui");
    for fixture in p3sym {
        cases.compile_fail(fixture);
    }
}
