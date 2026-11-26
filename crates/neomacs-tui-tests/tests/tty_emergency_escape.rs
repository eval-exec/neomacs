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
use std::time::Duration;

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

const MODE_STRINGS_PROBE: &str = r#";;; -*- lexical-binding: nil; -*-
(let* ((dir (getenv "EE_DIR"))
       (ready (expand-file-name "ready" dir))
       (done (expand-file-name "done" dir)))
  (push "<EE-RESET-1>" (terminal-parameter nil 'tty-mode-reset-strings))
  (push 42 (terminal-parameter nil 'tty-mode-reset-strings))
  (push "<EE-RESET-2>" (terminal-parameter nil 'tty-mode-reset-strings))
  (push "<EE-SET-1>" (terminal-parameter nil 'tty-mode-set-strings))
  (push 99 (terminal-parameter nil 'tty-mode-set-strings))
  (push "<EE-SET-2>" (terminal-parameter nil 'tty-mode-set-strings))
  (condition-case nil
      (progn
        (let ((inhibit-quit t))
          (with-temp-file ready
            (insert "ready\n"))
          (while (not (file-exists-p done))
            (sleep-for 0.02)))
        (sleep-for 0))
    (quit nil))
  (kill-emacs 0))
"#;

const ABORT_PROBE: &str = r#";;; -*- lexical-binding: nil; -*-
(let* ((dir (getenv "EE_DIR"))
       (target (expand-file-name "draft.txt" dir))
       (ready (expand-file-name "ready" dir))
       (kill-marker (expand-file-name "kill-hook-ran" dir))
       (autosave-marker (expand-file-name "auto-save-hook-ran" dir)))
  (add-hook 'kill-emacs-hook
            (lambda ()
              (with-temp-file kill-marker (insert "ran\n"))))
  (add-hook 'auto-save-hook
            (lambda ()
              (with-temp-file autosave-marker (insert "ran\n"))))
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
    let cap = neomacs_tui_tests::HangCap::new(timeout);
    while !path.exists() {
        if cap.is_expired() {
            return false;
        }
        session.read(Duration::from_millis(20));
    }
    true
}

fn assert_file_absent_during_soak(path: &Path, session: &mut TuiSession, context: &str) {
    let _soak = neomacs_tui_tests::NegativeSoak::observe(
        Duration::from_secs(1),
        Duration::from_millis(20),
        || {
            session.read(Duration::from_millis(20));
            assert!(!path.exists(), "{context}");
        },
    );
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
    assert_file_absent_during_soak(
        &stopped,
        &mut session,
        &format!("{name} must not stop on the 1st C-g"),
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
    autosave_existed_before_abort: bool,
    autosave_contents: Option<String>,
    kill_hook_ran: bool,
    auto_save_hook_ran: bool,
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
    let kill_hook_marker = dir.path().join("kill-hook-ran");
    let auto_save_hook_marker = dir.path().join("auto-save-hook-ran");
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
    assert_file_absent_during_soak(
        &stopped,
        &mut session,
        &format!("{name} must not stop on the 1st C-g"),
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
    // modified buffers, unlocks lock files, suppresses kill-emacs-hook and
    // auto-save-hook, and writes "Fatal error 6: Aborted".
    session.send(b"n\n");
    assert!(
        session.wait_for_raw_output(b"Abort (and dump core)? (y or n) ", Duration::from_secs(10)),
        "{name} missing Abort prompt"
    );
    let autosave_existed_before_abort = autosave_file.exists();
    session.send(b"y\n");

    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{name} job-control parent did not exit after Abort=y"
    );
    let child_status = std::fs::read_to_string(&status).expect("read abort status");
    let lock_exists_after_abort = std::fs::symlink_metadata(&lock_file).is_ok();
    let autosave_contents = std::fs::read_to_string(&autosave_file).ok();
    let kill_hook_ran = kill_hook_marker.exists();
    let auto_save_hook_ran = auto_save_hook_marker.exists();
    let saw_fatal_error_line = session
        .recent_output()
        .windows(b"Fatal error 6: Aborted".len())
        .any(|w| w == b"Fatal error 6: Aborted");
    AbortObservation {
        stop_signal,
        child_status,
        lock_existed_before_abort,
        lock_exists_after_abort,
        autosave_existed_before_abort,
        autosave_contents,
        kill_hook_ran,
        auto_save_hook_ran,
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
            autosave_existed_before_abort: false,
            autosave_contents: Some("unsaved abort line\n".to_string()),
            kill_hook_ran: false,
            auto_save_hook_ran: false,
            saw_fatal_error_line: true,
        },
        "GNU 31.1 abort orderly shutdown contract"
    );
    assert_eq!(
        neo_obs, gnu_obs,
        "Neomacs emergency escape abort diverged from GNU 31.1"
    );
}

#[derive(Debug, PartialEq, Eq)]
struct ModeStringsObservation {
    stop_signal: i32,
    reset_strings_ordered_before_te: bool,
    no_set_strings_before_sigcont: bool,
    set_strings_ordered_after_ti: bool,
    child_status: String,
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn find_subslice_from(haystack: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    find_subslice(&haystack[start..], needle).map(|offset| start + offset)
}

fn run_mode_strings_case(program: &Path, name: &str) -> ModeStringsObservation {
    const TERM_KE: &[u8] = b"\x1b[?1l\x1b>";
    const TERM_VE: &[u8] = b"\x1b[?12l\x1b[?25h";
    const TERM_TE: &[u8] = b"\x1b[?1049l";
    const TERM_TI: &[u8] = b"\x1b[?1049h";
    const TERM_KS: &[u8] = b"\x1b[?1h\x1b=";
    const RESET_PAIR: &[u8] = b"<EE-RESET-2><EE-RESET-1>";
    const SET_PAIR: &[u8] = b"<EE-SET-2><EE-SET-1>";

    let dir = neomacs_tui_tests::TuiTempDirectory::new("tty-ee-modes-");
    let script = dir.path().join("probe.el");
    let ready = dir.path().join("ready");
    let stopped = dir.path().join("stopped");
    let resume = dir.path().join("resume");
    let status = dir.path().join("status");
    let done = dir.path().join("done");
    std::fs::write(&script, MODE_STRINGS_PROBE).expect("write mode-strings probe");

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
        "{name} did not enter inhibit-quit loop"
    );

    session.clear_recent_output();
    session.send(b"\x07");
    assert_file_absent_during_soak(
        &stopped,
        &mut session,
        &format!("{name} must not stop on the 1st C-g"),
    );

    session.send(b"\x07");
    assert!(
        wait_for_file(&stopped, &mut session, Duration::from_secs(10)),
        "{name} did not stop with SIGTSTP on 2nd C-g"
    );
    let stop_signal: i32 = std::fs::read_to_string(&stopped)
        .expect("read stop signal")
        .trim()
        .parse()
        .expect("parse stop signal");
    session.read(Duration::from_millis(100));
    let pre_suspend = session.recent_output().to_vec();
    let reset_strings_ordered_before_te = find_subslice(&pre_suspend, RESET_PAIR)
        .and_then(|pair| find_subslice_from(&pre_suspend, pair + RESET_PAIR.len(), TERM_KE))
        .and_then(|ke| find_subslice_from(&pre_suspend, ke + TERM_KE.len(), TERM_VE))
        .and_then(|ve| find_subslice_from(&pre_suspend, ve + TERM_VE.len(), TERM_TE))
        .is_some();
    let no_set_strings_before_sigcont = find_subslice(&pre_suspend, b"<EE-SET-2>").is_none()
        && find_subslice(&pre_suspend, b"<EE-SET-1>").is_none();

    session.clear_recent_output();
    std::fs::write(&resume, "go\n").expect("signal SIGCONT");
    assert!(
        session.wait_for_raw_output(b"Auto-save? (y or n) ", Duration::from_secs(10)),
        "{name} missing Auto-save prompt"
    );
    session.send(b"n\n");
    assert!(
        session.wait_for_raw_output(b"Abort (and dump core)? (y or n) ", Duration::from_secs(10)),
        "{name} missing Abort prompt"
    );
    session.send(b"n\n");
    assert!(
        session.wait_for_raw_output(b"Continuing...", Duration::from_secs(10)),
        "{name} missing Continuing..."
    );

    std::fs::write(&done, "done\n").expect("write done sentinel");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(20)),
        TuiProcessOutcome::Exited,
        "{name} did not exit cleanly after Continuing..."
    );
    let post_resume = session.recent_output().to_vec();
    let set_strings_ordered_after_ti = find_subslice(&post_resume, b"Continuing...")
        .and_then(|cont| find_subslice_from(&post_resume, cont + b"Continuing...".len(), TERM_TI))
        .and_then(|ti| find_subslice_from(&post_resume, ti + TERM_TI.len(), TERM_KS))
        .and_then(|ks| find_subslice_from(&post_resume, ks + TERM_KS.len(), SET_PAIR))
        .is_some();
    let child_status = std::fs::read_to_string(&status).expect("read child status");

    ModeStringsObservation {
        stop_signal,
        reset_strings_ordered_before_te,
        no_set_strings_before_sigcont,
        set_strings_ordered_after_ti,
        child_status,
    }
}

#[test]
fn controlling_tty_second_ctrl_g_emits_tty_mode_reset_and_set_strings() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();

    let gnu_obs = run_mode_strings_case(&gnu, "GNU emergency escape mode strings");
    let neo_obs = run_mode_strings_case(&neo, "Neo emergency escape mode strings");
    eprintln!("GNU mode strings: {gnu_obs:?}\nNeo mode strings: {neo_obs:?}");

    assert_eq!(
        gnu_obs,
        ModeStringsObservation {
            stop_signal: libc::SIGTSTP,
            reset_strings_ordered_before_te: true,
            no_set_strings_before_sigcont: true,
            set_strings_ordered_after_ti: true,
            child_status: "exit:0".to_string(),
        },
        "GNU 31.1 emergency escape tty-mode-reset/set-strings contract"
    );
    assert_eq!(
        neo_obs, gnu_obs,
        "Neomacs emergency escape tty-mode-reset/set-strings diverged from GNU 31.1"
    );
}
