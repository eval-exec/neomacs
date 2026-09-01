use std::time::Duration;

use crate::{COND_LET_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const COND_LET_TEST_TIMEOUT: Duration = Duration::from_secs(120);

fn cond_let_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COND_LET_MELPA_PIN, "cond-let.el")
        .expect("prepare pinned Cond-Let source below ./tmp")
        .with_timeout(COND_LET_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Cond-Let parity test")
        .into()
}

pub(crate) fn assert_cond_let_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(cond_let_oracle(), &name, "cond_let_parity", cases);
}

#[test]
fn cond_let_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_cond_let_batch(&cases);
}
