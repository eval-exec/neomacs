use crate::{CachedMelpaOracle, YOUDOTCOM_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const YOUDOTCOM_TEST_PRELUDE: &str = r##"
(require 'youdotcom)

(defvar neomacs-melpa-youdotcom--response-buffers nil)

(defun neomacs-melpa-youdotcom--response-buffer (json)
  (let ((buffer (generate-new-buffer " *youdotcom-parity-http*")))
    (push buffer neomacs-melpa-youdotcom--response-buffers)
    (with-current-buffer buffer
      (insert "HTTP/1.1 200 OK\nContent-Type: application/json; charset=utf-8\n\n")
      (insert json)
      (goto-char (point-min)))
    buffer))

(defun neomacs-melpa-youdotcom--cleanup-response-buffers ()
  (dolist (buffer neomacs-melpa-youdotcom--response-buffers)
    (when (buffer-live-p buffer)
      (kill-buffer buffer))))
"##;

fn youdotcom_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(YOUDOTCOM_MELPA_PIN, "youdotcom.el")
        .expect("prepare pinned youdotcom source below ./tmp")
        .with_prelude(YOUDOTCOM_TEST_PRELUDE)
}

#[test]
fn youdotcom_package_batch() {
    assert_oracle_batch_cases(
        youdotcom_oracle(),
        "youdotcom_package_batch",
        "youdotcom_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
