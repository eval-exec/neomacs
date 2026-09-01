use std::time::Duration;

use crate::{AURORA_CONFIG_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod commands;
mod jobpath;
mod keywords;
mod mode;
mod registry;
mod workflows;

const AURORA_CONFIG_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AURORA_CONFIG_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defun aurora-config-test-error-data (thunk)
  (condition-case error-data
      (list :ok
            (funcall thunk))
    (error
     (list :error
           (car error-data)
           (cdr error-data)))))

(defun aurora-config-test-face-runs ()
  (let ((position
         (point-min))
        rows)
    (while
        (< position
           (point-max))
      (let* ((face
              (get-text-property
               position
               'face))
             (next
              (next-single-property-change
               position
               'face
               nil
               (point-max))))
        (when face
          (push
           (list
            (-
             position
             (point-min))
            (-
             next
             (point-min))
            (buffer-substring-no-properties
             position
             next)
            face)
           rows))
        (setq position next)))
    (nreverse rows)))

(defun aurora-config-test-buffer-state ()
  (list
   major-mode
   mode-name
   (derived-mode-p 'python-mode)
   (buffer-string)
   (buffer-modified-p)
   (and
    (local-variable-p
     'aurora-config-last-job-path)
    aurora-config-last-job-path)
   (local-variable-p
    'font-lock-defaults)
   (length
    (car font-lock-defaults))
   (lookup-key
    (current-local-map)
    (kbd "C-c a i"))
   (lookup-key
    (current-local-map)
    (kbd "C-c a d"))))
"##;

fn aurora_config_mode_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AURORA_CONFIG_MODE_MELPA_PIN, source_file)
        .expect("prepare pinned aurora-config-mode source below ./tmp")
        .with_prelude(AURORA_CONFIG_MODE_TEST_PRELUDE)
        .with_timeout(AURORA_CONFIG_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed aurora-config-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_aurora_config_mode_autoload_parity` cases (2a).
pub(crate) fn assert_aurora_config_mode_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        aurora_config_mode_oracle("aurora-config-mode-autoloads.el"),
        &name,
        "aurora_config_mode_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_aurora_config_mode_parity` cases (2a).
pub(crate) fn assert_aurora_config_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        aurora_config_mode_oracle("aurora-config-mode.el"),
        &name,
        "aurora_config_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn aurora_config_mode_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_aurora_config_mode_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_aurora_config_mode_autoload_batch(&cases);
}

#[test]
fn aurora_config_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        commands::commands_public_surface_batch_cases(),
        jobpath::jobpath_public_surface_batch_cases(),
        keywords::keywords_public_surface_batch_cases(),
        mode::mode_public_surface_batch_cases(),
        registry::registry_aurora_config_mode_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_aurora_config_mode_batch(&cases);
}

// END generated package batch tests
