use std::time::Duration;

use crate::{
    COUNSEL_MELPA_PIN, COUNSEL_PROJECTILE_MELPA_PIN, CachedMelpaOracle, PROJECTILE_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const COUNSEL_PROJECTILE_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const COUNSEL_PROJECTILE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'projectile)
(require 'counsel)
(require 'counsel-projectile)

(defvar neomacs-counsel-projectile-test-actions
  '(1
    ("o" identity "open")
    ("j" ignore "jump")
    ("x" ignore "extra"))
  "Mutable action list fixture for counsel-projectile-modify-action tests.")
"####;

fn counsel_projectile_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COUNSEL_PROJECTILE_MELPA_PIN, "counsel-projectile.el")
        .expect("prepare exact shallow counsel-projectile source below ./tmp")
        .with_melpa_dependency(COUNSEL_MELPA_PIN)
        .expect("prepare exact shallow Counsel dependency below ./tmp")
        .with_melpa_dependency(PROJECTILE_MELPA_PIN)
        .expect("prepare exact shallow Projectile dependency below ./tmp")
        .with_prelude(COUNSEL_PROJECTILE_TEST_PRELUDE)
        .with_timeout(COUNSEL_PROJECTILE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed counsel-projectile parity test")
        .into()
}

fn assert_counsel_projectile_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        counsel_projectile_oracle(),
        &current_test_name(),
        "counsel_projectile_parity",
        cases,
    );
}

#[test]
fn counsel_projectile_package_batch() {
    assert_counsel_projectile_batch(&workflows::workflow_batch_cases());
}
