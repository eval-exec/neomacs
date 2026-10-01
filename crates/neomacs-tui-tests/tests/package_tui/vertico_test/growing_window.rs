//! `vertico-resize`: whether the minibuffer window follows its candidate list.
//!
//! The candidate window is as tall as the candidates it has, up to
//! `vertico-count`, and whether it gives those rows back as the list narrows is
//! `vertico-resize`, which starts at `resize-mini-windows` -- `grow-only`, so
//! the rows a window has grown to stay its own for the rest of the session.
//! With `vertico-resize` set to shrink and grow, the window follows the list in
//! both directions.
//!
//! Where the window's height is visible off the candidate rows themselves is
//! the main window's mode line: it sits above the minibuffer window, so every
//! row the candidate window gives back moves the mode line one row down the
//! screen. The scenario types the fixture's prefix, narrows the list, and reads
//! that row at every step, freezing the screens it passes through on the way.

use std::time::Duration;

use expect_test::expect_file;

use super::harness::*;
use super::prelude::*;

/// The fixture's buffer prefix, which filters `C-x b` down to those buffers.
const PREFIX: &str = "grow-";

/// The `C-x b` prompt, whose row holds the count indicator and the input.
const PROMPT: &str = "Switch to buffer";

/// The fixture's candidate count: more than the window holds.
const TOTAL: usize = 12;

/// How many candidate rows the window holds at most: `vertico-count`'s default.
const WINDOW: usize = 10;

/// The terminal this scenario renders on.
const ROWS: u16 = 30;
const COLUMNS: u16 = 100;

pub(super) fn run() {
    let prelude = default_vertico(&fixture_buffers(&numbered_names("grow", TOTAL)));
    let mut pair = spawn_pair("vertico-growing-window", &prelude, ROWS, COLUMNS);
    let outcome =
        catch_phase("Vertico growing window scenario", || run_body(&mut pair)).and_then(|r| r);
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

    // The prefix alone: the window is as tall as the package lets it be.
    type_and_wait(pair, b"grow-", WINDOW)?;
    let full = step(pair, WINDOW, "the full window")?;
    record_checkpoint(
        pair,
        "Vertico growing window full",
        expect_file!["snapshots/growing_window_full_ansi_grid.ansi"],
        expect_file!["snapshots/growing_window_full_plain_grid.ansi"],
    );

    // Narrowing the list does not shorten the window: `vertico-resize` starts
    // at `resize-mini-windows`, which is `grow-only`, so the rows a window has
    // grown to are its own until the session ends.
    type_and_wait(pair, b"1", 3)?;
    let grown_only = step(pair, 3, "the narrowed window")?;
    assert_eq!(
        grown_only, full,
        "the default window gave rows back when the candidate list narrowed"
    );
    record_checkpoint(
        pair,
        "Vertico growing window narrowed",
        expect_file!["snapshots/growing_window_narrowed_ansi_grid.ansi"],
        expect_file!["snapshots/growing_window_narrowed_plain_grid.ansi"],
    );
    both(pair, "abandon the session", |session| {
        session.send_key("C-g");
        wait_for(
            session,
            Duration::from_secs(8),
            "the minibuffer to close",
            |grid| !grid.iter().any(|row| row.contains(PROMPT)),
        );
    })?;

    // `vertico-resize` set to shrink and grow at the keyboard: now the rows do
    // follow the list, down as it narrows and back up as it widens.
    eval_resize(pair)?;
    open_session(pair)?;
    type_and_wait(pair, b"grow-", WINDOW)?;
    let again = step(pair, WINDOW, "the full window under resize")?;
    assert_eq!(again, full, "the window is not where it was");
    type_and_wait(pair, b"1", 3)?;
    let shrunk = step(pair, 3, "the narrowed window under resize")?;
    assert_eq!(
        shrunk,
        full + (WINDOW - 3) as u16,
        "the window did not give back the rows the candidate list lost"
    );
    record_checkpoint(
        pair,
        "Vertico growing window resized",
        expect_file!["snapshots/growing_window_resized_ansi_grid.ansi"],
        expect_file!["snapshots/growing_window_resized_plain_grid.ansi"],
    );

    // And back: deleting the character widens the list, and the window with it.
    both(pair, "delete the last character", |session| {
        session.send_key("DEL");
        wait_for(
            session,
            Duration::from_secs(8),
            "the fixture's candidates",
            |grid| candidate_rows(grid, PREFIX).len() == WINDOW,
        );
    })?;
    let rewidened = step(pair, WINDOW, "the re-widened window")?;
    assert_eq!(
        rewidened, full,
        "the window did not take the rows back when the candidate list widened"
    );
    Ok(())
}

/// Evaluate `(setq vertico-resize t)` at the keyboard in both peers.
fn eval_resize(pair: &mut PackageTuiPair) -> Result<(), String> {
    both(pair, "let the window shrink and grow", |session| {
        invoke(session, "eval-expression", "Eval: ");
        session.send(b"(setq vertico-resize t)");
        session.send_key("RET");
        wait_for(
            session,
            Duration::from_secs(8),
            "the eval minibuffer to close",
            |grid| !grid.iter().any(|row| row.contains("Eval: ")),
        );
    })
}

/// Open the fixture's completion session.
fn open_session(pair: &mut PackageTuiPair) -> Result<(), String> {
    both(pair, "open switch-to-buffer", |session| {
        session.send_keys("C-x b");
        wait_for(
            session,
            Duration::from_secs(8),
            "the switch-to-buffer prompt",
            |grid| grid.iter().any(|row| row.contains(PROMPT)),
        );
    })
}

/// Type `input` and wait for the window to hold `expected` candidate rows.
fn type_and_wait(pair: &mut PackageTuiPair, input: &[u8], expected: usize) -> Result<(), String> {
    both(pair, "type into the minibuffer", |session| {
        session.send(input);
        wait_for(
            session,
            Duration::from_secs(8),
            "the candidate window to resize",
            |grid| candidate_rows(grid, PREFIX).len() == expected,
        );
    })
}

/// The main window's mode line row, with `expected` candidates on screen.
fn step(pair: &PackageTuiPair, expected: usize, description: &str) -> Result<u16, String> {
    for session in [&pair.gnu, &pair.neo] {
        assert_eq!(
            candidate_rows(&session.text_grid(), PREFIX).len(),
            expected,
            "{}: the candidate window does not hold {expected} rows for {description}",
            session.name
        );
    }
    assert_eq!(
        candidate_texts(&pair.neo.text_grid(), PREFIX),
        candidate_texts(&pair.gnu.text_grid(), PREFIX),
        "the candidate window differs from GNU's for {description}"
    );
    mode_line_row_in(&pair.gnu.text_grid())
        .ok_or_else(|| format!("GNU rendered no mode line for {description}"))
}
