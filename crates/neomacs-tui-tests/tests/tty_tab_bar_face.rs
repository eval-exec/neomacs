#![cfg(unix)]
//! Issue #856 (TUI): a reserved tab-bar row on a TTY frame must be painted with
//! the `tab-bar` face across the full frame width even when `tab-bar-format` is
//! `nil` or yields no items.
//!
//! GNU Emacs 31.1 (`src/xdisp.c:21546-21547,14671-14761`):
//! - On a TTY frame (`!FRAME_WINDOW_P (f)`), whenever `FRAME_TAB_BAR_LINES (f) > 0`,
//!   `redisplay_window` unconditionally calls `display_tab_bar (w)`.
//! - `display_tab_bar` initializes its iterator with `TAB_BAR_FACE_ID` and
//!   unconditionally fills the rest of the row with spaces (`display_string ("", ..., -1)`),
//!   painting all columns `0..FRAME_COLS (f)` in the `tab-bar` face even when
//!   `n_tab_bar_items == 0`.

use crate::support::{boot_pair, eval_expression, settle_session, wait_for_both};
use neomacs_tui_tests::{
    RawTerminalSnapshot, TuiSession, assert_raw_terminal_snapshots_eq, color_diffs_in,
    format_color_diffs,
};
use std::time::Duration;

const WAIT_CAP: Duration = Duration::from_secs(15);

fn eval_and_wait_message(gnu: &mut TuiSession, neo: &mut TuiSession, form: &str, marker: &str) {
    let expr = format!("(progn {form} (message {marker:?}))");
    eval_expression(gnu, neo, &expr);
    wait_for_both(gnu, neo, WAIT_CAP, |grid| {
        grid.last().is_some_and(|row| row.contains(marker))
    });
    assert!(
        gnu.text_grid()
            .last()
            .is_some_and(|row| row.contains(marker)),
        "GNU did not display marker {marker:?}:\n{}",
        gnu.text_grid().join("\n")
    );
    assert!(
        neo.text_grid()
            .last()
            .is_some_and(|row| row.contains(marker)),
        "Neomacs did not display marker {marker:?}:\n{}",
        neo.text_grid().join("\n")
    );
    settle_session(gnu);
    settle_session(neo);
}

fn assert_row_matches_gnu_cells(label: &str, gnu: &TuiSession, neo: &TuiSession, row: u16) {
    let (_, cols) = gnu.screen_size();
    let color_diffs = color_diffs_in(gnu.screen(), neo.screen(), row..row + 1, 0..cols);
    assert!(
        color_diffs.is_empty(),
        "{label}: row {row} cell colors diverged from GNU 31.1:\n{}",
        format_color_diffs(&color_diffs, 16)
    );
    let gnu_snap = RawTerminalSnapshot::capture_rows(gnu.screen(), row..row + 1);
    let neo_snap = RawTerminalSnapshot::capture_rows(neo.screen(), row..row + 1);
    assert_raw_terminal_snapshots_eq(label, &gnu_snap, &neo_snap);
}

fn assert_empty_tab_bar_row_painted(label: &str, gnu: &TuiSession, neo: &TuiSession, row: u16) {
    let (_, cols) = gnu.screen_size();
    assert!(
        gnu.row_text(row).trim().is_empty(),
        "{label}: expected GNU row {row} to be blank, got {:?}",
        gnu.row_text(row)
    );
    assert!(
        neo.row_text(row).trim().is_empty(),
        "{label}: expected Neomacs row {row} to be blank, got {:?}",
        neo.row_text(row)
    );
    let gnu_bg = gnu
        .screen()
        .cell(row, 0)
        .expect("GNU tab-bar cell (row, 0)")
        .bgcolor();
    assert_ne!(
        gnu_bg,
        vt100::Color::Default,
        "{label}: expected GNU empty tab-bar row {row} to carry a non-default tab-bar background"
    );
    for col in 0..cols {
        let g = gnu.screen().cell(row, col).expect("GNU tab-bar cell");
        let n = neo.screen().cell(row, col).expect("Neomacs tab-bar cell");
        assert_eq!(
            g.bgcolor(),
            gnu_bg,
            "{label}: GNU row {row} col {col} background {:?} != {gnu_bg:?}",
            g.bgcolor()
        );
        assert_eq!(
            n.bgcolor(),
            g.bgcolor(),
            "{label}: Neomacs row {row} col {col} background {:?} != GNU {:?}",
            n.bgcolor(),
            g.bgcolor()
        );
    }
    assert_row_matches_gnu_cells(label, gnu, neo, row);
}

