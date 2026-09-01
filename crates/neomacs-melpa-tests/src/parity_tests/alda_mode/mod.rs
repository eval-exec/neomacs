use std::time::Duration;

use crate::{ALDA_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod editing;
mod history;
mod registry;
mod workflows;

const ALDA_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Sandbox helpers shared by the workflows.
///
/// alda-mode shells out to the Alda CLI and never reads its output, so the
/// thing worth testing is the *argument vector* that reaches the binary.  The
/// corpus this replaces asserted that vector by redefining alda-mode's own
/// `alda-run-cmd' and `alda-location', which cannot catch a defect in either of
/// them and leaves the real boundary untouched.
///
/// These workflows install a recording stand-in on `exec-path' instead, so the
/// package's real discovery, real command construction and real
/// `start-process' all run, and the vector is captured where it actually
/// crosses out of Emacs.  Every reply the stand-in serves was recorded from
/// **Alda 2.3.2**, including which stream it arrived on -- alda writes
/// "Playing..." to stderr, not stdout -- and the stand-in refuses, loudly, to
/// answer an argument vector it has no recording for.  That refusal matters
/// more here than for a package that parses output: since alda-mode ignores
/// what comes back, a stand-in that silently returned nothing would be
/// indistinguishable from success.
const ALDA_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)

(defvar alda-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar alda-test-records
  (file-name-as-directory (expand-file-name "alda-records" alda-test-root)))

(defvar alda-test-calls (expand-file-name "alda-calls.log" alda-test-root))
(defvar alda-test-misses (expand-file-name "alda-misses.log" alda-test-root))

(defconst alda-test-recordings
  '(
    (("down") 1 "" "Usage:\n  alda [command]\n\nAvailable Commands:\n  completion  Generate the autocompletion script for the specified shell\n  doctor      Run health checks to determine if Alda can run correctly\n  export      Evaluate Alda source code and export to another format\n  help        Help about any command\n  import      Import Alda source code from other formats\n  instruments Display the list of available instruments\n  parse       Display the result of parsing Alda source code\n  play        Evaluate and play Alda source code\n  ps          List background processes\n  repl        Start an Alda REPL client/server\n  shutdown    Shut down background processes\n  stop        Stop playback\n  telemetry   Enable or disable telemetry\n  update      Update to the latest version of Alda\n  version     Print Alda version information\n\nFlags:\n  -v, --verbosity int   verbosity level (0-3) (default 1)\n\nUse \"alda [command] --help\" for more information about a command.\n\n---\n\nUsage error:\n\n  unknown command \"down\" for \"alda\"\n\n")
    (("play" "--file" "/home/exec/Projects/github.com/eval-exec/neomacs-windows/tmp/alda-rec/score.alda") 0 "" "Starting player processes...\nPlaying...\n")
    (("play" "-F" "alda-mode-internal-marker" "--code" "\npiano:\n  o4 c d e\n\n%alda-mode-internal-marker\nf g a") 0 "" "Playing...\n")
    (("play" "-F" "" "--code" "piano: o4 c d e") 0 "" "Playing...\n")
    (("stop") 0 "" "Stopping playback.\n")))

(defun alda-test-key (arguments)
  "Return the record key for ARGUMENTS.

Any argument holding a `/' is reduced to its base name first: alda-mode passes
`(buffer-file-name)' to `alda play --file', so the absolute path differs
between the machine a recording was made on and the per-case sandbox, and
keying on it verbatim would miss every time.  Must agree exactly with the
shell stand-in's key function."
  (mapconcat
   (lambda (argument)
     (let ((base (if (string-match-p "/" argument)
                     (file-name-nondirectory argument)
                   argument)))
       (concat "~" (replace-regexp-in-string "[^A-Za-z0-9._-]" "_" base))))
   arguments ""))

