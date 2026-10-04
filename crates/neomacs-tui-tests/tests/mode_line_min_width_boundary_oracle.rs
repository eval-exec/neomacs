#![cfg(unix)]
//! Min-width end-of-run decisions from live GNU display_string. In particular,
//! decoded percent fields do not handle display properties, and literal slices
//! at nonzero source positions only close a run with an EQ source predecessor.

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};
use std::path::PathBuf;
use std::time::Duration;

const PROBE_EL: &str = r##";;; -*- lexical-binding: t -*-
(let ((ready (getenv "NEOMACS_MIN_WIDTH_BOUNDARY_READY"))
      (release (getenv "NEOMACS_MIN_WIDTH_BOUNDARY_RELEASE"))
      (kind (intern (getenv "NEOMACS_MIN_WIDTH_BOUNDARY_CASE"))))
  (switch-to-buffer (get-buffer-create "d5-min-width-boundary"))
  (erase-buffer)
  (insert "D5-MIN-WIDTH-BOUNDARY-BODY\n")
  (goto-char (point-min))
  (setq header-line-format nil tab-line-format nil mode-name "MODE")
  (modify-frame-parameters nil '((name . "D5-F")))
  (setq buffer-file-coding-system 'utf-8-unix
        eol-mnemonic-unix "EOL")
  (setq mode-line-format
        (pcase kind
          ('mode-name '((:propertize "AB" display (min-width (10.0))) "%m" "|"))
          ('frame-name '((:propertize "AB" display (min-width (10.0))) "%F" "|"))
          ('eol-indicator '((:propertize "AB" display (min-width (10.0))) "%Z" "|"))
          ('other-source-literal
           '((:propertize "AB" display (min-width (10.0))) "%l rest" "|"))
          ((or 'same-source-predecessor 'other-identity-predecessor)
           (let* ((width (list 10.0))
                  (display (list 'min-width width))
                  (predecessor (if (eq kind 'same-source-predecessor)
                                   display
                                 (list 'min-width (list 10.0))))
                  (source (copy-sequence "%m rest")))
             (put-text-property 0 2 'display predecessor source)
             (list (list :propertize "AB" 'display display) source "|")))
          (_ (error "Unknown min-width boundary case: %S" kind))))
  (force-mode-line-update)
  (redisplay t)
  (let ((inhibit-redisplay t))
    (with-temp-file ready (insert "ready\n"))
    (while (not (file-exists-p release))
      (accept-process-output nil 0.01))))
(kill-emacs 0)
"##;

fn run_probe(name: &str, program: PathBuf, kind: &str, gnu: bool) -> String {
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-min-width-boundary-");
    let script = home.path().join("min-width-boundary.el");
    let ready = home.path().join("min-width-boundary.ready");
    let release = home.path().join("min-width-boundary.release");
    std::fs::write(&script, PROBE_EL).expect("write min-width boundary probe");
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
        .env("LC_ALL", "C.UTF-8")
        .env("NEOMACS_MIN_WIDTH_BOUNDARY_READY", &ready)
        .env("NEOMACS_MIN_WIDTH_BOUNDARY_RELEASE", &release)
        .env("NEOMACS_MIN_WIDTH_BOUNDARY_CASE", kind)
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
        "{name} min-width boundary {kind} not ready; grid:\n{}",
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
        "{name} min-width boundary {kind} did not exit"
    );
    if let Some(record_dir) = std::env::var_os("NEOMACS_TUI_RECORD_DIR") {
        let path = PathBuf::from(record_dir).join("min-width-boundary");
        std::fs::create_dir_all(&path).expect("create min-width capture directory");
        std::fs::write(path.join(format!("{kind}-{name}.row")), &row)
            .expect("archive live min-width boundary row");
    }
    row
}

fn compare_case(kind: &str) {
    let gnu_program = std::env::var_os("ORACLE_EMACS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("emacs"));
    let gnu = run_probe("GNU", gnu_program, kind, true);
    let program = neomacs_tui_tests::neomacs_binary();
    assert!(
        program.exists(),
        "Neomacs binary missing: {}",
        program.display()
    );
    let neo = run_probe("Neomacs", program, kind, false);
    assert_eq!(
        neo, gnu,
        "min-width boundary {kind}; expected row is live GNU"
    );
}

#[test]
fn min_width_mode_name_percent_keeps_run_open_like_gnu() {
    compare_case("mode-name");
}

#[test]
fn min_width_frame_name_percent_keeps_run_open_like_gnu() {
    compare_case("frame-name");
}

#[test]
fn min_width_eol_indicator_percent_keeps_run_open_like_gnu() {
    compare_case("eol-indicator");
}

#[test]
fn min_width_other_source_nonzero_literal_keeps_run_open_like_gnu() {
    compare_case("other-source-literal");
}

#[test]
fn min_width_same_source_predecessor_closes_run_like_gnu() {
    compare_case("same-source-predecessor");
}

#[test]
fn min_width_equal_nonidentical_predecessor_keeps_run_open_like_gnu() {
    compare_case("other-identity-predecessor");
}
