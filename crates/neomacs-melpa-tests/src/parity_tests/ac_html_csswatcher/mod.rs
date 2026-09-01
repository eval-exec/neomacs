use std::time::Duration;

use crate::{AC_HTML_CSSWATCHER_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_HTML_CSSWATCHER_TEST_TIMEOUT: Duration = Duration::from_secs(240);

/// Helpers shared by the workflows.
///
/// ac-html-csswatcher runs `csswatcher' - a Perl program from CPAN - over the
/// file being edited, reads two lines out of its output, and puts the directory
/// named on the `ACSOURCE:' line into a buffer-local variable that
/// `web-completion-data-sources' then points at.  Everything the package does is
/// around that call: building the argv, parsing the two lines, guarding on the
/// exit status, and installing itself on the right hooks.
///
/// `csswatcher' is stood in for and, as with the hoogle stand-in in
/// `ac_haskell_process', its output is **not** recorded from the real tool:
/// CSS::Watcher is not in nixpkgs and the package is from 2015.  What the
/// stand-in prints is the format the package's own regexps define -
/// `PROJECT: DIR' and `ACSOURCE: DIR' - so the parsing is exercised against the
/// shape the code declares rather than against a transcript.  The argv is
/// therefore the load-bearing assertion, being the only part of the exchange
/// the package actually authors.
///
/// `ac-html-csswatcher-test-settle' exists because the work happens in a process
/// sentinel.  A workflow that reads the variable without waiting sees whatever
/// was there before, and an *earlier* call's sentinel firing late overwrites it
/// - which is how the first probe of this package produced a stale reading that
///   looked like a package bug.
const AC_HTML_CSSWATCHER_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defun ac-html-csswatcher-test-site ()
  "Build a small web project in the sandbox and return its root."
  (let ((root (file-name-as-directory
               (expand-file-name "site" (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (make-directory (expand-file-name "css" root) t)
    (write-region "<html><body><div class=\"\"></div></body></html>\n" nil
                  (expand-file-name "index.html" root) nil 'silent)
    (write-region ".btn { color: red; }\n" nil
                  (expand-file-name "css/app.css" root) nil 'silent)
    root))

(defun ac-html-csswatcher-test-install (root script)
  "Install SCRIPT as the `csswatcher' stand-in for ROOT; return its argv log."
  (let* ((bin (expand-file-name "bin" root))
         (program (expand-file-name "csswatcher" bin))
         (log-file (expand-file-name "csswatcher.log" root)))
    (make-directory bin t)
    (write-region script nil program nil 'silent)
    (set-file-modes program #o755)
    (setenv "CSSWATCHER_TEST_LOG" log-file)
    (write-region "" nil log-file nil 'silent)
    (add-to-list 'exec-path bin)
    (setenv "PATH" (concat bin path-separator (getenv "PATH")))
    log-file))

(defconst ac-html-csswatcher-test-answering
  (concat "#!/bin/sh\n"
          "printf '<%s>\\n' \"$@\" >> \"$CSSWATCHER_TEST_LOG\"\n"
          "printf 'PROJECT: %s\\nACSOURCE: %s\\n' \"$CSSWATCHER_TEST_PROJECT\" \"$CSSWATCHER_TEST_SOURCE\"\n"
          "exit ${CSSWATCHER_TEST_STATUS:-0}\n")
  "A stand-in answering in the two-line format the package's regexps parse.")

(defun ac-html-csswatcher-test-answers (project source &optional status)
  "Make the stand-in report PROJECT and SOURCE, exiting STATUS."
  (setenv "CSSWATCHER_TEST_PROJECT" project)
  (setenv "CSSWATCHER_TEST_SOURCE" source)
  (setenv "CSSWATCHER_TEST_STATUS" (number-to-string (or status 0))))

(defun ac-html-csswatcher-test-settle ()
  "Wait for every csswatcher process, and for its sentinel, to finish."
  (let ((limit 400))
    (while (and (> limit 0)
                (cl-find-if (lambda (process)
                              (string-prefix-p "csswatcher-" (process-name process)))
                            (process-list)))
      (accept-process-output nil 0.05)
      (setq limit (1- limit))))
  (dotimes (_ 10) (accept-process-output nil 0.05)))

(defun ac-html-csswatcher-test-arguments (log-file)
  "Every argument the stand-in recorded, in call order."
  (with-temp-buffer
    (insert-file-contents log-file)
    (let (arguments)
      (goto-char (point-min))
      (while (re-search-forward "^<\\(.*\\)>$" nil t)
        (push (match-string-no-properties 1) arguments))
      (nreverse arguments))))

(defun ac-html-csswatcher-test-relative (text root)
  "TEXT with ROOT written as `SITE/', so paths stay visible but stable."
  (if (stringp text)
      (replace-regexp-in-string (regexp-quote root) "SITE/" text)
    text))

(defun ac-html-csswatcher-test-output-buffers ()
  "Any `*csswatcher-output*' buffers still alive."
  (sort (delq nil (mapcar (lambda (buffer)
                            (and (string-prefix-p "*csswatcher-output*"
                                                  (buffer-name buffer))
                                 (buffer-name buffer)))
                          (buffer-list)))
        #'string<))
"##;

fn ac_html_csswatcher_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_HTML_CSSWATCHER_MELPA_PIN, "ac-html-csswatcher.el")
        .expect("prepare pinned ac-html-csswatcher source below ./tmp")
        .with_prelude(AC_HTML_CSSWATCHER_TEST_PRELUDE)
        .with_timeout(AC_HTML_CSSWATCHER_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-html-csswatcher parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_html_csswatcher_parity` cases (2a).
pub(crate) fn assert_ac_html_csswatcher_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ac_html_csswatcher_oracle(),
        &name,
        "ac_html_csswatcher_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn ac_html_csswatcher_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_html_csswatcher_batch(&cases);
}

// END generated package batch tests
