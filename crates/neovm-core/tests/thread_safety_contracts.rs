//! Compile-time contracts for the thread-safety ownership boundaries.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
enum FixtureTarget {
    Common,
    Arm64MacOs,
}

impl FixtureTarget {
    fn current() -> Self {
        if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
            Self::Arm64MacOs
        } else {
            Self::Common
        }
    }

    fn generic_case(self, common: PathBuf) -> PathBuf {
        match self {
            Self::Common => common,
            Self::Arm64MacOs => {
                let overlay = Path::new("tests/thread_safety_ui/arm64_macos/generic")
                    .join(common.file_name().expect("fixture filename"));
                if overlay.is_file() { overlay } else { common }
            }
        }
    }

    fn context_cases(self) -> &'static str {
        match self {
            Self::Arm64MacOs => "tests/thread_safety_ui/arm64_macos/flow_word/context_*.rs",
            Self::Common => "tests/thread_safety_ui/flow_word/context_*.rs",
        }
    }
}

#[test]
#[cfg(feature = "contract-fixtures")]
fn thread_safety_compile_contracts() {
    let cases = trybuild::TestCases::new();
    let target = FixtureTarget::current();
    let mut generic: Vec<_> = std::fs::read_dir("tests/thread_safety_ui")
        .expect("compile-contract fixture directory")
        .map(|entry| entry.expect("compile-contract fixture entry").path())
        .filter(|path| {
            path.is_file() && path.extension().is_some_and(|extension| extension == "rs")
        })
        .collect();
    generic.sort();
    for common in generic {
        // An ARM diagnostic overlay includes the unchanged common fixture body.
        cases.compile_fail(target.generic_case(common));
    }
    // This control must compile: missing GC shim dependencies must never be
    // accepted as a confinement failure, including during snapshot overwrite.
    cases.pass("tests/thread_safety_ui/support/scan_contract_pass.rs");
    if matches!(target, FixtureTarget::Common) {
        // These overlays depend only on portable confinement markers, not on
        // platform fields. Exercise their include paths on every host too.
        for fixture in [
            "local_root_send",
            "raw_value_send_payload",
            "raw_value_static_cache",
            "scan_storage_lease_sync",
            "single_mutator_send",
            "tls_scope_send",
        ] {
            cases.compile_fail(format!(
                "tests/thread_safety_ui/arm64_macos/generic/{fixture}.rs"
            ));
        }
    }
    // A completed method has no public retirement Drop obligation to leak.
    // This fixture type-checks only; runtime coherence is admission_header coverage.
    cases.pass("tests/thread_safety_pass/obarray_writer_method_forget.rs");
    cases.pass("tests/thread_safety_pass/host_input_publisher.rs");
    // Context's diagnostics reflect the platform's field bounds.
    cases.compile_fail(target.context_cases());
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
