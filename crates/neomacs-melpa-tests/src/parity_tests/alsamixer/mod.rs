use std::time::Duration;

use crate::{ALSAMIXER_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALSAMIXER_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// alsamixer.el is a thin front end to the `amixer' command line tool: it
/// builds a *shell command string* -- not an argument vector -- and runs it
/// through `shell-command-to-string', then scrapes the volume out of amixer's
/// output with one regexp.
///
/// `amixer' is not installed on this host, so it is the one thing stood in for.
/// The stand-in goes on PATH under its real name, because the package runs the
/// command through a shell and finds it exactly as it would find the real
/// binary; it records every command line it is invoked with, and keeps the
/// mixer state in a file so that `sset' and `toggle' really change what a later
/// `sget' reports.  That makes the round trip genuine: raising the volume reads
/// the current level, computes the new one and writes it, and the next read
/// sees it.  Everything above the shell -- the command construction, the option
/// handling, the clamping, the regexp scrape and the message -- is the
/// package's own code.
const ALSAMIXER_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar als-test-home
  (file-name-as-directory
   (expand-file-name "alsa" (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defvar als-test-bin
  (file-name-as-directory (expand-file-name "bin" als-test-home)))

(defun als-test-write (path text)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun als-test-install-amixer (&optional volume switch)
  "Install a recording stand-in `amixer' on PATH and set its starting state.
The package builds a *shell* command string and runs it through
`shell-command-to-string', so the stand-in is found the way the real amixer
would be.  It records every command line, keeps the mixer state in a file so
`sset' and `toggle' really change what a later `sget' reports, and prints
output in amixer's own format."
  (let ((path (expand-file-name "amixer" als-test-bin)))
    (make-directory als-test-bin t)
    (als-test-write (expand-file-name "state" als-test-home)
                    (format "%d %s\n" (or volume 40) (or switch "on")))
    (als-test-write (expand-file-name "amixer.log" als-test-home) "")
    (als-test-write
     path
     (concat
      "#!/bin/sh\n"
      "home=" als-test-home "\n"
      "printf 'amixer %s\\n' \"$*\" >> \"$home/amixer.log\"\n"
      "if [ -f \"$home/override-output\" ]; then cat \"$home/override-output\"; fi\n"
      "if [ -f \"$home/override-status\" ]; then exit \"$(cat \"$home/override-status\")\"; fi\n"
      "if [ -f \"$home/override-output\" ]; then exit 0; fi\n"
      "while [ $# -gt 0 ]; do\n"
      "  case $1 in\n"
      "    -*) shift 2 ;;\n"
      "    *) break ;;\n"
      "  esac\n"
      "done\n"
      "action=$1; control=$2\n"
      "vol=$(cut -d' ' -f1 \"$home/state\")\n"
      "sw=$(cut -d' ' -f2 \"$home/state\")\n"
      "case $action in\n"
      "  sset) vol=$(printf '%s' \"$4\" | tr -d '%')\n"
      "        printf '%s %s\\n' \"$vol\" \"$sw\" > \"$home/state\" ;;\n"
      "  set)  if [ \"$sw\" = on ]; then sw=off; else sw=on; fi\n"
      "        printf '%s %s\\n' \"$vol\" \"$sw\" > \"$home/state\" ;;\n"
      "esac\n"
      "raw=$((vol * 65536 / 100))\n"
      "printf \"Simple mixer control '%s',0\\n\" \"$control\"\n"
      "printf '  Capabilities: pvolume pswitch pswitch-joined\\n'\n"
      "printf '  Playback channels: Front Left - Front Right\\n'\n"
      "printf '  Limits: Playback 0 - 65536\\n'\n"
      "printf '  Mono:\\n'\n"
      "printf '  Front Left: Playback %s [%s%%] [-20.00dB] [%s]\\n' \"$raw\" \"$vol\" \"$sw\"\n"
      "printf '  Front Right: Playback %s [%s%%] [-20.00dB] [%s]\\n' \"$raw\" \"$vol\" \"$sw\"\n"
      "exit 0\n"))
    (set-file-modes path #o755)
    (setenv "PATH" (concat (directory-file-name als-test-bin)
                           path-separator (getenv "PATH")))
    path))

(defun als-test-uninstall-amixer ()
  "Remove the stand-in so the shell cannot find any amixer at all."
  (let ((path (expand-file-name "amixer" als-test-bin)))
    (when (file-exists-p path) (delete-file path))))

(defun als-test-force (output status)
  "Make the next runs print OUTPUT and exit with STATUS."
  (if output
      (als-test-write (expand-file-name "override-output" als-test-home) output)
    (let ((path (expand-file-name "override-output" als-test-home)))
      (when (file-exists-p path) (delete-file path))))
  (if status
      (als-test-write (expand-file-name "override-status" als-test-home)
                      (number-to-string status))
    (let ((path (expand-file-name "override-status" als-test-home)))
      (when (file-exists-p path) (delete-file path)))))

(defun als-test-commands ()
  "Every command line the stand-in amixer was invoked with, oldest first."
  (let ((path (expand-file-name "amixer.log" als-test-home)))
    (and (file-exists-p path)
         (mapcar #'copy-sequence
                 (split-string
                  (with-temp-buffer
                    (let ((coding-system-for-read 'utf-8))
                      (insert-file-contents path))
                    (buffer-string))
                  "\n" t)))))

(defun als-test-reset-log ()
  (als-test-write (expand-file-name "amixer.log" als-test-home) ""))

(defun als-test-state ()
  "The mixer state the stand-in is holding, as (VOLUME SWITCH)."
  (let ((text (with-temp-buffer
                (insert-file-contents (expand-file-name "state" als-test-home))
                (buffer-string))))
    (let ((parts (split-string text nil t)))
      (list (string-to-number (car parts)) (copy-sequence (cadr parts))))))

(defun als-test-run (form)
  "Evaluate FORM and report what the user is told.
`alsamixer-set-volume' ends in `message', so its return value is the text the
echo area shows; it binds `message-log-max' to nil, so that text must never
reach *Messages*, which is checked here rather than assumed."
  (let ((result (funcall form)))
    (list :shown (if (stringp result) (copy-sequence result) result)
          :logged-to-messages
          (with-current-buffer (get-buffer-create "*Messages*")
            (and (string-match-p
                  "Volume set to"
                  (buffer-substring-no-properties (point-min) (point-max)))
                 t)))))

(defun als-test-hide-shell (text)
  "Replace the host's shell path in TEXT.
A `command not found' diagnostic names the shell binary, which on this host is
an absolute store path; it is the host's, not the package's."
  (if (stringp text)
      (copy-sequence
       (replace-regexp-in-string (regexp-quote shell-file-name) "[SHELL]"
                                 text t t))
    text))

(defun als-test-attempt (form)
  "Evaluate FORM, reporting a signal instead of letting it escape."
  (condition-case error
      (list :returned (als-test-hide-shell (funcall form)))
    (error (list :signal (car error)
                 :message (als-test-hide-shell
                           (error-message-string error))))))
"##;

fn alsamixer_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALSAMIXER_MELPA_PIN, "alsamixer.el")
        .expect("prepare pinned alsamixer source below ./tmp")
        .with_prelude(ALSAMIXER_TEST_PRELUDE)
        .with_timeout(ALSAMIXER_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed alsamixer parity test")
        .into()
}

/// Multi-probe batch for `assert_alsamixer_parity` cases (2a).
pub(crate) fn assert_alsamixer_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(alsamixer_oracle(), &name, "alsamixer_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn alsamixer_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_alsamixer_batch(&cases);
}

// END generated package batch tests
