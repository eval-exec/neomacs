use std::time::Duration;

use crate::{AMD_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AMD_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(240);

/// Helpers shared by the workflows.
///
/// amd-mode edits the `define([...], function(...) {...})` header of a
/// JavaScript AMD module: it keeps the dependency array and the function's
/// parameter list in step as you add, remove and reorder modules, works out the
/// module path of a file relative to the projectile project, and finds the
/// modules that require the current one by shelling out to `ag'.  Every
/// workflow here builds a real project tree in the sandbox, opens a real file
/// in `js2-mode' with `amd-mode' on, and asserts the resulting buffer text.
///
/// Two environmental boundaries are stood in for, and only these two.
///
/// `ag' is a real external program that need not be installed, so
/// `amd-test-configure-ag' installs a recording stand-in on PATH and in
/// `exec-path' - `executable-find' consults the latter - which writes its argv
/// to a log and prints the search output the workflow chose.  Everything after
/// the process call is the package's own: the regexp it built, the parsing of
/// `file:line:match', the false-positive filter and the xref construction.
///
/// `amd--import' always reads the module's local name from the minibuffer, and
/// `amd-import-file' reads the file through `projectile-completing-read'.
/// Unattended minibuffer input is a permitted double; `amd-test-answering'
/// supplies both answers and nothing else is faked.  Answering `read-string'
/// with the empty string is what a user pressing RET at the prompt does, and
/// makes the package fall back to its own default name.
///
/// `amd-test-idle' runs the idle timers the command loop would run.  js2-mode
/// reparses the buffer from an idle timer, so between two amd-mode commands
/// that both consult the AST the parse is otherwise stale and the second
/// command reads the pre-edit tree - see HARNESS-NOTES.md on idle timers in
/// batch.
const AMD_MODE_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(setq js2-mode-show-parse-errors nil
      js2-mode-show-strict-warnings nil)

(defun amd-test-project (name)
  "Create a projectile project called NAME in the sandbox and return its root."
  (let ((root (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
    (make-directory root t)
    (write-region "" nil (expand-file-name ".projectile" root) nil 'silent)
    (file-name-as-directory root)))

(defun amd-test-write (root relative contents)
  "Write CONTENTS to RELATIVE below ROOT and return the file name."
  (let ((file (expand-file-name relative root)))
    (make-directory (file-name-directory file) t)
    (write-region contents nil file nil 'silent)
    file))

(defun amd-test-open (root relative contents)
  "Open RELATIVE below ROOT as a parsed js2 buffer with amd-mode on."
  (let ((buffer (find-file-noselect (amd-test-write root relative contents))))
    (with-current-buffer buffer
      (js2-mode)
      (amd-mode 1)
      (js2-parse))
    ;; `execute-kbd-macro' only reaches the buffer of the selected window.
    (set-window-buffer (selected-window) buffer)
    buffer))

(defmacro amd-test-in (buffer &rest body)
  "Run BODY in BUFFER, then discard it without writing it back."
  (declare (indent 1))
  `(with-current-buffer ,buffer
     (unwind-protect (progn ,@body)
       (set-buffer-modified-p nil)
       (kill-buffer ,buffer))))

(defmacro amd-test-answering (module file &rest body)
  "Run BODY with amd-mode's two minibuffer reads answered by MODULE and FILE."
  (declare (indent 2))
  `(cl-letf (((symbol-function 'read-string) (lambda (&rest _) ,module))
             ((symbol-function 'projectile-completing-read) (lambda (&rest _) ,file)))
     ,@body))

(defun amd-test-idle ()
  "Let the editor go idle, which is when js2-mode reparses the buffer."
  (dolist (timer (copy-sequence timer-idle-list))
    (timer-event-handler timer)))

(defun amd-test-text ()
  "The buffer's text, with no properties."
  (buffer-substring-no-properties (point-min) (point-max)))

(defun amd-test-configure-ag (root output)
  "Install a recording `ag' stand-in for ROOT that prints OUTPUT.
Returns the log file it records its argv to."
  (let ((program (expand-file-name "ag" root))
        (log-file (expand-file-name "ag.log" root)))
    (write-region
     "#!/bin/sh\nprintf '<%s>\\n' \"$@\" > \"$AMD_TEST_AG_LOG\"\nprintf '%s' \"$AMD_TEST_AG_OUTPUT\"\n"
     nil program nil 'silent)
    (set-file-modes program #o755)
    (setenv "AMD_TEST_AG_LOG" log-file)
    (setenv "AMD_TEST_AG_OUTPUT" output)
    (add-to-list 'exec-path root)
    (setenv "PATH" (concat root path-separator (getenv "PATH")))
    log-file))

(defun amd-test-ag-arguments (log-file)
  "The argv the stand-in `ag' recorded, one string per argument."
  (with-temp-buffer
    (insert-file-contents log-file)
    (let (arguments)
      (goto-char (point-min))
      (while (re-search-forward "^<\\(.*\\)>$" nil t)
        (push (match-string-no-properties 1) arguments))
      (nreverse arguments))))

(defun amd-test-xref-text (project-name)
  "The `*xref*' buffer's text with each path cut back to PROJECT-NAME.
xref renders a group heading as a path relative to the enclosing VC
repository, which here is the Neomacs checkout the sandbox lives inside, so
the raw text carries a prefix that has nothing to do with the package."
  (if (not (get-buffer "*xref*"))
      :no-xref-buffer
    (with-current-buffer "*xref*"
      (replace-regexp-in-string
       (concat "^.*/" (regexp-quote project-name) "/") ""
       (buffer-substring-no-properties (point-min) (point-max))))))

(defconst amd-test-two-module-source
  "define([\n    'lib/router',\n    'widgets/button'\n], function(router, button) {\n    return router;\n});\n"
  "A module that already requires two dependencies, named in the same order.")
"##;

fn amd_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AMD_MODE_MELPA_PIN, "amd-mode.el")
        .expect("prepare pinned amd-mode source and dependencies below ./tmp")
        .with_prelude(AMD_MODE_TEST_PRELUDE)
        .with_timeout(AMD_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed amd-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_amd_mode_parity` cases (2a).
pub(crate) fn assert_amd_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(amd_mode_oracle(), &name, "amd_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn amd_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_amd_mode_batch(&cases);
}

// END generated package batch tests
