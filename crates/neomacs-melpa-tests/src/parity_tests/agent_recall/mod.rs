use std::time::Duration;

use crate::{AGENT_RECALL_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod interaction;
mod matching;
mod search;
mod smoke;
mod workflows;

const AGENT_RECALL_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn agent_recall_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AGENT_RECALL_MELPA_PIN, source_file)
        .expect("prepare pinned agent-recall source and dependency transaction below ./tmp")
        .with_timeout(AGENT_RECALL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed agent-recall parity test")
        .into()
}

/// Multi-probe batch for `assert_agent_recall_autoload_parity` cases (2a).
pub(crate) fn assert_agent_recall_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        agent_recall_oracle("agent-recall-autoloads.el"),
        &name,
        "agent_recall_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_agent_recall_parity` cases (2a).
pub(crate) fn assert_agent_recall_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        agent_recall_oracle("agent-recall.el"),
        &name,
        "agent_recall_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn agent_recall_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [smoke::smoke_agent_recall_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_agent_recall_autoload_batch(&cases);
}

#[test]
fn agent_recall_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        interaction::interaction_public_surface_batch_cases(),
        matching::matching_public_surface_batch_cases(),
        search::search_public_surface_batch_cases(),
        smoke::smoke_agent_recall_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_agent_recall_batch(&cases);
}

// END generated package batch tests
