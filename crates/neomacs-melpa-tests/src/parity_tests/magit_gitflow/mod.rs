use std::time::Duration;

use crate::{CachedMelpaOracle, MAGIT_GITFLOW_MELPA_PIN, MAGIT_MELPA_PIN, MAGIT_POPUP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const MAGIT_GITFLOW_TEST_TIMEOUT: Duration = Duration::from_secs(240);

const MAGIT_GITFLOW_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)

(setq magit-git-global-arguments
      (append
       '("-c" "init.defaultBranch=master"
         "-c" "core.quotePath=false"
         "-c" "user.name=Parity User"
         "-c" "user.email=parity@example.invalid")
       (and (boundp 'magit-git-global-arguments)
            magit-git-global-arguments)))

(require 'magit-gitflow)

(defvar neomacs-magit-gitflow-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun neomacs-magit-gitflow-test-write (path content &optional executable)
  "Write CONTENT to PATH and optionally make it EXECUTABLE."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region content nil path nil 'silent))
  (when executable
    (set-file-modes path #o755))
  path)

(defconst neomacs-magit-gitflow-test-executable
  ;; Exact argv-keyed responses recorded from Git Flow AVH 1.12.3,
  ;; revision d409eff2896b02e1ae1ac76c291aaf15213aac6d.  This boundary
  ;; deliberately records and replays the external tool; it never performs
  ;; Git Flow's repository mutations on the package's behalf.
  (string-join
   '("#!/bin/sh"
     "printf 'CALL' >> \"$NEOMACS_MAGIT_GITFLOW_TRACE\""
     "for argument in \"$@\"; do"
     "  printf '\\t%s' \"$argument\" >> \"$NEOMACS_MAGIT_GITFLOW_TRACE\""
     "done"
     "printf '\\n' >> \"$NEOMACS_MAGIT_GITFLOW_TRACE\""
     "case \"$*\" in"
     "  'init -d')"
     "    printf 'Using default branch names.\\n'"
     "    ;;"
     "  'feature start --fetch billing/quote-λ')"
     "    printf \"Switched to a new branch 'feature/billing/quote-λ'\\n\""
     "    ;;"
     "  'feature start checkout-v2')"
     "    printf 'Feature start transcript replayed.\\n'"
     "    ;;"
     "  'feature finish invoice/retry')"
     "    printf \"The feature branch 'feature/invoice/retry' was merged into 'develop'.\\n\""
     "    ;;"
     "  'feature rebase --preserve-merges shipping-label')"
     "    printf \"Will try to rebase 'shipping-label' which is based on 'develop'...\\n\""
     "    ;;"
     "  'release finish --notag --keep 2.4.0-rc1')"
     "    printf \"Release branch 'release/2.4.0-rc1' has been merged.\\n\""
     "    ;;"
     "  'support start 1.x master')"
     "    printf \"A new branch 'support/1.x' was created, based on 'master'.\\n\""
     "    ;;"
     "  *)"
     "    printf 'unsupported git-flow command: %s\\n' \"$*\" >&2"
     "    exit 64"
     "    ;;"
     "esac")
   "\n"))

(defun neomacs-magit-gitflow-test-trace (path)
  "Read exact git-flow argument vectors from PATH."
  (if (not (file-exists-p path))
      nil
    (with-temp-buffer
      (insert-file-contents path)
      (mapcar (lambda (line) (cdr (split-string line "\t")))
              (split-string (buffer-string) "\n" t)))))

(defun neomacs-magit-gitflow-test-git-lines (&rest arguments)
  "Return Git output for ARGUMENTS as stable nonempty lines."
  (split-string (apply #'magit-git-output arguments) "\n" t))

(defun neomacs-magit-gitflow-test-run-git (&rest arguments)
  "Run real Git with ARGUMENTS for deterministic fixture setup."
  (let ((status (apply #'call-process "git" nil nil nil arguments)))
    (unless (equal status 0)
      (error "Fixture Git command failed (%S): git %s"
             status (string-join arguments " ")))
    status))

(defun neomacs-magit-gitflow-test-configure-repository ()
  "Establish a realistic Git Flow repository precondition."
  (neomacs-magit-gitflow-test-run-git
   "config" "gitflow.branch.master" "master")
  (neomacs-magit-gitflow-test-run-git
   "config" "gitflow.branch.develop" "develop")
  (neomacs-magit-gitflow-test-run-git
   "config" "gitflow.prefix.feature" "feature/")
  (neomacs-magit-gitflow-test-run-git
   "config" "gitflow.prefix.bugfix" "bugfix/")
  (neomacs-magit-gitflow-test-run-git
   "config" "gitflow.prefix.release" "release/")
  (neomacs-magit-gitflow-test-run-git
   "config" "gitflow.prefix.hotfix" "hotfix/")
  (neomacs-magit-gitflow-test-run-git
   "config" "gitflow.prefix.support" "support/")
  (neomacs-magit-gitflow-test-run-git
   "config" "gitflow.prefix.versiontag" "")
  (unless (magit-branch-p "develop")
    (neomacs-magit-gitflow-test-run-git "branch" "develop" "master")))

(defun neomacs-magit-gitflow-test-checkout-topic (kind name base)
  "Create and check out KIND/NAME from BASE as a real fixture precondition."
  (let ((branch (format "%s/%s" kind name)))
    (neomacs-magit-gitflow-test-run-git "branch" branch base)
    (neomacs-magit-gitflow-test-run-git "switch" "--quiet" branch)
    branch))

(defun neomacs-magit-gitflow-test-await-process (process)
  "Wait for Magit's concrete PROCESS and return its exit status."
  (let ((deadline (+ (float-time) 20.0)))
    (while (and (process-live-p process)
                (< (float-time) deadline))
      (accept-process-output nil 0.02))
    (when (process-live-p process)
      (error "Magit Gitflow process did not finish: %S" process))
    (process-exit-status process)))

(defmacro neomacs-magit-gitflow-test-with-repository (name &rest body)
  "Run BODY in an isolated real Git repository named NAME."
  (declare (indent 1) (debug (form body)))
  `(save-window-excursion
     (let* ((case-root
             (file-name-as-directory
              (expand-file-name ,name neomacs-magit-gitflow-test-root)))
            (repo (file-name-as-directory (expand-file-name "repo" case-root)))
            (bin (file-name-as-directory (expand-file-name "bin" case-root)))
            (git-flow (expand-file-name "git-flow" bin))
            (trace (expand-file-name "git-flow-calls.log" case-root))
            (buffers-before (buffer-list))
            (processes-before (process-list))
            (process-environment (copy-sequence process-environment))
            (exec-path (cons (directory-file-name bin) exec-path))
            (default-directory repo)
            (unread-command-events nil)
            (minibuffer-history nil)
            (magit-revision-history nil)
            (magit-current-popup nil)
            (magit-current-popup-action nil)
            (magit-current-popup-args nil)
            (magit-current-pre-popup-buffer nil)
            (magit-popup-show-help-echo nil)
            (this-command nil)
            (real-this-command nil)
            (last-command nil))
       (unwind-protect
           (progn
             (when (file-directory-p case-root)
               (delete-directory case-root t))
             (make-directory repo t)
             (neomacs-magit-gitflow-test-write
              git-flow neomacs-magit-gitflow-test-executable t)
             (setenv "PATH" (concat bin path-separator (getenv "PATH")))
             (setenv "NEOMACS_MAGIT_GITFLOW_TRACE" trace)
             (neomacs-magit-gitflow-test-run-git
              "-c" "init.defaultBranch=master" "init" "--quiet" ".")
             (neomacs-magit-gitflow-test-run-git
              "config" "user.name" "Parity User")
             (neomacs-magit-gitflow-test-run-git
              "config" "user.email" "parity@example.invalid")
             (neomacs-magit-gitflow-test-run-git
              "config" "core.quotePath" "false")
             (neomacs-magit-gitflow-test-write
              (expand-file-name "service.txt" repo)
              "version=1\nchannel=stable\nowner=Zoë\n")
             (neomacs-magit-gitflow-test-run-git "add" "service.txt")
             (neomacs-magit-gitflow-test-run-git
              "commit" "--quiet" "-m" "baseline")
             ,@body)
         (dolist (process (process-list))
           (when (and (not (memq process processes-before))
                      (process-live-p process))
             (delete-process process)))
         (dolist (buffer (buffer-list))
           (when (and (not (memq buffer buffers-before))
                      (buffer-live-p buffer))
             (with-current-buffer buffer
               (set-buffer-modified-p nil))
             (kill-buffer buffer)))
         (when (file-directory-p case-root)
           (delete-directory case-root t))))))
"####;

fn magit_gitflow_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MAGIT_GITFLOW_MELPA_PIN, "magit-gitflow.el")
        .expect("prepare exact shallow Magit Gitflow source below ./tmp")
        .with_melpa_dependency(MAGIT_MELPA_PIN)
        .expect("prepare exact shallow Magit dependency below ./tmp")
        .with_melpa_dependency(MAGIT_POPUP_MELPA_PIN)
        .expect("prepare exact shallow Magit Popup dependency below ./tmp")
        .with_prelude(MAGIT_GITFLOW_TEST_PRELUDE)
        .with_timeout(MAGIT_GITFLOW_TEST_TIMEOUT)
}

fn assert_magit_gitflow_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        magit_gitflow_oracle(),
        "magit-gitflow-package-batch",
        "magit_gitflow_parity",
        cases,
    );
}

#[test]
fn magit_gitflow_package_batch() {
    assert_magit_gitflow_batch(&workflows::workflow_batch_cases());
}
