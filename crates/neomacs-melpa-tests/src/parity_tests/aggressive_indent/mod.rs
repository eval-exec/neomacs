use std::time::Duration;

use crate::{AGGRESSIVE_INDENT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AGGRESSIVE_INDENT_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Fixtures shared by the workflows.
///
/// aggressive-indent watches `after-change-functions', records the changed
/// regions, and reindents them from an idle timer once you stop typing.  So
/// every workflow types real keys with `execute-kbd-macro' -- which only
/// reaches the buffer of the *selected window*, hence `set-window-buffer' --
/// and then lets the editor go idle.
///
/// Batch Emacs has no command loop to notice idleness, so `agi-test-idle' runs
/// the timers on `timer-idle-list' the way the command loop does when you pause.
/// It is generic timer machinery: it names nothing belonging to the package,
/// and the package's own `after-change-functions' entry, idle timer, guards,
/// `indent-region' calls and `before-save-hook' entry all run for real.
///
/// The work buffer is left modified, because a buffer you have been typing in
/// is modified; one of the package's internal guards is `(null
/// (buffer-modified-p))', and pinning the guard against an artificially
/// unmodified buffer would test the fixture rather than the package.
const AGGRESSIVE_INDENT_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun agi-test-idle ()
  "Let the editor go idle, running the timers the command loop would run."
  (dolist (timer (copy-sequence timer-idle-list))
    (timer-event-handler timer)))

(defmacro agi-test-with-buffer (mode text &rest body)
  "Type into a window-displayed buffer in MODE holding TEXT."
  `(let ((buffer (generate-new-buffer "*aggressive-indent-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (funcall ,mode)
           (insert ,text)
           (goto-char (point-min))
           (aggressive-indent-mode 1)
           ,@body)
       (kill-buffer buffer))))

(defun agi-test-state ()
  "Everything the user can see: the text, where point is, and the mode state."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :mode aggressive-indent-mode
        :electric (bound-and-true-p electric-indent-mode)))

(defun agi-test-text ()
  (buffer-substring-no-properties (point-min) (point-max)))

(defconst agi-test-lisp-defun
  "(defun handler (request)\n  (message \"start\")\n  (process request))\n")

(defconst agi-test-nested-lisp-defun
  "(defun handler (request)\n  (when request\n    (message \"start\")\n    (process request)))\n")

(defconst agi-test-c-function
  "int handler(int ready) {\n  log(\"start\");\n  process(ready);\n}\n")

(defun agi-test-sandbox-file (name)
  (let ((path (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
    (make-directory (file-name-directory path) t)
    (when (file-exists-p path) (delete-file path))
    path))

(defun agi-test-file-contents (path)
  (when (file-exists-p path)
    (with-temp-buffer
      (let ((coding-system-for-read 'utf-8))
        (insert-file-contents path))
      (buffer-string))))
"##;

fn aggressive_indent_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AGGRESSIVE_INDENT_MELPA_PIN, "aggressive-indent.el")
        .expect("prepare pinned aggressive-indent source below ./tmp")
        .with_prelude(AGGRESSIVE_INDENT_TEST_PRELUDE)
        .with_timeout(AGGRESSIVE_INDENT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed aggressive-indent parity test")
        .into()
}

/// Multi-probe batch for `assert_aggressive_indent_parity` cases (2a).
pub(crate) fn assert_aggressive_indent_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        aggressive_indent_oracle(),
        &name,
        "aggressive_indent_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn aggressive_indent_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_aggressive_indent_batch(&cases);
}

// END generated package batch tests
