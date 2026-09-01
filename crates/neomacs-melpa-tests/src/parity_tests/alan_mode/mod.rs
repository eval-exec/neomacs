use std::time::Duration;

use crate::{ALAN_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod editing;
mod navigation;
mod project;
mod workflows;

const ALAN_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Helpers shared by the project workflows.
///
/// `alan-compiler-project-root` defaults to the single string `"."`, so
/// several buffers hand back the *same* string object and a snapshot
/// capturing more than one renders the rest as `#1#` back references.  Every
/// helper below therefore copies the strings it returns.
const ALAN_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun alan-test-copy (value)
  "Return VALUE with any string copied, so nothing prints as `#1#'."
  (if (stringp value) (copy-sequence value) value))

(defun alan-test-relative (path base)
  "PATH relative to BASE, or nil when PATH is nil."
  (and path (copy-sequence (file-relative-name path base))))

(defun alan-test-write (path contents)
  "Write CONTENTS to PATH, creating its directory."
  (make-directory (file-name-directory path) t)
  (write-region contents nil path nil 'silent)
  path)

(defun alan-test-write-standin (path)
  "Write the stand-in compiler to PATH and make it executable.

It answers according to its own argument vector -- the `build' branch and
the language-compiler branch reply from different files -- so a workflow
witnesses which of the checker's two `:command' branches actually ran
rather than pinning one canned reply.  It also records every invocation,
so what is asserted about the argument vector is a recording of what the
package sent, not a transcription of what the source appears to send."
  (alan-test-write
   path
   (concat "#!/bin/sh\n"
           "{ printf 'cwd=%s\\n' \"$(pwd)\"\n"
           "  printf 'argv:'\n"
           "  for argument in \"$@\"; do printf ' [%s]' \"$argument\"; done\n"
           "  printf '\\n'\n"
           "} >> \"$ALAN_STANDIN_LOG\"\n"
           "if [ \"$1\" = build ]\n"
           "then cat \"$ALAN_STANDIN_DIR/reply-build\"\n"
           "else cat \"$ALAN_STANDIN_DIR/reply-language\"\n"
           "fi\n"
           "exit 1\n"))
  (set-file-modes path #o755)
  path)

(defun alan-test-check-buffer ()
  "Run Flycheck to completion in the current buffer.
Waits on `flycheck-after-syntax-check-hook', which fires exactly once,
rather than on the process, whose death does not mean its output landed."
  (let (finished (rounds 0))
    (add-hook 'flycheck-after-syntax-check-hook
              (lambda () (setq finished t)) nil t)
    (flycheck-mode 1)
    (flycheck-buffer)
    (while (and (not finished) (< rounds 600))
      (accept-process-output nil 0.05)
      (setq rounds (1+ rounds)))
    finished))

(defun alan-test-diagnostics (base)
  "Every diagnostic Flycheck is currently showing, paths relative to BASE."
  (mapcar (lambda (diagnostic)
            (list (flycheck-error-line diagnostic)
                  (flycheck-error-column diagnostic)
                  (flycheck-error-level diagnostic)
                  (alan-test-relative (flycheck-error-filename diagnostic) base)
                  (alan-test-copy (flycheck-error-message diagnostic))))
          flycheck-current-errors))

(defun alan-test-invocations (log base)
  "The stand-in's recorded invocations, with BASE masked."
  (with-temp-buffer
    (insert-file-contents log)
    (replace-regexp-in-string (regexp-quote base) "[PROJECT]"
                              (buffer-string) t t)))
"##;

fn alan_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALAN_MODE_MELPA_PIN, "alan-mode.el")
        .expect("prepare pinned alan-mode source and dependencies below ./tmp")
        .with_prelude(ALAN_MODE_TEST_PRELUDE)
        .with_timeout(ALAN_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed alan-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_alan_mode_parity` cases (2a).
pub(crate) fn assert_alan_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(alan_mode_oracle(), &name, "alan_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn alan_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        editing::editing_public_surface_batch_cases(),
        navigation::navigation_public_surface_batch_cases(),
        project::project_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_alan_mode_batch(&cases);
}

// END generated package batch tests
