use std::time::Duration;

use crate::{CachedMelpaOracle, ORG_MIME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const ORG_MIME_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const ORG_MIME_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'org-mime)

(defun neomacs-org-mime-test-normalize-html (html)
  "Normalize only volatile HTML ids in HTML."
  (replace-regexp-in-string
   "org[[:xdigit:]]\\{7\\}" "org<ID>" html t))

(defun neomacs-org-mime-test-kill-message-buffers ()
  "Kill message and Org MIME edit buffers created by a workflow."
  (dolist (buffer (buffer-list))
    (when (or (derived-mode-p 'message-mode)
              (with-current-buffer buffer
                (bound-and-true-p org-mime-src-mode)))
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))
"##;

fn org_mime_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ORG_MIME_MELPA_PIN, "org-mime.el")
        .expect("prepare exact shallow Org MIME source below ./tmp")
        .with_prelude(ORG_MIME_TEST_PRELUDE)
        .with_timeout(ORG_MIME_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Org MIME parity test")
        .into()
}

fn assert_org_mime_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        org_mime_oracle(),
        &current_test_name(),
        "org_mime_parity",
        cases,
    );
}

#[test]
fn org_mime_package_batch() {
    assert_org_mime_batch(&workflows::workflow_batch_cases());
}
