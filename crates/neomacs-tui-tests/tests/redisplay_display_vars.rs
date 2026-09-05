#![cfg(unix)]
//! TUI parity guards for Finding 6: a requested redisplay must NOT be
//! dropped when only a display-affecting buffer variable changes.
//!
//! GNU Emacs re-reads the live per-buffer display slots
//! (`truncate_lines`, `tab_width`, `header_line_format`, …) on every
//! `redisplay_window` cycle and repaints whatever the changed value
//! produces (`src/xdisp.c:20535-20566`). Neomacs short-circuits
//! redisplay on an unchanged `RedisplaySignature`, so a bare
//! `(setq truncate-lines t)` used to leave the screen stale until the
//! NEXT keystroke bumped the signature — the "Doom blank pane" class of
//! bug. The fix marks redisplay dirty when a display-affecting variable
//! is set, mirroring GNU's `bset_redisplay`/`windows_or_buffers_changed`.
//!
//! Each guard evaluates the variable via `M-:` (whose own `RET` is the
//! LAST keystroke) and then asserts neomacs repaints to match GNU
//! WITHOUT any further keypress. If the redisplay were dropped, the
//! neomacs grid would still show the pre-change layout and diverge from
//! GNU.

use crate::support;

use neomacs_tui_tests::TuiSession;
use std::time::Duration;
use support::*;

/// Fill the scratch buffer with a single very long line (so truncation
/// vs. continuation is visually distinct) followed by a tab-prefixed
/// line (so `tab-width` changes the visible column), then move point to
/// the top. Returns after both editors render the inserted text.
fn seed_long_line(gnu: &mut TuiSession, neo: &mut TuiSession) {
    // Clear scratch and insert deterministic content.
    send_both(gnu, neo, "C-x h C-w");
    read_both(gnu, neo, Duration::from_millis(400));

    // A line wider than an 80-column window so truncate-lines is visible,
    // then a second line with a leading TAB so tab-width is visible.
    let long: String = std::iter::repeat('x').take(120).collect();
    let typed = format!("{long}\r\tTABBED-CELL");
    gnu.send(typed.as_bytes());
    neo.send(typed.as_bytes());

    // Point back to start of buffer so the long line is the current line.
    send_both(gnu, neo, "M-<");

    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("xxxxxxxxxx"))
            && grid.iter().any(|row| row.contains("TABBED-CELL"))
    };
    wait_for_both(gnu, neo, Duration::from_secs(10), ready);
    read_both(gnu, neo, Duration::from_millis(500));
}

/// Read both PTYs for a brief settling window WITHOUT sending any key,
/// so we observe only the repaint produced by the just-evaluated form.
fn settle_after_eval(gnu: &mut TuiSession, neo: &mut TuiSession) {
    read_both(gnu, neo, Duration::from_secs(2));
}

#[test]
fn setq_truncate_lines_repaints_without_extra_keystroke() {
    let (mut gnu, mut neo) = boot_pair("");
    seed_long_line(&mut gnu, &mut neo);

    // Baseline: with truncate-lines nil, the long line wraps. Both should
    // already agree.
    assert_pair_exact_display("truncate-lines baseline", &gnu, &neo);

    // Toggle truncate-lines. M-: ... RET is the only input; the RET is the
    // final keystroke. After it, the long line must show truncated (a
    // single visible row with a continuation/truncation marker) on BOTH.
    eval_expression(&mut gnu, &mut neo, "(setq truncate-lines t)");
    settle_after_eval(&mut gnu, &mut neo);

    // No further keypress. Neomacs must already match GNU's truncated
    // layout. If the redisplay were dropped the grids would diverge
    // (neomacs still wrapping).
    assert_pair_exact_display("truncate-lines repaint without extra keystroke", &gnu, &neo);
}

#[test]
fn setq_tab_width_repaints_without_extra_keystroke() {
    let (mut gnu, mut neo) = boot_pair("");
    seed_long_line(&mut gnu, &mut neo);

    assert_pair_exact_display("tab-width baseline", &gnu, &neo);

    // Move point onto the tabbed line so it is the current line, then
    // change tab-width. The visible column of "TABBED-CELL" must shift to
    // match GNU with no extra keystroke.
    eval_expression(&mut gnu, &mut neo, "(setq tab-width 16)");
    settle_after_eval(&mut gnu, &mut neo);

    assert_pair_exact_display("tab-width repaint without extra keystroke", &gnu, &neo);
}

#[test]
fn setq_header_line_format_repaints_without_extra_keystroke() {
    let (mut gnu, mut neo) = boot_pair("");
    seed_long_line(&mut gnu, &mut neo);

    assert_pair_exact_display("header-line baseline", &gnu, &neo);

    // Installing a header-line-format adds a whole new display row at the
    // top of the window. This is the most visually obvious display-var
    // change: if redisplay is dropped, neomacs shows no header line while
    // GNU does.
    eval_expression(
        &mut gnu,
        &mut neo,
        "(setq header-line-format \"NEOMACS-HEADER\")",
    );
    settle_after_eval(&mut gnu, &mut neo);

    let neo_has_header = neo
        .text_grid()
        .iter()
        .any(|row| row.contains("NEOMACS-HEADER"));
    let gnu_has_header = gnu
        .text_grid()
        .iter()
        .any(|row| row.contains("NEOMACS-HEADER"));
    if !neo_has_header || !gnu_has_header {
        dump_pair_grids("header-line after setq", &gnu, &neo);
    }
    assert!(
        gnu_has_header,
        "GNU should show the installed header line without an extra keystroke"
    );
    assert!(
        neo_has_header,
        "Neomacs should show the installed header line without an extra keystroke \
         (redisplay must not be dropped on a header-line-format change)"
    );
    assert_pair_exact_display("header-line repaint without extra keystroke", &gnu, &neo);
}
