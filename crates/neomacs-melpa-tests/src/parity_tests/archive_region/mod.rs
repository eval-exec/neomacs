use std::time::Duration;

use crate::{ARCHIVE_REGION_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ARCHIVE_REGION_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ARCHIVE_REGION_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun archive-region-test-path
    (filename)
  (expand-file-name
   filename
   (getenv
    "NEOMACS_TEST_SANDBOX_ROOT")))

(defun archive-region-test-read-file
    (path)
  (with-temp-buffer
    (insert-file-contents-literally
     path)
    (buffer-string)))

(defun archive-region-test-kill-file-buffers ()
  (let ((root
         (getenv
          "NEOMACS_TEST_SANDBOX_ROOT")))
    (dolist (buffer (buffer-list))
      (when-let ((file
                  (buffer-local-value
                   'buffer-file-name
                   buffer)))
        (when (string-prefix-p
               root
               file)
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (kill-buffer buffer))))))

(defun archive-region-test-cleanup
    (source archive)
  (archive-region-test-kill-file-buffers)
  (cond
   ((file-directory-p archive)
    (delete-directory archive t))
   ((file-exists-p archive)
    (delete-file archive)))
  (when
      (file-exists-p source)
    (delete-file source)))
"##;

fn archive_region_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ARCHIVE_REGION_MELPA_PIN, "archive-region.el")
        .expect("prepare pinned archive-region source below ./tmp")
        .with_prelude(ARCHIVE_REGION_TEST_PRELUDE)
        .with_timeout(ARCHIVE_REGION_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed archive-region parity test")
        .into()
}

/// Multi-probe batch for `assert_archive_region_parity` cases (2a).
pub(crate) fn assert_archive_region_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        archive_region_oracle(),
        &name,
        "archive_region_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn archive_region_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_archive_region_batch(&cases);
}

// END generated package batch tests
