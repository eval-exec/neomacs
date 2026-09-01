use std::time::Duration;

use crate::{CachedMelpaOracle, ZATHURA_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ZATHURA_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ZATHURA_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'org)

(defun neomacs-melpa-zathura--signal (thunk)
  (condition-case err
      (list :value (funcall thunk))
    (error (list :signal (car err) (cdr err)))))

(defun neomacs-melpa-zathura--buffer-string (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (buffer-substring-no-properties (point-min) (point-max)))))

(defun neomacs-melpa-zathura--outline-rows ()
  (save-excursion
    (goto-char (point-min))
    (let (rows)
      (while (not (eobp))
        (let ((begin (line-beginning-position))
              (end (line-end-position)))
          (push
           (list
            (buffer-substring-no-properties begin end)
            (get-text-property begin 'zathura-page)
            (when (looking-at outline-regexp)
              (funcall outline-level)))
           rows))
        (forward-line 1))
      (nreverse rows))))
"##;

fn zathura_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ZATHURA_MELPA_PIN, "zathura.el")
        .expect("prepare pinned zathura source below ./tmp")
        .with_prelude(ZATHURA_TEST_PRELUDE)
        .with_timeout(ZATHURA_TEST_TIMEOUT)
}

#[test]
fn zathura_package_batch() {
    assert_oracle_batch_cases(
        zathura_oracle(),
        "zathura_package_batch",
        "zathura_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
