use std::time::Duration;

use crate::{APPARMOR_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const APPARMOR_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const APPARMOR_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'flymake)

(defun apparmor-mode-test-root (name)
  (file-name-as-directory
   (expand-file-name
    name
    (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun apparmor-mode-test-cleanup (root)
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

(defun apparmor-mode-test-read-file (path)
  (with-temp-buffer
    (insert-file-contents-literally path)
    (buffer-string)))

(defun apparmor-mode-test-await
    (predicate description)
  (let ((attempts 0))
    (while
        (and
         (not
          (funcall predicate))
         (< attempts 1000))
      (setq attempts (1+ attempts))
      (accept-process-output nil 0.01))
    (when
        (not
         (funcall predicate))
      (error
       "Timed out waiting for AppArmor %s"
       description))))

(defun apparmor-mode-test-start-flymake
    (predicate description)
  (flymake-start nil t)
  (apparmor-mode-test-await
   predicate
   (format
    "Flymake %s"
    description)))

(defun apparmor-mode-test-diagnostics ()
  (mapcar
   (lambda (diagnostic)
     (let ((begin (flymake-diagnostic-beg diagnostic))
           (end (flymake-diagnostic-end diagnostic)))
       (list
        :type
        (flymake-diagnostic-type diagnostic)
        :text
        (flymake-diagnostic-text diagnostic)
        :begin
        (save-excursion
          (goto-char begin)
          (list
           (line-number-at-pos)
           (current-column)))
        :end
        (save-excursion
          (goto-char end)
          (list
           (line-number-at-pos)
           (current-column))))))
   (flymake-diagnostics
    (point-min)
    (point-max))))
"##;

fn apparmor_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(APPARMOR_MODE_MELPA_PIN, "apparmor-mode.el")
        .expect("prepare pinned apparmor-mode source below ./tmp")
        .with_prelude(APPARMOR_MODE_TEST_PRELUDE)
        .with_timeout(APPARMOR_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed apparmor-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_apparmor_mode_parity` cases (2a).
pub(crate) fn assert_apparmor_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(apparmor_mode_oracle(), &name, "apparmor_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn apparmor_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_apparmor_mode_batch(&cases);
}

// END generated package batch tests
