#![cfg(unix)]
//! Fresh live GNU31.1 full posns for the explicit window-buffer adjustment.
//! Each TTY child owns its own evaluator. The test does not share Lisp state.
use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};
use std::path::PathBuf;
use std::time::Duration;

const PROBE_EL: &str = r##";;; -*- lexical-binding: t -*-
(defvar d5-clear-window nil)
(defun d5-clear-normalize (posn)
  ;; Exactly Task3: retain all public source/coordinate/object cells, omitting
  ;; only the process-specific window identity and timestamp.
  (when posn (append (list 'window (nth 1 posn) (nth 2 posn)) (nthcdr 4 posn))))
(defun d5-clear-query (i)
  (let ((noninteractive t))
    (d5-clear-normalize (posn-at-x-y (+ 2 (% i 8)) (% i 4) d5-clear-window))))
(defun d5-clear-main ()
  (condition-case failure
      (progn
        (menu-bar-mode -1)
        (set-frame-parameter nil 'menu-bar-lines 0)
        (delete-other-windows)
        (switch-to-buffer (get-buffer-create "task3-text"))
        (erase-buffer) (fundamental-mode)
        (setq buffer-undo-list t truncate-lines nil mode-line-format nil
              header-line-format nil tab-line-format nil)
        (insert (apply #'concat (make-list (/ 2097152 81) (concat (make-string 80 ?a) "\n"))))
        (goto-char 10) (set-window-start (selected-window) 1)
        (redisplay t)
        (setq d5-clear-window (split-window (selected-window) 18 'below))
        (if (equal (getenv "NEOMACS_MATRIX_CLEAR_CASE") "keep")
            (set-window-buffer d5-clear-window (current-buffer) t)
          (set-window-buffer d5-clear-window (current-buffer)))
        (set-window-start d5-clear-window 1)
        (set-window-point d5-clear-window 10)
        (let ((samples (mapcar #'d5-clear-query '(0 1 2 3 4 5 6 7))))
          (with-temp-file (getenv "NEOMACS_MATRIX_CLEAR_OUT")
            (prin1 samples (current-buffer)) (insert "\n")))
        (write-region "done\n" nil (getenv "NEOMACS_MATRIX_CLEAR_DONE") nil 'silent)
        (kill-emacs 0))
    (error
      (with-temp-file (getenv "NEOMACS_MATRIX_CLEAR_OUT")
        (prin1 (list 'error failure) (current-buffer)))
      (kill-emacs 1))))
(setq inhibit-startup-screen t initial-scratch-message nil
      native-comp-jit-compilation nil native-comp-async-report-warnings-errors 'silent
      redisplay-dont-pause t)
(when (fboundp 'menu-bar-mode) (menu-bar-mode -1))
(when (fboundp 'tool-bar-mode) (tool-bar-mode -1))
(run-with-timer 0.1 nil #'d5-clear-main)
"##;

fn capture(program: PathBuf, gnu: bool, case: &str) -> String {
    let home = neomacs_tui_tests::TuiTempDirectory::new("neomacs-current-matrix-clear-");
    let script = home.path().join("probe.el");
    let out = home.path().join("posns.el");
    let done = home.path().join("complete");
    std::fs::write(&script, PROBE_EL).expect("write real TTY probe");
    let mut launch = TuiLaunch::new(program.as_os_str()).arg("-nw").arg("-Q");
    if gnu {
        launch = launch.arg("-no-comp-spawn")
            .arg("--eval=(progn(set'native-comp-jit-compilation())(set'native-comp-async-report-warnings-errors'silent))");
    }
    launch = launch
        .arg("-l")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("NEOMACS_MATRIX_CLEAR_CASE", case)
        .env("NEOMACS_MATRIX_CLEAR_OUT", &out)
        .env("NEOMACS_MATRIX_CLEAR_DONE", &done)
        .env("NEOMACS_POSN_OBJECT_EXTENT", "on")
        .env("NEOMACS_POSN_BOUNDED_TEXT", "off")
        .env("NEOMACS_MODE_LINE_ACCUM_ROOTS", "on")
        .env("NEOMACS_REDISPLAY_GNU_HOOKS", "on")
        .env("NEOMACS_MODE_LINE_NUMERIC_PADDING", "on");
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        if gnu { "GNU" } else { "Neomacs" },
        TuiTerminalConfig::new("xterm-256color", 40, 120),
    );
    assert_eq!(
        session.run_to_completion(Duration::from_secs(90)),
        TuiProcessOutcome::Exited,
        "{case}: child failed; grid:\n{}",
        session.text_grid().join("\n")
    );
    let contents = std::fs::read_to_string(out).expect("complete normalized ten-field capture");
    let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tmp/posn-current-matrix-clear-captures");
    std::fs::create_dir_all(&artifacts).expect("capture directory");
    std::fs::write(
        artifacts.join(format!(
            "{case}-{}-full.el",
            if gnu { "gnu" } else { "neo" }
        )),
        &contents,
    )
    .expect("retain strict GNU evidence before comparison");
    // Exited alone does not encode a successful exit. Never accept identical
    // fixture errors as oracle parity: require eight real full captures and
    // the success marker that only the completed probe writes.
    assert_eq!(
        std::fs::read_to_string(done).ok().as_deref(),
        Some("done\n"),
        "{case}: fixture did not complete successfully; retained capture: {contents}"
    );
    assert!(
        !contents.trim_start().starts_with("(error"),
        "fixture error: {contents}"
    );
    assert_eq!(
        contents.matches("(window ").count(),
        8,
        "{case}: expected every full public Task3 posn capture: {contents}"
    );
    contents
}

fn compare(case: &str) {
    let gnu = capture(PathBuf::from("emacs"), true, case);
    let neo = capture(neomacs_tui_tests::neomacs_binary(), false, case);
    assert_eq!(
        neo, gnu,
        "{case}: fresh GNU full posns; no expected tuple projection"
    );
}

#[test]
fn tty_default_same_buffer_setter_clears_current_matrix_like_gnu() {
    compare("clear");
}
#[test]
fn tty_keep_margins_same_buffer_setter_preserves_current_matrix_like_gnu() {
    compare("keep");
}
