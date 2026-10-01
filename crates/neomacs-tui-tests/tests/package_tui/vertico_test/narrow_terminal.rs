//! What narrowing the terminal does to the window above the candidate window.
//!
//! The suite's resize scenario drives the same resize over a one-line buffer,
//! and says why: *scratch*'s two long comment lines are wider than a narrow
//! window, so a narrowing terminal re-wraps them, and the two editors put the
//! window's start somewhere different once they have. GNU re-wraps and, with
//! the wrapped text no longer fitting the rows the window has, scrolls the
//! window to the bottom of the buffer, which its mode line reports as `Bot`.
//! Neomacs leaves the window where it was and reports the whole buffer as
//! visible, `All`.
//!
//! This scenario is that case rather than a scenario that avoids it: the
//! session runs over the *scratch* the scenario booted into, the terminal is
//! narrowed with a full candidate window open, and GNU's mode line -- `Bot L4`,
//! the bottom of a four-line buffer -- is asserted before the screen is frozen
//! from GNU. The comparison against Neomacs is what fails, and that failure is
//! the point of this scenario: it is not to be re-blessed away, and the frozen
//! files must not be re-blessed from Neomacs.

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

/// The terminal the scenario starts on, and the narrower one it freezes. The
/// height does not change: what the frozen screen is about is the width.
const START: (u16, u16) = (24, 100);
const NARROW: (u16, u16) = (18, 60);

/// What GNU's main window reports once the terminal is narrow: the buffer's
/// bottom, out of the four lines *scratch* holds.
const GNU_MODE_LINE: &str = "Bot L4";

/// What the whole of *scratch* weighs in rows once the window is sixty columns
/// wide: its two comment lines wrap to two rows each, and its two empty lines
/// stay one row each.
const WRAPPED_ROWS: usize = 6;

pub(super) fn run() {
    let prelude = default_vertico(&fixture_buffers(&numbered_names("facet", TOTAL)));
    let mut pair = spawn_pair("vertico-narrow-terminal", &prelude, START.0, START.1);
    let outcome =
        catch_phase("Vertico narrow terminal scenario", || run_body(&mut pair)).and_then(|r| r);
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
            |grid| candidate_rows(grid, PREFIX).len() == 10,
        );
    })?;

    pair.resize_both(NARROW.0, NARROW.1);
    both(
        pair,
        &format!("resize to {}x{}", NARROW.0, NARROW.1),
        |session| {
            wait_for(
                session,
                Duration::from_secs(8),
                "the reflowed candidate window",
                |grid| {
                    let Some(prompt) = prompt_row_in(grid) else {
                        return false;
                    };
                    grid[usize::from(prompt)].contains(PROMPT)
                        && candidate_rows(grid, PREFIX).len() == 10
                },
            );
        },
    )?;

    // GNU's window above the candidate window shows the bottom of *scratch*,
    // whose two comment lines the narrowing wrapped past the rows it has.
    let mode_line = mode_line(&pair.gnu);
    assert!(
        mode_line.contains(GNU_MODE_LINE),
        "GNU's mode line does not report {GNU_MODE_LINE:?} after the narrowing: {mode_line:?}"
    );

    record_checkpoint(
        pair,
        "Vertico narrow terminal",
        expect_file!["snapshots/narrow_terminal_ansi_grid.ansi"],
        expect_file!["snapshots/narrow_terminal_plain_grid.plain"],
    );
    Ok(())
}
