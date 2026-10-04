#![cfg(unix)]
//! Live GNU parity for chrome transitions and externally damaged TTY rows.

use neomacs_tui_tests::pair::{read_both, send_both, wait_for_both};
use neomacs_tui_tests::{TuiLaunch, TuiSession, TuiTempDirectory, TuiTerminalConfig};
use std::path::PathBuf;
use std::time::Duration;

const INIT: &str = r#"(setq inhibit-startup-screen t initial-scratch-message nil
      redisplay-dont-pause t native-comp-jit-compilation nil)
(when (fboundp 'menu-bar-mode) (menu-bar-mode -1))
(when (fboundp 'tool-bar-mode) (tool-bar-mode -1))
(switch-to-buffer (get-buffer-create "*redisplay-transitions*"))
(emacs-lisp-mode)
(font-lock-mode -1)
(insert "TTY-REPAINT-ANCHOR\nrow-two\nrow-three\n")
(goto-char (point-min))
(setq mode-line-format '("STATE:" (defining-kbd-macro "Def" "Idle") "|%[BODY%]"))
(global-set-key (kbd "C-c e") (lambda () (interactive) (recursive-edit)))
(global-set-key (kbd "C-c g")
                (lambda () (interactive) (send-string-to-terminal "\e[1;1HGARBAGE")))
(global-set-key (kbd "C-c d") #'redraw-display)
(global-set-key (kbd "C-c f") (lambda () (interactive) (redraw-frame)))
(global-set-key (kbd "C-c l")
                (lambda () (interactive)
                  (let ((recenter-redisplay 'tty)) (recenter nil t))))
"#;

fn pair(hooks: bool) -> (TuiSession, TuiSession, TuiTempDirectory) {
    let files = TuiTempDirectory::new("neomacs-redisplay-transitions-");
    let script = files.path().join("transitions.el");
    std::fs::write(&script, INIT).expect("write shared transition fixture");
    let gnu_program = std::env::var_os("ORACLE_EMACS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("GNU home")).join(".local/bin/emacs")
        });
    let terminal = || TuiTerminalConfig::new("xterm-256color", 32, 96);
    let mut gnu = TuiSession::spawn_launch_on_terminal(
        TuiLaunch::new(gnu_program.as_os_str())
            .arg("-nw")
            .arg("-Q")
            .arg("-no-comp-spawn")
            .arg("--load")
            .arg(&script)
            .env("LC_ALL", "C.UTF-8"),
        "GNU",
        terminal(),
    );
    let mut neo = TuiSession::spawn_launch_on_terminal(
        TuiLaunch::new(neomacs_tui_tests::neomacs_binary().as_os_str())
            .arg("-nw")
            .arg("-Q")
            .arg("--load")
            .arg(&script)
            .env("LC_ALL", "C.UTF-8")
            .env(
                "NEOMACS_REDISPLAY_GNU_HOOKS",
                if hooks { "on" } else { "off" },
            )
            .env("NEOMACS_TTY_SILENT", "on")
            .env("NEOMACS_MODE_LINE_GATE", "gnu")
            .env("NEOMACS_LAYOUT_EDIT_SYNC", "sync"),
        "Neomacs",
        terminal(),
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(20), |grid| {
        grid.first()
            .is_some_and(|row| row.starts_with("TTY-REPAINT-ANCHOR"))
            && grid.iter().any(|row| row.contains("STATE:Idle|BODY"))
    });
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    (gnu, neo, files)
}

fn assert_mode_line(gnu: &TuiSession, neo: &TuiSession, expected: &str) {
    let state = |session: &TuiSession| {
        session
            .text_grid()
            .into_iter()
            .find(|row| row.contains("STATE:"))
            .unwrap_or_else(|| panic!("{} has no mode line", session.name))
    };
    let gnu_line = state(gnu);
    let neo_line = state(neo);
    assert!(gnu_line.contains(expected), "GNU transition: {gnu_line}");
    assert_eq!(
        neo_line, gnu_line,
        "mode-line transition differs from live GNU"
    );
}

#[test]
fn keyboard_macro_mode_line_transitions_match_gnu_with_shipping_policy() {
    let (mut gnu, mut neo, _files) = pair(false);
    send_both(&mut gnu, &mut neo, "C-x (");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("STATE:Def|BODY"))
    });
    assert_mode_line(&gnu, &neo, "STATE:Def|BODY");
    send_both(&mut gnu, &mut neo, "a");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    assert_mode_line(&gnu, &neo, "STATE:Def|BODY");
    send_both(&mut gnu, &mut neo, "C-x )");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("STATE:Idle|BODY"))
    });
    assert_mode_line(&gnu, &neo, "STATE:Idle|BODY");
}

#[test]
fn recursive_edit_mode_line_transitions_match_gnu_with_shipping_policy() {
    let (mut gnu, mut neo, _files) = pair(false);
    send_both(&mut gnu, &mut neo, "C-c e");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("STATE:Idle|[BODY]"))
    });
    assert_mode_line(&gnu, &neo, "STATE:Idle|[BODY]");
    send_both(&mut gnu, &mut neo, "C-M-c");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("STATE:Idle|BODY"))
    });
    assert_mode_line(&gnu, &neo, "STATE:Idle|BODY");
}

fn assert_external_damage_is_repaired(hooks: bool) {
    let (mut gnu, mut neo, _files) = pair(hooks);
    for command in ["C-c d", "C-c f", "C-c l"] {
        send_both(&mut gnu, &mut neo, "C-c g");
        wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
            grid.first().is_some_and(|row| row.starts_with("GARBAGE"))
        });
        for session in [&gnu, &neo] {
            assert!(
                session.text_grid()[0].starts_with("GARBAGE"),
                "{} must expose the external terminal corruption before {command}",
                session.name
            );
        }
        send_both(&mut gnu, &mut neo, command);
        wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), |grid| {
            grid.first()
                .is_some_and(|row| row.starts_with("TTY-REPAINT-ANCHOR"))
        });
        let gnu_grid = gnu.text_grid();
        let neo_grid = neo.text_grid();
        assert!(
            gnu_grid[0].starts_with("TTY-REPAINT-ANCHOR"),
            "GNU {command}"
        );
        assert_eq!(
            neo_grid[0], gnu_grid[0],
            "{command}, hooks={hooks}: damaged row must repaint"
        );
        assert_mode_line(&gnu, &neo, "STATE:Idle|BODY");
    }
}

#[test]
fn explicit_redraw_repairs_external_tty_damage_with_silent_output_and_hooks_off() {
    assert_external_damage_is_repaired(false);
}

#[test]
fn explicit_redraw_repairs_external_tty_damage_with_silent_output_and_hooks_on() {
    assert_external_damage_is_repaired(true);
}
