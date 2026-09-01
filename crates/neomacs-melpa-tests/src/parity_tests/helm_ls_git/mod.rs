use std::time::Duration;

use crate::{CachedMelpaOracle, HELM_LS_GIT_MELPA_PIN, HELM_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const HELM_LS_GIT_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const HELM_LS_GIT_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'helm-ls-git)

(defun neomacs-helm-ls-git-test-with-repo (body)
  "Call BODY with a temporary Git repository root."
  (let* ((root (make-temp-file "neomacs-helm-ls-git-" t))
         (default-directory root))
    (unwind-protect
        (progn
          (call-process "git" nil nil nil "init" "-q")
          (call-process "git" nil nil nil "config" "user.email" "parity@example.com")
          (call-process "git" nil nil nil "config" "user.name" "Parity")
          (with-temp-file (expand-file-name "alpha.el" root)
            (insert ";; alpha\n"))
          (with-temp-file (expand-file-name "beta.txt" root)
            (insert "beta\n"))
          (make-directory (expand-file-name "sub" root) t)
          (with-temp-file (expand-file-name "sub/gamma.el" root)
            (insert ";; gamma\n"))
          (call-process "git" nil nil nil "add" "alpha.el" "beta.txt" "sub/gamma.el")
          (call-process "git" nil nil nil
                        "-c" "commit.gpgsign=false"
                        "commit" "-q" "-m" "init")
          (funcall body root))
      (ignore-errors
        (delete-directory root t)))))
"####;

fn helm_ls_git_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_LS_GIT_MELPA_PIN, "helm-ls-git.el")
        .expect("prepare exact shallow helm-ls-git source below ./tmp")
        .with_melpa_dependency(HELM_MELPA_PIN)
        .expect("prepare exact shallow Helm dependency below ./tmp")
        .with_prelude(HELM_LS_GIT_TEST_PRELUDE)
        .with_timeout(HELM_LS_GIT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed helm-ls-git parity test")
        .into()
}

fn assert_helm_ls_git_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        helm_ls_git_oracle(),
        &current_test_name(),
        "helm_ls_git_parity",
        cases,
    );
}

#[test]
fn helm_ls_git_package_batch() {
    assert_helm_ls_git_batch(&workflows::workflow_batch_cases());
}
