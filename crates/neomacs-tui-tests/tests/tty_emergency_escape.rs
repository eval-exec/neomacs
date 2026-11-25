//! GNU Emacs `handle_interrupt` (`src/keyboard.c:12618-12711`) controlling-TTY
//! 2nd-`C-g` emergency escape oracle test.
//!
//! When `quit-flag` is already non-nil (for example under `(let ((inhibit-quit t)) ...)`),
//! a 2nd `C-g` on an active controlling TTY resets terminal modes, suspends the
//! process group with `SIGTSTP`, and on `SIGCONT` prompts on the controlling TTY
//! for `Auto-save? (y or n) `, `Abort (and dump core)? (y or n) `, and
//! `Continuing...`.
//!
//! Each editor is launched inside a minimal POSIX job-control parent in the
//! PTY's session (`setpgid` + `tcsetpgrp`), matching an interactive shell so
//! the editor's process group is NOT orphaned (POSIX discards default `SIGTSTP`
//! in an orphaned process group).
#![cfg(unix)]

use neomacs_tui_tests::{TuiLaunch, TuiProcessOutcome, TuiSession, TuiTerminalConfig};
use std::path::Path;
use std::time::{Duration, Instant};

const JOB_CONTROL_HELPER: &str = r#"import os, resource, signal, sys, time
signal.signal(signal.SIGTTOU, signal.SIG_IGN)
signal.signal(signal.SIGTTIN, signal.SIG_IGN)
stopped_path, resume_path, status_path, disable_core = sys.argv[1:5]
cmd = sys.argv[5:]
pid = os.fork()
if pid == 0:
    os.setpgid(0, 0)
    signal.signal(signal.SIGTTOU, signal.SIG_DFL)
    signal.signal(signal.SIGTTIN, signal.SIG_DFL)
    if disable_core == "1":
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    os.execvp(cmd[0], cmd)
os.setpgid(pid, pid)
os.tcsetpgrp(0, pid)
while True:
    _, st = os.waitpid(pid, os.WUNTRACED)
    if os.WIFSTOPPED(st):
        sig = os.WSTOPSIG(st)
        os.tcsetpgrp(0, os.getpgrp())
        with open(stopped_path, "w") as f:
            f.write(str(sig))
        while not os.path.exists(resume_path):
            time.sleep(0.01)
        os.tcsetpgrp(0, pid)
        os.kill(-pid, signal.SIGCONT)
        continue
    if os.WIFSIGNALED(st):
        out = f"signal:{os.WTERMSIG(st)}"
    elif os.WIFEXITED(st):
        out = f"exit:{os.WEXITSTATUS(st)}"
    else:
        out = f"other:{st}"
    with open(status_path, "w") as f:
        f.write(out)
    sys.exit(0)
"#;

