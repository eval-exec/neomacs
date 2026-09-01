use std::time::Duration;

use crate::{ARXIV_CITATION_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ARXIV_CITATION_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn arxiv_citation_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ARXIV_CITATION_MELPA_PIN, "arxiv-citation.el")
        .expect("prepare pinned arxiv-citation source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned Dash dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency below ./tmp")
        .with_timeout(ARXIV_CITATION_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed arxiv-citation parity test")
        .into()
}

/// Multi-probe batch for `assert_arxiv_citation_parity` cases (2a).
pub(crate) fn assert_arxiv_citation_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        arxiv_citation_oracle(),
        &name,
        "arxiv_citation_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn arxiv_citation_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_arxiv_citation_batch(&cases);
}

// END generated package batch tests
