use std::time::Duration;

use crate::{CachedMelpaOracle, EVIL_MAGIT_MELPA_PIN, EVIL_MELPA_PIN, MAGIT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const EVIL_MAGIT_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const EVIL_MAGIT_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

;; Give every real Git process deterministic identity and branch behavior.
(setq magit-git-global-arguments
      (append
       '("-c" "init.defaultBranch=master"
         "-c" "core.quotePath=false"
         "-c" "user.name=Parity User"
         "-c" "user.email=parity@example.invalid")
       (and (boundp 'magit-git-global-arguments)
            magit-git-global-arguments)))

;; Load git-rebase before evil-magit so its deferred public integration is
;; installed immediately and is present in every logical case.
(require 'evil)
(require 'magit)
(require 'git-rebase)
(require 'evil-magit)

(defvar neomacs-evil-magit-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun neomacs-evil-magit-test-write (path text)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region text nil path nil 'silent))
  path)

(defun neomacs-evil-magit-test-project (name)
  "Create a committed real Git project named NAME in the case sandbox."
  (let ((root (file-name-as-directory
               (expand-file-name name neomacs-evil-magit-test-root))))
    (when (file-directory-p root)
      (delete-directory root t))
    (make-directory root t)
    (let ((default-directory root))
      (magit-git "init" ".")
      (neomacs-evil-magit-test-write
       (expand-file-name "README.md" root)
       "# Release workspace\n\nTracked baseline.\n")
      (magit-git "add" "README.md")
      (magit-git "commit" "-m" "baseline"))
    root))

(defun neomacs-evil-magit-test-await-process ()
  "Drain the current repository's Magit process with a bounded wait."
  (let ((deadline (+ (float-time) 15.0))
        process)
    (while (and (setq process
                      (get-buffer-process (magit-process-buffer t)))
                (process-live-p process)
                (< (float-time) deadline))
      (accept-process-output nil 0.02))
    (when (and process (process-live-p process))
      (error "evil-magit test process did not finish"))))

(defun neomacs-evil-magit-test-call-key (keys expected)
  "Resolve KEYS and invoke EXPECTED through the active real buffer map."
  (let ((command (key-binding (kbd keys))))
    (unless (eq command expected)
      (error "evil-magit key %s resolved to %S, expected %S"
             keys command expected))
    (let ((this-command command)
          (real-this-command command))
      (call-interactively command))
    command))

(defun neomacs-evil-magit-test-visible-text ()
  "Return the complete visible text of the current Magit buffer."
  (save-excursion
    (let (chunks)
      (goto-char (point-min))
      (while (< (point) (point-max))
        (let ((to (next-single-char-property-change
                   (point) 'invisible nil (point-max))))
          (unless (invisible-p (point))
            (push (buffer-substring-no-properties (point) to) chunks))
          (goto-char to)))
      (replace-regexp-in-string
       "\\b[[:xdigit:]]\\{7,40\\}\\b"
       "<HASH>"
       (string-trim-right (apply #'concat (nreverse chunks)))))))

(defun neomacs-evil-magit-test-kill-project
    (root buffers-before origin-buffer)
  "Restore ORIGIN-BUFFER, kill case-owned buffers, and remove ROOT."
  (let ((owned-buffers
         (cl-remove-if-not
          (lambda (buffer)
            (not (memq buffer buffers-before)))
          (buffer-list))))
    ;; The case owns only buffers created after BUFFERS-BEFORE.  Restore the
    ;; exact live origin before killing any of those owned buffers.
    (when (buffer-live-p origin-buffer)
      (switch-to-buffer origin-buffer))
    (dolist (buffer owned-buffers)
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (set-buffer-modified-p nil)
          (kill-buffer buffer)))))
  (when (file-directory-p root)
    (delete-directory root t)))
"####;

fn evil_magit_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_MAGIT_MELPA_PIN, "evil-magit.el")
        .expect("prepare exact shallow Evil Magit source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare exact shallow Evil dependency below ./tmp")
        .with_melpa_dependency(MAGIT_MELPA_PIN)
        .expect("prepare exact shallow Magit dependency closure below ./tmp")
        .with_prelude(EVIL_MAGIT_TEST_PRELUDE)
        .with_timeout(EVIL_MAGIT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed evil-magit parity test")
        .into()
}

fn assert_evil_magit_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        evil_magit_oracle(),
        &current_test_name(),
        "evil_magit_parity",
        cases,
    );
}

#[test]
fn evil_magit_package_batch() {
    assert_evil_magit_batch(&workflows::workflow_batch_cases());
}
