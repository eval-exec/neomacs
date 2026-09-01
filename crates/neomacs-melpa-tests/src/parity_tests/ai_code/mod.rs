use std::time::Duration;

use crate::{AI_CODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod backends;
mod behaviors;
mod core;
mod links;
mod mcp;
mod prompts;
mod sessions;
mod viewport;
mod workflows;

// The editor-helper workflow starts several real pty processes, so the
// 30s this used to allow was marginal: the case passed when run alone and
// timed out under package load, which reads as flakiness in the package
// rather than as a harness cap.  The other long-running suites allow
// 120-240s.
const AI_CODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const AI_CODE_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'map)
(require 'seq)
"##;

fn ai_code_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AI_CODE_MELPA_PIN, source_file)
        .expect("prepare pinned ai-code source below ./tmp")
        .with_prelude(AI_CODE_PRELUDE)
        .with_timeout(AI_CODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ai-code parity test")
        .into()
}

/// Multi-probe batch for `assert_ai_code_parity` cases (2a).
pub(crate) fn assert_ai_code_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ai_code_oracle("ai-code.el"), &name, "ai_code_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ai_code_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        backends::backends_public_surface_batch_cases(),
        behaviors::behaviors_public_surface_batch_cases(),
        core::core_public_surface_batch_cases(),
        links::links_public_surface_batch_cases(),
        mcp::mcp_public_surface_batch_cases(),
        prompts::prompts_public_surface_batch_cases(),
        sessions::sessions_public_surface_batch_cases(),
        viewport::viewport_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_ai_code_batch(&cases);
}

// END generated package batch tests
