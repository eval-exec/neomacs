use std::time::Duration;

use crate::{ALERT_TOAST_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALERT_TOAST_TEST_TIMEOUT: Duration = Duration::from_secs(240);

/// Helpers shared by the workflows.
///
/// alert-toast is an `alert' backend for Windows 10 toast notifications.  It
/// builds a toast XML document, wraps it in a PowerShell script, and writes that
/// script to a persistent `powershell.exe' process.  All of the building is
/// ordinary Elisp and runs anywhere; only the last step needs Windows.  So the
/// workflows go in through `alert' itself and assert what arrived at the
/// notifier's standard input, which is the package's entire product.
///
/// Three executables are stood in for, each a genuine environmental boundary.
/// `powershell.exe' records what it is sent - it is started twice by the
/// package, once with `-NoExit' as the long-lived notifier and once one-shot to
/// ask for the console encoding, and the stand-in answers both.  `uname' fixes
/// the kernel release, which is what `alert-toast--check-wsl' reads and
/// therefore what decides the platform for the whole session; without it the
/// suite would say something different on a WSL host than on this one.
/// `wslpath' and `cygpath.exe' are the path converters the two Windows-adjacent
/// platforms use.
///
/// The default icon is deliberately not pinned as a literal.  It is built from
/// `data-directory', which is part of the editor rather than the package, so
/// every workflow that does not care about icons passes one explicitly, and the
/// one that does care compares against `data-directory' computed in the same
/// form.
const ALERT_TOAST_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defconst alert-toast-test-bin
  (expand-file-name "bin" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
  "Where the stand-in executables live.")

(defun alert-toast-test-write-program (name body)
  "Write BODY as an executable called NAME in the stand-in directory."
  (make-directory alert-toast-test-bin t)
  (let ((path (expand-file-name name alert-toast-test-bin)))
    (write-region body nil path nil 'silent)
    (set-file-modes path #o755)
    path))

(defun alert-toast-test-log-file ()
  "The file the stand-in `powershell.exe' records to."
  (expand-file-name "powershell.log" (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

;; The kernel release decides the platform, and `alert-toast--check-wsl' reads
;; it through the shell, so a stand-in `uname' fixes it for the whole session.
(alert-toast-test-write-program
 "uname" "#!/bin/sh\nprintf '6.12.85-neomacs-parity\\n'\n")

;; `powershell.exe' is started twice: `-NoExit' for the persistent notifier,
;; which reads scripts forever, and one-shot to report the console encoding.
(alert-toast-test-write-program
 "powershell.exe"
 (concat "#!/bin/sh\n"
         "for argument in \"$@\"; do\n"
         "  if [ \"$argument\" = \"-NoExit\" ]; then\n"
         "    cat >> \"$ALERT_TOAST_TEST_LOG\"\n"
         "    exit 0\n"
         "  fi\n"
         "done\n"
         "printf '[one-shot] ' >> \"$ALERT_TOAST_TEST_LOG\"\n"
         "cat >> \"$ALERT_TOAST_TEST_LOG\"\n"
         "printf '\\n' >> \"$ALERT_TOAST_TEST_LOG\"\n"
         "printf 'utf-8\\r\\n'\n"))

;; The two Windows-adjacent path converters.
(alert-toast-test-write-program
 "wslpath" "#!/bin/sh\nprintf 'C:/from-wslpath%s\\n' \"$2\"\n")
(alert-toast-test-write-program
 "cygpath.exe" "#!/bin/sh\nprintf 'C:\\\\from-cygpath%s\\n' \"$2\"\n")

(setenv "ALERT_TOAST_TEST_LOG" (alert-toast-test-log-file))
(write-region "" nil (alert-toast-test-log-file) nil 'silent)
(add-to-list 'exec-path alert-toast-test-bin)
(setenv "PATH" (concat alert-toast-test-bin path-separator (getenv "PATH")))

(defconst alert-toast-test-icon "/home/user/pictures/emacs.png"
  "An icon path given explicitly, so `data-directory' stays out of the report.")

(defun alert-toast-test-settle ()
  "Let the notifier process read whatever was just written to it."
  (dotimes (_ 12) (accept-process-output nil 0.05)))

(defun alert-toast-test-truncate ()
  "Forget everything the notifier has been sent so far."
  (write-region "" nil (alert-toast-test-log-file) nil 'silent))

(defun alert-toast-test-sent ()
  "What the notifier has been sent since the last truncation."
  (alert-toast-test-settle)
  (with-temp-buffer
    (insert-file-contents (alert-toast-test-log-file))
    (buffer-string)))

(defun alert-toast-test-notify (&rest arguments)
  "Send one alert through the toast style and return the script it produced."
  (alert-toast-test-truncate)
  (apply #'alert (car arguments) :style 'toast (cdr arguments))
  (alert-toast-test-sent))

(defun alert-toast-test-lines (script &rest patterns)
  "The lines of SCRIPT matching any of PATTERNS, trimmed."
  (let (found)
    (dolist (line (split-string script "\n" t))
      (let ((trimmed (string-trim line)))
        (when (cl-find-if (lambda (pattern) (string-match-p pattern trimmed))
                          patterns)
          (push trimmed found))))
    (nreverse found)))
"##;

fn alert_toast_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALERT_TOAST_MELPA_PIN, "alert-toast.el")
        .expect("prepare pinned alert-toast source below ./tmp")
        .with_prelude(ALERT_TOAST_TEST_PRELUDE)
        .with_timeout(ALERT_TOAST_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed alert-toast parity test")
        .into()
}

/// Multi-probe batch for `assert_alert_toast_parity` cases (2a).
pub(crate) fn assert_alert_toast_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(alert_toast_oracle(), &name, "alert_toast_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn alert_toast_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_alert_toast_batch(&cases);
}

// END generated package batch tests
