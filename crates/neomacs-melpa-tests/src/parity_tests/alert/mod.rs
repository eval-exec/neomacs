use std::time::Duration;

use crate::{ALERT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALERT_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// alert is a notification framework with pluggable back ends, so the
/// workflows drive the real `alert' entry point and watch where each
/// notification ends up.
///
/// Nothing here is stubbed.  The styles these workflows route to are defined
/// with the package's own `alert-define-style' and simply record the plist they
/// are handed, which is exactly the extension contract a back end implements.
/// Of the shipped back ends only `message', `log', `fringe', `mode-line' and
/// `momentary' can run on this host - growlnotify, terminal-notifier,
/// notify-send, osascript and kdialog are all absent - so the growl workflow
/// covers both halves of that: the documented fallback when the command is
/// missing, and the exact command line the package builds when it is present,
/// using a recording stand-in executable in the sandbox.
const ALERT_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defvar al-test-captured nil
  "Every info plist the recording style received, oldest first.")

(defun al-test-copy (value)
  "Return VALUE with strings copied.
Repeated string objects would otherwise print as `#N=' back references,
and whether the harness's normaliser preserves that sharing is not
stable between runs."
  (if (stringp value) (copy-sequence value) value))

(defun al-test-info (info &rest keys)
  "Return the interesting parts of an alert INFO plist."
  (mapcar (lambda (key)
            (cons key
                  (let ((value (plist-get info key)))
                    (al-test-copy (if (bufferp value) (buffer-name value) value)))))
          (or keys
              '(:message :title :severity :category :mode :buffer
                :data :id :persistent :never-persist :style))))

(defun al-test-record (name kind info)
  "Record that style NAME was asked to KIND an alert with INFO."
  (push (list name kind info) al-test-captured))

(defun al-test-define-recorder (name)
  "Define a style NAME that records every plist it is handed."
  (alert-define-style
   name
   :title (format "Recorder %s" name)
   :notifier (apply-partially #'al-test-record name :notify)
   :remover (apply-partially #'al-test-record name :remove)))

(defun al-test-captured-infos (&optional kind)
  "Return (STYLE . DESCRIPTION) for each recorded call, oldest first."
  (let ((entries (reverse al-test-captured)))
    (mapcar (lambda (entry) (cons (nth 0 entry) (nth 2 entry)))
            (if kind
                (cl-remove-if-not (lambda (entry) (eq (nth 1 entry) kind)) entries)
              entries))))

(defun al-test-style-summary (name)
  "Describe the registered style NAME without printing its closures."
  (let ((definition (cdr (assq name alert-styles))))
    (list name
          (plist-get definition :title)
          (functionp (plist-get definition :notifier))
          (functionp (plist-get definition :remover)))))

(defun al-test-messages-since (mark)
  "Return the *Messages* lines added since MARK."
  (with-current-buffer (get-buffer-create "*Messages*")
    (split-string (buffer-substring-no-properties
                   (min mark (point-max)) (point-max))
                  "\n" t)))

(defun al-test-messages-mark ()
  (with-current-buffer (get-buffer-create "*Messages*") (point-max)))

(defun al-test-buffer-text (name)
  "Return NAME's text with clock times replaced, or a marker when absent."
  (let ((buffer (get-buffer name)))
    (if (not buffer)
        'no-buffer
      (with-current-buffer buffer
        (replace-regexp-in-string
         "[0-9][0-9]:[0-9][0-9]\\(:[0-9][0-9]\\)?\\( [AP]M\\)?" "<TIME>"
         (buffer-substring-no-properties (point-min) (point-max)) t t)))))

(defun al-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun al-test-install-growlnotify ()
  "Install a recording stand-in for growlnotify and return its path.
The real command is not installed on this host, which is exactly the
situation `alert-growl-notify' guards against."
  (let ((path (al-test-path "bin/growlnotify")))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert "#!/bin/sh\n"
              "printf '%s\\n' \"growlnotify $*\" >> \"$AL_TEST_LOG\"\n"
              "exit 0\n")
      (write-region (point-min) (point-max) path nil 'silent))
    (set-file-modes path #o755)
    (setenv "AL_TEST_LOG" (al-test-path "commands.log"))
    path))

(defun al-test-commands ()
  (let ((log (al-test-path "commands.log")))
    (if (file-exists-p log)
        (with-temp-buffer
          (insert-file-contents log)
          (split-string (buffer-string) "\n" t))
      'no-command-ran)))

(defun al-test-copy-selectors (selectors)
  "Copy SELECTORS deeply enough that nothing prints as a back reference."
  (mapcar (lambda (selector)
            (cons (car selector)
                  (if (consp (cdr selector))
                      (copy-sequence (cdr selector))
                    (al-test-copy (cdr selector)))))
          selectors))

(defun al-test-rule-summary (rule)
  "Describe RULE without printing the closures it may contain."
  (list :selectors (al-test-copy-selectors (nth 0 rule))
        :style (nth 1 rule)
        :options (mapcar (lambda (option)
                           (cons (car option)
                                 (if (functionp (cdr option))
                                     :function
                                   (cdr option))))
                         (nth 2 rule))))

(defun al-test-pending-fades ()
  "Messages of the alerts that still have a fade timer scheduled."
  (sort (delq nil
              (mapcar (lambda (timer)
                        (and (eq (timer--function timer) #'alert-remove-when-active)
                             (al-test-copy
                              (plist-get (nth 1 (timer--args timer)) :message))))
                      timer-list))
        #'string<))

(defun al-test-active-alerts ()
  "Describe `alert-active-alerts' as (BUFFER MESSAGE REMOVER) entries."
  (mapcar (lambda (entry)
            (list (al-test-copy
                   (and (buffer-live-p (nth 0 entry)) (buffer-name (nth 0 entry))))
                  (al-test-copy (plist-get (nth 1 entry) :message))
                  (nth 2 entry)))
          alert-active-alerts))
"##;

fn alert_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALERT_MELPA_PIN, "alert.el")
        .expect("prepare pinned alert source below ./tmp")
        .with_prelude(ALERT_TEST_PRELUDE)
        .with_timeout(ALERT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed alert parity test").into()
}

/// Multi-probe batch for `assert_alert_parity` cases (2a).
pub(crate) fn assert_alert_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(alert_oracle(), &name, "alert_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn alert_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_alert_batch(&cases);
}

// END generated package batch tests
