use std::time::Duration;

use crate::{ALCHEMIST_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod completion;
mod eval;
mod help_navigation;
mod mix_test;
mod modes;
mod process;
mod project;
mod utils_scope;
mod workflows;

const ALCHEMIST_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const ALCHEMIST_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar byte-compile-current-file nil
  "Compatibility declaration for Alchemist's legacy macros.")

;;; Recorded output of the real Elixir toolchain.
;;
;; Elixir 1.18.4 / Erlang OTP 27 (erts-15.2.7.8), obtained with
;; `nix shell nixpkgs#elixir'.  Checked first that this is the Elixir build
;; tool and not a same-named different program: nixpkgs also ships an
;; unrelated `alan', and `mix' here reports "Mix 1.18.4".
;;
;; Captured by running `mix test --seed 0' against a real `mix new' project
;; whose suite has two passing tests, one failing assertion and one raising
;; test.  The seed is fixed so ExUnit's ordering is stable; the elapsed-time
;; and max_cases lines are frozen inside the recording, which is the reason
;; the suite replays a recording rather than shelling out at test time.
;;
;; These constants are written to disk and diffed against on every run --
;; a fixture that is another language's output can be mangled by escaping
;; without anything signalling.
(defconst alchemist-test-recording-full
  "Compiling 1 file (.ex)\nGenerated parity_project app\nRunning ExUnit with seed: 0, max_cases: 64\n\n...\n\n  1) test detects a wrong total (ParityProjectTest)\n     test/parity_project_test.exs:13\n     Assertion with == failed\n     code:  assert Enum.sum([1, 2, 3]) == 7\n     left:  6\n     right: 7\n     stacktrace:\n       test/parity_project_test.exs:14: (test)\n\n\n\n  2) test raises on bad input (ParityProjectTest)\n     test/parity_project_test.exs:17\n     ** (ArgumentError) deliberate failure for the parity fixture\n     code: ParityProject.explode()\n     stacktrace:\n       (parity_project 0.1.0) lib/parity_project.ex:20: ParityProject.explode/0\n       test/parity_project_test.exs:18: (test)\n\n\nFinished in 0.08 seconds (0.00s async, 0.08s sync)\n1 doctest, 4 tests, 2 failures\n")

(defconst alchemist-test-recording-at-point
  "Running ExUnit with seed: 0, max_cases: 64\nExcluding tags: [:test]\nIncluding tags: [location: {\"test/parity_project_test.exs\", 14}]\n\n\n\n  1) test detects a wrong total (ParityProjectTest)\n     test/parity_project_test.exs:13\n     Assertion with == failed\n     code:  assert Enum.sum([1, 2, 3]) == 7\n     left:  6\n     right: 7\n     stacktrace:\n       test/parity_project_test.exs:14: (test)\n\n\nFinished in 0.06 seconds (0.00s async, 0.06s sync)\n1 doctest, 4 tests, 1 failure, 4 excluded\n")

(defconst alchemist-test-recording-stale
  "Running ExUnit with seed: 0, max_cases: 64\n\n...\n\n  1) test detects a wrong total (ParityProjectTest)\n     test/parity_project_test.exs:13\n     Assertion with == failed\n     code:  assert Enum.sum([1, 2, 3]) == 7\n     left:  6\n     right: 7\n     stacktrace:\n       test/parity_project_test.exs:14: (test)\n\n\n\n  2) test raises on bad input (ParityProjectTest)\n     test/parity_project_test.exs:17\n     ** (ArgumentError) deliberate failure for the parity fixture\n     code: ParityProject.explode()\n     stacktrace:\n       (parity_project 0.1.0) lib/parity_project.ex:20: ParityProject.explode/0\n       test/parity_project_test.exs:18: (test)\n\n\nFinished in 0.08 seconds (0.00s async, 0.08s sync)\n1 doctest, 4 tests, 2 failures\n")

