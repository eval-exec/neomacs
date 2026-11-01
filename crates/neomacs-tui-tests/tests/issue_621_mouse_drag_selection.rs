//! Issue #621: a tty mouse drag must leave the dragged region active.
//!
//! The editors load a fixture that enables `xterm-mouse-mode` before the
//! command loop starts and records the region state in `post-command-hook`
//! right after the drag command.  The host sends an SGR press/motion/release
//! drag once the fixture signals readiness; a timer writes the state and
//! exits the editor.

use neomacs_tui_tests::{TuiProcessOutcome, TuiSession, TuiTerminalConfig};
use std::path::Path;
use std::time::Duration;

const DRAG: &[u8] = b"\x1b[<0;2;2M\x1b[<32;10;2M\x1b[<0;10;2m";

fn fixture(ready: &Path, out: &Path) -> String {
    format!(
        r#";;; -*- lexical-binding: t; -*-
(menu-bar-mode -1)
(xterm-mouse-mode 1)
(switch-to-buffer (get-buffer-create "issue621"))
(erase-buffer)
(dotimes (i 8)
  (insert (make-string 60 (+ ?a (% i 26))) "\n"))
(goto-char (point-min))
(set-window-start (selected-window) (point-min))
(setq issue621-state nil)
(add-hook 'post-command-hook
          (lambda ()
            (when (memq this-command
                        '(mouse-drag-region mouse-set-region mouse-set-point))
              (setq issue621-state
                    (list this-command
                          (region-active-p)
                          (ignore-errors (mark))
                          (point))))))
(with-temp-file {ready:?} (insert "ready"))
(run-with-timer 4 nil
  (lambda ()
    (with-temp-file {out:?}
      (prin1 issue621-state (current-buffer)))
    (kill-emacs 0)))
"#,
        ready = ready,
        out = out
    )
}

#[test]
fn a_tty_mouse_drag_leaves_the_region_active_like_gnu() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let terminal = TuiTerminalConfig::new("xterm-256color", 40, 120);

    let mut sessions = Vec::new();
    for (label, script_name, out_name, ready_name) in [
        (
            "gnu",
            "issue-621-gnu.el",
            "issue-621-gnu.out",
            "issue-621-gnu.ready",
        ),
        (
            "neo",
            "issue-621-neomacs.el",
            "issue-621-neomacs.out",
            "issue-621-neomacs.ready",
        ),
    ] {
        let out = dir.path().join(out_name);
        let ready = dir.path().join(ready_name);
        let script = dir.path().join(script_name);
        std::fs::write(&script, fixture(&ready, &out)).expect("write drag fixture");
        let session = if label == "gnu" {
            TuiSession::gnu_emacs_on_terminal(&format!("-l {}", script.display()), terminal.clone())
        } else {
            TuiSession::neomacs_on_terminal(&format!("-l {}", script.display()), terminal.clone())
        };
        sessions.push((label, session, out, ready));
    }

    let (gnu_label, mut gnu, gnu_out, gnu_ready) = sessions.remove(0);
    let (neo_label, mut neo, neo_out, neo_ready) = sessions.remove(0);
    assert_eq!((gnu_label, neo_label), ("gnu", "neo"));

    for (session, ready, label) in [
        (&mut gnu, &gnu_ready, "GNU"),
        (&mut neo, &neo_ready, "Neomacs"),
    ] {
        session.read_until(Duration::from_secs(20), |_| ready.exists());
        assert!(
            ready.exists(),
            "{label} drag fixture did not become ready; grid:\n{}",
            session.text_grid().join("\n")
        );
    }

    // Let the fixture return and the command loop start reading input.
    std::thread::sleep(Duration::from_millis(300));
    gnu.send(DRAG);
    neo.send(DRAG);
    assert_eq!(
        gnu.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "GNU drag probe did not exit; grid:\n{}",
        gnu.text_grid().join("\n")
    );
    assert_eq!(
        neo.run_to_completion(Duration::from_secs(15)),
        TuiProcessOutcome::Exited,
        "Neomacs drag probe did not exit; grid:\n{}",
        neo.text_grid().join("\n")
    );

    let gnu_result = std::fs::read_to_string(&gnu_out).expect("GNU drag state");
    let neo_result = std::fs::read_to_string(&neo_out).expect("Neomacs drag state");
    assert!(
        gnu_result.starts_with("(mouse-set-region t "),
        "GNU must run mouse-set-region and keep the region active; \
         GNU state: {gnu_result:?}\nGNU grid:\n{}\nNeomacs grid:\n{}",
        gnu.text_grid().join("\n"),
        neo.text_grid().join("\n")
    );
    let positions: Vec<i64> = gnu_result
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split_whitespace()
        .filter_map(|field| field.parse().ok())
        .collect();
    assert_eq!(
        positions.len(),
        2,
        "GNU drag state must carry mark and point: {gnu_result:?}"
    );
    assert!(
        positions[0] < positions[1],
        "GNU must leave a non-empty dragged region: {gnu_result:?}"
    );
    assert_eq!(
        neo_result,
        gnu_result,
        "Neomacs must record the same post-drag region state; \
         GNU grid:\n{}\nNeomacs grid:\n{}",
        gnu.text_grid().join("\n"),
        neo.text_grid().join("\n")
    );
}
