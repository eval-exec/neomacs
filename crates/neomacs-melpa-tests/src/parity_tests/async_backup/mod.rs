use std::time::Duration;

use crate::{ASYNC_BACKUP_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod hooks;
mod paths;
mod predicates;
mod process;
mod registry;

const ASYNC_BACKUP_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ASYNC_BACKUP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)

(defun async-backup-test-path (filename)
  (expand-file-name filename (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun async-backup-test-write-file (filename content)
  (let ((path (async-backup-test-path filename)))
    (make-directory (file-name-directory path) t)
    (with-temp-file path
      (insert content))
    path))

(defun async-backup-test-read-file (filename)
  (with-temp-buffer
    (insert-file-contents-literally filename)
    (buffer-string)))

(defun async-backup-test-make-executable (name body)
  (let ((path
         (async-backup-test-write-file
          (concat "bin/" name)
          (concat "#!/bin/sh\nset -eu\n" body "\n"))))
    (set-file-modes path #o755)
    path))

(defun async-backup-test-install-emacs-stub (&optional body)
  (let* ((program
          (async-backup-test-make-executable
           "emacs"
           (or
            body
            (concat
             "if [ -n \"${ASYNC_BACKUP_TEST_GATE:-}\" ]; then\n"
             "  while [ ! -e \"$ASYNC_BACKUP_TEST_GATE\" ]; do :; done\n"
             "fi\n"
             "if [ -e \"$ASYNC_BACKUP_TEST_OUTPUT\" ]; then\n"
             "  printf '%s\\n' 'backup collision' >&2\n"
             "  exit 73\n"
             "fi\n"
             "cp -- \"$ASYNC_BACKUP_TEST_INPUT\" \"$ASYNC_BACKUP_TEST_OUTPUT\"\n"
             "printf 'copied:%s\\n' \"$(basename \"$ASYNC_BACKUP_TEST_INPUT\")\""))))
         (bin (file-name-directory program)))
    (setq exec-path (cons bin (delete bin exec-path)))
    (setenv "PATH"
            (concat
             (directory-file-name bin)
             path-separator
             (or (getenv "PATH") "")))
    program))

(defun async-backup-test-error-data (thunk)
  (condition-case error-data
      (list :ok (funcall thunk))
    (error (list :error (car error-data) (cdr error-data)))))

(defun async-backup-test-wait (process)
  (while (process-live-p process)
    (accept-process-output process 0.05))
  (accept-process-output process 0.05)
  process)

(defun async-backup-test-kill-buffer (buffer)
  (when (buffer-live-p buffer)
    (with-current-buffer buffer
      (set-buffer-modified-p nil))
    (kill-buffer buffer)))

(defun async-backup-test-kill-file-buffer (file)
  (when-let ((buffer (find-buffer-visiting file)))
    (async-backup-test-kill-buffer buffer)))

(defun async-backup-test-normalize-command (command)
  (mapcar
   (lambda (argument)
     (replace-regexp-in-string
      (regexp-quote (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
      "$ROOT/"
      argument))
   command))

(defun async-backup-test-output-file (root input stamp)
  (let ((extension (file-name-extension input)))
    (concat
     (directory-file-name (expand-file-name root))
     (file-name-directory (expand-file-name input))
     (file-name-base input)
     "-"
     stamp
     (if extension
         (concat "." extension)
       ""))))
"##;

fn async_backup_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ASYNC_BACKUP_MELPA_PIN, source_file)
        .expect("prepare pinned async-backup source below ./tmp")
        .with_prelude(ASYNC_BACKUP_TEST_PRELUDE)
        .with_timeout(ASYNC_BACKUP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed async-backup parity test")
        .into()
}

/// Multi-probe batch for `assert_async_backup_autoload_parity` cases (2a).
pub(crate) fn assert_async_backup_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        async_backup_oracle("async-backup-autoloads.el"),
        &name,
        "async_backup_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_async_backup_parity` cases (2a).
pub(crate) fn assert_async_backup_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        async_backup_oracle("async-backup.el"),
        &name,
        "async_backup_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn async_backup_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_async_backup_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_async_backup_autoload_batch(&cases);
}

#[test]
fn async_backup_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        hooks::hooks_public_surface_batch_cases(),
        paths::paths_public_surface_batch_cases(),
        predicates::predicates_public_surface_batch_cases(),
        process::process_public_surface_batch_cases(),
        registry::registry_async_backup_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_async_backup_batch(&cases);
}

// END generated package batch tests
