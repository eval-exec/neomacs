//! The grid's separator as the package ships it.
//!
//! `vertico-grid-separator` defaults to a bar with three spaces on either side,
//! and the bar itself carries an `:inverse-video` display property. Both halves
//! of that default are part of the terminal state the grid leaves behind: the
//! blank cells the separator's spaces occupy, and the cell the bar is painted
//! on, which is drawn in the terminal's default foreground over its default
//! background because the display property inverts them for that one cell.
//!
//! The suite's other grid scenario configures the separator to a single bar
//! with no spaces around it, on purpose: it is about the grid's column
//! arithmetic, which the separator's *length* alone decides, and a space-less
//! separator leaves no blank cells for the two editors to disagree about. This
//! scenario is the one that leaves the separator alone, so the cells it paints
//! and the cells it skips are what the screen shows.
//!
//! That is where the divergence is: Neomacs paints the cell behind the bar with
//! its own background where GNU leaves the terminal's default, and it moves the
//! cursor over the separator's leading blanks without writing them where GNU
//! writes spaces. The candidates and their columns are the same text in both
//! editors, so the frozen screen is blessed from GNU and the comparisons below
//! are what fail until the separator is rendered the way GNU renders it. That
//! failure is the point of this scenario: it is not to be re-blessed away.

use std::time::Duration;

use expect_test::expect_file;

use super::harness::*;
use super::prelude::*;

/// The fixture's buffer prefix, which filters `C-x b` down to those buffers.
const PREFIX: &str = "facet-";

/// The `C-x b` prompt, whose row holds the count indicator and the input.
const PROMPT: &str = "Switch to buffer";

/// The fixture's candidate count: three columns of ten, at the default
/// separator's width.
const TOTAL: usize = 30;

/// The terminal this scenario renders on.
const ROWS: u16 = 30;
const COLUMNS: u16 = 100;

pub(super) fn run() {
    let prelude = default_grid_separator_vertico(&fixture_buffers(&numbered_names("facet", TOTAL)));
    let mut pair = spawn_pair("vertico-grid-separator", &prelude, ROWS, COLUMNS);
    let outcome =
        catch_phase("Vertico grid separator scenario", || run_body(&mut pair)).and_then(|r| r);
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

    both(pair, "toggle the grid display", |session| {
        session.send_key("M-G");
        wait_for(
            session,
            Duration::from_secs(8),
            "the grid display",
            |grid| candidate_cells(grid, PREFIX).len() == TOTAL,
        );
    })?;

    // The grid's own arithmetic is the separator's length, and the leading
    // spaces of the default separator are part of it: at this geometry the
    // window is a hundred characters wide and the separator seven characters
    // long, so the columns are eight characters of candidate plus seven of
    // separator, and the whole list is three columns of ten.
    assert_grid(pair);

    record_checkpoint(
        pair,
        "Vertico grid default separator",
        expect_file!["snapshots/grid_separator_ansi_grid.ansi"],
        expect_file!["snapshots/grid_separator_plain_grid.plain"],
    );
    Ok(())
}

/// The grid divides the window into columns, and names the candidates column by
/// column rather than row by row.
fn assert_grid(pair: &PackageTuiPair) {
    for session in [&pair.gnu, &pair.neo] {
        let grid = session.text_grid();
        let cells = candidate_cells(&grid, PREFIX);
        assert_eq!(
            cells.len(),
            TOTAL,
            "{}: the grid does not show every candidate:\n{}",
            session.name,
            grid.join("\n")
        );
        // Column-major, with the separator's seven characters between one
        // column's candidates and the next column's: eight for `facet-NN` and
        // seven for `   |   `.
        assert_eq!(
            cells
                .iter()
                .filter(|(row, ..)| *row == cells[0].0)
                .map(|(_, column, _)| *column)
                .collect::<Vec<_>>(),
            vec![0, 15, 30],
            "{}: the grid's columns are not where the separator's length puts \
             them:\n{}",
            session.name,
            grid.join("\n")
        );
        assert_eq!(
            cells
                .last()
                .map(|(_, column, text)| (*column, text.as_str())),
            Some((30, "facet-30")),
            "{}: the grid does not end with the last candidate",
            session.name
        );
    }
    assert_eq!(
        candidate_cells(&pair.neo.text_grid(), PREFIX),
        candidate_cells(&pair.gnu.text_grid(), PREFIX),
        "the grid's candidates differ from GNU's"
    );
}
