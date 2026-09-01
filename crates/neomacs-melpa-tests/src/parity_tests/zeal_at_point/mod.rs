use crate::{CachedMelpaOracle, ZEAL_AT_POINT_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod load_time;
mod workflows;

const ZEAL_AT_POINT_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'thingatpt)

(defun neomacs-melpa-zeal-at-point--capture-signal (thunk)
  (condition-case err
      (list :value (funcall thunk))
    (error (list :signal (car err) (cdr err)))))

(defun neomacs-melpa-zeal-at-point--wait-process (process)
  (while (process-live-p process)
    (accept-process-output process 0.01))
  (list (process-status process) (process-exit-status process)))
"##;

const MODERN_ZEAL_SCRIPT: &str = "#!/bin/sh\nprintf '%s\\n' 'Zeal 0.6.1'\n";

fn zeal_at_point_oracle_for_load_profile(script: Option<&str>) -> CachedMelpaOracle {
    let sandbox_setup = match script {
        Some(script) => format!(
            r##"
(let* ((bin (expand-file-name "zeal-load-bin"
                              (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (program (expand-file-name "zeal" bin)))
  (make-directory bin t)
  (with-temp-file program
    (insert {script:?}))
  (set-file-modes program #o755)
  (setq exec-path (list bin))
  (setenv "PATH" bin))
"##
        ),
        None => r##"
(let ((bin (expand-file-name "zeal-load-bin"
                             (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
  (make-directory bin t)
  (setq exec-path (list bin))
  (setenv "PATH" bin))
"##
        .to_string(),
    };
    CachedMelpaOracle::new(ZEAL_AT_POINT_MELPA_PIN, "zeal-at-point.el")
        .expect("prepare pinned zeal-at-point source below ./tmp")
        .with_prelude(format!("{sandbox_setup}\n{ZEAL_AT_POINT_TEST_PRELUDE}"))
}

fn zeal_at_point_oracle() -> CachedMelpaOracle {
    zeal_at_point_oracle_for_load_profile(Some(MODERN_ZEAL_SCRIPT))
}

#[test]
fn zeal_at_point_package_batch() {
    assert_oracle_batch_cases(
        zeal_at_point_oracle(),
        "zeal_at_point_package_batch",
        "zeal_at_point_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
