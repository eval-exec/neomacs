//! `vertico-multiform-buffer` (`M-B`): the candidates in a window of their own.
//!
//! The buffer display moves the candidates out of the minibuffer window and
//! into a window above it, with `vertico-count` recomputed from that window's
//! height, so the layout is a window the package creates rather than the
//! minibuffer's own. The candidates, the current one and the count indicator
//! are the same package state the other display modes show, and this scenario
//! checks them against GNU's and freezes the screen.
//!
//! The screen is where the divergence is: GNU writes the blank cells that fill
//! each candidate row out to the window's edge, and Neomacs moves the cursor
//! over them and leaves them unwritten. The visible text is the same, so the
//! resolved display matches and only the terminal state differs -- which is
//! what the raw-terminal checkpoint below compares, cell by cell, and what
//! makes this scenario red.
//!
//! The frozen files are blessed from GNU's screen: GNU's terminal state is the
//! behaviour being asserted, and Neomacs' is the divergence. That failure is
//! the point of this scenario -- it is not to be re-blessed away, and the
//! frozen files must not be re-blessed from Neomacs.

use std::time::Duration;

use expect_test::expect_file;

use super::harness::*;
use super::prelude::*;

/// The fixture's buffer prefix, which filters `C-x b` down to those buffers.
const PREFIX: &str = "facet-";

/// The `C-x b` prompt, whose row holds the count indicator and the input.
const PROMPT: &str = "Switch to buffer";

/// The fixture's candidate count: more than the candidate window holds.
const TOTAL: usize = 30;

/// The terminal this scenario renders on.
const ROWS: u16 = 30;
const COLUMNS: u16 = 100;

/// How many candidate rows the minibuffer window holds before the toggle:
/// `vertico-count`'s default, which the prelude leaves alone.
const WINDOW: u16 = 10;

/// How many candidate rows the window the buffer display creates holds: the
/// height that window is given, less its mode line.
const BUFFER_WINDOW: usize = 12;

pub(super) fn run() {
    let prelude = multiform_buffer_vertico(&fixture_buffers(&numbered_names("facet", TOTAL)));
    let mut pair = spawn_pair("vertico-multiform-buffer", &prelude, ROWS, COLUMNS);
    let outcome =
        catch_phase("Vertico multiform buffer scenario", || run_body(&mut pair)).and_then(|r| r);
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
    assert_candidate_state(pair, PREFIX, PROMPT, "1/30", Some("facet-01"));

    // `M-B` is `vertico-multiform-buffer`'s key. The window it creates has a
    // mode line of its own, which carries the prompt the minibuffer is reading
    // for, so the screen holds that prompt twice once the toggle has happened.
    both(pair, "toggle the buffer display", |session| {
        session.send_key("M-B");
        wait_for(
            session,
            Duration::from_secs(8),
            "the candidate window and its mode line",
            |grid| {
                grid.iter()
                    .any(|row| row.contains(PROMPT) && !row.contains(": "))
            },
        );
    })?;

    // The candidate window holds every candidate the buffer display shows, in
    // the order the package sorted them, and the same one is current.
    assert_candidate_state(pair, PREFIX, PROMPT, "1/30", Some("facet-01"));
    assert_eq!(
        candidate_rows(&pair.gnu.text_grid(), PREFIX).len(),
        BUFFER_WINDOW,
        "GNU's candidate window does not hold the rows its own height gives it"
    );

    // GNU's terminal state for the whole screen, blessed from GNU.
    record_checkpoint(
        pair,
        "Vertico multiform buffer",
        expect_file!["snapshots/multiform_buffer_ansi_grid.ansi"],
        expect_file!["snapshots/multiform_buffer_plain_grid.plain"],
    );
    Ok(())
}
