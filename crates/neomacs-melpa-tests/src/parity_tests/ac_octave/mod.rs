use std::time::Duration;

use crate::{AC_OCTAVE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_OCTAVE_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ac-octave answers auto-complete from a running `inferior-octave' process, so
/// every workflow needs a real Octave session.  Only the `octave' executable
/// itself is replaced: a recording stand-in first on `PATH' and `exec-path'
/// speaks the real inferior-octave startup handshake (banner, `octave> ' prompt,
/// `PS2', `disp (getenv ('OCTAVE_SRCDIR'))', `more off;', `PS1', `disp (pwd
/// ())'), logs its argv and every command line it receives, and answers each
/// request from a canned file keyed by the exact command text.  octave.el keeps
/// doing its own process startup, prompt matching and output digesting, and
/// ac-octave keeps building its own requests and parsing its own replies.
///
/// `inferior-octave-startup' finishes by sending a bare newline whose prompt it
/// never digests, so that reply is still in flight when the next request is made
/// and the very next digest would return it instead of its own output.
/// `aco-test-settle' starts the session through the public `run-octave' and then
/// drains until a read times out, which puts request and reply back in lockstep
/// without touching a package function.
const AC_OCTAVE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'auto-complete)
(require 'octave)

(defun aco-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun aco-test-write (name text)
  "Write TEXT into sandbox file NAME and return its absolute path."
  (let ((path (aco-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun aco-test-canned (command text)
  "Record TEXT as the inferior Octave answer to the exact line COMMAND."
  (aco-test-write
   (format "canned/%s.txt"
           (replace-regexp-in-string "[^A-Za-z0-9._]" "_" command))
   (if (string= text "") "" (concat text "\n"))))

(defconst aco-test-octave-program
  (concat
   "#!/bin/sh\n"
   "case \"$*\" in *--help*) exit 0 ;; esac\n"
   "printf 'ARGV %s\\n' \"$*\" >> \"$AC_OCTAVE_LOG\"\n"
   "printf 'GNU Octave, version 9.2.0\\n'\n"
   "printf 'Copyright (C) 2024 The Octave Project Developers.\\n'\n"
   "printf 'octave> '\n"
   "while IFS= read -r line; do\n"
   "  printf 'CMD %s\\n' \"$line\" >> \"$AC_OCTAVE_LOG\"\n"
   "  key=$(printf '%s' \"$line\" | tr -c 'A-Za-z0-9._' '_')\n"
   "  file=\"$AC_OCTAVE_CANNED/$key.txt\"\n"
   "  if [ -f \"$file\" ]; then\n"
   "    cat \"$file\"\n"
   "  fi\n"
   "  printf 'octave> '\n"
   "done\n"))

(defun aco-test-start-octave (responses)
  "Install a recording `octave' stand-in on PATH answering RESPONSES.

RESPONSES is a list of (COMMAND TEXT): COMMAND is the exact line the
inferior process receives, TEXT the lines it answers with.  A command
with no canned answer gets the bare prompt, which is what Octave sends
for a request that produces no output."
  (aco-test-canned "PS2" "ans = > ")
  (aco-test-canned "disp (getenv ('OCTAVE_SRCDIR'))" "")
  (aco-test-canned "disp (pwd ())" (aco-test-path "project"))
  (dolist (response responses)
    (apply #'aco-test-canned response))
  (set-file-modes (aco-test-write "bin/octave" aco-test-octave-program) #o755)
  (setenv "AC_OCTAVE_LOG" (aco-test-path "octave.log"))
  (setenv "AC_OCTAVE_CANNED" (aco-test-path "canned"))
  (setenv "PATH" (concat (aco-test-path "bin") path-separator (getenv "PATH")))
  (add-to-list 'exec-path (aco-test-path "bin"))
  (setq inferior-octave-program "octave"
        inferior-octave-startup-file nil))

(defun aco-test-settle ()
  "Start the Octave session and drain the reply nobody digests.

`inferior-octave-startup' ends by sending a bare newline without digesting
its prompt, so that reply would otherwise be picked up by the next
request instead of its own."
  (run-octave t)
  (while (accept-process-output inferior-octave-process 0.1)))

(defun aco-test-log-lines ()
  "Return the argv and command lines the `octave' stand-in recorded."
  (let ((log (aco-test-path "octave.log"))
        (root (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
    (if (not (file-exists-p log))
        'no-octave-process-started
      (with-temp-buffer
        (insert-file-contents log)
        (mapcar (lambda (line)
                  (replace-regexp-in-string (regexp-quote root) "<sandbox>/" line t t))
                (split-string (buffer-string) "\n" t))))))

(defun aco-test-visit (name)
  "Visit sandbox Octave file NAME in the selected window and arm the source."
  (let ((buffer (find-file-noselect (aco-test-path name))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (ac-octave-setup)
    (auto-complete-mode 1)
    buffer))

(defun aco-test-complete ()
  "Run one completion pass and return the plain candidate strings."
  (ac-start :force-init t)
  (ac-update t)
  (mapcar #'substring-no-properties ac-candidates))

(defmacro aco-test-session (&rest body)
  "Run BODY against the recording Octave session, then shut it down."
  `(unwind-protect
       (progn ,@body)
     (when (inferior-octave-process-live-p)
       (set-process-query-on-exit-flag inferior-octave-process nil)
       (delete-process inferior-octave-process))
     (dolist (buffer (buffer-list))
       (when (or (buffer-file-name buffer)
                 (equal (buffer-name buffer) inferior-octave-buffer))
         (with-current-buffer buffer (set-buffer-modified-p nil))
         (kill-buffer buffer)))))
"##;

fn ac_octave_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_OCTAVE_MELPA_PIN, "ac-octave.el")
        .expect("prepare pinned ac-octave source below ./tmp")
        .with_prelude(AC_OCTAVE_TEST_PRELUDE)
        .with_timeout(AC_OCTAVE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-octave parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_octave_parity` cases (2a).
pub(crate) fn assert_ac_octave_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_octave_oracle(), &name, "ac_octave_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_octave_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_octave_batch(&cases);
}

// END generated package batch tests
