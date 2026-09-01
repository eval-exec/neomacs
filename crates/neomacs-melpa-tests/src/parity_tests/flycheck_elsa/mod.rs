use std::time::Duration;

use crate::{CachedMelpaOracle, FLYCHECK_ELSA_MELPA_PIN, FLYCHECK_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const FLYCHECK_ELSA_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const FLYCHECK_ELSA_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'flycheck)
(require 'flycheck-elsa)

(defun neomacs-flycheck-elsa-test-with-project (backend body)
  "Call BODY in a temp project configured for BACKEND (`cask' or `eask')."
  (let* ((root (make-temp-file "neomacs-flycheck-elsa-" t))
         (config (if (eq backend 'eask) "Eask" "Cask"))
         (src (expand-file-name "src" root))
         (file (expand-file-name "foo.el" src)))
    (unwind-protect
        (progn
          (make-directory src t)
          (with-temp-file (expand-file-name config root)
            (insert
             "(source gnu)\n"
             "(depends-on \"elsa\")\n"
             "(development\n"
             " (depends-on \"buttercup\"))\n"))
          (with-temp-file file
            (insert ";;; foo.el --- test -*- lexical-binding: t -*-\n"
                    "(defun foo ())\n"
                    "(provide 'foo)\n"))
          (let ((default-directory root)
                (flycheck-elsa-backend backend)
                (buffer (find-file-noselect file)))
            (unwind-protect
                (with-current-buffer buffer
                  (emacs-lisp-mode)
                  (funcall body root file buffer))
              (when (buffer-live-p buffer)
                (let ((kill-buffer-hook nil)
                      (kill-buffer-query-functions nil))
                  (kill-buffer buffer))))))
      (ignore-errors (delete-directory root t)))))
"####;

fn flycheck_elsa_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FLYCHECK_ELSA_MELPA_PIN, "flycheck-elsa.el")
        .expect("prepare exact shallow flycheck-elsa source below ./tmp")
        .with_melpa_dependency(FLYCHECK_MELPA_PIN)
        .expect("prepare exact shallow Flycheck dependency below ./tmp")
        .with_prelude(FLYCHECK_ELSA_TEST_PRELUDE)
        .with_timeout(FLYCHECK_ELSA_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed flycheck-elsa parity test")
        .into()
}

fn assert_flycheck_elsa_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        flycheck_elsa_oracle(),
        &current_test_name(),
        "flycheck_elsa_parity",
        cases,
    );
}

#[test]
fn flycheck_elsa_package_batch() {
    assert_flycheck_elsa_batch(&workflows::workflow_batch_cases());
}
