use std::time::Duration;

use crate::{
    AIO_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN, DOCKER_MELPA_PIN, S_MELPA_PIN,
    TABLIST_MELPA_PIN, TRANSIENT_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const DOCKER_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const DOCKER_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'docker-utils)
(require 'docker-core)
(require 'docker-process)
"####;

fn docker_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DOCKER_MELPA_PIN, "docker.el")
        .expect("prepare exact shallow docker source below ./tmp")
        .with_melpa_dependency(AIO_MELPA_PIN)
        .expect("prepare aio")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare dash")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare s")
        .with_melpa_dependency(TABLIST_MELPA_PIN)
        .expect("prepare tablist")
        .with_melpa_dependency(TRANSIENT_MELPA_PIN)
        .expect("prepare transient")
        .with_prelude(DOCKER_TEST_PRELUDE)
        .with_timeout(DOCKER_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed docker parity test")
        .into()
}

fn assert_docker_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        docker_oracle(),
        &current_test_name(),
        "docker_parity",
        cases,
    );
}

#[test]
fn docker_package_batch() {
    assert_docker_batch(&workflows::workflow_batch_cases());
}