const CONTINUE_PROBE: &str = r#";;; -*- lexical-binding: nil; -*-
(let* ((dir (getenv "EE_DIR"))
       (target (expand-file-name "draft.txt" dir))
       (ready (expand-file-name "ready" dir))
       (done (expand-file-name "done" dir))
       (inside (expand-file-name "inside" dir))
       (after (expand-file-name "after" dir)))
  (find-file target)
  (auto-save-mode 1)
  (insert "unsaved emergency escape line\n")
  (let ((outcome
         (condition-case err
             (progn
               (let ((inhibit-quit t))
                 (with-temp-file ready
                   (insert "ready\n"))
                 (while (not (file-exists-p done))
                   (sleep-for 0.02))
                 (with-temp-file inside
                   (prin1 (list (and quit-flag t) (and inhibit-quit t))
                          (current-buffer))))
               ;; Force a quit safe point right after inhibit-quit unbinds.
               (sleep-for 0)
               'no-quit)
           (quit (list 'caught-quit (car err))))))
    (with-temp-file after
      (prin1 outcome (current-buffer))))
  (set-buffer-modified-p nil)
  (kill-emacs 0))
"#;

const ABORT_PROBE: &str = r#";;; -*- lexical-binding: nil; -*-
(let* ((dir (getenv "EE_DIR"))
       (target (expand-file-name "draft.txt" dir))
       (ready (expand-file-name "ready" dir)))
  (find-file target)
  (auto-save-mode 1)
  (insert "unsaved abort line\n")
  (let ((inhibit-quit t))
    (with-temp-file ready
      (insert "ready\n"))
    (while t
      (sleep-for 0.02))))
"#;

fn wait_for_file(path: &Path, session: &mut TuiSession, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        if Instant::now() >= deadline {
            return false;
        }
        session.read(Duration::from_millis(20));
    }
    true
}

fn spawn_job_controlled_editor(
    program: &Path,
    name: &str,
    dir: &Path,
    script: &Path,
    stopped: &Path,
    resume: &Path,
    status: &Path,
    disable_core: bool,
) -> TuiSession {
    let helper = dir.join("job_control.py");
    std::fs::write(&helper, JOB_CONTROL_HELPER).expect("write job-control helper");
    let launch = TuiLaunch::new("python3")
        .arg("-I")
        .arg(&helper)
        .arg(stopped)
        .arg(resume)
        .arg(status)
        .arg(if disable_core { "1" } else { "0" })
        .arg(program)
        .arg("-nw")
        .arg("-Q")
        .arg("-l")
        .arg(script)
        .env("HOME", dir)
        .env("TMPDIR", dir)
        .env("EE_DIR", dir);

    TuiSession::spawn_launch_on_terminal(
        launch,
        name,
        TuiTerminalConfig::new("xterm-256color", 24, 80),
    )
}

#[derive(Debug, PartialEq, Eq)]
struct ContinueObservation {
    stop_signal: i32,
    saw_resume_before_sigcont: bool,
    saw_resuming: bool,
    saw_autosave_prompt: bool,
    saw_autosave_done: bool,
    saw_abort_prompt: bool,
    saw_continuing: bool,
    autosave_contents: String,
    inside_state: String,
    after_outcome: String,
    child_status: String,
}

fn run_continue_case(program: &Path, name: &str) -> ContinueObservation {
    let dir = neomacs_tui_tests::TuiTempDirectory::new("tty-ee-cont-");
    let script = dir.path().join("probe.el");
    let ready = dir.path().join("ready");
    let stopped = dir.path().join("stopped");
    let resume = dir.path().join("resume");
    let status = dir.path().join("status");
    let done = dir.path().join("done");
    let inside = dir.path().join("inside");
    let after = dir.path().join("after");
    let autosave_file = dir.path().join("#draft.txt#");
    std::fs::write(&script, CONTINUE_PROBE).expect("write continue probe");

    let mut session = spawn_job_controlled_editor(
        program,
        name,
        dir.path(),
        &script,
        &stopped,
        &resume,
        &status,
        false,
    );
    assert!(
        wait_for_file(&ready, &mut session, Duration::from_secs(30)),
        "{name} did not enter inhibit-quit loop\nscreen:\n{}\nraw:\n{}",
        session.text_grid().join("\n"),
        String::from_utf8_lossy(session.recent_output()),
    );

    // 1st C-g under inhibit-quit sets quit-flag = t without stopping the process.
    session.clear_recent_output();
    session.send(b"\x07");
    assert!(
        !wait_for_file(&stopped, &mut session, Duration::from_millis(250)),
        "{name} must not stop on the 1st C-g"
    );

    // 2nd C-g while quit-flag is non-nil triggers emergency escape and SIGTSTP.
    session.send(b"\x07");
    assert!(
        wait_for_file(&stopped, &mut session, Duration::from_secs(10)),
        "{name} did not stop with SIGTSTP on 2nd C-g\nscreen:\n{}\nraw:\n{}",
        session.text_grid().join("\n"),
        String::from_utf8_lossy(session.recent_output()),
    );
    let stop_signal: i32 = std::fs::read_to_string(&stopped)
        .expect("read stop signal")
        .trim()
        .parse()
        .expect("parse stop signal");
    assert_eq!(stop_signal, libc::SIGTSTP, "{name} stop signal");

    // While stopped, the resume prompt must not have been emitted yet.
    session.read(Duration::from_millis(100));
    let saw_resume_before_sigcont = session
        .recent_output()
        .windows(b"Emacs is resuming".len())
        .any(|w| w == b"Emacs is resuming");

    // Resume with SIGCONT and answer Auto-save=y, Abort=n.
    session.clear_recent_output();
    std::fs::write(&resume, "go\n").expect("signal SIGCONT");

    let saw_resuming = session.wait_for_raw_output(
        b"Emacs is resuming after an emergency escape.",
        Duration::from_secs(10),
    );
    let saw_autosave_prompt =
        session.wait_for_raw_output(b"Auto-save? (y or n) ", Duration::from_secs(10));
    assert!(
        saw_resuming && saw_autosave_prompt,
        "{name} missing resume/auto-save prompt; raw:\n{}",
        String::from_utf8_lossy(session.recent_output()),
    );

    session.send(b"y\n");
    let saw_autosave_done = session.wait_for_raw_output(b"Auto-save done", Duration::from_secs(10));
    let saw_abort_prompt =
        session.wait_for_raw_output(b"Abort (and dump core)? (y or n) ", Duration::from_secs(10));
    assert!(
        saw_autosave_done && saw_abort_prompt,
        "{name} missing Auto-save done / Abort prompt; raw:\n{}",
        String::from_utf8_lossy(session.recent_output()),
    );
    let autosave_contents = std::fs::read_to_string(&autosave_file).unwrap_or_else(|err| {
        panic!(
            "{name} did not write auto-save file {}: {err}",
            autosave_file.display()
        )
    });

    session.send(b"n\n");
    let saw_continuing = session.wait_for_raw_output(b"Continuing...", Duration::from_secs(10));
    assert!(
        saw_continuing,
        "{name} missing Continuing...; raw:\n{}",
        String::from_utf8_lossy(session.recent_output()),
    );

    // Let the inhibit-quit loop finish cleanly.
    std::fs::write(&done, "done\n").expect("write done sentinel");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(20)),
        TuiProcessOutcome::Exited,
        "{name} did not exit cleanly after Continuing..."
    );

    let inside_state = std::fs::read_to_string(&inside).expect("read inside state");
    let after_outcome = std::fs::read_to_string(&after).expect("read after outcome");
    let child_status = std::fs::read_to_string(&status).expect("read child status");
    ContinueObservation {
        stop_signal,
        saw_resume_before_sigcont,
        saw_resuming,
        saw_autosave_prompt,
        saw_autosave_done,
        saw_abort_prompt,
        saw_continuing,
        autosave_contents,
        inside_state,
        after_outcome,
        child_status,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct AbortObservation {
    stop_signal: i32,
    child_status: String,
    lock_existed_before_abort: bool,
    lock_exists_after_abort: bool,
    autosave_contents: Option<String>,
    saw_fatal_error_line: bool,
}

fn run_abort_case(program: &Path, name: &str) -> AbortObservation {
    let dir = neomacs_tui_tests::TuiTempDirectory::new("tty-ee-abort-");
    let script = dir.path().join("probe.el");
    let ready = dir.path().join("ready");
    let stopped = dir.path().join("stopped");
    let resume = dir.path().join("resume");
    let status = dir.path().join("status");
    let autosave_file = dir.path().join("#draft.txt#");
    let lock_file = dir.path().join(".#draft.txt");
    std::fs::write(&script, ABORT_PROBE).expect("write abort probe");

    let mut session = spawn_job_controlled_editor(
        program,
        name,
        dir.path(),
        &script,
        &stopped,
        &resume,
        &status,
        true,
    );
    assert!(
        wait_for_file(&ready, &mut session, Duration::from_secs(30)),
        "{name} did not enter inhibit-quit loop"
    );
    // Emacs lock files are symlinks whose target does not exist on disk; use
    // symlink_metadata rather than Path::exists.
    let lock_existed_before_abort = std::fs::symlink_metadata(&lock_file).is_ok();

    session.clear_recent_output();
    session.send(b"\x07");
    assert!(
        !wait_for_file(&stopped, &mut session, Duration::from_millis(250)),
        "{name} must not stop on the 1st C-g"
    );

    session.send(b"\x07");
    assert!(
        wait_for_file(&stopped, &mut session, Duration::from_secs(10)),
        "{name} did not stop with SIGTSTP on 2nd C-g\nraw:\n{}",
        String::from_utf8_lossy(session.recent_output()),
    );
    let stop_signal: i32 = std::fs::read_to_string(&stopped)
        .expect("read stop signal")
        .trim()
        .parse()
        .expect("parse stop signal");

    session.clear_recent_output();
    std::fs::write(&resume, "go\n").expect("signal SIGCONT");
    assert!(
        session.wait_for_raw_output(b"Auto-save? (y or n) ", Duration::from_secs(10)),
        "{name} missing Auto-save prompt"
    );
    // Answer "n" to Auto-save, then "y" to Abort: GNU emacs_abort ->
    // terminate_due_to_signal(SIGABRT, 40) -> shut_down_emacs STILL auto-saves
    // modified buffers, unlocks lock files, and writes "Fatal error 6: Aborted".
    session.send(b"n\n");
    assert!(
        session.wait_for_raw_output(b"Abort (and dump core)? (y or n) ", Duration::from_secs(10)),
        "{name} missing Abort prompt"
    );
    session.send(b"y\n");

    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{name} job-control parent did not exit after Abort=y"
    );
    let child_status = std::fs::read_to_string(&status).expect("read abort status");
    let lock_exists_after_abort = std::fs::symlink_metadata(&lock_file).is_ok();
    let autosave_contents = std::fs::read_to_string(&autosave_file).ok();
    let saw_fatal_error_line = session
        .recent_output()
        .windows(b"Fatal error 6: Aborted".len())
        .any(|w| w == b"Fatal error 6: Aborted");
    AbortObservation {
        stop_signal,
        child_status,
        lock_existed_before_abort,
        lock_exists_after_abort,
        autosave_contents,
        saw_fatal_error_line,
    }
}

#[test]
fn controlling_tty_second_ctrl_g_suspends_autosaves_and_continues() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();

    let gnu_obs = run_continue_case(&gnu, "GNU emergency escape continue");
    let neo_obs = run_continue_case(&neo, "Neo emergency escape continue");
    eprintln!("GNU continue: {gnu_obs:?}\nNeo continue: {neo_obs:?}");

    assert_eq!(
        gnu_obs,
        ContinueObservation {
            stop_signal: libc::SIGTSTP,
            saw_resume_before_sigcont: false,
            saw_resuming: true,
            saw_autosave_prompt: true,
            saw_autosave_done: true,
            saw_abort_prompt: true,
            saw_continuing: true,
            autosave_contents: "unsaved emergency escape line\n".to_string(),
            inside_state: "(t t)".to_string(),
            after_outcome: "(caught-quit quit)".to_string(),
            child_status: "exit:0".to_string(),
        },
        "GNU 31.1 oracle contract"
    );
    assert_eq!(
        neo_obs, gnu_obs,
        "Neomacs emergency escape continue diverged from GNU 31.1"
    );
}

#[test]
fn controlling_tty_second_ctrl_g_abort_terminates_with_sigabrt() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();

    let gnu_obs = run_abort_case(&gnu, "GNU emergency escape abort");
    let neo_obs = run_abort_case(&neo, "Neo emergency escape abort");
    eprintln!("GNU abort: {gnu_obs:?}\nNeo abort: {neo_obs:?}");

    assert_eq!(
        gnu_obs,
        AbortObservation {
            stop_signal: libc::SIGTSTP,
            child_status: format!("signal:{}", libc::SIGABRT),
            lock_existed_before_abort: true,
            lock_exists_after_abort: false,
            autosave_contents: Some("unsaved abort line\n".to_string()),
            saw_fatal_error_line: true,
        },
        "GNU 31.1 abort orderly shutdown contract"
    );
    assert_eq!(
        neo_obs, gnu_obs,
        "Neomacs emergency escape abort diverged from GNU 31.1"
    );
}
