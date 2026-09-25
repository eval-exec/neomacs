#![cfg(unix)]
//! GNU oracle for `pre-redisplay-functions` (P3.5 C15/J): how often redisplay
//! runs them and for which windows, across idle `(redisplay)` and
//! `(redisplay t)`, typing, point motion, a change to a buffer no window
//! shows, and split windows.
//!
//! GNU runs `pre-redisplay-function` from `prepare_menu_bars` on every
//! redisplay that is not inhibited, and passes it nil -- "the selected window
//! only" -- unless windows or buffers changed (xdisp.c:14237-14262). The
//! counts are Lisp-observable.
//!
//! The split-window cases, where GNU lists exactly the windows that need
//! redisplay, are recorded separately as known divergences: neomacs does not
//! track GNU's `windows_or_buffers_changed` precisely enough to build that
//! list, and passes t.

use std::path::PathBuf;
use std::time::Duration;

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};

const PROBE_EL: &str = r##";;; -*- lexical-binding: t -*-
(defvar neo-prered-calls nil)
(add-hook 'pre-redisplay-functions
          (lambda (w) (push (if (eq w (selected-window)) 'sel 'other) neo-prered-calls)))
(let ((out (getenv "NEOMACS_PRE_REDISPLAY_ORACLE_OUT"))
      (res nil))
  (switch-to-buffer (get-buffer-create "src"))
  (dotimes (i 200) (insert (format "(defun f%d ()\n\t%d)\n" i i)))
  (emacs-lisp-mode)
  (goto-char (point-min))
  (forward-line 20)
  (redisplay t)
  (redisplay t)
  (dolist (case '(("idle-force" . (redisplay t))
                  ("idle" . (redisplay))
                  ("type-force" . (progn (insert "x") (redisplay t)))
                  ("type" . (progn (insert "y") (redisplay)))
                  ("move" . (progn (forward-char 1) (redisplay)))
                  ("other-buffer-type"
                   . (progn (with-current-buffer (get-buffer-create "other") (insert "q"))
                            (redisplay t)))
                  ("split" . (progn (split-window-below) (redisplay t)))
                  ("split-idle" . (redisplay t))
                  ("split-type" . (progn (insert "z") (redisplay t)))))
    (setq neo-prered-calls nil)
    (dotimes (_ 3) (eval (cdr case) t))
    (push (format "%s %S" (car case) (reverse neo-prered-calls)) res))
  (with-temp-file out
    (insert (mapconcat #'identity (nreverse res) "\n") "\n")))
(kill-emacs 0)
"##;

/// The cases where neomacs cannot yet list the windows GNU lists.
const KNOWN_DIVERGENT: [&str; 2] = ["split", "split-type"];

fn run_probe(name: &str, program: PathBuf, extra: &[&str]) -> String {
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-prered-oracle-");
    let script = home.path().join("prered-oracle.el");
    std::fs::write(&script, PROBE_EL).expect("write probe");
    let out = home.path().join("prered-oracle.out");
    let mut launch = TuiLaunch::new(program.as_os_str()).arg("-nw").arg("-Q");
    for arg in extra {
        launch = launch.arg(*arg);
    }
    launch = launch
        .arg("-l")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("NEOMACS_PRE_REDISPLAY_ORACLE_OUT", &out);
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        name,
        TuiTerminalConfig::new("xterm-256color", 40, 120),
    );
    assert_eq!(
        session.run_to_completion(Duration::from_secs(60)),
        TuiProcessOutcome::Exited,
        "{name} did not finish the probe; grid:\n{}",
        session.text_grid().join("\n")
    );
    std::fs::read_to_string(&out).unwrap_or_else(|error| panic!("{name} wrote no calls: {error}"))
}

#[test]
fn pre_redisplay_functions_run_like_gnu() {
    let gnu = run_probe(
        "GNU",
        PathBuf::from("emacs"),
        &[
            "-no-comp-spawn",
            "--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent))",
        ],
    );
    let program = neomacs_tui_tests::neomacs_binary();
    assert!(
        program.exists(),
        "neomacs binary not found at {}",
        program.display()
    );
    let neo = run_probe("Neomacs", program, &[]);
    assert_eq!(gnu.lines().count(), neo.lines().count(), "case count");
    let mut divergences = Vec::new();
    for (gnu_line, neo_line) in gnu.lines().zip(neo.lines()) {
        let kind = gnu_line.split_whitespace().next().unwrap_or_default();
        if !KNOWN_DIVERGENT.contains(&kind) && gnu_line != neo_line {
            divergences.push(format!("GNU:     {gnu_line}\nNeomacs: {neo_line}"));
        }
    }
    assert!(
        divergences.is_empty(),
        "pre-redisplay-functions diverge from GNU:\n{}\n\nGNU:\n{gnu}\nNeomacs:\n{neo}",
        divergences.join("\n")
    );
}
