#![cfg(unix)]
//! Paired GNU/Neomacs row for #625: a C-g that runs `abort-minibuffers`
//! inside a Lisp-called `read-from-minibuffer` must
//!
//!   * signal `minibuffer-quit` (condition data list `(minibuffer-quit)`) to
//!     the caller's `condition-case` quit handler,
//!   * run every `unwind-protect` between the read and that handler, and
//!   * leave the caller's cleanup in control (so consult's process/timer
//!     cleanup would run).
//!
//! The read is exercised from a command and from a `-l` startup load, plain
//! and with the consult-shaped wrappers: `minibuffer-with-setup-hook`, a
//! running timer, and a live child process.

use crate::support;

use neomacs_tui_tests::TuiSession;
use std::ffi::OsStr;
use std::fs;
use std::time::Duration;
use support::{
    boot_pair, eval_expression_enters_minibuffer, send_both, write_home_file,
    write_shared_temp_file,
};

const ORACLE_FILE: &str = "i625-oracle.el";
const RESULT_FILE: &str = "i625-result.tsv";

const SCENARIOS: [(&str, &str); 4] = [
    ("plain", "i625-S1: "),
    ("setup", "i625-S2: "),
    ("timer", "i625-S3: "),
    ("process", "i625-S4: "),
];

const ORACLE_ELISP: &str = r#";;; -*- lexical-binding: t; -*-

(defvar i625-record-file
  (expand-file-name "i625-result.tsv" (getenv "HOME")))

(defun i625-record (scenario cleaned caught)
  (append-to-file
   (format "%s\t%s\t%s\n" scenario (if cleaned "t" "nil")
           (if (eq caught 'normal) "normal" (format "%S" caught)))
   nil i625-record-file))

(defun i625-scenario (name thunk)
  (let (cleaned caught)
    (unwind-protect
        (condition-case err
            (progn (funcall thunk) (setq caught 'normal))
          (quit (setq caught err)))
      (setq cleaned t))
    (i625-record name cleaned caught)))

(defun i625-scenario-plain ()
  (read-from-minibuffer "i625-S1: "))

(defun i625-scenario-setup ()
  (minibuffer-with-setup-hook (lambda () nil)
    (read-from-minibuffer "i625-S2: ")))

(defun i625-scenario-timer ()
  (let ((timer (run-with-timer 0.05 0.05 (lambda () (+ 1 1)))))
    (unwind-protect
        (read-from-minibuffer "i625-S3: ")
      (cancel-timer timer))))

(defun i625-scenario-process ()
  (let* ((buf (generate-new-buffer " *i625-proc*"))
         (proc (make-process :name "i625-sleep" :buffer buf
                             :command '("sh" "-c" "sleep 30")
                             :filter (lambda (_p _s) nil))))
    (unwind-protect
        (read-from-minibuffer "i625-S4: ")
      (delete-process proc))))

(defun i625-run-all ()
  (interactive)
  (ignore-errors (delete-file i625-record-file))
  (i625-scenario "plain" #'i625-scenario-plain)
  (i625-scenario "setup" #'i625-scenario-setup)
  (i625-scenario "timer" #'i625-scenario-timer)
  (i625-scenario "process" #'i625-scenario-process)
  (i625-record "DONE" t 'normal))

(if (getenv "I625_AUTOSTART")
    (i625-run-all))
"#;

const EXPECTED: &str = "plain\tt\t(minibuffer-quit)
setup\tt\t(minibuffer-quit)
timer\tt\t(minibuffer-quit)
process\tt\t(minibuffer-quit)
DONE\tt\tnormal
";

fn expect_prompt(session: &mut TuiSession, prompt: &str, role: &str) {
    session.read_until(Duration::from_secs(15), |grid| {
        grid.iter().any(|row| row.contains(prompt))
    });
    let seen = session.text_grid().iter().any(|row| row.contains(prompt));
    assert!(
        seen,
        "{role} did not reach the {prompt:?} prompt; grid:\n{}\nrecent PTY output:\n{}",
        session.text_grid().join("\n"),
        String::from_utf8_lossy(session.recent_output())
    );
}

fn read_result(session: &TuiSession) -> String {
    fs::read_to_string(session.home_dir().join(RESULT_FILE)).unwrap_or_default()
}

fn wait_for_results(gnu: &mut TuiSession, neo: &mut TuiSession) -> (String, String) {
    let deadline = neomacs_tui_tests::HangCap::new(Duration::from_secs(30));
    loop {
        gnu.read(Duration::from_millis(100));
        neo.read(Duration::from_millis(100));
        let g = read_result(gnu);
        let n = read_result(neo);
        if g.lines().any(|line| line.starts_with("DONE\t"))
            && n.lines().any(|line| line.starts_with("DONE\t"))
        {
            return (g, n);
        }
        if deadline.is_expired() {
            panic!(
                "timed out waiting for both result files.\nGNU result:\n{g}\n\
                 Neo result:\n{n}\nGNU grid:\n{}\nNeo grid:\n{}\n\
                 GNU PTY tail:\n{}\nNeo PTY tail:\n{}",
                gnu.text_grid().join("\n"),
                neo.text_grid().join("\n"),
                String::from_utf8_lossy(gnu.recent_output()),
                String::from_utf8_lossy(neo.recent_output())
            );
        }
    }
}

fn assert_results(gnu_result: &str, neo_result: &str, where_: &str) {
    assert_eq!(
        neo_result, gnu_result,
        "{where_}: Neomacs and GNU cleanup results differ"
    );
    assert_eq!(
        gnu_result, EXPECTED,
        "{where_}: GNU oracle did not record the expected minibuffer-quit contract"
    );
}

fn drive_scenarios(gnu: &mut TuiSession, neo: &mut TuiSession) {
    for (name, prompt) in SCENARIOS {
        expect_prompt(gnu, prompt, "GNU");
        expect_prompt(neo, prompt, "Neomacs");
        send_both(gnu, neo, "C-g");
        eprintln!("sent C-g for scenario {name}");
    }
}

#[test]
fn minibuffer_quit_cleanup_from_command_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    write_home_file(&gnu, ORACLE_FILE, ORACLE_ELISP);
    write_home_file(&neo, ORACLE_FILE, ORACLE_ELISP);

    eval_expression_enters_minibuffer(
        &mut gnu,
        &mut neo,
        r#"(progn (load "~/i625-oracle.el" nil t) (i625-run-all))"#,
    );

    drive_scenarios(&mut gnu, &mut neo);
    let (g, n) = wait_for_results(&mut gnu, &mut neo);
    assert_results(&g, &n, "command-invoked read-from-minibuffer");
}

#[test]
fn minibuffer_quit_cleanup_from_startup_load_matches_gnu() {
    let oracle = write_shared_temp_file(ORACLE_FILE, ORACLE_ELISP);
    let args = format!("-l {}", oracle.path().display());
    let env = [("I625_AUTOSTART", OsStr::new("1"))];
    let mut gnu = TuiSession::gnu_emacs_with_env(&args, &env);
    let mut neo = TuiSession::neomacs_with_env(&args, &env);

    drive_scenarios(&mut gnu, &mut neo);
    let (g, n) = wait_for_results(&mut gnu, &mut neo);
    assert_results(&g, &n, "startup -l read-from-minibuffer");
}
