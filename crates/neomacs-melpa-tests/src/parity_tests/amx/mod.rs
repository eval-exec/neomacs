use std::time::Duration;

use crate::{AMX_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AMX_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Five probe commands plus sandbox helpers.
///
/// amx ranks commands by how often they are run, so the fixture's names are
/// chosen to exercise all three documented sorting rules with no reliance on
/// the surrounding obarray: `amx-probe-open', `amx-probe-quit' and
/// `amx-probe-zoom' are all the same length (so alphabetical order decides),
/// `amx-probe-close' is one character longer and `amx-probe-refresh' three
/// longer.  Every assertion filters `amx-cache' down to these five, because the
/// cache holds every command in the editor and its absolute contents differ.
///
/// The `amx' command itself reads from the minibuffer, which cannot be driven
/// in batch (DIVERGENCES.md entry 1), so no workflow here depends on the
/// prompt.  Ranking is exercised through `amx-rank', the same public function
/// `amx-read-and-run' calls once the user's chosen command has run, and the
/// `M-x' takeover is asserted through the key binding rather than by invoking
/// it.  Nothing is stubbed: the cache, the save file and the command counting
/// are all real.
const AMX_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst amx-test-commands
  '(amx-probe-open amx-probe-quit amx-probe-zoom
    amx-probe-close amx-probe-refresh)
  "The probe commands every workflow filters the cache down to.")

(defun amx-probe-open () (interactive) 'open)
(defun amx-probe-quit () (interactive) 'quit)
(defun amx-probe-zoom () (interactive) 'zoom)
(defun amx-probe-close () (interactive) 'close)
(defun amx-probe-refresh () (interactive) 'refresh)
(defun amx-probe-mouse (event) (interactive "e") event)
(defun amx-probe-helper () 'not-a-command)

(defun amx-test-path (name)
  "Return the absolute sandbox path of NAME."
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun amx-test-order ()
  "Return the probe commands in cache order, with their run counts."
  (cl-loop for entry in amx-cache
           when (memq (car entry) amx-test-commands)
           collect (cons (car entry) (cdr entry))))

(defun amx-test-run (&rest commands)
  "Rank COMMANDS the way `amx-read-and-run' does after running each one."
  (dolist (command commands)
    (amx-rank command)))

(defun amx-test-read-save-file ()
  "Return the exact contents of the save file, or `no-save-file'."
  (let ((path (expand-file-name amx-save-file)))
    (if (file-exists-p path)
        (with-temp-buffer
          (insert-file-contents path)
          (buffer-string))
      'no-save-file)))

(defun amx-test-warnings ()
  "Return amx's own warning lines, without the editor's unrelated ones."
  (let ((buffer (get-buffer "*Warnings*"))
        lines)
    (when buffer
      (dolist (line (split-string (with-current-buffer buffer (buffer-string))
                                  "\n" t))
        (when (string-prefix-p "Warning (amx)" line)
          (push (substring-no-properties line) lines))))
    (nreverse lines)))

(defun amx-test-fresh-session ()
  "Forget everything amx learned, as a newly started editor would."
  (setq amx-initialized nil
        amx-cache nil
        amx-data nil
        amx-history nil
        amx-last-update-time nil))

(defun amx-test-setup (&optional history-length)
  "Point amx at a sandbox save file and shorten its history.
A three-entry history keeps the assertions independent of which other
commands the editor happens to define."
  (setq amx-save-file (amx-test-path "amx-items")
        amx-history-length (or history-length 3))
  (when (file-exists-p (amx-test-path "amx-items"))
    (delete-file (amx-test-path "amx-items")))
  (amx-test-fresh-session))

(defun amx-test-cleanup ()
  (when (bound-and-true-p amx-mode)
    (amx-mode 0))
  (dolist (command amx-test-commands)
    (put command 'amx-ignored nil)))
"##;

fn amx_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AMX_MELPA_PIN, "amx.el")
        .expect("prepare pinned amx source and dependencies below ./tmp")
        .with_prelude(AMX_TEST_PRELUDE)
        .with_timeout(AMX_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed amx parity test").into()
}

/// Multi-probe batch for `assert_amx_parity` cases (2a).
pub(crate) fn assert_amx_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(amx_oracle(), &name, "amx_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn amx_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_amx_batch(&cases);
}

// END generated package batch tests
