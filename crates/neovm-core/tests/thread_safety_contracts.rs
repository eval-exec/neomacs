//! Compile-time contracts for the thread-safety ownership boundaries.

#[cfg(feature = "contract-fixtures")]
#[path = "thread_safety_ui/support/fixture_target.rs"]
mod fixture_target;

#[cfg(feature = "contract-fixtures")]
fn context_cases(target: fixture_target::FixtureTarget) -> &'static str {
    use fixture_target::FixtureTarget;
    match target {
        FixtureTarget::Arm64MacOs => "tests/thread_safety_ui/arm64_macos/flow_word/context_*.rs",
        FixtureTarget::Common => "tests/thread_safety_ui/flow_word/context_*.rs",
    }
}

#[test]
#[cfg(feature = "contract-fixtures")]
fn thread_safety_compile_contracts() {
    use fixture_target::FixtureTarget;

    let cases = trybuild::TestCases::new();
    let target = FixtureTarget::current();
    // An ARM diagnostic overlay includes the unchanged common fixture body.
    // Overlays compile only on their own target: the macOS compile-contracts
    // job gates them, and a Linux bless can never rewrite a macOS golden.
    for fixture in target.fixtures(
        "tests/thread_safety_ui",
        "tests/thread_safety_ui/arm64_macos/generic",
    ) {
        cases.compile_fail(fixture);
    }
    // This control must compile: missing GC shim dependencies must never be
    // accepted as a confinement failure, including during snapshot overwrite.
    cases.pass("tests/thread_safety_ui/support/scan_contract_pass.rs");
    // A completed method has no public retirement Drop obligation to leak.
    // This fixture type-checks only; runtime coherence is admission_header coverage.
    cases.pass("tests/thread_safety_pass/obarray_writer_method_forget.rs");
    cases.pass("tests/thread_safety_pass/host_input_publisher.rs");
    // Context's diagnostics reflect the platform's field bounds.
    cases.compile_fail(context_cases(target));
}

// The sole-owner eval phase's fixtures sit next to its unit tests, but the
// runner lives here: trybuild compiles them against the non-test lib, so the
// contract jobs need no opt-level-2 build of the whole lib unit-test binary.
#[test]
#[cfg(feature = "contract-fixtures")]
fn sole_owner_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("src/emacs_core/runtime/eval/tests/sole_owner_pass/*.rs");
    cases.compile_fail("src/emacs_core/runtime/eval/tests/sole_owner_ui/*.rs");
}

// The parallel-Lisp contract runners below lived in the `core` binary, which
// no contract job builds with `contract-fixtures`, so CI never ran them. Their
// files and fixtures stay where they are.
#[path = "p2ht_contracts.rs"]
mod p2ht_contracts;

#[path = "pentry_contracts.rs"]
mod pentry_contracts;

#[cfg(feature = "contract-fixtures")]
#[test]
fn nmjit_n1_sketch_compile_contracts() {
    // The sketches have no engine dependency. A trybuild driver in this crate
    // inherits all core dependencies and rebuilds them for its diagnostic flags,
    // so use an isolated specification workspace without changing any client.
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/nmjit-type-contracts/Cargo.toml");
    let status = std::process::Command::new(env!("CARGO"))
        .args(["test", "--locked", "--manifest-path"])
        .arg(manifest)
        .args([
            "--test",
            "contracts",
            "--",
            "--nocapture",
            "--test-threads=1",
        ])
        .status()
        .expect("launch isolated N1 sketch contracts");
    assert!(
        status.success(),
        "N1 sketch compiler contract failed: {status}"
    );
}

#[path = "host_effect_contracts.rs"]
mod host_effect_contracts;

#[path = "intern_shared_contracts.rs"]
mod intern_shared_contracts;
