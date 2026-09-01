use std::time::Duration;

use crate::{AC_RTAGS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_RTAGS_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// ac-rtags is an auto-complete back end over the rtags C/C++ indexer.
/// `ac-rtags-candidates' asks rtags.el to run the `rc' client synchronously
/// with `--code-complete-at <file>:<line>:<col>: --synchronous-completions
/// --elisp', evaluates the form rc prints, and turns its
/// `(name signature kind ...)' entries into candidates carrying
/// `ac-rtags-full' and `ac-rtags-type'.  `rc' needs a running rdm daemon and a
/// real index, so it is the one boundary the workflows fake: a recording
/// stand-in is installed as the only executable on `exec-path' and answers
/// with realistic rtags elisp and a chosen exit status.  Everything else —
/// the location rtags.el computes, the whole argument vector it assembles,
/// the unsaved-buffer temp file, the reading and evaluation of rc's output,
/// candidate construction and auto-complete's rendering, insertion and
/// `ac-rtags-action' parameter expansion — is real package code.
const AC_RTAGS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar ac-rtags-test-root
  (file-name-as-directory
   (expand-file-name "cpp" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defvar ac-rtags-test-bin
  (file-name-as-directory (expand-file-name "bin" ac-rtags-test-root)))

(defvar ac-rtags-test-requests
  (file-name-as-directory (expand-file-name "requests" ac-rtags-test-root)))

(defvar ac-rtags-test-responses
  (file-name-as-directory (expand-file-name "responses" ac-rtags-test-root)))

(make-directory ac-rtags-test-requests t)
(make-directory ac-rtags-test-responses t)

(defun ac-rtags-test-write (path text)
  "Write TEXT to PATH as UTF-8 and return PATH."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun ac-rtags-test-install-rc ()
  "Install a recording stand-in `rc' as the only executable on `exec-path'.
It records its argument vector, working directory and stdin, copies in any
`--unsaved-file' buffer image it was handed, prints the reply configured for
its invocation number and exits with the status configured for it."
  (let ((path (expand-file-name "rc" ac-rtags-test-bin)))
    (make-directory ac-rtags-test-bin t)
    (ac-rtags-test-write
     path
     (concat
      "#!/bin/sh\n"
      "root=" ac-rtags-test-root "\n"
      "requests=" ac-rtags-test-requests "\n"
      "responses=" ac-rtags-test-responses "\n"
      "n=1\n"
      "[ -f \"$root/.total\" ] && n=$(($(cat \"$root/.total\") + 1))\n"
      "printf '%s' \"$n\" > \"$root/.total\"\n"
      "record=$(printf '%s%02d-request' \"$requests\" \"$n\")\n"
      "if [ -t 0 ]; then stdin='<terminal>'; else stdin=$(cat); fi\n"
      "{\n"
      "  printf 'argv:\\n'\n"
      "  for arg in \"$@\"; do printf '  %s\\n' \"$arg\"; done\n"
      "  printf 'cwd: %s\\n' \"$PWD\"\n"
      "  printf 'stdin: %s\\n' \"$stdin\"\n"
      "  for arg in \"$@\"; do\n"
      "    case $arg in\n"
      "      --unsaved-file=*)\n"
      "        unsaved=${arg#--unsaved-file=}\n"
      "        printf 'unsaved-file(%s):\\n' \"${unsaved%%:*}\"\n"
      "        cat \"${unsaved##*:}\"\n"
      "        ;;\n"
      "    esac\n"
      "  done\n"
      "} > \"$record\"\n"
      "[ -f \"$responses/stdout.$n\" ] && cat \"$responses/stdout.$n\"\n"
      "[ -f \"$responses/stdout\" ] && [ ! -f \"$responses/stdout.$n\" ] && cat \"$responses/stdout\"\n"
      "status=0\n"
      "[ -f \"$responses/status.$n\" ] && status=$(cat \"$responses/status.$n\")\n"
      "exit \"$status\"\n"))
    (set-file-modes path #o755)
    (setq exec-path (list (directory-file-name ac-rtags-test-bin)))
    path))

(defun ac-rtags-test-uninstall-rc ()
  "Remove the stand-in, leaving rtags with no `rc' to run."
  (delete-file (expand-file-name "rc" ac-rtags-test-bin)))

(defun ac-rtags-test-reply (stdout &optional nth status)
  "Make the NTH stand-in run print STDOUT, then exit with STATUS.
Without NTH the output answers every run that has no specific reply."
  (ac-rtags-test-write
   (expand-file-name (if nth (format "stdout.%d" nth) "stdout")
                     ac-rtags-test-responses)
   stdout)
  (when status
    (ac-rtags-test-write
     (expand-file-name (format "status.%d" (or nth 1)) ac-rtags-test-responses)
     (number-to-string status))))

(defun ac-rtags-test-completions (location entries)
  "Return the elisp form rc prints for a code completion at LOCATION.
ENTRIES are rtags' (name signature kind parent brief-comment) tuples; the
form is what `ac-rtags-candidates' reads and evaluates."
  (format "(list 'completions (list %S '(%s)))"
          location
          (mapconcat (lambda (entry) (format "%S" entry)) entries "\n   ")))

(defun ac-rtags-test-file-bytes (path)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (buffer-string)))

(defun ac-rtags-test-recorded ()
  "Return every invocation the stand-in rc recorded, in order.
The `--unsaved-file' switch names a `make-nearby-temp-file' path, whose
random component is replaced so only the switch's shape is pinned."
  (mapcar (lambda (file)
            (cons (file-name-nondirectory file)
                  (replace-regexp-in-string
                   "\\(--unsaved-file=[^:\n]*:\\)[^\n]*" "\\1<TEMPFILE>"
                   (decode-coding-string (ac-rtags-test-file-bytes file) 'utf-8))))
          (sort (directory-files ac-rtags-test-requests t "\\`[0-9]") #'string<)))

(defun ac-rtags-test-recorded-argv ()
  "Return just the argument vector of each recorded rc invocation."
  (mapcar (lambda (record)
            (let ((lines (cdr (split-string (cdr record) "\n")))
                  argv)
              (while (and lines (string-prefix-p "  " (car lines)))
                (push (substring (pop lines) 2) argv))
              (cons (car record) (nreverse argv))))
          (ac-rtags-test-recorded)))

(defun ac-rtags-test-invocations ()
  "Return how many times the package has run rc so far."
  (length (directory-files ac-rtags-test-requests nil "\\`[0-9]")))

(defconst ac-rtags-test-widget-header
  (concat "#pragma once\n"
          "#include <string>\n"
          "\n"
          "namespace ui {\n"
          "class Widget {\n"
          "public:\n"
          "    void insert(int idx, char ch);\n"
          "    void insertAll(const std::string &text, int idx);\n"
          "    std::string label;\n"
          "};\n"
          "}\n"))

(defun ac-rtags-test-open (relative text)
  "Write TEXT to RELATIVE below the sandbox, visit it, and arm ac-rtags.
The package ships no setup command: its documented use is to make
`ac-source-rtags' the buffer's `ac-sources' and turn on `auto-complete-mode'."
  (let ((buffer (find-file-noselect
                 (ac-rtags-test-write
                  (expand-file-name relative ac-rtags-test-root) text))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (setq ac-sources '(ac-source-rtags))
    (auto-complete-mode 1)
    buffer))

(defun ac-rtags-test-candidate-details (candidates)
  "Return each candidate with the rtags kind, signature and document string."
  (mapcar (lambda (candidate)
            (list (substring-no-properties candidate)
                  (get-text-property 0 'ac-rtags-type candidate)
                  (get-text-property 0 'ac-rtags-full candidate)
                  (ac-rtags-document candidate)))
          candidates))

(defun ac-rtags-test-last-completion ()
  "Describe the candidate auto-complete last inserted, and where."
  (and ac-last-completion
       (let ((candidate (cdr ac-last-completion)))
         (list (substring-no-properties candidate)
               (get-text-property 0 'ac-rtags-type candidate)
               (ac-rtags-document candidate)
               (marker-position (car ac-last-completion))))))

(defun ac-rtags-test-line ()
  (buffer-substring-no-properties
   (line-beginning-position) (line-end-position)))

(defun ac-rtags-test-messages (regexp)
  "Return the echo-area lines matching REGEXP, in order."
  (with-current-buffer (get-buffer-create "*Messages*")
    (cl-remove-if-not
     (lambda (line) (string-match-p regexp line))
     (split-string
      (buffer-substring-no-properties (point-min) (point-max)) "\n" t))))

(defun ac-rtags-test-attempt ()
  "Complete at point, reporting a signal instead of letting it escape."
  (list :error (condition-case error
                   (progn (auto-complete) 'completed)
                 (error (list (car error) (error-message-string error))))
        :line (ac-rtags-test-line)
        :point (point)
        :candidates ac-candidates
        :not-indexed rtags-last-request-not-indexed
        :not-connected rtags-last-request-not-connected
        :invocations (ac-rtags-test-invocations)))

;; rtags.el resolves `rc' through `exec-path', so the stand-in has to exist
;; before a workflow runs.
(ac-rtags-test-install-rc)
"##;

fn ac_rtags_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_RTAGS_MELPA_PIN, "ac-rtags.el")
        .expect("prepare pinned ac-rtags source below ./tmp")
        .with_prelude(AC_RTAGS_TEST_PRELUDE)
        .with_timeout(AC_RTAGS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-rtags parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_rtags_parity` cases (2a).
pub(crate) fn assert_ac_rtags_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_rtags_oracle(), &name, "ac_rtags_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_rtags_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_rtags_batch(&cases);
}

// END generated package batch tests
