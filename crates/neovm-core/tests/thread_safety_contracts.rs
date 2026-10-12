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

    fn context_cases(self, flow: FlowRepresentation) -> &'static str {
        match (self, flow) {
            (Self::Arm64MacOs, FlowRepresentation::Word) => {
                "tests/thread_safety_ui/arm64_macos/flow_word/*.rs"
            }
            (Self::Arm64MacOs, FlowRepresentation::Enum) => {
                "tests/thread_safety_ui/arm64_macos/plain_flow/*.rs"
            }
            (Self::Common, FlowRepresentation::Word) => "tests/thread_safety_ui/flow_word/*.rs",
            (Self::Common, FlowRepresentation::Enum) => "tests/thread_safety_ui/plain_flow/*.rs",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum FlowRepresentation {
    Word,
    Enum,
}

impl FlowRepresentation {
    fn current() -> Self {
        if cfg!(feature = "flow-word") {
            Self::Word
        } else {
            Self::Enum
        }
    }
}

#[test]
#[ignore = "compiles separate fixtures; run explicitly with the light gates"]
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
        // The source-to-case gate audits the exact approved overlay inventory.
        cases.compile_fail(target.generic_case(common));
    }
    // A completed method has no public retirement Drop obligation to leak.
    // This fixture type-checks only; runtime coherence is admission_header coverage.
    cases.pass("tests/thread_safety_pass/obarray_writer_method_forget.rs");
    // Context's diagnostics select its Flow carrier and platform field bounds.
    cases.compile_fail(target.context_cases(FlowRepresentation::current()));
}
