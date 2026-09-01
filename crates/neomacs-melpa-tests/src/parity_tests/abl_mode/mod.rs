use std::time::Duration;

use crate::{ABL_MODE_MELPA_PIN, CachedMelpaOracle, F_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ABL_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Sandbox helpers shared by the workflows.
///
/// abl-mode is a Python TDD minor mode: it locates the project of the visited
/// file, derives a per-branch shell buffer name, and drives `python', `pytest',
/// `black', `isort' and `workon' inside a real `shell' buffer.  The workflows
/// therefore build a real git-controlled Python project below the per-case
/// sandbox and install recording stand-ins for those command line tools, plus a
/// recording interactive shell that logs every line abl-mode sends through
/// comint and prints `abl-ready' once the line has finished.  abl-mode keeps
/// running its real project detection, option parsing, command composition and
/// comint path; only the external tools are stand-ins.
///
/// abl-mode declares no MELPA dependencies but calls into `f' and `s' at run
/// time, so both pinned packages are installed and required for real.
const ABL_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'subr-x)
(require 'f)
(require 's)

(defvar abl-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar abl-test-bin
  (file-name-as-directory (expand-file-name "bin" abl-test-root)))

(defvar abl-test-command-log (expand-file-name "commands.log" abl-test-root))
(defvar abl-test-directory-log (expand-file-name "directories.log" abl-test-root))
(defvar abl-test-shell-log (expand-file-name "shell.log" abl-test-root))

(defun abl-test-relative (path)
  (file-relative-name path abl-test-root))

(defun abl-test-write-executable (name body)
  (let ((path (expand-file-name name abl-test-bin)))
    (make-directory abl-test-bin t)
    (with-temp-buffer
      (insert body)
      (write-region (point-min) (point-max) path nil 'silent))
    (set-file-modes path #o755)
    path))

(defun abl-test-install-recorder (name)
  "Install NAME as a stand-in tool recording its exact argv and directory."
  (abl-test-write-executable
   name
   (concat "#!/bin/sh\n"
           "{ printf '%s' \"" name "\"\n"
           "  for argument in \"$@\"; do printf '|%s' \"$argument\"; done\n"
           "  printf '\\n'; } >> \"$ABL_COMMAND_LOG\"\n"
           "printf '%s\\n' \"$PWD\" >> \"$ABL_DIRECTORY_LOG\"\n"
           "exit 0\n")))

(defun abl-test-install-shell ()
  "Install the recording interactive shell abl-mode's comint buffer runs."
  (abl-test-write-executable
   "abl-test-shell"
   (concat "#!/bin/sh\n"
           "while IFS= read -r line; do\n"
           "  printf '%s\\n' \"$line\" >> \"$ABL_SHELL_LOG\"\n"
           "  sh -c \"$line\"\n"
           "  printf 'abl-ready\\n'\n"
           "done\n")))

(defun abl-test-setup (&rest tools)
  "Install the recording shell and TOOLS, and put them first on `PATH'."
  (make-directory abl-test-bin t)
  (dolist (tool tools) (abl-test-install-recorder tool))
  (abl-test-install-shell)
  (setenv "ABL_COMMAND_LOG" abl-test-command-log)
  (setenv "ABL_DIRECTORY_LOG" abl-test-directory-log)
  (setenv "ABL_SHELL_LOG" abl-test-shell-log)
  (setenv "PATH" (concat (directory-file-name abl-test-bin)
                         path-separator (getenv "PATH")))
  (setq explicit-shell-file-name
        (expand-file-name "abl-test-shell" abl-test-bin))
  abl-test-bin)

(defun abl-test-log-lines (path)
  (if (file-exists-p path)
      (with-temp-buffer
        (insert-file-contents path)
        (split-string (buffer-string) "\n" t))
    'nothing-recorded))

(defun abl-test-commands ()
  "Return `TOOL|ARGUMENT...' for every stand-in tool invocation."
  (abl-test-log-lines abl-test-command-log))

(defun abl-test-directories ()
  "Return the sandbox-relative directory each stand-in tool ran in."
  (let ((lines (abl-test-log-lines abl-test-directory-log)))
    (if (listp lines) (mapcar #'abl-test-relative lines) lines)))

(defun abl-test-shell-inputs ()
  "Return every command line abl-mode sent to the shell process."
  (abl-test-log-lines abl-test-shell-log))

(defun abl-test-shell-text (name)
  (let ((buffer (get-buffer name)))
    (if (not buffer)
        'no-shell-buffer
      (with-current-buffer buffer
        (buffer-substring-no-properties (point-min) (point-max))))))

(defun abl-test-ready-count (name)
  (let ((buffer (get-buffer name)))
    (if (not buffer)
        0
      (with-current-buffer buffer
        (let ((count 0))
          (save-excursion
            (goto-char (point-min))
            (while (search-forward "abl-ready\n" nil t)
              (setq count (1+ count))))
          count)))))

(defun abl-test-wait-for-shell (name count)
  "Block until the shell buffer NAME reported COUNT finished commands."
  (let ((deadline (+ (float-time) 60)))
    (while (and (< (float-time) deadline)
                (< (abl-test-ready-count name) count))
      (let* ((buffer (get-buffer name))
             (process (and buffer (get-buffer-process buffer))))
        (if process
            (accept-process-output process 0.05)
          (sleep-for 0.05))))
    (abl-test-ready-count name)))

(defun abl-test-git (directory &rest arguments)
  (let ((default-directory (file-name-as-directory directory)))
    (apply #'call-process "git" nil nil nil arguments)))

(defconst abl-test-unicode-tests
  "import unittest


class ÜnicodeTests(unittest.TestCase):

    def test_encodes_a_name(self):
        self.assertEqual(\"Ünïcode\", \"Ünïcode\")

    def test_rejects_empty_input(self):
        self.assertFalse(\"\")
")

(defconst abl-test-service-tests
  "import unittest


def test_service_root():
    assert True
")

(defconst abl-test-conftest
  "import os


SETTINGS = {\"locale\": \"tr_TR\"}
")

(defun abl-test-project (&optional abl-file)
  "Create the shared git-controlled Python project and return its base.

The project directory and one test module carry non-ASCII characters, the
branch name carries a slash, and one test module sits in a directory whose
name contains a space, so path handling, branch mangling and command line
composition all stay visible in the recorded commands."
  (let ((files (list (cons "pyproject.toml"
                           "[project]\nname = \"ünïcode-projekt\"\n")
                     (cons "conftest.py" abl-test-conftest)
                     (cons "tests/ünïcode_tests.py" abl-test-unicode-tests)
                     (cons "tests/api layer/service_tests.py"
                           abl-test-service-tests)))
        (base (file-name-as-directory
               (expand-file-name "ünïcode-projekt" abl-test-root))))
    (when abl-file (setq files (cons (cons ".abl" abl-file) files)))
    (make-directory base t)
    (dolist (file files)
      (let ((path (expand-file-name (car file) base)))
        (make-directory (file-name-directory path) t)
        (with-temp-buffer
          (insert (cdr file))
          (write-region (point-min) (point-max) path nil 'silent))))
    (abl-test-git base "init" "--quiet")
    (abl-test-git base "config" "user.email" "abl@example.invalid")
    (abl-test-git base "config" "user.name" "Abl Tester")
    (abl-test-git base "checkout" "--quiet" "-b" "feature/ünïcode-tests")
    (abl-test-git base "add" "--all")
    (abl-test-git base "commit" "--quiet" "-m" "initial")
    base))

(defun abl-test-loose-file ()
  "Create a Python file that is not inside any Python project."
  (let ((path (expand-file-name "scratch/notes.py" abl-test-root)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert "print(\"no project here\")\n")
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun abl-test-virtualenv (name)
  "Create an existing virtualenv NAME below the default base directory."
  (let ((path (expand-file-name name (expand-file-name "~/.virtualenvs"))))
    (make-directory (expand-file-name "bin" path) t)
    path))

(defun abl-test-message-mark ()
  (with-current-buffer (get-buffer-create "*Messages*") (point-max)))

(defun abl-test-messages-since (mark)
  "Return the messages logged since MARK."
  (with-current-buffer (get-buffer-create "*Messages*")
    (split-string
     (buffer-substring-no-properties (min mark (point-max)) (point-max))
     "\n" t)))
"##;

fn abl_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ABL_MODE_MELPA_PIN, "abl-mode.el")
        .expect("prepare pinned abl-mode source below ./tmp")
        .with_melpa_dependency(F_MELPA_PIN)
        .expect("prepare pinned f source below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s source below ./tmp")
        .with_prelude(ABL_MODE_TEST_PRELUDE)
        .with_timeout(ABL_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed abl-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_abl_mode_parity` cases (2a).
pub(crate) fn assert_abl_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(abl_mode_oracle(), &name, "abl_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn abl_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_abl_mode_batch(&cases);
}

// END generated package batch tests
