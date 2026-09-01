use std::time::Duration;

use crate::{AC_HASKELL_PROCESS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_HASKELL_PROCESS_TEST_TIMEOUT: Duration = Duration::from_secs(240);

/// Helpers shared by the workflows.
///
/// ac-haskell-process is an auto-complete source backed by the inferior Haskell
/// process haskell-mode runs, plus a hoogle-backed documentation popup.  Its
/// whole surface is the four cells of `ac-source-haskell-process', the command
/// that installs it, and the command that pops up documentation.
///
/// **The completion path cannot be driven in batch, and that is haskell-mode's
/// doing rather than the harness's.**  A real GHCi 9.10.3 was fetched and a real
/// session started - `haskell-process-start' returns a live process and GHCi's
/// banner arrives - but haskell-mode's asynchronous command queue never drains:
/// `haskell-process-cmd' is still non-nil after thirty seconds of pumping, and
/// the interactive buffer sits at "Restarting process ...".
/// `haskell-process-get-repl-completions' blocks on that queue, so any workflow
/// reaching it hangs, with a real GHCi or with a stand-in - the blockage is
/// above the subprocess.  Everything except that one call is covered here, and
/// the no-session branch of `ac-haskell-process-candidates' is asserted
/// directly.
///
/// `popup-tip' has the same shape: it waits for an event to dismiss the
/// tooltip, so an `ac-haskell-process-popup-doc' call that really has
/// documentation to show never returns in batch.  The branch that is covered is
/// the one where hoogle offers nothing, which is also what a user without
/// hoogle installed gets.
///
/// hoogle is stood in for, and unlike the ameba suite's linter its output is
/// **not** recorded from the real tool: hoogle needs a generated package
/// database, and building one here would mean building a Haskell package set.
/// The argv is therefore the load-bearing assertion in the documentation
/// workflow - it is what the package actually constructs - and the replayed
/// text is plausible filler, marked as such rather than passed off as recorded.
const AC_HASKELL_PROCESS_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defconst ac-haskell-test-source
  "module Main where\n\nimport Data.List\n\nmain :: IO ()\nmain = putStrLn (map id \"hello\")\n"
  "A small Haskell module the workflows open and move around in.")

(defun ac-haskell-test-project ()
  "Write the fixture module into the sandbox and return its file name."
  (let ((root (file-name-as-directory
               (expand-file-name "project" (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (make-directory root t)
    (let ((file (expand-file-name "Main.hs" root)))
      (write-region ac-haskell-test-source nil file nil 'silent)
      file)))

(defun ac-haskell-test-install-hoogle (output)
  "Install a recording `hoogle' stand-in printing OUTPUT, and return its log."
  (let* ((root (file-name-as-directory
                (expand-file-name "project" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
         (bin (expand-file-name "bin" root))
         (program (expand-file-name "hoogle" bin))
         (log-file (expand-file-name "hoogle.log" root)))
    (make-directory bin t)
    (write-region
     (concat "#!/bin/sh\n"
             "printf '<%s>\\n' \"$@\" >> \"$HOOGLE_TEST_LOG\"\n"
             "printf '%s' \"$HOOGLE_TEST_OUTPUT\"\n")
     nil program nil 'silent)
    (set-file-modes program #o755)
    (setenv "HOOGLE_TEST_LOG" log-file)
    (setenv "HOOGLE_TEST_OUTPUT" output)
    (write-region "" nil log-file nil 'silent)
    ;; `executable-find' consults `exec-path', not PATH.
    (add-to-list 'exec-path bin)
    (setenv "PATH" (concat bin path-separator (getenv "PATH")))
    log-file))

(defun ac-haskell-test-hoogle-arguments (log-file)
  "Every argument the stand-in hoogle recorded, in call order."
  (with-temp-buffer
    (insert-file-contents log-file)
    (let (arguments)
      (goto-char (point-min))
      (while (re-search-forward "^<\\(.*\\)>$" nil t)
        (push (match-string-no-properties 1) arguments))
      (nreverse arguments))))

(defun ac-haskell-test-open ()
  "Open the fixture module in `haskell-mode' in the selected window."
  (let ((buffer (find-file-noselect (ac-haskell-test-project))))
    (with-current-buffer buffer
      (haskell-mode))
    (set-window-buffer (selected-window) buffer)
    buffer))

(defmacro ac-haskell-test-in (buffer &rest body)
  "Run BODY in BUFFER, then discard it without writing it back."
  (declare (indent 1))
  `(with-current-buffer ,buffer
     (unwind-protect (progn ,@body)
       (set-buffer-modified-p nil)
       (kill-buffer ,buffer))))
"##;

fn ac_haskell_process_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_HASKELL_PROCESS_MELPA_PIN, "ac-haskell-process.el")
        .expect("prepare pinned ac-haskell-process source below ./tmp")
        .with_prelude(AC_HASKELL_PROCESS_TEST_PRELUDE)
        .with_timeout(AC_HASKELL_PROCESS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-haskell-process parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_haskell_process_parity` cases (2a).
pub(crate) fn assert_ac_haskell_process_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ac_haskell_process_oracle(),
        &name,
        "ac_haskell_process_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn ac_haskell_process_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_haskell_process_batch(&cases);
}

// END generated package batch tests
