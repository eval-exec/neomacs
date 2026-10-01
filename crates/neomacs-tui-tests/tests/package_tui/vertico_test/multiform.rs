//! Two of the display modes `vertico-multiform` toggles between, each rendered
//! from the same candidates.
//!
//! The toggles are the keys the extension binds in its own map -- `M-G` for the
//! grid, `M-F` for the flat display -- so both layouts below are reached from
//! one completion session by pressing a key, and the session's candidates are
//! the same thirty buffers throughout. What changes is the arithmetic: the grid
//! divides the window into columns wide enough for the longest candidate, and
//! the flat display fits candidates into one line and ends with an ellipsis.
//!
//! Each layout is frozen as a screen at all three levels, and the rows the
//! layout decides -- where the columns start, how many candidates the line
//! holds -- are compared with GNU's on the way.
//!
//! The third display mode, `vertico-multiform-buffer` (`M-B`), is not covered
//! here. It renders the candidates into a window of their own, and Neomacs does
//! not reproduce GNU's terminal state for that window yet: GNU writes the blank
//! cells after each candidate row, Neomacs moves the cursor over them and
//! leaves them unwritten, which the wire-level comparison distinguishes even
//! though the screen looks the same. The layout itself works -- its candidate
//! rows, its mode line and its recomputed `vertico-count` all match GNU's
//! resolved display -- so the mode can be covered as soon as the emission
//! matches, and the same difference is why this scenario's grid separator is
//! configured in the prelude rather than left at its default.

use std::time::Duration;

use expect_test::expect_file;

use super::harness::*;
use super::prelude::*;

/// The fixture's buffer prefix, which filters `C-x b` down to those buffers.
const PREFIX: &str = "facet-";

/// The `C-x b` prompt, whose row holds the count indicator and the input.
const PROMPT: &str = "Switch to buffer";

/// The fixture's candidate count: three columns of ten, at this geometry.
const TOTAL: usize = 30;

/// The terminal this scenario renders on.
const ROWS: u16 = 30;
const COLUMNS: u16 = 100;

pub(super) fn run() {
    let prelude = multiform_vertico(&fixture_buffers(&numbered_names("facet", TOTAL)));
    let mut pair = spawn_pair("vertico-multiform", &prelude, ROWS, COLUMNS);
    let outcome =
        catch_phase("Vertico multiform scenario", || run_body(&mut pair)).and_then(|result| result);
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

    // The session starts in the vertical display: one candidate per row, the
    // first ten of them. This is where the toggles below start from.
    assert_candidate_state(pair, PREFIX, PROMPT, "1/30", Some("facet-01"));
    assert_eq!(
        candidate_rows(&pair.gnu.text_grid(), PREFIX).len(),
        usize::from(WINDOW),
        "the vertical display shows vertico-count candidates"
    );

    toggle(pair, "M-G", "the grid display")?;
    assert_grid(pair);
    record_checkpoint(
        pair,
        "Vertico multiform grid",
        expect_file!["snapshots/multiform_grid_ansi_grid.ansi"],
        expect_file!["snapshots/multiform_grid_plain_grid.plain"],
    );

    toggle(pair, "M-F", "the flat display")?;
    assert_flat(pair);
    record_checkpoint(
        pair,
        "Vertico multiform flat",
        expect_file!["snapshots/multiform_flat_ansi_grid.ansi"],
        expect_file!["snapshots/multiform_flat_plain_grid.plain"],
    );

    // `vertico-multiform-vertical` is the same key sequence with no display
    // mode behind it: it turns the current one off, and the session is back to
    // one candidate per row in the minibuffer's own window.
    toggle(pair, "M-V", "the vertical display")?;
    assert_candidate_state(pair, PREFIX, PROMPT, "1/30", Some("facet-01"));
    assert_eq!(
        candidate_rows(&pair.gnu.text_grid(), PREFIX).len(),
        usize::from(WINDOW),
        "the vertical display is not back"
    );

    // `vertico-quick`'s key column: the extension prefixes each candidate with
    // the key that jumps to it, and the column is part of the candidate window
    // -- which candidate it is in front of is the layout's business, not the
    // key's.
    quick_column(pair)?;
    record_checkpoint(
        pair,
        "Vertico multiform quick keys",
        expect_file!["snapshots/multiform_quick_ansi_grid.ansi"],
        expect_file!["snapshots/multiform_quick_plain_grid.plain"],
    );
    // The first quick key is the first candidate's, and pressing it inserts
    // that candidate into the minibuffer.
    both(pair, "press the first quick key", |session| {
        session.send(b"a");
        wait_for(
            session,
            Duration::from_secs(8),
            "the inserted candidate",
            |grid| count_indicator_in(grid, PROMPT) == "1/1",
        );
    })?;
    assert_eq!(
        minibuffer_input(&pair.neo, PROMPT),
        minibuffer_input(&pair.gnu, PROMPT),
        "the quick key inserted a different candidate in the two editors"
    );
    assert_eq!(
        minibuffer_input(&pair.gnu, PROMPT),
        "facet-01",
        "the first quick key is not the first candidate's"
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

    // `vertico-preselect`: whether the prompt or the first candidate is what
    // the session starts on, which the count indicator says outright -- `*/30`
    // for the prompt, `1/30` for a candidate.
    preselect(pair, "prompt")?;
    open_session(pair)?;
    assert_candidate_state(pair, PREFIX, PROMPT, "*/30", None);
    record_checkpoint(
        pair,
        "Vertico multiform prompt preselected",
        expect_file!["snapshots/multiform_preselect_prompt_ansi_grid.ansi"],
        expect_file!["snapshots/multiform_preselect_prompt_plain_grid.plain"],
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

    preselect(pair, "first")?;
    open_session(pair)?;
    assert_candidate_state(pair, PREFIX, PROMPT, "1/30", Some("facet-01"));
    Ok(())
}

/// Open the fixture's completion session without setting anything up.
fn open_session(pair: &mut PackageTuiPair) -> Result<(), String> {
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
    })
}

