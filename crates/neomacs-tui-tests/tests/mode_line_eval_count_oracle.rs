#![cfg(unix)]
//! GNU oracle for WHEN redisplay evaluates the mode line (P3.5 C9/E1).
//!
//! A mode line with a counting `(:eval ...)` is driven through the redisplay
//! shapes an editor produces -- idle `(redisplay t)` and `(redisplay)`, point
//! motion within and across lines, and typing cycles (self-insert, redisplay,
//! delete, redisplay) at the end of the buffer and mid-window, with font-lock
//! on and off -- and the per-case counts are compared with GNU's, split by
//! which redisplay of a typing cycle ran the form. The counts are
//! Lisp-observable (`:eval` forms have side effects), so they must equal
//! GNU's exactly.
//!
//! Both editors run the same probe on a 40x120 pty and exit; batch mode has
//! no redisplay and cannot answer this.
//!
//! The comparison runs under `NEOMACS_MODE_LINE_GATE=gnu`. The default
//! (`legacy`) gate is pinned separately with the divergences it is known to
//! have, so a change to either shows up here.

use std::path::PathBuf;
use std::time::Duration;

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};

const PROBE_EL: &str = r##";;; -*- lexical-binding: t -*-
(defvar neo-ml-count 0)
(let ((out (getenv "NEOMACS_MODE_LINE_ORACLE_OUT"))
      (n 10)
      (res nil))
  (switch-to-buffer (get-buffer-create "oracle-src"))
  (dotimes (i 200)
    (insert (format "(defun f%d (a b)\n\t(let ((x a))\n\t  (+ x b)))\n" i)))
  (emacs-lisp-mode)
  (font-lock-ensure (point-min) (point-max))
  (setq mode-line-format
        (append (default-value 'mode-line-format)
                '((:eval (progn (setq neo-ml-count (1+ neo-ml-count)) "")))))
  (dolist (kind '(noop noforce cursor vmove type typemid typenofl))
    (if (eq kind 'typemid)
        (progn (goto-char (point-max)) (forward-line -20) (end-of-line))
      (goto-char (point-max)))
    (when (eq kind 'typenofl)
      (font-lock-mode -1))
    (redisplay t)
    (redisplay t)
    (let ((a 0) (b 0) (flip nil))
      (dotimes (_ n)
        (setq neo-ml-count 0)
        (pcase kind
          ('noop (redisplay t))
          ('noforce (redisplay))
          ('cursor (if flip (forward-char 1) (backward-char 1)) (redisplay t))
          ('vmove (if flip (forward-line 1) (forward-line -1)) (redisplay t))
          (_
           (let ((last-command-event ?x))
             (call-interactively #'self-insert-command))
           (redisplay t)
           (setq a (+ a neo-ml-count) neo-ml-count 0)
           (delete-char -1)
           (redisplay t)))
        (setq b (+ b neo-ml-count))
        (setq flip (not flip)))
      (push (if (memq kind '(type typemid typenofl))
                (format "%s ins=%d del=%d" kind a b)
              (format "%s %d" kind b))
            res)))
  (with-temp-file out
    (insert (mapconcat #'identity (nreverse res) "\n") "\n")))
(kill-emacs 0)
"##;

/// Run the probe in EDITOR on a 40x120 pty and return what it recorded.
fn run_probe(name: &str, program: PathBuf, extra: &[&str], env: &[(&str, &str)]) -> String {
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-ml-oracle-");
    let script = home.path().join("ml-oracle.el");
    std::fs::write(&script, PROBE_EL).expect("write probe");
    let out = home.path().join("ml-oracle.out");
    let mut launch = TuiLaunch::new(program.as_os_str()).arg("-nw").arg("-Q");
    for arg in extra {
        launch = launch.arg(*arg);
    }
    launch = launch
        .arg("-l")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("NEOMACS_MODE_LINE_ORACLE_OUT", &out);
    for (name, value) in env {
        launch = launch.env(*name, *value);
    }
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
    std::fs::read_to_string(&out).unwrap_or_else(|error| panic!("{name} wrote no counts: {error}"))
}

fn gnu_counts() -> String {
    run_probe(
        "GNU",
        PathBuf::from("emacs"),
        &[
            "-no-comp-spawn",
            "--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent))",
        ],
        &[],
    )
}

fn neomacs_counts(gate: &str) -> String {
    let program = neomacs_tui_tests::neomacs_binary();
    assert!(
        program.exists(),
        "neomacs binary not found at {}",
        program.display()
    );
    run_probe("Neomacs", program, &[], &[("NEOMACS_MODE_LINE_GATE", gate)])
}

#[test]
fn mode_line_eval_counts_match_gnu_under_the_gnu_gate() {
    let gnu = gnu_counts();
    let neo = neomacs_counts("gnu");
    assert_eq!(
        neo, gnu,
        "mode-line :eval counts diverge from GNU\nGNU:\n{gnu}\nNeomacs:\n{neo}"
    );
}

/// The default (`legacy`) gate's divergences, recorded so that a change is
/// visible: at the end of the buffer GNU keeps the mode line on the delete
/// (and with font-lock off on both halves of the cycle); the legacy gate
/// evaluates.
#[test]
fn mode_line_eval_counts_under_the_legacy_gate_diverge_only_where_recorded() {
    let gnu = gnu_counts();
    let neo = neomacs_counts("legacy");
    let known = ["type", "typenofl"];
    let mut unexpected = Vec::new();
    let mut expected_seen = 0;
    for (gnu_line, neo_line) in gnu.lines().zip(neo.lines()) {
        let kind = gnu_line.split_whitespace().next().unwrap_or_default();
        if known.contains(&kind) {
            if gnu_line != neo_line {
                expected_seen += 1;
            }
        } else if gnu_line != neo_line {
            unexpected.push(format!("GNU:     {gnu_line}\nNeomacs: {neo_line}"));
        }
    }
    assert_eq!(gnu.lines().count(), neo.lines().count(), "case count");
    assert!(
        unexpected.is_empty(),
        "new legacy-gate divergences:\n{}",
        unexpected.join("\n")
    );
    assert_eq!(
        expected_seen,
        known.len(),
        "a recorded legacy divergence disappeared -- update this pin\nGNU:\n{gnu}\nNeomacs:\n{neo}"
    );
}