(defconst alchemist-test-recording-pass
  "Running ExUnit with seed: 0, max_cases: 64\n\n...\nFinished in 0.07 seconds (0.00s async, 0.07s sync)\n1 doctest, 2 tests, 0 failures\n")

(defconst alchemist-test-recording-version
  "Erlang/OTP 27 [erts-15.2.7.8] [source] [64-bit] [smp:32:32] [ds:32:32:10] [async-threads:1] [jit:ns]\n\nElixir 1.18.4 (compiled with Erlang/OTP 27)\n")

(defun alchemist-test-write (path contents)
  "Write CONTENTS to PATH, creating its directory."
  (make-directory (file-name-directory path) t)
  (write-region contents nil path nil 'silent)
  path)

(defun alchemist-test-file-contents (path)
  "The literal bytes of PATH."
  (with-temp-buffer
    (insert-file-contents-literally path)
    (buffer-string)))

(defun alchemist-test-install-recordings (directory)
  "Write every recording into DIRECTORY and verify the bytes that landed."
  (let (mismatched)
    (dolist (pair (list (cons "full" alchemist-test-recording-full)
                        (cons "at-point" alchemist-test-recording-at-point)
                        (cons "stale" alchemist-test-recording-stale)
                        (cons "pass" alchemist-test-recording-pass)
                        (cons "version" alchemist-test-recording-version)))
      (let ((path (expand-file-name (concat "out-" (car pair) ".txt") directory)))
        (alchemist-test-write path (cdr pair))
        (unless (equal (alchemist-test-file-contents path) (cdr pair))
          (push (car pair) mismatched))))
    (nreverse mismatched)))

(defun alchemist-test-install-standins (directory log)
  "Write the mix and elixir stand-ins into DIRECTORY, logging to LOG.

Both answer according to their own argument vector, so a workflow witnesses
which command the package built rather than pinning one canned reply, and
both record every invocation so what is asserted about the argument vector
is a recording of what the package sent."
  (let ((mix (expand-file-name "mix-standin" directory))
        (elixir (expand-file-name "elixir-standin" directory)))
    (alchemist-test-write
     mix
     (concat "#!/bin/sh\n"
             "{ printf 'cwd=%s\\n' \"$(pwd)\"\n"
             "  printf 'argv:'\n"
             "  for argument in \"$@\"; do printf ' [%s]' \"$argument\"; done\n"
             "  printf '\\n'\n"
             "} >> \"$ALCHEMIST_LOG\"\n"
             "case \" $* \" in\n"
             "  *' --stale '*) key=stale ;;\n"
             "  *:[0-9]*) key=at-point ;;\n"
             "  *) key=\"${ALCHEMIST_MIX_REPLY:-full}\" ;;\n"
             "esac\n"
             "cat \"$ALCHEMIST_REC/out-$key.txt\"\n"
             "if [ \"$key\" = pass ]; then exit 0; else exit 2; fi\n"))
    (set-file-modes mix #o755)
    (alchemist-test-write
     elixir
     (concat "#!/bin/sh\n"
             "{ printf 'elixir argv:'\n"
             "  for argument in \"$@\"; do printf ' [%s]' \"$argument\"; done\n"
             "  printf '\\n'\n"
             "} >> \"$ALCHEMIST_LOG\"\n"
             "cat \"$ALCHEMIST_REC/out-version.txt\"\n"))
    (set-file-modes elixir #o755)
    (write-region "" nil log nil 'silent)
    (setenv "ALCHEMIST_LOG" log)
    (setenv "ALCHEMIST_REC" (directory-file-name directory))
    (cons mix elixir)))

(defun alchemist-test-make-project (root)
  "Create a real Elixir project layout below ROOT and return its directory.

The test file's line numbers are the ones the recording refers to, so a
button that claims line 14 can be checked against what is on line 14."
  (let ((project (file-name-as-directory (expand-file-name "parity_project" root))))
    (alchemist-test-write (expand-file-name "mix.exs" project)
                          "defmodule ParityProject.MixProject do\nend\n")
    (alchemist-test-write
     (expand-file-name "lib/parity_project.ex" project)
     "defmodule ParityProject do\n  def hello do\n    :world\n  end\nend\n")
    (alchemist-test-write
     (expand-file-name "test/parity_project_test.exs" project)
     (concat "defmodule ParityProjectTest do\n"
             "  use ExUnit.Case\n"
             "  doctest ParityProject\n"
             "\n"
             "  test \"greets the world\" do\n"
             "    assert ParityProject.hello() == :world\n"
             "  end\n"
             "\n"
             "  test \"computes a sum\" do\n"
             "    assert Enum.sum([1, 2, 3]) == 6\n"
             "  end\n"
             "\n"
             "  test \"detects a wrong total\" do\n"
             "    assert Enum.sum([1, 2, 3]) == 7\n"
             "  end\n"
             "\n"
             "  test \"raises on bad input\" do\n"
             "    ParityProject.explode()\n"
             "  end\n"
             "end\n"))
    project))

(defun alchemist-test-await-report ()
  "Wait until the test report buffer stops changing, and return it.

The sentinel is the last writer here -- it renders the buttons after the
process exits -- so waiting for the process to die would capture a buffer
that is about to change."
  (let ((report (get-buffer alchemist-test-report-buffer-name))
        (previous :none) (stable 0) (rounds 0))
    (while (and (< stable 5) (< rounds 600))
      (accept-process-output nil 0.05)
      (setq rounds (1+ rounds))
      (setq report (get-buffer alchemist-test-report-buffer-name))
      (let ((now (and report (buffer-live-p report)
                      (with-current-buffer report (buffer-string)))))
        (if (equal now previous) (setq stable (1+ stable))
          (setq stable 0 previous now))))
    report))

(defun alchemist-test-report-text ()
  "The report buffer's text."
  (with-current-buffer alchemist-test-report-buffer-name
    (copy-sequence
     (buffer-substring-no-properties (point-min) (point-max)))))

(defun alchemist-test-report-buttons ()
  "Every button the renderer produced, with its face and file property."
  (with-current-buffer alchemist-test-report-buffer-name
    (let ((position (point-min)) buttons)
      (while (setq position (next-button position))
        (push (list (copy-sequence (button-label position))
                    (button-get position 'face)
                    (copy-sequence (button-get position 'file)))
              buttons)
        (setq position (button-end position)))
      (nreverse buttons))))

(defun alchemist-test-invocations (log base)
  "The stand-ins' recorded invocations, with BASE masked.

BASE is masked both with and without its trailing slash: the process's
working directory is logged without one, so masking only the slashed form
leaves an absolute path in the snapshot that the oracle's own sandbox
normaliser then rewrites -- which works under the harness and nowhere else."
  (with-temp-buffer
    (insert-file-contents log)
    (let ((text (buffer-string)))
      (dolist (form (list (file-name-as-directory base)
                          (directory-file-name base)))
        (setq text (replace-regexp-in-string
                    (regexp-quote form) "[PROJECT]" text t t)))
      text)))
"##;

fn alchemist_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALCHEMIST_MELPA_PIN, "alchemist.el")
        .expect("prepare pinned Alchemist source and dependencies below ./tmp")
        .with_prelude(ALCHEMIST_TEST_PRELUDE)
        .with_timeout(ALCHEMIST_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Alchemist parity test")
        .into()
}

/// Multi-probe batch for `assert_alchemist_parity` cases (2a).
pub(crate) fn assert_alchemist_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(alchemist_oracle(), &name, "alchemist_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn alchemist_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        completion::completion_public_surface_batch_cases(),
        eval::eval_public_surface_batch_cases(),
        help_navigation::help_navigation_public_surface_batch_cases(),
        mix_test::mix_test_public_surface_batch_cases(),
        modes::modes_public_surface_batch_cases(),
        process::process_public_surface_batch_cases(),
        project::project_public_surface_batch_cases(),
        utils_scope::utils_scope_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_alchemist_batch(&cases);
}

// END generated package batch tests
