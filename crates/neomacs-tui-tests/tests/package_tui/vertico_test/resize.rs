//! What the candidate window does when the terminal changes size under it.
//!
//! Resizing the terminal while a completion session is open is a redraw the
//! editors own, and the candidate window is what has to give way: it wants
//! `vertico-count` rows plus the prompt, so a shorter terminal is the main
//! window's rows that go first, and a taller one does not buy the candidate
//! window any more rows than `vertico-count`. Width does not move it at all
//! while the candidates fit their rows: the window is as tall as
//! `vertico-count`, not as wide as the frame.
//!
//! Two things the editors do not agree on are therefore kept out of the
//! screens: how short the terminal has to be before the *candidate* window
//! itself gives rows back, and what the window above it shows once the terminal
//! is too narrow for its buffer's lines.
//!
//! The first: GNU keeps the main window at `window-min-height` (four rows) and
//! takes the rest off the candidate window, while Neomacs lets the main window
//! shrink to its mode line and keeps the candidate window full. At fourteen
//! rows that is eight candidates against ten.
//!
//! The second: the session here runs over a one-line fixture buffer rather than
//! over *scratch*, because narrowing the terminal wraps *scratch*'s long
//! comment lines, and GNU then scrolls that window to the bottom of the buffer
//! (`Bot L4`) where Neomacs keeps it at the top showing all of it (`All L4`).
//! Both are real differences, reported rather than frozen.
//!
//! Each resize is followed by a wait for the layout it must produce -- the
//! candidate rows back, the prompt row directly under the main window's mode
//! line -- before anything is compared, so a half-drawn screen is never the
//! thing that gets frozen.

use std::time::Duration;

use expect_test::expect_file;

use super::harness::*;
use super::prelude::*;

/// The fixture's buffer prefix, which filters `C-x b` down to those buffers.
const PREFIX: &str = "facet-";

/// The `C-x b` prompt, whose row holds the count indicator and the input.
const PROMPT: &str = "Switch to buffer";

/// The fixture's candidate count: more than the window holds.
const TOTAL: usize = 14;

/// How many candidate rows the window holds: `vertico-count`'s default.
const WINDOW: usize = 10;

/// The terminal the scenario starts on.
const START: (u16, u16) = (30, 100);

pub(super) fn run() {
    let prelude = default_vertico(&format!(
        "{}{}",
        fixture_buffers(&numbered_names("facet", TOTAL)),
        VERTICO_HELD_PRELUDE
    ));
    let mut pair = spawn_pair("vertico-resize", &prelude, START.0, START.1);
    let outcome =
        catch_phase("Vertico resize scenario", || run_body(&mut pair)).and_then(|result| result);
    assert!(outcome.is_ok(), "{}", outcome.err().unwrap_or_default());
}

fn run_body(pair: &mut PackageTuiPair) -> Result<(), String> {
    // The session runs over the one-line buffer, not over *scratch*: narrowing
    // the terminal wraps *scratch*'s long comment lines, and the two editors
    // scroll their windows to different places when it happens. That is a
    // finding of its own (see the module documentation), not something this
    // scenario's screens should pin.
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
            |grid| candidate_rows(grid, PREFIX).len() == WINDOW,
        );
    })?;

    assert_candidate_state(pair, PREFIX, PROMPT, "1/14", Some("facet-01"));
    assert_visible(pair, WINDOW);
    assert_candidates_start_under_mode_line(pair, PREFIX, PROMPT);
    record_checkpoint(
        pair,
        "Vertico resize start terminal",
        expect_file!["snapshots/resize_start_terminal_ansi_grid.ansi"],
        expect_file!["snapshots/resize_start_terminal_plain_grid.plain"],
    );

    // Shorter: the rows go to the candidate window first, so it is unchanged
    // and the main window above it is what is left.
    resize(pair, (16, START.1))?;
    assert_candidate_state(pair, PREFIX, PROMPT, "1/14", Some("facet-01"));
    assert_visible(pair, WINDOW);
    assert_candidates_start_under_mode_line(pair, PREFIX, PROMPT);
    record_checkpoint(
        pair,
        "Vertico resize short terminal",
        expect_file!["snapshots/resize_short_terminal_ansi_grid.ansi"],
        expect_file!["snapshots/resize_short_terminal_plain_grid.plain"],
    );

    // Narrower, at the height it started on: the candidates fit their rows at
    // both widths, so the window is the same window.
    resize(pair, (START.0, 60))?;
    assert_candidate_state(pair, PREFIX, PROMPT, "1/14", Some("facet-01"));
    assert_visible(pair, WINDOW);
    assert_candidates_start_under_mode_line(pair, PREFIX, PROMPT);
    record_checkpoint(
        pair,
        "Vertico resize narrow terminal",
        expect_file!["snapshots/resize_narrow_terminal_ansi_grid.ansi"],
        expect_file!["snapshots/resize_narrow_terminal_plain_grid.plain"],
    );

    // Taller and wider than it started: the window is `vertico-count` rows, so
    // it does not grow past them.
    resize(pair, (START.0 + 10, START.1 + 20))?;
    assert_candidate_state(pair, PREFIX, PROMPT, "1/14", Some("facet-01"));
    assert_visible(pair, WINDOW);
    assert_candidates_start_under_mode_line(pair, PREFIX, PROMPT);

    // And back to the terminal it started on, with the selection walked to the
    // end of the list first: the window it reflows to still has to hold the
    // candidate that is selected.
    for number in 2..=TOTAL {
        let count = format!("{number}/{TOTAL}");
        let candidate = format!("facet-{number:02}");
        press_and_wait_for_count(pair, "C-n", PROMPT, PREFIX, &count, Some(&candidate))?;
    }
    resize(pair, START)?;
    assert_candidate_state(pair, PREFIX, PROMPT, "14/14", Some("facet-14"));
    assert_visible(pair, WINDOW);
    assert_candidates_start_under_mode_line(pair, PREFIX, PROMPT);
    Ok(())
}

/// Resize both peers and wait for the layout the new geometry must produce.
///
/// A resize can land in the middle of a redraw, so the wait is for the finished
/// layout: the prompt row directly under the main window's mode line, which is
/// where the grown minibuffer window starts, and the candidate rows starting
/// directly under it.
fn resize(pair: &mut PackageTuiPair, (rows, columns): (u16, u16)) -> Result<(), String> {
    pair.resize_both(rows, columns);
    both(pair, &format!("resize to {rows}x{columns}"), |session| {
        wait_for(
            session,
            Duration::from_secs(8),
            "the reflowed screen",
            |grid| {
                let Some(prompt) = prompt_row_in(grid) else {
                    return false;
                };
                grid[usize::from(prompt)].contains(PROMPT)
                    && candidate_rows(grid, PREFIX).first().copied() == Some(prompt + 1)
            },
        );
    })
}

/// How many candidate rows the window holds now.
fn visible(pair: &PackageTuiPair) -> usize {
    let gnu = candidate_rows(&pair.gnu.text_grid(), PREFIX).len();
    assert_eq!(
        gnu,
        candidate_rows(&pair.neo.text_grid(), PREFIX).len(),
        "the two editors hold different numbers of candidate rows"
    );
    gnu
}

/// The window holds this many candidate rows in both peers.
fn assert_visible(pair: &PackageTuiPair, expected: usize) {
    assert_eq!(
        visible(pair),
        expected,
        "the candidate window does not hold the rows it should"
    );
}