(defun alda-test-write (path content &optional executable)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert content)
      (write-region (point-min) (point-max) path nil 'silent)))
  (when executable (set-file-modes path #o755))
  path)

(defconst alda-test-stand-in
  (string-join
   (list
    "#!/bin/sh"
    "# Replay stand-in for the Alda CLI 2.3.2.  Every reply below was recorded"
    "# from the real binary; this script only looks one up, and refuses to"
    "# invent an answer for an argument vector it has no recording for."
    "key=\"\""
    "for a in \"$@\"; do"
    "  case \"$a\" in */*) a=${a##*/} ;; esac"
    "  key=\"$key~$(printf '%s' \"$a\" | tr -c 'A-Za-z0-9._-' '_')\""
    "done"
    "# Newlines inside one argument -- alda-mode hands a whole multi-line score"
    "# to --code -- must not fragment a single invocation into several log"
    "# lines, so they are folded to ~ for the log only."
    "printf '%s\\n' \"$(printf '%s|' \"$@\" | tr '\\n' '~')\" >> \"$ALDA_TEST_CALLS\""
    "d=\"$ALDA_TEST_RECORDS/$key\""
    "if [ ! -f \"$d/rc\" ]; then"
    "  printf '%s\\n' \"$(printf '%s|' \"$@\" | tr '\\n' '~')\" >> \"$ALDA_TEST_MISSES\""
    "  printf 'UNRECORDED alda invocation: %s\\n' \"$*\" >&2"
    "  exit 99"
    "fi"
    "cat \"$d/out\""
    "cat \"$d/err\" >&2"
    "exit \"$(cat \"$d/rc\")\""
    "")
   "\n"))

(defun alda-test-install-alda ()
  "Install the recorded Alda CLI stand-in first on `exec-path' and PATH.

Returns the number of records installed.  The stand-in goes on `exec-path'
rather than being wired in through `alda-binary-location', so that the
package's own `alda-location' discovery runs for real."
  (let ((installed nil)
        (bin (expand-file-name "bin" alda-test-root)))
    (dolist (recording alda-test-recordings)
      (let* ((key (alda-test-key (nth 0 recording)))
             (path (expand-file-name key alda-test-records)))
        (when (member path installed)
          (error "Record key collision for %S" (nth 0 recording)))
        (push path installed)
        (alda-test-write (expand-file-name "rc" path)
                         (format "%d\n" (nth 1 recording)))
        (alda-test-write (expand-file-name "out" path) (nth 2 recording))
        (alda-test-write (expand-file-name "err" path) (nth 3 recording))))
    (setenv "ALDA_TEST_RECORDS" (directory-file-name alda-test-records))
    (setenv "ALDA_TEST_CALLS" alda-test-calls)
    (setenv "ALDA_TEST_MISSES" alda-test-misses)
    (alda-test-write (expand-file-name "alda" bin) alda-test-stand-in t)
    (setq exec-path (cons bin exec-path))
    (setenv "PATH" (concat bin path-separator (getenv "PATH")))
    (length installed)))

(defun alda-test-calls ()
  "Every argument vector that reached the Alda binary, oldest first."
  (if (not (file-exists-p alda-test-calls))
      'alda-was-never-run
    (with-temp-buffer
      (insert-file-contents alda-test-calls)
      (split-string (buffer-string) "\n" t))))

(defun alda-test-unrecorded ()
  "Invocations the stand-in had no recording for.

Every workflow asserts this is empty.  alda-mode never reads the binary's
output, so a stand-in that silently answered nothing would look exactly like a
successful run and the argument vector under test would go unchecked -- this
list is the only thing that distinguishes the two."
  (if (not (file-exists-p alda-test-misses))
      nil
    (with-temp-buffer
      (insert-file-contents alda-test-misses)
      (split-string (buffer-string) "\n" t))))

(defun alda-test-score-buffer (name text)
  "Visit a real .alda file called NAME containing TEXT and select it."
  (let* ((path (alda-test-write (expand-file-name name alda-test-root) text))
         (buffer (let ((enable-dir-local-variables nil))
                   (find-file-noselect path))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    buffer))

(defun alda-test-settle (&optional seconds)
  "Wait for alda-mode's asynchronous playback processes to finish."
  (let ((deadline (+ (float-time) (or seconds 10.0)))
        (previous nil)
        (stable 0))
    (while (and (< (float-time) deadline) (< stable 3))
      (accept-process-output nil 0.05)
      (let ((now (list (alda-test-calls)
                       (and (get-buffer "*alda-output*")
                            (buffer-size (get-buffer "*alda-output*")))
                       (seq-count (lambda (p) (process-live-p p))
                                  (process-list)))))
        (if (equal now previous) (setq stable (1+ stable)) (setq stable 0))
        (setq previous now)))
    stable))

(defun alda-test-output ()
  "What the user sees in alda-mode's playback output buffer."
  (let ((buffer (get-buffer "*alda-output*")))
    (if (not buffer)
        'no-output-buffer
      (with-current-buffer buffer
        (buffer-substring-no-properties (point-min) (point-max))))))
"##;

fn alda_mode_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALDA_MODE_MELPA_PIN, source_file)
        .expect("prepare pinned alda-mode source below ./tmp")
        .with_prelude(ALDA_MODE_TEST_PRELUDE)
        .with_timeout(ALDA_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed alda-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_alda_mode_autoload_parity` cases (2a).
pub(crate) fn assert_alda_mode_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        alda_mode_oracle("alda-mode-autoloads.el"),
        &name,
        "alda_mode_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_alda_mode_parity` cases (2a).
pub(crate) fn assert_alda_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        alda_mode_oracle("alda-mode.el"),
        &name,
        "alda_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn alda_mode_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_alda_mode_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_alda_mode_autoload_batch(&cases);
}

#[test]
fn alda_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        editing::editing_public_surface_batch_cases(),
        history::history_public_surface_batch_cases(),
        registry::registry_alda_mode_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_alda_mode_batch(&cases);
}

// END generated package batch tests
