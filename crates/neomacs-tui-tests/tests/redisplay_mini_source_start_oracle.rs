#![cfg(unix)]
//! Real minibuffer reader, canonical full display producer, and live GNU oracle.
//! Each process owns its Lisp state; no shared fixture cache or env mutation.
//! Height/start/point/scroll are compared to live GNU. Full tuples are retained;
//! main's unsigned WindowEndRecord clamp is invariant in same-binary OFF/ON.
use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};
use std::path::PathBuf;
use std::time::Duration;

const PROBE_EL: &str = r##";;; -*- lexical-binding: t -*-
(defvar d5-mini-start-scrolls nil)
(defun d5-mini-start-main ()
 (condition-case data
  (progn
   (delete-other-windows)
   (switch-to-buffer (get-buffer-create "mini-start-body"))
   (setq-local mode-line-format nil header-line-format nil tab-line-format nil)
   (setq window-scroll-functions nil window-buffer-change-functions nil
         window-size-change-functions nil window-selection-change-functions nil
         window-state-change-functions nil window-state-change-hook nil
         window-configuration-change-hook nil pre-redisplay-function nil)
   (redisplay t) (redisplay t)
   (let ((scenario (getenv "D5_MINI_START_CASE")))
    (minibuffer-with-setup-hook
     (lambda ()
      (setq-local resize-mini-windows t max-mini-window-height 3
                  truncate-lines nil mode-line-format nil header-line-format nil
                  tab-line-format nil bidi-paragraph-direction 'left-to-right)
      (cond ((equal scenario "wrapped") (insert (make-string 260 ?a)))
            ((equal scenario "final-newline") (insert "input\n")))
      (redisplay t) (redisplay t)
      (setq d5-mini-start-scrolls nil)
      (setq-local window-scroll-functions
       (list (lambda (_window start)
              (push start d5-mini-start-scrolls))))
      (if (equal scenario "plain")
       (insert "first\nsecond\nthird\nfourth\nfifth")
       (let ((overlay (make-overlay (point-max) (point-max))))
        (overlay-put overlay 'after-string "\nfirst\nsecond\nthird\nfourth\nfifth")))
      (redisplay t)
      (let* ((window (minibuffer-window))
             (full (list (window-total-height window) (window-start window)
                         (window-point window) (window-end window t)
                         (nreverse d5-mini-start-scrolls))))
       (with-temp-file (getenv "D5_MINI_START_OUT")
        (prin1 full (current-buffer)))
       (with-temp-file (getenv "NEOMACS_MINI_START_ORACLE_CONTRACT")
        (prin1 (list (nth 0 full) (nth 1 full) (nth 2 full) (nth 4 full))
               (current-buffer)))
       (with-temp-file (getenv "NEOMACS_MINI_START_ORACLE_RESIDUAL")
        (prin1 (nth 3 full) (current-buffer))))
      (run-with-timer 0 nil #'exit-minibuffer))
     (read-from-minibuffer "probe: ")))
   (kill-emacs 0))
  (error (with-temp-file (getenv "D5_MINI_START_OUT")
           (insert "ERROR: " (prin1-to-string data))) (kill-emacs 1))))
(run-with-timer 0.1 nil #'d5-mini-start-main)
"##;

struct ProbeCapture {
    full: String,
    contract: String,
    residual: String,
}

fn probe(program: PathBuf, case: &str, gnu: bool, setting: &str) -> ProbeCapture {
    let home = neomacs_tui_tests::TuiTempDirectory::new("d5-mini-source-start-");
    let script = home.path().join("source-start.el");
    let output = home.path().join("source-start.out");
    let contract = home.path().join("source-start.contract");
    let residual = home.path().join("source-start.residual");
    std::fs::write(&script, PROBE_EL).expect("write real reader probe");
    let mut launch = TuiLaunch::new(program.as_os_str()).arg("-nw").arg("-Q");
    if gnu {
        launch = launch.arg("-no-comp-spawn")
            .arg("--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent))");
    } else {
        launch = launch.env("NEOMACS_REDISPLAY_GNU_HOOKS", setting);
    }
    launch = launch
        .arg("--load")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("D5_MINI_START_CASE", case)
        .env("D5_MINI_START_OUT", &output)
        .env("NEOMACS_MINI_START_ORACLE_CONTRACT", &contract)
        .env("NEOMACS_MINI_START_ORACLE_RESIDUAL", &residual);
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        case,
        TuiTerminalConfig::new("xterm-256color", 40, 120),
    );
    assert_eq!(
        session.run_to_completion(Duration::from_secs(60)),
        TuiProcessOutcome::Exited,
        "{case} process did not finish, grid:\n{}",
        session.text_grid().join("\n")
    );
    let capture = ProbeCapture {
        full: std::fs::read_to_string(output).expect("full oracle trace"),
        contract: std::fs::read_to_string(contract).expect("owned mini source-start contract"),
        residual: std::fs::read_to_string(residual).expect("main window-end residual"),
    };
    assert!(
        !capture.full.starts_with("ERROR:"),
        "{case}: {}",
        capture.full
    );
    let artifacts =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tmp/mini-source-start-captures");
    std::fs::create_dir_all(&artifacts).expect("create TMP mini source-start capture directory");
    let editor = if gnu { "gnu" } else { "neo" };
    for (kind, contents) in [
        ("full", &capture.full),
        ("contract", &capture.contract),
        ("residual", &capture.residual),
    ] {
        std::fs::write(
            artifacts.join(format!("{case}-{editor}-{setting}-{kind}.el")),
            contents,
        )
        .expect("retain full, contract and residual TMP evidence");
    }
    capture
}

fn compare(case: &str) {
    let gnu = probe(PathBuf::from("emacs"), case, true, "on");
    let baseline = probe(neomacs_tui_tests::neomacs_binary(), case, false, "off");
    let neo = probe(neomacs_tui_tests::neomacs_binary(), case, false, "on");
    assert_eq!(
        neo.residual, baseline.residual,
        "{case}: GNU hooks changed main's unsigned/clamped window-end; ON full:{}\nOFF full:{}",
        neo.full, baseline.full
    );
    assert_eq!(
        neo.contract, gnu.contract,
        "{case}: live GNU mini height/start/point/scroll contract; GNU full:{}\nNeo full:{}\nOFF full:{}",
        gnu.full, neo.full, baseline.full
    );
}

#[test]
fn after_string_at_eob_keeps_prompt_line_start_like_gnu() {
    compare("prompt");
}
#[test]
fn after_string_at_wrapped_eob_uses_source_screen_line_start_like_gnu() {
    compare("wrapped");
}
#[test]
fn after_string_after_final_newline_uses_source_line_start_like_gnu() {
    compare("final-newline");
}
#[test]
fn ordinary_buffer_overflow_keeps_last_source_screenful_like_gnu() {
    compare("plain");
}
