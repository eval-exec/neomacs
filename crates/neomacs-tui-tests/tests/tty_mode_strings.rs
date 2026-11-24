#![cfg(unix)]
//! Issue #835: `tty-mode-reset-strings` and `tty-mode-set-strings` terminal
//! parameters must be emitted on TTY exit, suspend, and resume in GNU's byte
//! order.
//!
//! GNU Emacs 31.1 (`src/term.c:169-247`):
//! - `tty_reset_terminal_modes` emits `tty-mode-reset-strings` FIRST (in list
//!   order, skipping non-strings), before `ke`, `ve`, `te`.
//! - `tty_set_terminal_modes` emits `tty-mode-set-strings` LAST (in list
//!   order, skipping non-strings), after `ti` and `ks`.
//! - Both run via `reset_sys_modes`/`init_sys_modes` from exit (`C-x C-c`),
//!   `suspend-emacs` (`C-z`), `suspend-tty`, `resume-tty`, and `delete-terminal`.

use crate::support::{eval_expression_one, settle_session};
use neomacs_tui_tests::{
    QUIET_MX_SUGGESTION_EVAL, QUIET_NATIVE_COMP_EVAL, TuiLaunch, TuiProcessOutcome, TuiSession,
    TuiTerminalConfig,
};
use std::path::Path;
use std::time::{Duration, Instant};

const TERM_TYPE: &str = "xterm-256color";
const MOUSE_ENABLE: &[u8] = b"\x1b[?1000h\x1b[?1003h\x1b[?1006h";
const MOUSE_DISABLE: &[u8] = b"\x1b[?1006l\x1b[?1003l\x1b[?1000l";
const TERM_TI: &[u8] = b"\x1b[?1049h";
const TERM_TE: &[u8] = b"\x1b[?1049l";
const TERM_KS: &[u8] = b"\x1b[?1h\x1b=";
const TERM_KE: &[u8] = b"\x1b[?1l\x1b>";
const TERM_VE: &[u8] = b"\x1b[?12l\x1b[?25h";

const SETUP_EL: &str = r#"(progn
  (xterm-mouse-mode 1)
  (push "<TTY-MODE-RESET-1>" (terminal-parameter nil 'tty-mode-reset-strings))
  (push 42 (terminal-parameter nil 'tty-mode-reset-strings))
  (push "<TTY-MODE-RESET-2>" (terminal-parameter nil 'tty-mode-reset-strings))
  (push "<TTY-MODE-SET-1>" (terminal-parameter nil 'tty-mode-set-strings))
  (push 99 (terminal-parameter nil 'tty-mode-set-strings))
  (push "<TTY-MODE-SET-2>" (terminal-parameter nil 'tty-mode-set-strings)))"#;

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn find_subslice_from(haystack: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    find_subslice(&haystack[start..], needle).map(|offset| start + offset)
}

fn spawn_editor(program: &Path, label: &str, is_gnu: bool) -> TuiSession {
    let mut launch = TuiLaunch::new(program.as_os_str())
        .with_uncertified_startup()
        .arg("-nw")
        .arg("-Q");
    if is_gnu {
        launch = launch
            .arg("-no-comp-spawn")
            .arg(QUIET_NATIVE_COMP_EVAL)
            .arg(QUIET_MX_SUGGESTION_EVAL);
    } else {
        launch = launch.arg(QUIET_MX_SUGGESTION_EVAL);
    }
    launch = launch.arg("--eval").arg(SETUP_EL);
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        label,
        TuiTerminalConfig::new(TERM_TYPE, 24, 80),
    );
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && grid
                .iter()
                .any(|row| row.contains("This buffer is for text that is not saved"))
    };
    session.read_until(Duration::from_secs(20), ready);
    assert!(
        ready(&session.text_grid()),
        "{label}: editor did not reach *scratch*:\n{}",
        session.text_grid().join("\n")
    );
    settle_session(&mut session);
    session
}

fn assert_reset_order(output: &[u8], label: &str, phase: &str) -> usize {
    let reset2 = find_subslice(output, b"<TTY-MODE-RESET-2>").unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing <TTY-MODE-RESET-2> in PTY output: {:?}",
            String::from_utf8_lossy(output)
        )
    });
    let reset1 = find_subslice_from(output, reset2 + 1, b"<TTY-MODE-RESET-1>").unwrap_or_else(
        || {
            panic!(
                "{label} ({phase}): missing <TTY-MODE-RESET-1> after <TTY-MODE-RESET-2> in PTY output: {:?}",
                String::from_utf8_lossy(output)
            )
        },
    );
    let mouse_disable = find_subslice_from(output, reset1 + 1, MOUSE_DISABLE).unwrap_or_else(
        || {
            panic!(
                "{label} ({phase}): missing xterm mouse disable sequence after reset markers in PTY output: {:?}",
                String::from_utf8_lossy(output)
            )
        },
    );
    let ke = find_subslice_from(output, mouse_disable + MOUSE_DISABLE.len(), TERM_KE)
        .unwrap_or_else(|| {
            panic!(
                "{label} ({phase}): missing ke after tty-mode-reset-strings in PTY output: {:?}",
                String::from_utf8_lossy(output)
            )
        });
    let ve = find_subslice_from(output, ke + TERM_KE.len(), TERM_VE).unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing ve after ke in PTY output: {:?}",
            String::from_utf8_lossy(output)
        )
    });
    let te = find_subslice_from(output, ve + TERM_VE.len(), TERM_TE).unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing te after ve in PTY output: {:?}",
            String::from_utf8_lossy(output)
        )
    });
    te + TERM_TE.len()
}

fn assert_set_order(output: &[u8], after: usize, label: &str, phase: &str) {
    let ti = find_subslice_from(output, after, TERM_TI).unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing ti after te in PTY output: {:?}",
            String::from_utf8_lossy(&output[after..])
        )
    });
    let ks = find_subslice_from(output, ti + TERM_TI.len(), TERM_KS).unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing ks after ti in PTY output: {:?}",
            String::from_utf8_lossy(&output[after..])
        )
    });
    let set2 =
        find_subslice_from(output, ks + TERM_KS.len(), b"<TTY-MODE-SET-2>").unwrap_or_else(|| {
            panic!(
                "{label} ({phase}): missing <TTY-MODE-SET-2> after ks in PTY output: {:?}",
                String::from_utf8_lossy(&output[after..])
            )
        });
    let set1 = find_subslice_from(output, set2 + 1, b"<TTY-MODE-SET-1>").unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing <TTY-MODE-SET-1> after <TTY-MODE-SET-2> in PTY output: {:?}",
            String::from_utf8_lossy(&output[after..])
        )
    });
    let _mouse_enable = find_subslice_from(output, set1 + 1, MOUSE_ENABLE).unwrap_or_else(|| {
        panic!(
            "{label} ({phase}): missing xterm mouse enable sequence after set markers in PTY output: {:?}",
            String::from_utf8_lossy(&output[after..])
        )
    });
}

