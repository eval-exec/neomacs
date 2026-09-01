use std::time::Duration;

use crate::{AFTERGLOW_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AFTERGLOW_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// afterglow puts a short-lived overlay on whatever a command just moved to,
/// so every workflow needs a real buffer in the selected window (that is where
/// `execute-kbd-macro' delivers keys) and a way to make the overlay's timer
/// expire.
///
/// Waiting is not that way: a batch editor has no command loop, so the
/// workflows capture `timer-list' before triggering and then run exactly the
/// timers that appeared, with `timer-event-handler'.  That is deterministic and
/// instant, and it never runs the editor's own unrelated timers -- comparing
/// against a captured list rather than matching on a timer's printed function
/// also keeps it independent of how each editor prints a closure.
///
/// Nothing is stubbed: the advice, the overlays and the timers are the real
/// ones the package creates.
const AFTERGLOW_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst afterglow-test-text
  "alpha beta gamma\ndelta epsilon zeta\neta theta iota\n\nkappa lambda mu\n"
  "Five lines, the fourth deliberately empty.")

(defun afterglow-test-command () (interactive) nil)
(defun afterglow-test-move () (interactive) (forward-line 1))
(defun afterglow-test-bounds () (cons (point) (+ (point) 4)))

(defun afterglow-test-buffer (&optional text)
  "Return a displayed buffer holding TEXT with point at the beginning."
  (let ((buffer (generate-new-buffer "*afterglow-workflow*")))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (insert (or text afterglow-test-text))
    (goto-char (point-min))
    buffer))

(defun afterglow-test-overlays ()
  "Report every overlay a user would see, in buffer order."
  (sort (mapcar (lambda (overlay)
                  (list (overlay-start overlay)
                        (overlay-end overlay)
                        (overlay-get overlay 'face)
                        (overlay-get overlay 'priority)
                        (buffer-name (overlay-buffer overlay))))
                (overlays-in (point-min) (point-max)))
        (lambda (a b) (< (car a) (car b)))))

(defun afterglow-test-new-timers (known)
  "Return the timers created since KNOWN was captured, oldest first."
  (nreverse (cl-remove-if (lambda (timer) (memq timer known))
                          (reverse timer-list))))

(defun afterglow-test-delays (known start)
  "Return each new timer's delay in tenths of a second."
  (mapcar (lambda (timer)
            (round (* 10 (- (float-time (timer--time timer)) start))))
          (afterglow-test-new-timers known)))

(defun afterglow-test-run-new-timers (known)
  "Fire every timer created since KNOWN, and return how many ran."
  (let ((timers (afterglow-test-new-timers known)))
    (dolist (timer timers)
      (timer-event-handler timer))
    (length timers)))

(defun afterglow-test-advice-armed (function)
  (and (advice-member-p (afterglow--advice-fn-symbol function) function) t))

(defun afterglow-test-state (&rest functions)
  "Report the package state a user's configuration produces."
  (list :mode afterglow-mode
        :triggers (hash-table-count afterglow--triggers)
        :advised (mapcar #'car afterglow--advised-functions)
        :armed (mapcar (lambda (function)
                         (cons function (afterglow-test-advice-armed function)))
                       functions)))

(defun afterglow-test-cleanup ()
  (dolist (function (afterglow--trigger-functions))
    (afterglow-remove-trigger function))
  (when afterglow-mode (afterglow-mode 0))
  (dolist (timer (copy-sequence timer-list))
    (when (string-match-p "afterglow" (format "%S" (timer--function timer)))
      (cancel-timer timer))))
"##;

fn afterglow_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AFTERGLOW_MELPA_PIN, "afterglow.el")
        .expect("prepare pinned afterglow source below ./tmp")
        .with_prelude(AFTERGLOW_TEST_PRELUDE)
        .with_timeout(AFTERGLOW_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed afterglow parity test")
        .into()
}

/// Multi-probe batch for `assert_afterglow_parity` cases (2a).
pub(crate) fn assert_afterglow_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(afterglow_oracle(), &name, "afterglow_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn afterglow_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_afterglow_batch(&cases);
}

// END generated package batch tests
