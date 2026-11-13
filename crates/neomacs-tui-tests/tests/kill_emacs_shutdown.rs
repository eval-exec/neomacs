//! GNU emacs.c:3017-3021 dispatches interactive kill hooks through the Lisp
//! symbol, preserving overrides and subr.el's query/continuation behavior.
#![cfg(unix)]

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};
use std::{path::Path, time::Duration};

const PROBE: &str = r#";;; -*- lexical-binding: nil; -*-
(defvar shutdown-query-events nil)
(defun shutdown-query-record (event)
  (setq shutdown-query-events (append shutdown-query-events (list event))))
(defvar shutdown-query-original (symbol-function 'run-hook-query-error-with-timeout))
(fset 'run-hook-query-error-with-timeout
      (lambda (hook)
        (shutdown-query-record (list 'symbol-dispatch hook noninteractive))
        (funcall shutdown-query-original hook)))
(fset 'y-or-n-p-with-timeout
      (lambda (_prompt seconds default)
        (shutdown-query-record (list 'query seconds default))
        t))
(defun shutdown-query-bad ()
  (shutdown-query-record 'bad)
  (error "shutdown query control"))
(defun shutdown-query-next ()
  (shutdown-query-record 'next)
  (shutdown-query-record (list 'broken-hook-retained
                              (and (memq 'shutdown-query-bad kill-emacs-hook) t)))
  (with-temp-file (getenv "SHUTDOWN_QUERY_OUT")
    (prin1 shutdown-query-events (current-buffer))))
(setq kill-emacs-hook '(shutdown-query-bad shutdown-query-next))
(with-temp-file (concat (getenv "SHUTDOWN_QUERY_OUT") ".ready")
  (prin1 noninteractive (current-buffer)))
(when (getenv "SHUTDOWN_QUERY_UNINTERN")
  (unintern "run-hook-query-error-with-timeout" obarray)
  (fset (intern "run-hook-query-error-with-timeout")
        (lambda (_hook) (shutdown-query-record 'replacement-dispatch))))
(kill-emacs 0)
"#;

fn run(program: &Path, name: &str, unintern: bool, probe: &str) -> String {
    let home = neomacs_tui_tests::TuiTempDirectory::new("kill-emacs-query-");
    let script = home.path().join("probe.el");
    let output = home.path().join("events");
    std::fs::write(&script, probe).unwrap();
    let mut launch = TuiLaunch::new(program.as_os_str())
        .arg("-nw")
        .arg("-Q")
        .arg("-l")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("SHUTDOWN_QUERY_OUT", &output);
    launch = if unintern {
        launch.env("SHUTDOWN_QUERY_UNINTERN", "yes")
    } else {
        launch.env_remove("SHUTDOWN_QUERY_UNINTERN")
    };
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        name,
        TuiTerminalConfig::new("xterm-256color", 24, 80),
    );
    assert_eq!(
        session.run_to_completion(Duration::from_secs(45)),
        TuiProcessOutcome::Exited,
        "{name} did not exit:\n{}",
        session.text_grid().join("\n")
    );
    assert_eq!(
        std::fs::read_to_string(output.with_extension("ready")).unwrap(),
        "nil",
        "{name} interactive setup checkpoint"
    );
    std::fs::read_to_string(output).unwrap_or_else(|error| panic!("{name} wrote no trace: {error}"))
}

fn assert_query(unintern: bool) {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let expected =
        "((symbol-dispatch kill-emacs-hook nil) bad (query 5 t) next (broken-hook-retained t))";
    let reference = run(&gnu, "GNU shutdown query", unintern, PROBE);
    assert_eq!(reference, expected, "GNU interactive shutdown contract");
    let neo = run(
        &neomacs_tui_tests::neomacs_binary(),
        "Neo shutdown query",
        unintern,
        PROBE,
    );
    eprintln!("GNU: {reference}\nNeo: {neo}");
    assert_eq!(neo, reference, "interactive shutdown query differs");
}

#[test]
fn interactive_kill_emacs_calls_the_query_symbol_and_continues_after_yes() {
    assert_query(false);
}

#[test]
fn interactive_kill_emacs_keeps_the_original_query_symbol_after_unintern() {
    assert_query(true);
}

const ABORT_PROBE: &str = r#";;; -*- lexical-binding: nil; -*-
(defvar shutdown-query-events nil)
(defvar shutdown-query-armed t)
(defun shutdown-query-record (event)
  (setq shutdown-query-events (append shutdown-query-events (list event))))
(defun shutdown-query-flush ()
  (with-temp-file (getenv "SHUTDOWN_QUERY_OUT")
    (prin1 shutdown-query-events (current-buffer))))
(defun shutdown-query-first ()
  (shutdown-query-record 'UNEXPECTED-first-hook)
  (shutdown-query-flush))
(defun shutdown-query-second ()
  (shutdown-query-record 'second-hook)
  (shutdown-query-flush))
(fset 'run-hook-query-error-with-timeout
      (lambda (hook)
        (shutdown-query-record (list 'query hook))
        (if shutdown-query-armed
            (progn (setq shutdown-query-armed nil)
                   (throw 'shutdown-query-abort 'escaped))
          (run-hooks hook))))
(setq kill-emacs-hook '(shutdown-query-first))
(with-temp-file (concat (getenv "SHUTDOWN_QUERY_OUT") ".ready")
  (prin1 noninteractive (current-buffer)))
(shutdown-query-record (list 'result (catch 'shutdown-query-abort (kill-emacs 3))))
(shutdown-query-record 'still-alive)
(setq kill-emacs-hook '(shutdown-query-second))
(kill-emacs 0)
"#;

#[test]
fn interactive_query_nonlocal_exit_aborts_shutdown_and_allows_a_second_kill() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let expected = "((query kill-emacs-hook) (result escaped) still-alive (query kill-emacs-hook) second-hook)";
    let reference = run(&gnu, "GNU query abort", false, ABORT_PROBE);
    assert_eq!(reference, expected, "GNU query escape contract");
    let neo = run(
        &neomacs_tui_tests::neomacs_binary(),
        "Neo query abort",
        false,
        ABORT_PROBE,
    );
    eprintln!("GNU: {reference}\nNeo: {neo}");
    assert_eq!(neo, reference, "interactive query escape differs");
}