fn wait_for_bytes(session: &mut TuiSession, needle: &[u8], timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if find_subslice(session.recent_output(), needle).is_some() {
            // Drain any trailing bytes from the same burst.
            session.read(Duration::from_millis(150));
            return;
        }
        session.read(Duration::from_millis(80));
    }
}

fn check_exit(program: &Path, label: &str, is_gnu: bool) {
    let mut session = spawn_editor(program, label, is_gnu);
    session.clear_recent_output();
    session.send_keys("C-x C-c");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{label}: did not exit after C-x C-c:\n{}",
        session.text_grid().join("\n")
    );
    assert_reset_order(session.recent_output(), label, "exit C-x C-c");
}

fn check_suspend_emacs_and_resume(program: &Path, label: &str, is_gnu: bool) {
    let mut session = spawn_editor(program, label, is_gnu);
    session.clear_recent_output();
    session.send_key("C-z");
    wait_for_bytes(&mut session, MOUSE_ENABLE, Duration::from_secs(10));
    let output = session.recent_output().to_vec();
    let after_te = assert_reset_order(&output, label, "C-z suspend");
    assert_set_order(&output, after_te, label, "C-z resume");

    session.send_keys("C-x C-c");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{label}: did not exit cleanly after C-z resume"
    );
}

fn check_suspend_tty_and_resume_tty(program: &Path, label: &str, is_gnu: bool) {
    let mut session = spawn_editor(program, label, is_gnu);
    session.clear_recent_output();
    eval_expression_one(&mut session, "(progn (suspend-tty) (resume-tty))");
    wait_for_bytes(&mut session, MOUSE_ENABLE, Duration::from_secs(10));
    let output = session.recent_output().to_vec();
    let after_te = assert_reset_order(&output, label, "suspend-tty");
    assert_set_order(&output, after_te, label, "resume-tty");

    session.send_keys("C-x C-c");
    assert_eq!(
        session.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "{label}: did not exit cleanly after suspend-tty/resume-tty"
    );
}

#[test]
fn tty_mode_reset_strings_emitted_on_exit_like_gnu() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();
    check_exit(&gnu, "GNU", true);
    check_exit(&neo, "Neomacs", false);
}

#[test]
fn tty_mode_strings_emitted_on_suspend_emacs_and_resume_like_gnu() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();
    check_suspend_emacs_and_resume(&gnu, "GNU", true);
    check_suspend_emacs_and_resume(&neo, "Neomacs", false);
}

#[test]
fn tty_mode_strings_emitted_on_suspend_tty_and_resume_tty_like_gnu() {
    let gnu = neomacs_tui_tests::oracle::gnu_oracle_program();
    let neo = neomacs_tui_tests::neomacs_binary();
    check_suspend_tty_and_resume_tty(&gnu, "GNU", true);
    check_suspend_tty_and_resume_tty(&neo, "Neomacs", false);
}
