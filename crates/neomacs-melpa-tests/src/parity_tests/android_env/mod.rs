use std::time::Duration;

use crate::{ANDROID_ENV_MELPA_PIN, CachedMelpaOracle, S_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANDROID_ENV_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// android-env drives gradle, adb and avdmanager, none of which exist on this
/// host.  It never links against them though: every call is a command line it
/// builds and hands to a shell, at a path it computes from `ANDROID_SDK_ROOT'
/// or from `locate-dominating-file'.  So the boundary is an argv, and the
/// workflows put a real executable where the package looks for one --
/// recording its arguments, printing realistic output and exiting with a
/// realistic status -- while the package's own path building, quoting,
/// process handling and output parsing all run for real.
///
/// `android-env-refactor' and `android-env-recursive-refactor' need no
/// external tool at all and work on real files in the sandbox.
const ANDROID_ENV_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun aenv-test-plain (value)
  (cond ((stringp value) (substring-no-properties value))
        ((consp value)
         (cons (aenv-test-plain (car value)) (aenv-test-plain (cdr value))))
        (t value)))

(defun aenv-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun aenv-test-write (name text &optional executable)
  "Write TEXT to NAME below the sandbox and return its absolute path."
  (let ((path (aenv-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    (when executable (set-file-modes path #o755))
    path))

(defun aenv-test-read (path)
  (if (file-exists-p path)
      (with-temp-buffer (insert-file-contents path) (buffer-string))
    ""))

(defvar aenv-test-argv-log nil
  "Absolute path the stand-in executables append their argv to.")

(defun aenv-test-install-sdk ()
  "Install a recording adb and avdmanager where android-env looks for them.
Return the SDK root, which is also left in `ANDROID_SDK_ROOT'."
  (setq aenv-test-argv-log (aenv-test-path "sdk/argv.log"))
  (aenv-test-write
   "sdk/platform-tools/adb"
   (concat "#!/bin/sh\n"
           "{ printf 'adb'; for a in \"$@\"; do printf ' [%s]' \"$a\"; done;"
           " printf '\\n'; } >> '" aenv-test-argv-log "'\n"
           "case \"$1 $2 $3\" in\n"
           "  'shell ps '*) printf '  PID CMD\\n"
           "  911 com.example.checkout\\n"
           " 1024 com.example.tools:remote\\n' ;;\n"
           "  'logcat -c '*) printf 'logcat cleared\\n' ;;\n"
           "  'logcat -b crash') printf"
           " 'F/libc( 911): Fatal signal 11 in tid 911\\n' ;;\n"
           "  'logcat *:S Checkout') printf"
           " 'I/Checkout( 911): charge accepted\\n' ;;\n"
           "  'logcat  ') printf 'I/Checkout( 911): charge accepted\\n"
           "D/Gateway( 911): retrying\\n"
           "I/Sync( 1024): idle\\n' ;;\n"
           "  *) printf 'ok\\n' ;;\n"
           "esac\n")
   t)
  (aenv-test-write
   "sdk/tools/bin/avdmanager"
   (concat "#!/bin/sh\n"
           "{ printf 'avdmanager'; for a in \"$@\"; do printf ' [%s]' \"$a\";"
           " done; printf '\\n'; } >> '" aenv-test-argv-log "'\n"
           "printf 'Pixel_6_API_33\\000Pixel_Tablet_API_34\\000"
           "Nexus_5X_API_29\\000'\n")
   t)
  ;; `android-env-emulator-command' is a defcustom precisely so the emulator
  ;; can live somewhere other than PATH, so point it at a real script too.
  (setq android-env-emulator-command
        (aenv-test-write
         "sdk/emulator/emulator"
         (concat "#!/bin/sh\n"
                 "{ printf 'emulator'; for a in \"$@\"; do"
                 " printf ' [%s]' \"$a\"; done; printf '\\n'; } >> '"
                 aenv-test-argv-log "'\n"
                 "printf 'boot completed: %s\\n' \"$1\"\n")
         t))
  (setenv "ANDROID_SDK_ROOT" (aenv-test-path "sdk"))
  (aenv-test-path "sdk"))

(defun aenv-test-argv ()
  "Return each recorded command line, oldest first."
  (split-string (aenv-test-read aenv-test-argv-log) "\n" t))

(defun aenv-test-await (buffer)
  "Wait for BUFFER's process to finish, then return how the wait ended.
For a compilation buffer use `aenv-test-await-compilation' instead: a
compilation's text is not complete when its process dies."
  (let ((waited 0))
    (while (and (< waited 600)
                (get-buffer-process buffer)
                (process-live-p (get-buffer-process buffer)))
      (accept-process-output nil 0.05)
      (setq waited (1+ waited)))
    (if (< waited 600) :finished :timed-out)))

(defun aenv-test-compilation-complete-p (buffer)
  "Non-nil once `compilation-handle-exit' has written BUFFER's last line.
That line is the causal end of the output, not a guess about it: Emacs
drains a dying process's remaining reads before it runs the sentinel, the
sentinel is what calls `compilation-handle-exit', and that function marks
the text it writes with a `compilation-handle-exit' text property
\(lisp/progmodes/compile.el:2630).  The property therefore cannot appear
until every byte the child wrote has already been through
`compilation-filter'."
  (and (buffer-live-p (get-buffer buffer))
       (with-current-buffer buffer
         (and (text-property-not-all (point-min) (point-max)
                                     'compilation-handle-exit nil)
              t))))

(defun aenv-test-await-compilation (buffer)
  "Wait until BUFFER holds all of its compilation's output, or signal.
`process-live-p' going nil is NOT that condition.  A process can be gone
with reads still queued, and a pin taken at that moment records however
much of the child's output the kernel happened to have delivered -- a fact
about scheduling rather than about either editor, which is the defect
DIVERGENCES.md 133 removed from the `rg' suite.  Waiting for the sentinel's
own end marker removes the choice here the same way, and signalling rather
than returning means a future edit that reintroduces the race fails on its
first run instead of moving a snapshot months later."
  (let ((waited 0))
    (while (and (< waited 1200)
                (not (aenv-test-compilation-complete-p buffer)))
      (accept-process-output nil 0.05)
      (setq waited (1+ waited)))
    (unless (aenv-test-compilation-complete-p buffer)
      (error "aenv-test-await-compilation: %s never reached \
`compilation-handle-exit'; its text records only as much of the child's \
output as had been read" buffer))
    :finished))

(defun aenv-test-settle (buffer)
  "Wait for BUFFER's process to die and its output and sentinel to land.
A process that has just exited still has output queued and its sentinel
still to run, so waiting only for `process-live-p' captures a buffer that
is about to change -- and leaves the sentinel's line to appear in
whatever the next command puts in the buffer."
  (aenv-test-await buffer)
  (let ((stable 0) (previous nil) (rounds 0))
    (while (and (< rounds 600) (< stable 5))
      (accept-process-output nil 0.02)
      (let ((now (with-current-buffer buffer (buffer-string))))
        (setq stable (if (equal now previous) (1+ stable) 0))
        (setq previous now))
      (setq rounds (1+ rounds)))
    (if (< rounds 600) :settled :timed-out)))

(defun aenv-test-compilation-text (buffer)
  "Return BUFFER's text with the wall-clock parts replaced.
`compilation-start' stamps a start time and a duration into the buffer;
everything else about the run is deterministic."
  (let ((text (with-current-buffer buffer
                (buffer-substring-no-properties (point-min) (point-max)))))
    (setq text (replace-regexp-in-string
                "started at .*$" "started at <TIME>" text))
    (setq text (replace-regexp-in-string
                "\\(finished\\|exited abnormally with code [0-9]+\\) at .*$"
                "\\1 at <TIME>" text))
    text))

(defun aenv-test-locations (buffer)
  "Walk BUFFER's compilation errors and return where each one points."
  (with-current-buffer buffer
    (goto-char (point-min))
    (let (locations (done nil))
      (while (not done)
        (condition-case nil
            (progn
              (compilation-next-error 1)
              (let* ((message (get-text-property (point) 'compilation-message))
                     (loc (and message (compilation--message->loc message))))
                (setq locations
                      (append locations
                              (list (list :file
                                          (aenv-test-plain
                                           (caar (compilation--loc->file-struct
                                                  loc)))
                                          :line (compilation--loc->line loc)
                                          :column
                                          (compilation--loc->col loc)))))))
          (error (setq done t))))
      locations)))
"##;

fn android_env_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANDROID_ENV_MELPA_PIN, "android-env.el")
        .expect("prepare pinned android-env source below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s source below ./tmp")
        .with_prelude(ANDROID_ENV_TEST_PRELUDE)
        .with_timeout(ANDROID_ENV_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed android-env parity test")
        .into()
}

/// Multi-probe batch for `assert_android_env_parity` cases (2a).
pub(crate) fn assert_android_env_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(android_env_oracle(), &name, "android_env_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn android_env_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_android_env_batch(&cases);
}

// END generated package batch tests
