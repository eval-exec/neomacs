use std::time::Duration;

use crate::{ALARM_CLOCK_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALARM_CLOCK_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Fixtures shared by the workflows.
///
/// alarm-clock schedules a `run-at-time' timer per alarm, renders them in a
/// `*alarm clock*' buffer, persists them to a cache file and notifies when one
/// fires.  Three things about that have to be handled carefully in batch, all
/// of them noted in HARNESS-NOTES.md:
///
/// Timers: the editor has pending timers of its own, and merely touching a
/// buffer starts `undo-auto--boundary-timer', so a workflow captures
/// `timer-list' before the call and reports only what appeared.  A delay is
/// asserted as a delta in tenths from a timestamp taken immediately before the
/// call, never as `timer--time'.  The delays of the package's own timers are
/// read through the alarm records in `alarm-clock--alist', which is
/// editor-neutral -- unlike matching a timer by its printed function.
///
/// Rendering and persistence contain wall-clock text.  `alarm-clock-test-lines'
/// compares each rendered time column against the alarm's own `:time' rendered
/// the same way, so the assertion is exact without being a clock reading, and
/// keeps only the hour:minute of the countdown column.  `alarm-clock-test-cache'
/// replaces each ISO timestamp in the saved file with `<ISO>', so the file's
/// exact layout is pinned, and the timestamps are checked separately by parsing
/// them back.
///
/// Notification: none of the notifier programs this package can use exist on
/// this host, and the optional `alert' package is not installed, so the echo
/// area message is the whole of what a firing alarm produces here.  The
/// workflow that covers it records that absence rather than assuming it.
const ALARM_CLOCK_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defvar alarm-clock-test-root nil)

