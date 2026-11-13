//! Test-profile codegen for the crates carved out of neovm-core.
//!
//! `[profile.test.package.neovm-core] opt-level = 2` exists so the
//! interpreter-heavy tests do not run at opt-level 0 ("Optimize
//! interpreter-heavy tests without changing ordinary editor debug builds").
//! The split moved that runtime into sibling crates while the override stayed
//! on neovm-core alone, so the moved modules silently dropped to opt-level 0
//! in every `cargo nextest` build.  Measured on the macmini (test profile):
//! the `chain_resume_equals_tier0_at_the_mapc_callback_depth_limit` falsify
//! body went 2.58 s -> 24.0 s, every resumed-chain family member regressed
//! 8-10x on the macOS CI runner, and that test crossed nextest's 600 s
//! per-test timeout at 470f484c21.
//!
//! Moving a module out of core must not change its test-profile codegen:
//! every crate that carries former core runtime code must declare the same
//! override.  This guard parses the workspace manifest and REDs a missing or
//! lowered entry; the mutation test exercises both RED forms and the GREEN
//! form.

use super::architecture_production_source_roots::workspace_root;

/// Crates that carry runtime modules carved out of neovm-core while the test
/// profile's `opt-level = 2` stayed behind on the core crate.  `neovm-value`
/// was the first move and its override already exists; the guard covers it so
/// the whole set stays complete and consistent.
const MOVED_CORE_TEST_PROFILE_CRATES: [&str; 6] = [
    "neovm-value",
    "neovm-eval",
    "neovm-text",
    "neovm-edit",
    "neovm-system",
    "neovm-display",
];

fn opt_level(manifest: &toml::Value, package: &str) -> Option<i64> {
    manifest
        .get("profile")?
        .get("test")?
        .get("package")?
        .get(package)?
        .get("opt-level")?
        .as_integer()
}

fn test_profile_codegen_violation(manifest: &toml::Value) -> Result<(), String> {
    let mut failures = Vec::new();
    for package in MOVED_CORE_TEST_PROFILE_CRATES {
        match opt_level(manifest, package) {
            Some(level) if level >= 2 => {}
            Some(level) => failures.push(format!("{package}: opt-level {level} < 2")),
            None => failures.push(format!(
                "{package}: missing [profile.test.package.{package}] opt-level"
            )),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "TEST-PROFILE-CODEGEN: moving a module out of neovm-core must not \
             change its test-profile codegen: {}",
            failures.join("; ")
        ))
    }
}

/// `[profile.test.package.<name>] opt-level = <level>` entries.
fn manifest_with(entries: &[(&str, Option<i64>)]) -> toml::Value {
    let mut text = String::from("[workspace]\nmembers = []\n");
    for (name, level) in entries {
        if let Some(level) = level {
            text.push_str(&format!(
                "[profile.test.package.{name}]\nopt-level = {level}\n"
            ));
        }
    }
    toml::from_str(&text).expect("synthetic manifest")
}

fn full_set(level: i64) -> Vec<(&'static str, Option<i64>)> {
    MOVED_CORE_TEST_PROFILE_CRATES
        .iter()
        .map(|name| (*name, Some(level)))
        .collect()
}

#[test]
fn moved_core_crates_keep_the_core_test_profile_codegen() {
    let path = workspace_root().join("Cargo.toml");
    let text = std::fs::read_to_string(&path).expect("workspace manifest");
    let manifest: toml::Value = toml::from_str(&text).expect("workspace manifest parses");
    test_profile_codegen_violation(&manifest).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        opt_level(&manifest, "neovm-core"),
        Some(2),
        "neovm-core keeps its own test-profile opt-level"
    );
}

#[test]
fn test_profile_codegen_mutations() {
    // GREEN: every moved crate at the core level.
    test_profile_codegen_violation(&manifest_with(&full_set(2))).expect("complete set");

    // RED: a moved crate without an entry (the 470f484c21 state).
    let mut missing = full_set(2);
    missing
        .iter_mut()
        .find(|(name, _)| *name == "neovm-eval")
        .unwrap()
        .1 = None;
    assert!(test_profile_codegen_violation(&manifest_with(&missing)).is_err());

    // RED: an entry lowered below the core level.
    let mut lowered = full_set(2);
    lowered
        .iter_mut()
        .find(|(name, _)| *name == "neovm-text")
        .unwrap()
        .1 = Some(1);
    assert!(test_profile_codegen_violation(&manifest_with(&lowered)).is_err());
}