#[test]
fn tty_empty_tab_bar_paints_tab_bar_face_like_gnu() {
    let (mut gnu, mut neo) = boot_pair("");

    // 1. Show: tab-bar-mode enabled with default tab-bar-format (row 1 under menu bar).
    eval_and_wait_message(
        &mut gnu,
        &mut neo,
        "(tab-bar-mode 1) (setq my-tab-bar-backup tab-bar-format)",
        "tab-bar-show-ready",
    );
    assert_row_matches_gnu_cells("tab-bar show (row 1)", &gnu, &neo, 1);

    // 2. Hide: tab-bar-format set to nil. On a TTY frame the row stays reserved
    //    at row 1 (top window edge stays 2) and must still be painted with the
    //    `tab-bar` face across all columns, matching GNU `display_tab_bar`.
    eval_and_wait_message(
        &mut gnu,
        &mut neo,
        "(setq tab-bar-format nil) (tab-bar--update-tab-bar-lines) \
         (unless (= (nth 1 (window-edges)) 2) \
           (error \"expected top edge 2 with menu-bar + empty tab-bar, got %S\" (window-edges)))",
        "tab-bar-hide-ready",
    );
    assert_empty_tab_bar_row_painted("tab-bar hide format=nil (row 1)", &gnu, &neo, 1);

    // 3. Empty-string format item + explicit `grey85` background (matching the
    //    reporter's light-theme face value in #856): non-nil tab-bar-format
    //    whose items produce "".
    eval_and_wait_message(
        &mut gnu,
        &mut neo,
        "(set-face-background 'tab-bar \"grey85\") \
         (setq tab-bar-format (list (lambda () \"\"))) \
         (tab-bar--update-tab-bar-lines)",
        "tab-bar-empty-item-ready",
    );
    assert_empty_tab_bar_row_painted("tab-bar empty-string item grey85 (row 1)", &gnu, &neo, 1);

    // 4. Restore: tab-bar-format restored shows tabs identically to GNU.
    eval_and_wait_message(
        &mut gnu,
        &mut neo,
        "(setq tab-bar-format my-tab-bar-backup) (tab-bar--update-tab-bar-lines)",
        "tab-bar-restore-ready",
    );
    assert_row_matches_gnu_cells("tab-bar restored (row 1)", &gnu, &neo, 1);

    // 5. Menu bar off + empty tab-bar: reserved row shifts to row 0 and still
    //    paints the `tab-bar` face across all columns, while *scratch* shifts to row 1.
    eval_and_wait_message(
        &mut gnu,
        &mut neo,
        "(menu-bar-mode -1) (setq tab-bar-format nil) (tab-bar--update-tab-bar-lines) (redisplay t) \
         (unless (= (window-pixel-top (frame-root-window)) 1) \
           (error \"expected root window top 1 with menu-bar off + empty tab-bar, got %S\" \
                  (window-pixel-top (frame-root-window))))",
        "tab-bar-nomenu-ready",
    );
    assert_empty_tab_bar_row_painted("tab-bar hide with menu-bar off (row 0)", &gnu, &neo, 0);
    assert_row_matches_gnu_cells(
        "tab-bar hide with menu-bar off (*scratch* row 1)",
        &gnu,
        &neo,
        1,
    );

    // 6. Tab-bar disabled: row 0 reverts to ordinary *scratch* buffer content
    //    with default background (no stale tab-bar band).
    eval_and_wait_message(&mut gnu, &mut neo, "(tab-bar-mode -1)", "tab-bar-off-ready");
    assert_eq!(
        neo.screen()
            .cell(0, 0)
            .expect("Neomacs row 0 col 0")
            .bgcolor(),
        vt100::Color::Default,
        "tab-bar disabled: row 0 must revert to default background"
    );
    assert_row_matches_gnu_cells("tab-bar disabled (row 0)", &gnu, &neo, 0);
}
