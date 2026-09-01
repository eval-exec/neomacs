use std::time::Duration;

use crate::{AC_ALCHEMIST_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_ALCHEMIST_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ac-alchemist is an `auto-complete` source answered by a real
/// alchemist-server subprocess, which alchemist starts as
/// `elixir <alchemist>/alchemist-server/run.exs dev` inside the current Mix
/// project.  Only that process boundary is replaced: every workflow installs a
/// recording `elixir` stand-in first on `PATH` which reads the request lines
/// alchemist writes (`COMP ...`, `DOCL ...`) and answers with canned server
/// output, end markers included.  Everything above it — project discovery,
/// process startup, request construction, the process filter, candidate
/// parsing, popup items, `ac-complete` and the documentation lookup — is the
/// package's own code.
///
/// The stand-in appends its log line only after the answer is already on the
/// pipe, so `ac-alchemist-test-await' can wait for a definite number of
/// answers and then drain the pipe.  That makes the asynchronous workflows
/// deterministic without stubbing a single package function.
const AC_ALCHEMIST_TEST_PRELUDE: &str = r##"
(defvar byte-compile-current-file nil
  "Compatibility declaration for Alchemist's legacy macros.")

(require 'cl-lib)

(defconst ac-alchemist-test-mix-exs
  "defmodule Blogpost.MixProject do\n  use Mix.Project\n\n  def project do\n    [app: :blogpost, version: \"0.1.0\", elixir: \"~> 1.14\", deps: deps()]\n  end\n\n  def application do\n    [extra_applications: [:logger]]\n  end\n\n  defp deps, do: []\nend\n")

(defun ac-alchemist-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ac-alchemist-test-write (name text)
  "Write TEXT into sandbox file NAME and return its absolute path."
  (let ((path (ac-alchemist-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun ac-alchemist-test-canned (code hint text)
  "Record TEXT as the alchemist server answer to a CODE request for HINT."
  (ac-alchemist-test-write
   (format "canned/%s-%s.txt" code
           (replace-regexp-in-string "[^A-Za-z0-9._]" "_" hint))
   (if (string= text "")
       (format "END-OF-%s\n" code)
     (format "%s\nEND-OF-%s\n" text code))))

(defun ac-alchemist-test-start-project (responses)
  "Create a real Mix project and a recording `elixir' stand-in on PATH.

RESPONSES is a list of (CODE HINT TEXT) canned alchemist server answers.
A request with no canned answer receives the bare end marker real Elixir
returns for a hint that matches nothing."
  (ac-alchemist-test-write "blogpost/mix.exs" ac-alchemist-test-mix-exs)
  (dolist (code '("COMP" "DOCL" "DEFL" "EVAL" "INFO"))
    (ac-alchemist-test-canned code "default" ""))
  (dolist (response responses)
    (apply #'ac-alchemist-test-canned response))
  (set-file-modes
   (ac-alchemist-test-write
    "bin/elixir"
    (concat "#!/bin/sh\n"
            "printf 'ARGV %s\\n' \"$*\" >> \"$AC_ALCHEMIST_LOG\"\n"
            "while IFS= read -r line; do\n"
            "  printf 'REQ %s\\n' \"$line\" >> \"$AC_ALCHEMIST_LOG\"\n"
            "  code=${line%% *}\n"
            "  rest=${line#*\\\"}\n"
            "  hint=${rest%%\\\"*}\n"
            "  key=$(printf '%s' \"$hint\" | tr -c 'A-Za-z0-9._' '_')\n"
            "  file=\"$AC_ALCHEMIST_CANNED/$code-$key.txt\"\n"
            "  [ -f \"$file\" ] || file=\"$AC_ALCHEMIST_CANNED/$code-default.txt\"\n"
            "  cat \"$file\"\n"
            "  printf 'ANS %s\\n' \"$code\" >> \"$AC_ALCHEMIST_LOG\"\n"
            "done\n"))
   #o755)
  (setenv "AC_ALCHEMIST_LOG" (ac-alchemist-test-path "elixir.log"))
  (setenv "AC_ALCHEMIST_CANNED" (ac-alchemist-test-path "canned"))
  (setenv "PATH" (concat (ac-alchemist-test-path "bin") path-separator (getenv "PATH"))))

(defun ac-alchemist-test-visit (name text site)
  "Visit sandbox Elixir file NAME holding TEXT and put point after SITE.

The buffer is displayed, not merely made current, because auto-complete
draws its candidate menu into the selected window."
  (let ((buffer (find-file-noselect (ac-alchemist-test-write name text))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (goto-char (point-min))
    (search-forward site)
    buffer))

(defun ac-alchemist-test-elixir-log ()
  "Return the argv, request and answer lines recorded by the stand-in."
  (let ((log (ac-alchemist-test-path "elixir.log"))
        (alchemist (file-name-directory (locate-library "alchemist-server"))))
    (if (not (file-exists-p log))
        'no-elixir-process-started
      (with-temp-buffer
        (insert-file-contents log)
        (mapcar (lambda (line)
                  (replace-regexp-in-string
                   (regexp-quote alchemist) "<alchemist>/" line t t))
                (split-string (buffer-string) "\n" t))))))

(defun ac-alchemist-test-answers ()
  "Return how many answers the `elixir' stand-in finished writing."
  (let ((log (ac-alchemist-test-elixir-log)))
    (if (listp log)
        (length (cl-remove-if-not (lambda (line) (string-prefix-p "ANS " line)) log))
      0)))

(defun ac-alchemist-test-await (answers)
  "Block until the stand-in wrote ANSWERS replies and Emacs drained them.

A reply reaches the pipe before its log line, so draining until a read
times out hands every answered byte to the package's process filter."
  (let ((process (alchemist-server-process))
        (deadline (+ (float-time) 30)))
    (while (and (< (ac-alchemist-test-answers) answers) (< (float-time) deadline))
      (accept-process-output process 0.05))
    (while (accept-process-output process 0.05))
    (ac-alchemist-test-answers)))

(defun ac-alchemist-test-shutdown ()
  "Kill every alchemist server process a workflow started."
  (dolist (entry alchemist-server-processes)
    (when (process-live-p (cdr entry))
      (set-process-query-on-exit-flag (cdr entry) nil)
      (delete-process (cdr entry))))
  (setq alchemist-server-processes nil)
  (dolist (name '("*alchemist-server*" "*alchemist help*"))
    (when (get-buffer name)
      (kill-buffer name))))

(defmacro ac-alchemist-test-session (responses &rest body)
  "Run BODY in a real Mix project answered by canned RESPONSES, then clean up."
  (declare (indent 1))
  `(let ((ac-alchemist--output-cache nil)
         (ac-alchemist--candidate-cache nil)
         (ac-alchemist--prefix nil)
         (ac-alchemist--document nil)
         (original-path (getenv "PATH")))
     (unwind-protect
         (progn
           (ac-alchemist-test-start-project ,responses)
           ,@body)
       (ac-alchemist-test-shutdown)
       (setenv "PATH" original-path)
       (dolist (buffer (buffer-list))
         (when (buffer-file-name buffer)
           (with-current-buffer buffer (set-buffer-modified-p nil))
           (kill-buffer buffer))))))
"##;

fn ac_alchemist_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_ALCHEMIST_MELPA_PIN, "ac-alchemist.el")
        .expect("prepare pinned ac-alchemist source below ./tmp")
        .with_prelude(AC_ALCHEMIST_TEST_PRELUDE)
        .with_timeout(AC_ALCHEMIST_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-alchemist parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_alchemist_parity` cases (2a).
pub(crate) fn assert_ac_alchemist_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_alchemist_oracle(), &name, "ac_alchemist_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_alchemist_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_alchemist_batch(&cases);
}

// END generated package batch tests
