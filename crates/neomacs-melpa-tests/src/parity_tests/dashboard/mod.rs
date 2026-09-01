use std::time::Duration;

use crate::{CachedMelpaOracle, DASHBOARD_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const DASHBOARD_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const DASHBOARD_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'bookmark)
(require 'dashboard)

(defmacro neomacs-dashboard-test-with-workspace
    (directory dashboard-name &rest body)
  "Run BODY in an isolated Dashboard workspace and restore editor state."
  (declare (indent 2) (debug t))
  `(save-window-excursion
     (let* ((root (expand-file-name
                   (file-name-as-directory ,directory)
                   (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
            (dashboard-buffer-name ,dashboard-name)
            (dashboard-show-shortcuts nil)
            (dashboard-center-content nil)
            (dashboard-vertically-center-content nil)
            (dashboard-set-heading-icons nil)
            (dashboard-set-file-icons nil)
            (dashboard-remove-missing-entry nil)
            (dashboard-recentf-show-base t)
            (dashboard-recentf-item-format "%s")
            (dashboard-bookmarks-show-base t)
            (dashboard-bookmarks-item-format "%s")
            (dashboard--section-starts nil)
            (dashboard-recentf-alist nil)
            (dashboard--recentf-cache-item-format nil)
            (dashboard--bookmarks-cache-item-format nil)
            (recentf-list nil)
            (recentf-save-file (expand-file-name "recentf" root))
            (bookmark-alist nil)
            (bookmark-save-flag nil)
            (bookmark-default-file (expand-file-name "bookmarks" root))
            (inhibit-startup-screen inhibit-startup-screen)
            (buffers-before (buffer-list))
            (recentf-was-enabled (recentf-enabled-p)))
       (make-directory root t)
       (when (file-exists-p recentf-save-file)
         (delete-file recentf-save-file))
       (unwind-protect
           (progn ,@body)
         (unless recentf-was-enabled (recentf-mode -1))
         (dolist (buffer (buffer-list))
           (unless (memq buffer buffers-before)
             (with-current-buffer buffer (set-buffer-modified-p nil))
             (kill-buffer buffer)))))))
"####;

fn dashboard_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DASHBOARD_MELPA_PIN, "dashboard.el")
        .expect("prepare exact shallow dashboard source below ./tmp")
        .with_prelude(DASHBOARD_TEST_PRELUDE)
        .with_timeout(DASHBOARD_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed dashboard parity test")
        .into()
}

fn assert_dashboard_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        dashboard_oracle(),
        &current_test_name(),
        "dashboard_parity",
        cases,
    );
}

#[test]
fn dashboard_package_batch() {
    assert_dashboard_batch(&workflows::workflow_batch_cases());
}
