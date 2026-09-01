use std::time::Duration;

use crate::{APT_SOURCES_LIST_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const APT_SOURCES_LIST_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const APT_SOURCES_LIST_TEST_PRELUDE: &str = r##"
(defun apt-sources-list-test-root (name)
  (file-name-as-directory
   (expand-file-name
    name
    (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun apt-sources-list-test-cleanup (root)
  (dolist (buffer (buffer-list))
    (let ((file (buffer-file-name buffer)))
      (when
          (and file (string-prefix-p root file))
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer))))
  (when
      (file-exists-p root)
    (delete-directory root t)))

(defun apt-sources-list-test-read-file (path)
  (with-temp-buffer
    (insert-file-contents-literally path)
    (buffer-string)))
"##;

fn apt_sources_list_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(APT_SOURCES_LIST_MELPA_PIN, "apt-sources-list.el")
        .expect("prepare pinned apt-sources-list source below ./tmp")
        .with_prelude(APT_SOURCES_LIST_TEST_PRELUDE)
        .with_timeout(APT_SOURCES_LIST_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed apt-sources-list parity test")
        .into()
}

/// Multi-probe batch for `assert_apt_sources_list_parity` cases (2a).
pub(crate) fn assert_apt_sources_list_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        apt_sources_list_oracle(),
        &name,
        "apt_sources_list_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn apt_sources_list_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_apt_sources_list_batch(&cases);
}

// END generated package batch tests
