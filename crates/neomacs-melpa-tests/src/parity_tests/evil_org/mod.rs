use std::time::Duration;

use crate::{CachedMelpaOracle, EVIL_MELPA_PIN, EVIL_ORG_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const EVIL_ORG_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const EVIL_ORG_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'org)
(require 'evil)
(require 'evil-org)

(defun neomacs-evil-org-test-with-buffer (text body)
  "Insert TEXT in an org buffer with evil-org-mode and call BODY."
  (with-temp-buffer
    (org-mode)
    (evil-mode 1)
    (evil-org-mode 1)
    (insert text)
    (goto-char (point-min))
    (funcall body)))
"####;

fn evil_org_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_ORG_MELPA_PIN, "evil-org.el")
        .expect("prepare exact shallow evil-org source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare exact shallow Evil dependency below ./tmp")
        .with_prelude(EVIL_ORG_TEST_PRELUDE)
        .with_timeout(EVIL_ORG_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed evil-org parity test")
        .into()
}

fn assert_evil_org_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        evil_org_oracle(),
        &current_test_name(),
        "evil_org_parity",
        cases,
    );
}

#[test]
fn evil_org_package_batch() {
    assert_evil_org_batch(&workflows::workflow_batch_cases());
}
