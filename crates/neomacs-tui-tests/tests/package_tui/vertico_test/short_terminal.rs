//! A terminal too short for both windows: which one gives up its rows.
//!
//! The candidate window wants `vertico-count` rows plus the prompt, and the
//! main window above it has a minimum height of its own (`window-min-height`,
//! four rows). A terminal shorter than the two together is the case where
//! something has to give, and the editors do not agree on what:
//!
//! * GNU keeps the main window at its minimum and takes the rest off the
//!   candidate window, so at fourteen rows the candidates are eight;
//! * Neomacs keeps the candidate window full and shrinks the main window to its
//!   mode line, so at fourteen rows the candidates are ten.
//!
//! The scenario resizes to fifteen rows and then to fourteen, checks GNU's
//! layout at each (the main window's mode line, the candidate rows under it),
//! and freezes the fourteen-row screen from GNU. The comparison against Neomacs
//! is what fails -- the candidate window holds two rows more than GNU's, and
//! the main window is two rows shorter -- and that failure is the point of this
//! scenario. It is not to be re-blessed away, and the frozen files must not be
//! re-blessed from Neomacs.
//!
//! The session runs over the suite's one-line buffer rather than over
//! *scratch*: a window that is four rows tall cannot show *scratch*'s four
//! lines and its mode line at the same time, and which end of the buffer the
//! two editors scroll to is a divergence of its own (see the resize scenario).
//! The one-line buffer leaves the main window's contents identical in both
//! editors, so what the screens disagree about is the rows, not the text.

use std::time::Duration;

use expect_test::expect_file;

use super::harness::*;
use super::prelude::*;

/// The fixture's buffer prefix, which filters `C-x b` down to those buffers.
const PREFIX: &str = "facet-";

/// The `C-x b` prompt, whose row holds the count indicator and the input.
const PROMPT: &str = "Switch to buffer";

/// The fixture's candidate count -- more than any of these windows holds.
const TOTAL: usize = 14;

/// The terminal the scenario starts on.
const START: (u16, u16) = (30, 100);

/// GNU's layout at each height below, as `(mode line row, candidate rows)`.
///
/// The main window keeps its minimum height and the candidate window gets what
/// is left of the terminal after the menu bar, the main window and the echo
/// area: at fifteen rows there are nine candidates left for it, and at fourteen
/// rows eight.
const FIFTEEN_ROWS: (u16, usize) = (4, 9);
const FOURTEEN_ROWS: (u16, usize) = (4, 8);

pub(super) fn run() {
    let prelude = default_vertico(&format!(
        "{}{}",
        fixture_buffers(&numbered_names("facet", TOTAL)),
        VERTICO_HELD_PRELUDE
    ));
    let mut pair = spawn_pair("vertico-short-terminal", &prelude, START.0, START.1);
    let outcome =
        catch_phase("Vertico short terminal scenario", || run_body(&mut pair)).and_then(|r| r);
    assert!(outcome.is_ok(), "{}", outcome.err().unwrap_or_default());
}

fn run_body(pair: &mut PackageTuiPair) -> Result<(), String> {
    both(pair, "show the held buffer", |session| {
        session.send_keys("C-x b");
        wait_for(
            session,
            Duration::from_secs(8),
            "the switch-to-buffer prompt",
            |grid| grid.iter().any(|row| row.contains(PROMPT)),
        );
        session.send(format!("{VERTICO_HELD_BUFFER}\r").as_bytes());
        wait_for(session, Duration::from_secs(8), "the held buffer", |grid| {
            grid.iter().any(|row| row.contains("held buffer line"))
        });
    })?;
    both(pair, "open switch-to-buffer", |session| {
        session.send_keys("C-x b");
        wait_for(
            session,
            Duration::from_secs(8),
            "the switch-to-buffer prompt",
            |grid| grid.iter().any(|row| row.contains(PROMPT)),
        );
    })?;
    both(pair, "type the fixture prefix", |session| {
        session.send(b"facet-");
        wait_for(
            session,
            Duration::from_secs(8),
            "a full candidate window",
            |grid| candidate_rows(grid, PREFIX).len() == 10,
        );
    })?;
    assert_candidates_start_under_mode_line(pair, PREFIX, PROMPT);

    // Fifteen rows: already short enough for the candidate window to give a row
    // back rather than push the main window under its minimum height.
    resize(pair, (15, START.1))?;
    assert_gnu_layout(pair, FIFTEEN_ROWS, "fifteen rows");

    // Fourteen rows: one row fewer still, and the screen that is frozen.
    resize(pair, (14, START.1))?;
    assert_gnu_layout(pair, FOURTEEN_ROWS, "fourteen rows");
    record_checkpoint(
        pair,
        "Vertico short terminal",
        expect_file!["snapshots/short_terminal_ansi_grid.ansi"],
        expect_file!["snapshots/short_terminal_plain_grid.plain"],
    );
    Ok(())
}

/// Resize both peers and wait for the layout the new geometry must produce.
fn resize(pair: &mut PackageTuiPair, (rows, columns): (u16, u16)) -> Result<(), String> {
    pair.resize_both(rows, columns);
    both(pair, &format!("resize to {rows}x{columns}"), |session| {
        wait_for(
            session,
            Duration::from_secs(8),
            "the reflowed candidate window",
            |grid| {
                let Some(prompt) = prompt_row_in(grid) else {
                    return false;
                };
                grid[usize::from(prompt)].contains(PROMPT)
                    && !candidate_rows(grid, PREFIX).is_empty()
            },
        );
    })
}

/// GNU's layout at the current geometry: the main window keeps its minimum
/// height -- its mode line is where `expected` says -- and the candidate window
/// holds `expected`'s candidate rows.
fn assert_gnu_layout(pair: &PackageTuiPair, expected: (u16, usize), description: &str) {
    let grid = pair.gnu.text_grid();
    assert_eq!(
        mode_line_row_in(&grid),
        Some(expected.0),
        "GNU's main window is not at its minimum height at {description}:\n{}",
        grid.join("\n")
    );
    assert_eq!(
        candidate_rows(&grid, PREFIX).len(),
        expected.1,
        "GNU's candidate window does not hold its share of the rows at {description}:\n{}",
        grid.join("\n")
    );
}
