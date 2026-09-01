use std::time::Duration;

use crate::{ATTRAP_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN, F_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ATTRAP_TEST_TIMEOUT: Duration = Duration::from_secs(300);

/// attrap repairs the error at point.  Its elisp fixer works on diagnostics
/// from `elisp-flymake-checkdoc' and `elisp-flymake-byte-compile', both of
/// which ship with Emacs, so the main workflows are end to end with nothing
/// stood in: a real `.el' file in the sandbox, real `flymake-mode', real
/// diagnostics, `M-x attrap-attrap' at point, and the rewritten source read
/// back off the buffer.
///
/// The Haskell and LaTeX fixers need toolchains and modes that are not
/// installed, but the package's own Commentary defines a fixer as the
/// extension point -- "a side-effect-free function mapping an error message
/// MSG to a list of options" -- and `attrap-flymake-backends-alist' is a
/// defcustom users populate with their own.  So those fixers are exercised
/// through that documented contract.
const ATTRAP_TEST_PRELUDE: &str = r##"
(require 'flymake)
(require 'seq)
(require 'cl-lib)

(defun attrap-test-plain (value)
  (cond ((stringp value) (substring-no-properties value))
        ((consp value)
         (cons (attrap-test-plain (car value)) (attrap-test-plain (cdr value))))
        (t value)))

(defun attrap-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defconst attrap-test-sample
  (concat ";;; sample.el --- A sample\n"
          "(defun sample-greet (name)\n"
          "  \"say hello to NAME. it is nice\"\n"
          "  (message \"hello %s.\" name))\n")
  "A real elisp file with several things checkdoc objects to.")

(defun attrap-test-open (&optional text)
  "Write TEXT to a real file, visit it, and wait for checkdoc's diagnostics."
  (let ((path (attrap-test-path "sample.el")))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert (or text attrap-test-sample))
      (write-region (point-min) (point-max) path nil 'silent))
    (let ((buffer (find-file-noselect path t)))
      (with-current-buffer buffer
        (emacs-lisp-mode)
        (setq-local flymake-diagnostic-functions '(elisp-flymake-checkdoc))
        (flymake-mode 1)
        (flymake-start)
        ;; Diagnostics arrive from a subprocess, so wait for the list to
        ;; appear and then stop changing rather than for the backend to be
        ;; reported as finished.
        (let ((rounds 0) (stable 0) (previous nil))
          (while (and (< rounds 900) (< stable 6))
            (accept-process-output nil 0.02)
            (let ((now (mapcar #'flymake-diagnostic-text (flymake-diagnostics))))
              (setq stable (if (and now (equal now previous)) (1+ stable) 0))
              (setq previous now))
            (setq rounds (1+ rounds)))))
      buffer)))

(defun attrap-test-diagnostics (buffer)
  (with-current-buffer buffer
    (mapcar (lambda (diagnostic)
              (list :beg (flymake-diagnostic-beg diagnostic)
                    :end (flymake-diagnostic-end diagnostic)
                    :backend (flymake-diagnostic-backend diagnostic)
                    :text (substring-no-properties
                           (flymake-diagnostic-text diagnostic))))
            (flymake-diagnostics))))

(defvar attrap-test-offered nil
  "The repair descriptions the last run was asked to choose between.")

(defun attrap-test-repair (matching &optional choice)
  "Repair the diagnostic whose text matches MATCHING, choosing CHOICE.
Return the file's text afterwards, or the error the command signalled."
  (setq attrap-test-offered nil)
  (let ((buffer (attrap-test-open)))
    (unwind-protect
        (with-current-buffer buffer
          (let ((diagnostic
                 (seq-find (lambda (candidate)
                             (string-match-p
                              matching (flymake-diagnostic-text candidate)))
                           (flymake-diagnostics))))
            (if (not diagnostic)
                (list :no-such-diagnostic matching)
              (goto-char (flymake-diagnostic-beg diagnostic))
              (let ((failure
                     (cl-letf (((symbol-function 'completing-read)
                                (lambda (_prompt collection &rest _ignored)
                                  (setq attrap-test-offered
                                        (mapcar #'car collection))
                                  (or choice (car (car collection))))))
                       (condition-case error
                           (progn (attrap-attrap (point)) nil)
                         (error (attrap-test-plain error))))))
                (list :diagnostic (substring-no-properties
                                   (flymake-diagnostic-text diagnostic))
                      :offered attrap-test-offered
                      :signalled failure
                      :source (buffer-substring-no-properties
                               (point-min) (point-max)))))))
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))

(defun attrap-test-fixer (fixer message &optional text position)
  "Call FIXER with MESSAGE the way the package documents fixers.
Report the options it returns and whether it touched the buffer while
doing so, which the Commentary says it must not."
  (with-temp-buffer
    (when text (insert text))
    (goto-char (or position (point-min)))
    (let* ((before (buffer-substring-no-properties (point-min) (point-max)))
           (options (funcall fixer message (point)
                             (min (point-max) (+ 3 (point)))))
           (after (buffer-substring-no-properties (point-min) (point-max))))
      (list :options (mapcar (lambda (option) (attrap-test-plain (car option)))
                             options)
            :buffer-untouched (equal before after)
            :buffer after
            :applying-each
            (mapcar (lambda (option)
                      (let ((scratch (generate-new-buffer " *attrap-test*")))
                        (unwind-protect
                            (with-current-buffer scratch
                              (when text (insert text))
                              (goto-char (or position (point-min)))
                              (funcall (cdr option))
                              (cons (attrap-test-plain (car option))
                                    (buffer-substring-no-properties
                                     (point-min) (point-max))))
                          (kill-buffer scratch))))
                    options)))))
"##;

fn attrap_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ATTRAP_MELPA_PIN, "attrap.el")
        .expect("prepare pinned attrap source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash source below ./tmp")
        .with_melpa_dependency(F_MELPA_PIN)
        .expect("prepare pinned f source below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s source below ./tmp")
        .with_prelude(ATTRAP_TEST_PRELUDE)
        .with_timeout(ATTRAP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed attrap parity test").into()
}

/// Multi-probe batch for `assert_attrap_parity` cases (2a).
pub(crate) fn assert_attrap_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(attrap_oracle(), &name, "attrap_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn attrap_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_attrap_batch(&cases);
}

// END generated package batch tests