/// Press `M-q`, which the prelude bound to `vertico-quick-insert`, and wait for
/// the key column it draws in front of the candidates.
///
/// The column is three characters wide: `a` and two spaces for the candidates
/// the first quick key reaches, and two nested keys -- `ja` for the seventh --
/// with one space for the rest.
fn quick_column(pair: &mut PackageTuiPair) -> Result<(), String> {
    both(pair, "show the quick keys", |session| {
        session.send_key("M-q");
        wait_for(
            session,
            Duration::from_secs(8),
            "the quick key column",
            |grid| {
                let starts = |row: &str, prefix: &str| row.trim_start().starts_with(prefix);
                grid.iter().any(|row| starts(row, FIRST_QUICK_KEY))
                    && grid.iter().any(|row| starts(row, SEVENTH_QUICK_KEY))
            },
        );
    })
}

/// Set `vertico-preselect` to `value` at the keyboard.
fn preselect(pair: &mut PackageTuiPair, value: &str) -> Result<(), String> {
    both(pair, "set the preselected candidate", |session| {
        invoke(session, "eval-expression", "Eval: ");
        session.send(format!("(setq vertico-preselect '{value})").as_bytes());
        session.send_key("RET");
        wait_for(
            session,
            Duration::from_secs(8),
            "the eval minibuffer to close",
            |grid| !grid.iter().any(|row| row.contains("Eval: ")),
        );
    })
}

/// The key column's first and seventh rows, keys and spaces included: the
/// seventh candidate is where `vertico-quick` runs out of single keys and
/// starts nesting them.
const FIRST_QUICK_KEY: &str = "a  facet-01";
const SEVENTH_QUICK_KEY: &str = "ja facet-07";

/// How many candidate rows the vertical window holds: `vertico-count`'s
/// default, which the prelude leaves alone.
const WINDOW: u16 = 10;

/// Press one of `vertico-multiform-map`'s display keys in both peers.
///
/// Each toggle is a command that runs inside the completion session, so the
/// wait is for the layout it leaves behind rather than for a marker.
fn toggle(pair: &mut PackageTuiPair, key: &str, description: &str) -> Result<(), String> {
    both(pair, description, |session| {
        session.send_key(key);
        wait_for(session, Duration::from_secs(8), description, |grid| {
            grid.iter().any(|row| row.contains(PREFIX))
        });
    })
}

/// The grid divides the window into columns, and names the candidates column by
/// column rather than row by row.
///
/// At this geometry `vertico--arrange-candidates` computes the maximum of eight
/// columns -- the window is a hundred characters wide, and each column needs
/// the widest candidate plus the one-character separator -- while the
/// candidates only fill three of them: thirty candidates, ten to a column.
fn assert_grid(pair: &PackageTuiPair) {
    for session in [&pair.gnu, &pair.neo] {
        let grid = session.text_grid();
        let cells = candidate_cells(&grid, PREFIX);
        assert_eq!(
            cells.len(),
            TOTAL,
            "{}: the grid does not show every candidate",
            session.name
        );
        // Column-major: the first row holds the first candidate of each column,
        // nine characters apart -- eight for `facet-NN`, the widest candidate,
        // and one for the separator. The columns are where the candidate widths
        // put them, not where the maximum cell width does.
        let first_row = &grid[usize::from(cells[0].0)];
        assert_eq!(
            cells
                .iter()
                .filter(|(row, ..)| *row == cells[0].0)
                .map(|(_, column, text)| (*column, text.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, "facet-01"), (9, "facet-11"), (18, "facet-21")],
            "{}: the first grid row is not laid out as computed:\n{}",
            session.name,
            grid.join("\n")
        );
        // Ten rows of three, so the last row ends the list.
        assert_eq!(
            cells
                .last()
                .map(|(_, column, text)| (*column, text.as_str())),
            Some((18, "facet-30")),
            "{}: the grid does not end with the last candidate",
            session.name
        );
        assert!(first_row.contains("facet-01"));
    }
    assert_eq!(
        candidate_cells(&pair.neo.text_grid(), PREFIX),
        candidate_cells(&pair.gnu.text_grid(), PREFIX),
        "the grid's candidates differ from GNU's"
    );
}

/// The flat display puts the candidates on the prompt's own line, separated and
/// terminated by an ellipsis when they do not all fit.
fn assert_flat(pair: &PackageTuiPair) {
    assert_eq!(
        exact_row(&pair.neo, PROMPT),
        exact_row(&pair.gnu, PROMPT),
        "the flat display's line differs from GNU's"
    );
    let line = exact_row(&pair.gnu, PROMPT);
    assert!(
        line.contains("{facet-01 | facet-02 | facet-03 | …}"),
        "the flat display does not fit its candidates the way the width says: {line}"
    );
}
