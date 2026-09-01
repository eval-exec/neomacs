use crate::{CachedMelpaOracle, YOUDAO_DICTIONARY_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const YOUDAO_DICTIONARY_TEST_PRELUDE: &str = r##"
(require 'youdao-dictionary)

(defvar neomacs-melpa-youdao--response-buffers nil)

(defun neomacs-melpa-youdao--response-buffer (json)
  (let ((buffer (generate-new-buffer " *youdao-parity-http*")))
    (push buffer neomacs-melpa-youdao--response-buffers)
    (with-current-buffer buffer
      (setq-local url-http-response-status 200)
      (insert "HTTP/1.1 200 OK\nContent-Type: application/json; charset=utf-8\n\n")
      (insert json)
      (goto-char (point-min)))
    buffer))

(defun neomacs-melpa-youdao--cleanup-response-buffers ()
  (dolist (buffer neomacs-melpa-youdao--response-buffers)
    (when (buffer-live-p buffer)
      (kill-buffer buffer))))
"##;

fn youdao_dictionary_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(YOUDAO_DICTIONARY_MELPA_PIN, "youdao-dictionary.el")
        .expect("prepare pinned youdao-dictionary source below ./tmp")
        .with_prelude(YOUDAO_DICTIONARY_TEST_PRELUDE)
}

#[test]
fn youdao_dictionary_package_batch() {
    assert_oracle_batch_cases(
        youdao_dictionary_oracle(),
        "youdao_dictionary_package_batch",
        "youdao_dictionary_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
