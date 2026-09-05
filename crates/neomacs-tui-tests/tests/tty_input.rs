#![cfg(unix)]
//! End-to-end Unix TTY input regressions.
//!
//! These tests deliberately enter bytes through a real PTY.  Unit tests in
//! `neovm-core` cover decoding and translation in isolation; this suite owns
//! the complete frontend -> bridge -> evaluator -> command-loop seam.

use crate::support;

use std::time::Duration;

use support::*;

const ESCAPE_RECEIVED: &str = "standalone-escape-received";
const UP_RECEIVED: &str = "tty-up-received";

fn probe_visible(grid: &[String], probe: &str) -> bool {
    grid.iter().any(|row| row.contains(probe))
}

/// A Unix TTY sends both a standalone Escape key and terminal function keys
/// as byte sequences beginning with ESC.  GNU delivers a standalone ESC to
/// the command loop while still translating a complete CSI sequence through
/// `input-decode-map`.  Exercise both outcomes through the real PTY so an
/// eager frontend decoder cannot silently retain ESC forever or consume CSI.
#[test]
fn standalone_escape_and_csi_up_reach_distinct_commands() {
    let (mut gnu, mut neo) = boot_pair("");

    let setup = format!(
        r#"(progn
             (defun neomacs-tui-escape-probe ()
               (interactive)
               ;; Restore the ordinary ESC prefix after this one-shot probe so
               ;; the following CSI assertion exercises input-decode-map under
               ;; the same keymap state as a normal terminal session.
               (local-unset-key "\e")
               (erase-buffer)
               (insert {escape:?}))
             (defun neomacs-tui-up-probe ()
               (interactive)
               (erase-buffer)
               (insert {up:?}))
             (local-set-key "\e" #'neomacs-tui-escape-probe)
             (local-set-key [up] #'neomacs-tui-up-probe)
             (message "tty-input-probe-ready"))"#,
        escape = ESCAPE_RECEIVED,
        up = UP_RECEIVED,
    );
    eval_expression(&mut gnu, &mut neo, &setup);

    let setup_ready = |grid: &[String]| probe_visible(grid, "tty-input-probe-ready");
    gnu.read_until(Duration::from_secs(8), setup_ready);
    neo.read_until(Duration::from_secs(12), setup_ready);

    // Send exactly one raw ESC byte.  Do not use a named/key-event helper:
    // preserving this transport fact is the behavior under test.
    send_both_raw(&mut gnu, &mut neo, b"\x1b");
    gnu.read_until(Duration::from_secs(8), |grid| {
        probe_visible(grid, ESCAPE_RECEIVED)
    });
    neo.read_until(Duration::from_secs(12), |grid| {
        probe_visible(grid, ESCAPE_RECEIVED)
    });

    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            probe_visible(&session.text_grid(), ESCAPE_RECEIVED),
            "{label} should dispatch a standalone TTY ESC byte:\n{}",
            session.text_grid().join("\n")
        );
    }

    // A complete CSI Up sequence shares the ESC prefix but must still be
    // translated to the symbolic `up` event rather than dispatching ESC and
    // inserting the remaining bytes.
    send_both_raw(&mut gnu, &mut neo, b"\x1b[A");
    gnu.read_until(Duration::from_secs(8), |grid| {
        probe_visible(grid, UP_RECEIVED)
    });
    neo.read_until(Duration::from_secs(12), |grid| {
        probe_visible(grid, UP_RECEIVED)
    });

    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            probe_visible(&session.text_grid(), UP_RECEIVED),
            "{label} should translate a complete TTY CSI Up sequence:\n{}",
            session.text_grid().join("\n")
        );
    }
    assert_pair_exact_display(
        "standalone_escape_and_csi_up_reach_distinct_commands",
        &gnu,
        &neo,
    );
}
