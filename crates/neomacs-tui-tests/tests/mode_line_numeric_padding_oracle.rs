#![cfg(unix)]
//! Direct redisplay oracle for numeric pads inside inherited min-width runs.
//! Expected rows are collected from live GNU; format-mode-line's separate
//! string-target min-width materialization is deliberately outside this seam.

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};
use std::path::PathBuf;
use std::time::Duration;

const PROBE_EL: &str = r##";;; -*- lexical-binding: t -*-
(let ((ready (getenv "NEOMACS_NUMERIC_PADDING_READY"))
      (release (getenv "NEOMACS_NUMERIC_PADDING_RELEASE"))
      (kind (intern (getenv "NEOMACS_NUMERIC_PADDING_CASE"))))
  (switch-to-buffer (get-buffer-create "d5-numeric-padding"))
  (erase-buffer)
  (insert "D5-NUMERIC-PADDING-BODY\n")
  (goto-char (point-min))
  (setq header-line-format nil tab-line-format nil)
  (setq mode-line-format
        (pcase kind
          ('prefix
           '("P[" (:propertize ((5 "") "A")
                               display (min-width (9.0)) help-echo "outer")
             (:propertize "B" display (min-width (2.0)) help-echo "next") "]"))
          ('only
           '("P[" (:propertize (5 "")
                               display (min-width (9.0)) help-echo "outer")
             (:propertize "B" display (min-width (2.0)) help-echo "next") "]"))
          ('gaps-precision
           '("P[" (:propertize (-7 ("A" (3 "") "B" (4 "")))
                               display (min-width (9.0)) help-echo "outer")
             (:propertize "C" display (min-width (2.0)) help-echo "next") "]"))
          (_ (error "Unknown numeric-padding case: %S" kind))))
  (force-mode-line-update)
  (redisplay t)
  (let ((inhibit-redisplay t))
    (with-temp-file ready (insert "ready\n"))
    (while (not (file-exists-p release))
      (accept-process-output nil 0.01))))
(kill-emacs 0)
"##;

fn run_probe(name: &str, program: PathBuf, kind: &str, gnu: bool) -> String {
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-numeric-padding-");
    let script = home.path().join("numeric-padding.el");
    let ready = home.path().join("numeric-padding.ready");
    let release = home.path().join("numeric-padding.release");
    std::fs::write(&script, PROBE_EL).expect("write numeric-padding probe");
    let mut launch = TuiLaunch::new(program.as_os_str()).arg("-nw").arg("-Q");
    if gnu {
        launch = launch
            .arg("-no-comp-spawn")
            .arg("--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent))");
    }
    launch = launch
        .arg("-l")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("NEOMACS_NUMERIC_PADDING_READY", &ready)
        .env("NEOMACS_NUMERIC_PADDING_RELEASE", &release)
        .env("NEOMACS_NUMERIC_PADDING_CASE", kind)
        .env("NEOMACS_MODE_LINE_NUMERIC_PADDING", "on")
        .env("NEOMACS_MODE_LINE_GATE", "gnu");
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        name,
        TuiTerminalConfig::new("xterm-256color", 40, 120),
    );
    session.read_until(Duration::from_secs(60), |_| ready.exists());
    assert!(
        ready.exists(),
        "{name} numeric-padding {kind} not ready; grid:\n{}",
        session.text_grid().join("\n")
    );
    session.read(Duration::from_millis(100));
    let row = session
        .text_grid()
        .get(38)
        .expect("40-row terminal")
        .clone();
    std::fs::write(&release, "release\n").expect("release owned editor");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{name} numeric-padding {kind} did not exit"
    );
    row
}

fn compare_case(kind: &str) {
    let gnu = run_probe("GNU", PathBuf::from("emacs"), kind, true);
    let program = neomacs_tui_tests::neomacs_binary();
    assert!(
        program.exists(),
        "Neomacs binary missing: {}",
        program.display()
    );
    let neo = run_probe("Neomacs", program, kind, false);
    assert_eq!(
        neo, gnu,
        "numeric-padding {kind}; expected row comes from live GNU"
    );
}

#[test]
fn numeric_padding_prefix_does_not_start_outer_min_width_run_like_gnu() {
    compare_case("prefix");
}

#[test]
fn numeric_padding_only_child_has_no_outer_min_width_run_like_gnu() {
    compare_case("only");
}

#[test]
fn numeric_padding_gaps_precision_and_width_identity_match_gnu() {
    compare_case("gaps-precision");
}