(defun alarm-clock-test-setup ()
  "A clean sandbox, a cache file inside it, and no alarms."
  (setq alarm-clock-test-root
        (expand-file-name "alarms" (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
  (when (file-directory-p alarm-clock-test-root)
    (delete-directory alarm-clock-test-root t))
  (make-directory alarm-clock-test-root t)
  (setq alarm-clock-cache-file
        (expand-file-name "alarm-clock.cache" alarm-clock-test-root))
  (alarm-clock--kill-all)
  (when (get-buffer "*alarm clock*") (kill-buffer "*alarm clock*"))
  alarm-clock-test-root)

(defun alarm-clock-test-new-timers (baseline)
  "The timers that appeared since BASELINE was captured."
  (seq-difference timer-list baseline #'eq))

(defun alarm-clock-test-set (spec message)
  "Set an alarm through the public command and report what it scheduled.
The baseline is taken immediately before the call, so the delay is this
alarm's own and not the time the workflow spent setting earlier ones."
  (let* ((baseline (copy-sequence timer-list))
         (start (float-time)))
    (alarm-clock-set spec message)
    ;; `alarm-clock-set' re-sorts the list, so the new alarm is not its head.
    (let* ((alarm (seq-find (lambda (candidate)
                              (equal (plist-get candidate :message)
                                     (string-trim message)))
                            alarm-clock--alist))
           (timer (plist-get alarm :timer)))
      (list (copy-sequence (plist-get alarm :message))
            (round (- (float-time (plist-get alarm :time)) start))
            (and (memq timer timer-list) t)
            (length (alarm-clock-test-new-timers baseline))))))

(defun alarm-clock-test-state ()
  "Each alarm as (MESSAGE SCHEDULED-P), newest first, with no clock reading."
  (mapcar (lambda (alarm)
            (list (copy-sequence (plist-get alarm :message))
                  (and (memq (plist-get alarm :timer) timer-list) t)))
          alarm-clock--alist))

(defun alarm-clock-test-alarms (start &optional unit)
  "Each alarm as (MESSAGE DELAY SCHEDULED-P), newest first.
DELAY is whole seconds, or whole minutes when UNIT is `minutes'.  Tenths would
record the time the workflow itself spent between calls; minutes are needed for
alarms restored from the file, whose timestamps were truncated to the second."
  (mapcar (lambda (alarm)
            (let ((timer (plist-get alarm :timer))
                  (delta (- (float-time (plist-get alarm :time)) start)))
              (list (copy-sequence (plist-get alarm :message))
                    (if (eq unit 'minutes) (round (/ delta 60.0)) (round delta))
                    (and (memq timer timer-list) t))))
          alarm-clock--alist))

(defun alarm-clock-test-lines ()
  "Each rendered line, with the clock readings made comparable."
  (when (get-buffer "*alarm clock*")
    (with-current-buffer "*alarm clock*"
      (let (lines (pos (point-min)))
        (while (< pos (point-max))
          (let* ((end (line-end-position))
                 (line (buffer-substring-no-properties pos end))
                 (alarm (get-text-property pos 'alarm-clock))
                 (time-column (substring line 0 (min 19 (length line))))
                 (remaining (and (> (length line) 21) (substring line 21 (min 29 (length line))))))
            (push (list :time-matches-the-alarm
                        (equal time-column
                               (and alarm (format-time-string "%F %X" (plist-get alarm :time))))
                        :remaining-hour-minute (and remaining (substring remaining 0 5))
                        :message (copy-sequence (string-trim (substring line (min 35 (length line)))))
                        :property-message (copy-sequence (or (plist-get alarm :message) "none"))
                        :property-only-on-first-character
                        (null (get-text-property (1+ pos) 'alarm-clock)))
                  lines))
          (setq pos (1+ (line-end-position)))
          (goto-char (min pos (point-max))))
        (nreverse lines)))))

(defun alarm-clock-test-header ()
  (when (get-buffer "*alarm clock*")
    (with-current-buffer "*alarm clock*"
      (list (copy-sequence header-line-format) major-mode buffer-read-only))))

(defun alarm-clock-test-cache ()
  "The saved file with every ISO timestamp replaced by `<ISO>'."
  (when (file-exists-p alarm-clock-cache-file)
    (with-temp-buffer
      (let ((coding-system-for-read 'utf-8))
        (insert-file-contents alarm-clock-cache-file))
      (replace-regexp-in-string
       "[0-9]\\{4\\}-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9][-+][0-9]\\{4\\}"
       "<ISO>" (buffer-string)))))

(defun alarm-clock-test-saved ()
  "The entries in the saved file, as read back."
  (when (file-exists-p alarm-clock-cache-file)
    (with-temp-buffer
      (insert-file-contents alarm-clock-cache-file)
      (goto-char (point-min))
      (ignore-errors (read (current-buffer))))))

(defun alarm-clock-test-cache-matches-alarms ()
  "For each saved entry, (MESSAGE . RECORDS-THE-ALARMS-INSTANT).
The file stores whole seconds, so the saved timestamp must be the alarm's own
time truncated to the second."
  (mapcar (lambda (entry)
            (let* ((message (plist-get entry :message))
                   (saved (float-time (parse-iso8601-time-string (plist-get entry :time))))
                   (alarm (seq-find (lambda (a) (equal (plist-get a :message) message))
                                    alarm-clock--alist))
                   (scheduled (and alarm (float-time (plist-get alarm :time)))))
              (cons (copy-sequence message)
                    (and scheduled (<= 0 (- scheduled saved) 1) t))))
          (alarm-clock-test-saved)))

(defun alarm-clock-test-cache-minutes (start)
  "Each saved (MESSAGE . DELAY-MINUTES) measured from START."
  (mapcar (lambda (entry)
            (cons (copy-sequence (plist-get entry :message))
                  (round (/ (- (float-time
                                (parse-iso8601-time-string (plist-get entry :time)))
                               start)
                            60.0))))
          (alarm-clock-test-saved)))

(defun alarm-clock-test-files ()
  (sort (directory-files alarm-clock-test-root) #'string<))

(defun alarm-clock-test-write-cache (entries)
  "Write ENTRIES, a list of (SECONDS-FROM-NOW . MESSAGE), as a saved file."
  (with-temp-buffer
    (insert ";; Auto-generated file; don't edit\n(")
    (dolist (entry entries)
      (insert (format "(:time \"%s\" :message \"%s\")\n "
                      (format-time-string "%FT%T%z" (time-add nil (car entry)))
                      (cdr entry))))
    (insert ")\n")
    (let ((coding-system-for-write 'utf-8-unix))
      (write-region (point-min) (point-max) alarm-clock-cache-file nil 'silent))))

(defun alarm-clock-test-notifiers ()
  "Which notifier this package could use exists on this host."
  (mapcar (lambda (program) (cons program (and (executable-find program) t)))
          '("notify-send" "terminal-notifier" "mpg123" "afplay")))

(defun alarm-clock-test-message-mark ()
  (with-current-buffer (get-buffer-create "*Messages*") (point-max)))

(defun alarm-clock-test-messages-since (mark)
  (with-current-buffer (get-buffer-create "*Messages*")
    (mapcar #'copy-sequence
            (split-string
             (buffer-substring-no-properties (min mark (point-max)) (point-max))
             "\n" t))))
"##;

fn alarm_clock_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALARM_CLOCK_MELPA_PIN, "alarm-clock.el")
        .expect("prepare pinned alarm-clock source below ./tmp")
        .with_prelude(ALARM_CLOCK_TEST_PRELUDE)
        .with_timeout(ALARM_CLOCK_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed alarm-clock parity test")
        .into()
}

/// Multi-probe batch for `assert_alarm_clock_parity` cases (2a).
pub(crate) fn assert_alarm_clock_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(alarm_clock_oracle(), &name, "alarm_clock_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn alarm_clock_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_alarm_clock_batch(&cases);
}

// END generated package batch tests
