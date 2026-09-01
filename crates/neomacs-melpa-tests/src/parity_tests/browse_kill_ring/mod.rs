use std::time::Duration;

use crate::{BROWSE_KILL_RING_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const BROWSE_KILL_RING_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const BROWSE_KILL_RING_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'browse-kill-ring)
"####;

fn browse_kill_ring_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(BROWSE_KILL_RING_MELPA_PIN, "browse-kill-ring.el")
        .expect("prepare exact shallow browse-kill-ring source below ./tmp")
        .with_prelude(BROWSE_KILL_RING_TEST_PRELUDE)
        .with_timeout(BROWSE_KILL_RING_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed browse-kill-ring parity test")
        .into()
}

fn assert_browse_kill_ring_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        browse_kill_ring_oracle(),
        &current_test_name(),
        "browse_kill_ring_parity",
        cases,
    );
}

#[test]
fn browse_kill_ring_package_batch() {
    assert_browse_kill_ring_batch(&workflows::workflow_batch_cases());
}
