use std::time::Duration;

use crate::{ALT_CODES_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALT_CODES_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Fixtures shared by the workflows.
///
/// alt-codes watches `pre-command-hook': when the key just pressed is the
/// *symbol* `M-kp-N' it appends N to a pending code, and when it is any other
/// symbol event it looks the pending code up and inserts the character.  Two
/// consequences shape every workflow.
///
/// The commit only happens for a symbol event.  A letter arrives as a
/// character, `(symbolp last-input-event)' is nil and the hook returns without
/// doing anything, so a pending code survives ordinary typing.  The workflows
/// commit with `<f5>' bound to `ignore', which keeps the assertion about the
/// package rather than about whatever command the committing key happens to
/// run -- the insertion happens in the hook, before that command.
///
/// The lookup is `(eval (cons 'pcase (cons code alt-codes--list)))' over 383
/// clauses, which needs roughly 6400 `max-lisp-eval-depth' to expand.  At the
/// default 1600 the first lookup of a session signals `excessive-lisp-nesting',
/// identically in both editors; `pcase' memoises its expansion, so once one
/// lookup has succeeded the rest are cheap.  Workflows that exercise the
/// insertion therefore raise the limit, and one workflow pins the failure a
/// user meets with the default.
const ALT_CODES_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defmacro alt-codes-test-with-buffer (&rest body)
  "A window-displayed buffer with the mode on and a harmless commit key."
  `(let ((buffer (generate-new-buffer "*alt-codes-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (text-mode)
           (setq prefix-arg nil current-prefix-arg nil)
           (alt-codes-mode 1)
           (local-set-key [f5] #'ignore)
           ,@body)
       (kill-buffer buffer))))

(defun alt-codes-test-type (&rest events)
  "Type EVENTS, clearing any numeric prefix the keypad digits accumulated."
  (setq prefix-arg nil current-prefix-arg nil)
  (execute-kbd-macro (vconcat events)))

(defun alt-codes-test-code (&rest digits)
  "The events for typing DIGITS on the keypad with Meta held."
  (mapcar (lambda (digit) (intern (format "M-kp-%c" digit))) digits))

(defun alt-codes-test-enter (digits)
  "Type DIGITS on the keypad and commit them, returning what the buffer got."
  (erase-buffer)
  (apply #'alt-codes-test-type (append (apply #'alt-codes-test-code
                                              (append digits nil))
                                       (list 'f5)))
  (list (copy-sequence (buffer-string)) (copy-sequence alt-codes--code)))

(defun alt-codes-test-message-mark ()
  (with-current-buffer (get-buffer-create "*Messages*") (point-max)))

(defun alt-codes-test-messages-since (mark &optional matching)
  "Messages logged since MARK, optionally only those containing MATCHING."
  (with-current-buffer (get-buffer-create "*Messages*")
    (let ((lines (mapcar #'copy-sequence
                         (split-string
                          (buffer-substring-no-properties (min mark (point-max)) (point-max))
                          "\n" t))))
      (if matching
          (seq-filter (lambda (line) (string-match-p matching line)) lines)
        lines))))

(defun alt-codes-test-hook ()
  (list (and (memq #'alt-codes--pre-command-hook pre-command-hook) t)
        (local-variable-p 'pre-command-hook)
        alt-codes-mode))
"##;

fn alt_codes_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALT_CODES_MELPA_PIN, "alt-codes.el")
        .expect("prepare pinned alt-codes source below ./tmp")
        .with_prelude(ALT_CODES_TEST_PRELUDE)
        .with_timeout(ALT_CODES_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed alt-codes parity test")
        .into()
}

/// Multi-probe batch for `assert_alt_codes_parity` cases (2a).
pub(crate) fn assert_alt_codes_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(alt_codes_oracle(), &name, "alt_codes_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn alt_codes_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_alt_codes_batch(&cases);
}

// END generated package batch tests
