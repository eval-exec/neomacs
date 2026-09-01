use std::time::Duration;

use crate::{AMREAD_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AMREAD_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// amread-mode is a speed-reading minor mode: it runs a repeating timer that
/// walks a highlight overlay through the buffer one word or one line at a time.
///
/// Timers are driven the way HARNESS-NOTES requires rather than by waiting.
/// `timer-list` is captured before the mode is enabled and only the timers that
/// appeared are run, because the editor has its own pending timers; the number
/// that appeared is pinned; and an interval is asserted as a delta from a
/// baseline taken immediately before the triggering call, rounded to whole
/// units, never as a wall-clock `timer--time`.
///
/// The voice reader shells out to espeak, festival or say.  It is off by
/// default and every workflow leaves it off, so no subprocess is ever started.
/// The two `completing-read` prompts `amread-start` issues -- for the scroll
/// style and the voice language -- are a real interactive boundary and are the
/// only things stubbed; the prompts and the answers given are themselves
/// pinned, so the stub cannot quietly drift from what the package asks.
const AMREAD_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(setq make-backup-files nil create-lockfiles nil)

;; The voice reader shells out to espeak/festival/say and is off by default;
;; every workflow leaves it off, so no subprocess is ever started.  The two
;; `completing-read' prompts amread-start issues -- for the scroll style and
;; the voice language -- are a genuine interactive boundary and are the only
;; things stubbed.  Answers are recorded so the prompts themselves are pinned.
(defvar amr-test-prompts nil)

(defmacro amr-test-with-answers (answers &rest body)
  `(let ((amr-test-prompts nil)
         (remaining ,answers))
     (cl-letf (((symbol-function 'completing-read)
                (lambda (prompt &rest _)
                  (let ((answer (pop remaining)))
                    (push (list (copy-sequence prompt) (copy-sequence answer))
                          amr-test-prompts)
                    answer))))
       ,@body)))

(defconst amr-test-prose
  "Der Weg ist das Ziel.
Wer lesen kann, ist klar im Vorteil.
Grüße aus dem Zeilenmodus.
")

(defmacro amr-test-in-buffer (&rest body)
  "A window-displayed buffer holding the fixture prose, with clean amread state."
  `(let ((buffer (generate-new-buffer "*amread-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (insert amr-test-prose)
           (goto-char (point-min))
           (setq amread--timer nil
                 amread--overlay nil
                 amread--current-position nil)
           ,@body)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (when amread--timer (cancel-timer amread--timer))
           (setq amread--timer nil)
           (when amread--overlay (delete-overlay amread--overlay))
           (setq amread--overlay nil))
         (let ((kill-buffer-query-functions nil)) (kill-buffer buffer))))))

;; Timer helpers.  HARNESS-NOTES: capture `timer-list' before the trigger and
;; run only what appeared, because the editor has its own pending timers; and
;; assert a delay as a delta from a baseline taken immediately before the
;; triggering call, in whole units.
(defun amr-test-timer-baseline () (copy-sequence timer-list))

(defun amr-test-new-timers (baseline)
  (cl-remove-if (lambda (timer) (memq timer baseline)) timer-list))

(defun amr-test-run-new-timers (baseline n)
  "Run the timers that appeared since BASELINE, N times each, in order."
  (let ((new (amr-test-new-timers baseline)))
    (dotimes (_ n)
      (dolist (timer new)
        (when (memq timer timer-list)
          (timer-event-handler timer))))
    (length new)))

(defun amr-test-overlay ()
  "The highlight overlay described by position and covered text."
  (if (and amread--overlay (overlay-buffer amread--overlay))
      (list :start (overlay-start amread--overlay)
            :end (overlay-end amread--overlay)
            :text (copy-sequence
                   (buffer-substring-no-properties
                    (overlay-start amread--overlay) (overlay-end amread--overlay)))
            :face (overlay-get amread--overlay 'face))
    'no-overlay))
"##;

fn amread_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AMREAD_MODE_MELPA_PIN, "amread-mode.el")
        .expect("prepare pinned amread-mode source below ./tmp")
        .with_prelude(AMREAD_MODE_TEST_PRELUDE)
        .with_timeout(AMREAD_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed amread-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_amread_mode_parity` cases (2a).
pub(crate) fn assert_amread_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(amread_mode_oracle(), &name, "amread_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn amread_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_amread_mode_batch(&cases);
}

// END generated package batch tests
