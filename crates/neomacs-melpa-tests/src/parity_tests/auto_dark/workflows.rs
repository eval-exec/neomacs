//! The two shell detectors run against real programs on `PATH`.
//!
//! `detection.rs` reaches every detector by replacing
//! `shell-command-to-string`, which pins the command auto-dark *composes* and
//! how it parses a fixture reply, but never crosses the subprocess seam. The
//! command is asserted as a string; it is never asserted as an argument vector
//! a program actually received, and nothing runs a shell.
//!
//! Three of the five detectors cannot run here -- `ns-do-applescript` and
//! `mac-application-state` are macOS primitives and `w32-read-registry` is a
//! Windows one -- so replacing them is the legitimate kind of double. The two
//! shell detectors are different: they go through `/bin/sh`, and a stand-in on
//! `PATH` makes the whole path real.
//!
//! **Stated limit on what is recorded.** The argument vectors below are
//! observed -- they are what the shell handed the program. The *outputs* are
//! fixtures, not recordings: real `osascript` needs macOS and real `cmd
//! uimode` needs Android, and neither is obtainable here, so the stand-ins
//! reply with the text the package's own parser looks for. Nothing here claims
//! to know what those tools return on their own platforms; that half stays
//! with `detection.rs`, where it is honestly a fixture too.

use expect_test::expect;

use super::ParityBatchCase;

/// Run the osascript and termux detectors against real programs.
///
/// Two things fall out that a replaced `shell-command-to-string` cannot show.
///
/// The AppleScript reaches `osascript` as **one** argument. The package writes
/// `-e 'tell application "System Events" to …'`, and the shell strips the
/// single quotes and keeps the embedded double quotes, so argv is exactly two
/// elements. A pinned command *string* is equally consistent with the script
/// arriving as eight separate words, which would be a different program
/// invocation entirely.
///
/// The termux detector succeeds on a program that writes **only to standard
/// error**. Its command line is `echo -n $(cmd uimode night 2>&1 </dev/null)`,
/// and the stand-in here puts its reply on stderr and nothing at all on
/// stdout; the detector still returns `t`.
///
/// The control was meant to show that the `2>&1` is what makes that work, and
/// it shows the opposite, which is why it is worth keeping in the snapshot:
/// running the same program **without** the redirect returns the text too.
/// `shell-command-to-string` merges standard error into its output on its own,
/// so the package's redirect is not what rescues a stderr-only reply here.
/// That is worth pinning precisely because the intuition it corrects is the
/// obvious one -- a reader who assumes the redirect is load-bearing would also
/// assume a detector could be broken by removing it.
fn the_shell_detectors_reach_real_programs_with_one_argument_vector_each() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_shell_detectors_reach_real_programs_with_one_argument_vector_each",
        r##"(let* ((root (file-name-as-directory
                     (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
               (bin (expand-file-name "auto-dark-bin" root))
               (argv (expand-file-name "argv" root))
               (write
                (lambda (path content)
                  (let ((coding-system-for-write 'utf-8-unix))
                    (with-temp-buffer
                      (insert content)
                      (write-region (point-min) (point-max) path nil 'silent)))
                  (set-file-modes path #o755)))
               (record
                ;; Log every argument the shell really passed, NUL separated
                ;; so an argument containing whitespace stays one field.
                (concat "#!/bin/sh\n"
                        ": > \"$AUTO_DARK_TEST_ARGV.$(basename \"$0\")\"\n"
                        "for a in \"$@\"; do"
                        " printf '%s\\0' \"$a\""
                        " >> \"$AUTO_DARK_TEST_ARGV.$(basename \"$0\")\";"
                        " done\n")))
          (make-directory bin t)
          (funcall write (expand-file-name "osascript" bin)
                   (concat record "printf 'true\\n'\n"))
          ;; Replies on stderr only, and writes nothing to stdout.
          (funcall write (expand-file-name "cmd" bin)
                   (concat record "printf 'Night mode: yes' >&2\n"))
          (setenv "AUTO_DARK_TEST_ARGV" argv)
          (setq exec-path (cons bin exec-path))
          (setenv "PATH" (concat bin path-separator (getenv "PATH")))
          (let* ((osascript (auto-dark--is-dark-mode-osascript))
                 (termux (auto-dark--is-dark-mode-termux))
                 (read-argv
                  (lambda (program)
                    (let ((file (concat argv "." program)))
                      (and (file-readable-p file)
                           (split-string
                            (with-temp-buffer
                              (insert-file-contents-literally file)
                              (buffer-string))
                            "\0" t))))))
            (list
             :osascript-said osascript
             :osascript-argv (funcall read-argv "osascript")
             :termux-said termux
             :termux-argv (funcall read-argv "cmd")
             ;; Control: the same program without the package's redirect.
             :without-the-redirect
             (shell-command-to-string "cmd uimode night")
             :with-the-redirect
             (shell-command-to-string
              "cmd uimode night 2>&1 </dev/null"))))"##,
        expect![[
            r#"OK (:osascript-said t :osascript-argv ("-e" "tell application \"System Events\" to tell appearance preferences to return dark mode") :termux-said t :termux-argv ("uimode" "night") :without-the-redirect "Night mode: yes" :with-the-redirect "Night mode: yes")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![the_shell_detectors_reach_real_programs_with_one_argument_vector_each()]
}
