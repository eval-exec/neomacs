//! More candidates than the window holds: which ones are visible, and where the
//! window scrolls to keep the current one in view.
//!
//! The window holds `vertico-count` candidates and this scenario does not set
//! it, so it holds the ten the package defaults to; the fixture is fourteen
//! buffers. The candidate window is therefore full from the first screen, the
//! main window above it is ten rows shorter for it, and moving the selection
//! down has to scroll the window rather than grow it -- by one candidate per
//! step once the selection comes within `vertico-scroll-margin` (two rows) of
//! the window's bottom edge, and no further once the last candidate is on
//! screen.
//!
//! The rows below the prompt are compared with GNU's at every step, and the
//! ends of the visible window, the count indicator, the current candidate and
//! where the window sits on the screen are each checked against what the
//! arithmetic says they must be.

use std::time::Duration;

use expect_test::expect_file;

use super::harness::*;
use super::prelude::*;

/// The fixture's buffer prefix, which filters `C-x b` down to those buffers.
const PREFIX: &str = "facet-";

/// The `C-x b` prompt, whose row holds the count indicator and the input.
const PROMPT: &str = "Switch to buffer";

/// The fixture's candidate count -- more than the window, which is the point.
const TOTAL: usize = 14;

/// The terminal this scenario renders on.
const ROWS: u16 = 24;
const COLUMNS: u16 = 80;

pub(super) fn run() {
    let prelude = default_vertico(&fixture_buffers(&numbered_names("facet", TOTAL)));
    let mut pair = spawn_pair("vertico-overflow", &prelude, ROWS, COLUMNS);
    let outcome =
        catch_phase("Vertico overflow scenario", || run_body(&mut pair)).and_then(|result| result);
    assert!(outcome.is_ok(), "{}", outcome.err().unwrap_or_default());
}

fn run_body(pair: &mut PackageTuiPair) -> Result<(), String> {
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
            |grid| candidate_rows(grid, PREFIX).len() == usize::from(WINDOW),
        );
    })?;

    // The window is full and the main window above it is that much shorter: the
    // mode line has moved up by the prompt row and the ten candidate rows.
    assert_candidate_state(pair, PREFIX, PROMPT, "1/14", Some("facet-01"));
    assert_window(pair, "facet-01", "facet-10");
    assert_candidates_start_under_mode_line(pair, PREFIX, PROMPT);
    record_checkpoint(
        pair,
        "Vertico overflow first screen",
        expect_file!["snapshots/overflow_first_screen_ansi_grid.ansi"],
        expect_file!["snapshots/overflow_first_screen_plain_grid.plain"],
    );

    // The selection walks down inside the window. The first seven steps leave
    // the window exactly where it was, because the selection is still more than
    // two rows above the window's bottom edge.
    for number in 2..=8 {
        step(pair, number, ("facet-01", "facet-10"))?;
    }

    // From the ninth candidate on, the selection is within that margin, and the
    // window follows it down one candidate per step.
    step(pair, 9, ("facet-02", "facet-11"))?;
    step(pair, 10, ("facet-03", "facet-12"))?;
    step(pair, 11, ("facet-04", "facet-13"))?;
    record_checkpoint(
        pair,
        "Vertico overflow scrolled window",
        expect_file!["snapshots/overflow_scrolled_window_ansi_grid.ansi"],
        expect_file!["snapshots/overflow_scrolled_window_plain_grid.plain"],
    );

    // The next step brings the window's bottom edge to the end of the list: it
    // can only move far enough to show the last candidate, and then stops.
    step(pair, 12, ("facet-05", "facet-14"))?;
    step(pair, 13, ("facet-05", "facet-14"))?;
    step(pair, 14, ("facet-05", "facet-14"))?;
    record_checkpoint(
        pair,
        "Vertico overflow last candidate",
        expect_file!["snapshots/overflow_last_candidate_ansi_grid.ansi"],
        expect_file!["snapshots/overflow_last_candidate_plain_grid.plain"],
    );

    // At the end of the list, with cycling off (`vertico-cycle` is at its
    // default nil here), the selection stays where it is.
    step(pair, 14, ("facet-05", "facet-14"))?;
    Ok(())
}

/// Move the selection to fixture candidate `number` and check the window it
/// leaves behind: the ends of the visible window are the package's own account
/// of where it scrolled to.
fn step(pair: &mut PackageTuiPair, number: usize, window: (&str, &str)) -> Result<(), String> {
    let count = format!("{number}/{TOTAL}");
    let candidate = format!("facet-{number:02}");
    press_and_wait_for_count(pair, "C-n", PROMPT, PREFIX, &count, Some(&candidate))?;
    assert_window(pair, window.0, window.1);
    Ok(())
}

/// How many candidate rows the window holds: `vertico-count`'s default.
const WINDOW: u16 = 10;

/// The ends of the visible candidate window, which is what scrolling moves.
fn assert_window(pair: &PackageTuiPair, first: &str, last: &str) {
    let texts = candidate_texts(&pair.gnu.text_grid(), PREFIX);
    assert_eq!(
        texts.len(),
        usize::from(WINDOW),
        "the window holds vertico-count candidates: {texts:?}"
    );
    assert_eq!(
        (
            texts.first().map(String::as_str),
            texts.last().map(String::as_str)
        ),
        (Some(first), Some(last)),
        "the visible candidate window is not the expected one"
    );
    assert_eq!(
        candidate_texts(&pair.neo.text_grid(), PREFIX),
        texts,
        "the visible candidate window differs from GNU's"
    );
}
