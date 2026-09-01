use std::time::Duration;

use crate::{AC_RACER_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_RACER_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// ac-racer is an auto-complete source that shells out to the `racer' binary:
/// `ac-racer--candidates' writes the buffer to `ac-racer--tempfile', then runs
/// `racer complete <line> <column> <file> <tempfile>' with RUST_SRC_PATH in
/// the environment and parses the `MATCH name,line,column,path,type,signature'
/// lines it prints.  That binary needs a Rust toolchain and source tree, so it
/// is the one boundary the workflows fake: a recording stand-in is installed
/// as the only executable on `exec-path' *before* the package is loaded, so
/// racer.el's own `racer-cmd' and `racer-rust-src-path' defaults resolve to
/// the sandbox.  The stand-in records its exact argument vector, environment,
/// stdin, working directory and the temporary file it was handed, then prints
/// realistic racer output.  Everything else — prefix detection, the process
/// call, MATCH parsing, popup item construction and auto-complete's rendering
/// and insertion — is the package's own real code.
const AC_RACER_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar ac-racer-test-root
  (file-name-as-directory
   (expand-file-name "rust" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defvar ac-racer-test-bin
  (file-name-as-directory (expand-file-name "bin" ac-racer-test-root)))

(defvar ac-racer-test-requests
  (file-name-as-directory (expand-file-name "requests" ac-racer-test-root)))

(defvar ac-racer-test-responses
  (file-name-as-directory (expand-file-name "responses" ac-racer-test-root)))

(defvar ac-racer-test-src
  (file-name-as-directory (expand-file-name "rust-src" ac-racer-test-root)))

(make-directory ac-racer-test-requests t)
(make-directory ac-racer-test-responses t)
(make-directory ac-racer-test-src t)

(defun ac-racer-test-write (path text)
  "Write TEXT to PATH as UTF-8 and return PATH."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun ac-racer-test-install-racer ()
  "Install a recording stand-in `racer' as the only executable on `exec-path'."
  (let ((path (expand-file-name "racer" ac-racer-test-bin)))
    (make-directory ac-racer-test-bin t)
    (ac-racer-test-write
     path
     (concat
      "#!/bin/sh\n"
      "root=" ac-racer-test-root "\n"
      "requests=" ac-racer-test-requests "\n"
      "responses=" ac-racer-test-responses "\n"
      "n=1\n"
      "[ -f \"$root/.total\" ] && n=$(($(cat \"$root/.total\") + 1))\n"
      "printf '%s' \"$n\" > \"$root/.total\"\n"
      "record=$(printf '%s%02d-request' \"$requests\" \"$n\")\n"
      "if [ -t 0 ]; then stdin='<terminal>'; else stdin=$(cat); fi\n"
      "{\n"
      "  printf 'argv:'\n"
      "  printf ' %s' \"$@\"\n"
      "  printf '\\nRUST_SRC_PATH: %s\\n' \"$RUST_SRC_PATH\"\n"
      "  printf 'stdin: %s\\n' \"$stdin\"\n"
      "  printf 'cwd: %s\\n' \"$PWD\"\n"
      "  if [ -f \"$5\" ]; then\n"
      "    printf 'tempfile(%s):\\n' \"$5\"\n"
      "    cat \"$5\"\n"
      "  else\n"
      "    printf 'tempfile: <missing>\\n'\n"
      "  fi\n"
      "} > \"$record\"\n"
      "[ -f \"$responses/stdout.$n\" ] && cat \"$responses/stdout.$n\"\n"
      "[ -f \"$responses/stdout\" ] && [ ! -f \"$responses/stdout.$n\" ] && cat \"$responses/stdout\"\n"
      "[ -f \"$responses/stderr.$n\" ] && cat \"$responses/stderr.$n\" >&2\n"
      "status=0\n"
      "[ -f \"$responses/status.$n\" ] && status=$(cat \"$responses/status.$n\")\n"
      "exit \"$status\"\n"))
    (set-file-modes path #o755)
    (setq exec-path (list (directory-file-name ac-racer-test-bin)))
    path))

(defun ac-racer-test-reply (stdout &optional nth status stderr)
  "Make the NTH stand-in run print STDOUT and STDERR, then exit with STATUS.
Without NTH the output is used for every run that has no specific reply."
  (ac-racer-test-write
   (expand-file-name (if nth (format "stdout.%d" nth) "stdout")
                     ac-racer-test-responses)
   stdout)
  (when status
    (ac-racer-test-write
     (expand-file-name (format "status.%d" (or nth 1)) ac-racer-test-responses)
     (number-to-string status)))
  (when stderr
    (ac-racer-test-write
     (expand-file-name (format "stderr.%d" (or nth 1)) ac-racer-test-responses)
     stderr)))

(defun ac-racer-test-file-bytes (path)
  "Return the exact bytes of PATH as a unibyte string."
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (buffer-string)))

(defun ac-racer-test-file-text (path)
  (decode-coding-string (ac-racer-test-file-bytes path) 'utf-8))

(defun ac-racer-test-recorded ()
  "Return every invocation the stand-in racer recorded, in order."
  (mapcar (lambda (file)
            (cons (file-name-nondirectory file)
                  (ac-racer-test-file-text file)))
          (sort (directory-files ac-racer-test-requests t "\\`[0-9]") #'string<)))

(defun ac-racer-test-invocations ()
  "Return how many times the package has run racer so far."
  (length (directory-files ac-racer-test-requests nil "\\`[0-9]")))

(defun ac-racer-test-project ()
  "Create the manifest of a small real cargo project in the sandbox."
  (ac-racer-test-write
   (expand-file-name "Cargo.toml" ac-racer-test-root)
   "[package]\nname = \"scoreboard\"\nversion = \"0.1.0\"\nedition = \"2018\"\n"))

(defun ac-racer-test-open (relative text)
  "Write TEXT to RELATIVE below the sandbox and visit it in the live window."
  (let ((buffer (find-file-noselect
                 (ac-racer-test-write
                  (expand-file-name relative ac-racer-test-root) text))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    buffer))

(defun ac-racer-test-candidate-details (candidates)
  "Return each candidate with the popup summary and document racer supplied."
  (mapcar (lambda (candidate)
            (list (substring-no-properties candidate)
                  (popup-item-summary candidate)
                  (popup-item-document candidate)))
          candidates))

(defun ac-racer-test-last-completion ()
  "Describe the candidate auto-complete last inserted, and where."
  (and ac-last-completion
       (let ((candidate (cdr ac-last-completion)))
         (list (substring-no-properties candidate)
               (popup-item-summary candidate)
               (popup-item-document candidate)
               (marker-position (car ac-last-completion))))))

(defun ac-racer-test-line ()
  (buffer-substring-no-properties
   (line-beginning-position) (line-end-position)))

(defun ac-racer-test-byte-column ()
  "Return the column racer really wants, so the character column it gets shows."
  (string-bytes
   (buffer-substring-no-properties (line-beginning-position) (point))))

;; racer.el resolves `racer-cmd' with `executable-find' and `racer-rust-src-path'
;; from $RUST_SRC_PATH when it is loaded, so both have to exist before the
;; harness loads ac-racer.el.
(setenv "RUST_SRC_PATH" (directory-file-name ac-racer-test-src))
(ac-racer-test-install-racer)
"##;

fn ac_racer_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_RACER_MELPA_PIN, "ac-racer.el")
        .expect("prepare pinned ac-racer source below ./tmp")
        .with_prelude(AC_RACER_TEST_PRELUDE)
        .with_timeout(AC_RACER_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-racer parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_racer_parity` cases (2a).
pub(crate) fn assert_ac_racer_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_racer_oracle(), &name, "ac_racer_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_racer_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_racer_batch(&cases);
}

// END generated package batch tests
