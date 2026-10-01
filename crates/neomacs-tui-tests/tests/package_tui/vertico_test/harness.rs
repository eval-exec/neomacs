//! The screen vocabulary the Vertico suite shares.
//!
//! Everything here is about the candidate window Vertico owns: which rows hold
//! candidates, which one carries the highlight, and what the minibuffer's count
//! indicator says. The waits, the symmetric steps and the panic-to-value
//! conversion come from [`neomacs_tui_tests::package_harness`], re-exported so
//! the scenario modules reach them the same way they reach these helpers.

use std::time::Duration;

use expect_test::ExpectFile;
use neomacs_tui_tests::{RawTerminalSnapshot, TuiSession};

use super::super::scenario::{
    DisplayCheckpoint, PackageTuiScenario, PairTimeout, ReadinessCheckpoint,
};
use super::super::{COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, VERTICO_MELPA_PIN};

// Re-exported so the scenario modules reach the shared vocabulary the same way
// they reach this module's own helpers.
pub(super) use super::super::scenario::PackageTuiPair;
pub(super) use neomacs_tui_tests::package_harness::{
    both, catch_phase, exact_row, invoke, wait_for,
};

/// The suite's readiness checkpoint: both editors at first scratch screen.
const READINESS: &str = "initial scratch buffer";

/// Spawn one Vertico pair on `prelude`, at a fixed terminal geometry.
///
/// The geometry is fixed at every call site rather than left at whatever the
/// harness defaults to: the candidate window Vertico renders -- how many rows
/// it grows to, how many candidates fit, where the main window's mode line ends
/// up -- is a function of the terminal's height and width, so a scenario that
/// freezes a screen has to say which terminal it rendered on.
pub(super) fn spawn_pair(label: &str, prelude: &str, rows: u16, columns: u16) -> PackageTuiPair {
    let oracle = CachedMelpaOracle::new(VERTICO_MELPA_PIN, "vertico.el")
        .expect("prepare revision-pinned Vertico source")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare exact Compat dependency")
        .with_prelude(prelude);
    let mut pair = PackageTuiScenario::new(label, oracle.prepared_packages())
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                READINESS,
                PairTimeout::per_editor(Duration::from_secs(15), Duration::from_secs(20)),
            ),
            |grid| grid.iter().any(|row| row.contains("*scratch*")),
        )
        .expect("spawn ready package TUI pair");
    pair.resize_both(rows, columns);
    // The mode line is the last row of the main window once the terminal has
    // been resized: the echo area is the last row of the screen, and an inactive
    // minibuffer is not drawn, so nothing is between the echo area and it.
    let mode_line = idle_mode_line_row(rows);
    both(&mut pair, "terminal geometry", |session| {
        wait_for(
            session,
            Duration::from_secs(8),
            "the resized screen",
            |grid| {
                grid.get(usize::from(mode_line))
                    .is_some_and(|row| row.contains(MODE_LINE_MARKER))
            },
        );
    })
    .expect("both peers redraw at the scenario's geometry");
    pair
}

/// The mode line's leading indicator, which no other row carries.
///
/// It is the coding-system indicator the mode line always starts with, so it
/// survives the buffer being modified (`-UUU:**-`) or not (`-UUU:---`).
const MODE_LINE_MARKER: &str = "-UUU:";

/// The row the main window's mode line occupies with the minibuffer inactive.
///
/// The echo area is the screen's last row, an inactive minibuffer is not drawn
/// at all, and the main window ends on the row above the echo area.
pub(super) fn idle_mode_line_row(rows: u16) -> u16 {
    rows - 2
}

/// The row the main window's mode line is on, if the screen has one.
pub(super) fn mode_line_row_in(grid: &[String]) -> Option<u16> {
    grid.iter()
        .position(|row| row.contains(MODE_LINE_MARKER))
        .map(|row| row as u16)
}

/// The row the minibuffer window's prompt is on, directly under the mode line.
///
/// Vertico grows the minibuffer window and draws the candidates into it, so the
/// window's own first row -- the prompt -- is the row after the main window's
/// mode line, and the candidate rows follow it without a gap. How tall the
/// window grows is the package's decision and is pinned by the frozen screens;
/// that the window starts here is what this returns.
pub(super) fn prompt_row_in(grid: &[String]) -> Option<u16> {
    mode_line_row_in(grid).map(|row| row + 1)
}

/// The rows holding a candidate whose text starts with `prefix`, in screen
/// order.
///
/// Vertico renders one candidate per row below the prompt, so a row that
/// begins with the fixture's prefix is a candidate row and nothing else on the
/// screen is.
pub(super) fn candidate_rows(grid: &[String], prefix: &str) -> Vec<u16> {
    grid.iter()
        .enumerate()
        .filter_map(|(row, contents)| {
            contents
                .trim_start()
                .starts_with(prefix)
                .then_some(row as u16)
        })
        .collect()
}

/// Every candidate the screen shows as `(row, column, text)`.
///
/// The layouts that put several candidates on one row -- the grid, the flat
/// display -- need the column a candidate starts at as much as the row it is
/// on. A candidate here is the fixture's prefix and the characters that follow
/// it up to the next separator or space: what the fixture's names are made of,
/// which is also what tells a candidate apart from the minibuffer's own input
/// (the prefix with nothing after it).
pub(super) fn candidate_cells(grid: &[String], prefix: &str) -> Vec<(u16, usize, String)> {
    let mut cells = Vec::new();
    for (row, contents) in grid.iter().enumerate() {
        for (column, _) in contents.match_indices(prefix) {
            let after = &contents[column + prefix.len()..];
            let text = after
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '|' && *c != '│')
                .collect::<String>();
            if text.is_empty() {
                continue;
            }
            cells.push((row as u16, column, format!("{prefix}{text}")));
        }
    }
    cells
}

/// The candidate texts the screen shows, in screen order.
pub(super) fn candidate_texts(grid: &[String], prefix: &str) -> Vec<String> {
    candidate_rows(grid, prefix)
        .into_iter()
        .map(|row| grid[row as usize].trim().to_owned())
        .collect()
}

/// The candidate row carrying Vertico's `vertico-current` highlight.
///
/// The highlight is a background colour on the row, so the current candidate is
/// the candidate row whose cells carry a background the row above the candidate
/// window does not: that row is the prompt, whose own first cell is the count
/// indicator rather than prompt or input text. `None` means no candidate is
/// current, which is what selecting the prompt leaves behind.
pub(super) fn current_candidate(session: &TuiSession, prefix: &str) -> Option<String> {
    let grid = session.text_grid();
    let rows = candidate_rows(&grid, prefix);
    let reference = background_of(session, rows.first()?.checked_sub(1)?, 0)?;
    let mut current = rows
        .into_iter()
        .filter(|row| background_of(session, *row, 0) != Some(reference))
        .map(|row| grid[row as usize].trim().to_owned());
    let candidate = current.next()?;
    assert!(
        current.next().is_none(),
        "{} highlighted more than one candidate row",
        session.name
    );
    Some(candidate)
}

fn background_of(session: &TuiSession, row: u16, column: u16) -> Option<vt100::Color> {
    session
        .screen()
        .cell(row, column)
        .map(|cell| cell.bgcolor())
}

/// What Vertico's count indicator says on the row holding `prompt`.
///
/// The indicator is the package's own report of the selection: the candidate's
/// position as `index/total`, `*/total` once the prompt itself is selected, and
/// `!/total` while the prompt cannot be selected at all. `vertico-count-format`
/// pads it to six columns, so it is the row's first token.
pub(super) fn count_indicator(session: &TuiSession, prompt: &str) -> String {
    count_indicator_in(&session.text_grid(), prompt)
}

/// [`count_indicator`] against an already-read grid, for use in a wait.
pub(super) fn count_indicator_in(grid: &[String], prompt: &str) -> String {
    grid.iter()
        .find(|row| row.contains(prompt))
        .and_then(|row| row.split_whitespace().next())
        .unwrap_or_default()
        .to_owned()
}

/// The text the minibuffer holds after `prompt`, without the count indicator.
pub(super) fn minibuffer_input(session: &TuiSession, prompt: &str) -> String {
    exact_row(session, prompt)
        .split_once(": ")
        .map(|(_, input)| input.to_owned())
        .unwrap_or_default()
}

/// The main window's mode line, trimmed.
pub(super) fn mode_line(session: &TuiSession) -> String {
    exact_row(session, MODE_LINE_MARKER)
}

/// Pin one Vertico screen: the rows the package owns, GNU's frozen full screen,
/// and both editors' displays.
///
/// GNU is the oracle. The frozen files are blessed from its screen, the two
/// editors are compared against each other through the resolved display and
/// through their raw terminal cells, and the state the package owns -- which
/// candidates are on screen, which one is current, what the count indicator
/// says -- is compared separately by the caller, which can report *what*
/// differs rather than just the terminal state.
pub(super) fn record_checkpoint(
    pair: &mut PackageTuiPair,
    label: &str,
    expected_ansi: ExpectFile,
    expected_plain: ExpectFile,
) {
    let gnu = RawTerminalSnapshot::capture_full_screen(pair.gnu.screen());
    expected_ansi.assert_eq(&gnu.ansi_grid());
    expected_plain.assert_eq(&gnu.plain_grid());
    pair.assert_display(DisplayCheckpoint::new(format!(
        "{label} full-screen terminal state"
    )));
    pair.assert_display(DisplayCheckpoint::raw_terminal(format!(
        "{label} full-screen terminal wire state"
    )));
}

/// The candidate window's own state, compared with GNU's row by row.
///
/// `expected_count` is the count indicator both peers must be showing and
/// `expected_current` the candidate that must carry the highlight (`None` when
/// the prompt is selected and no candidate is current).
pub(super) fn assert_candidate_state(
    pair: &PackageTuiPair,
    prefix: &str,
    prompt: &str,
    expected_count: &str,
    expected_current: Option<&str>,
) {
    assert_eq!(
        candidate_texts(&pair.neo.text_grid(), prefix),
        candidate_texts(&pair.gnu.text_grid(), prefix),
        "Vertico candidate rows differ from GNU"
    );
    assert_eq!(
        current_candidate(&pair.neo, prefix),
        current_candidate(&pair.gnu, prefix),
        "Vertico's current candidate differs from GNU"
    );
    assert_eq!(
        count_indicator(&pair.neo, prompt),
        count_indicator(&pair.gnu, prompt),
        "Vertico's count indicator differs from GNU"
    );
    assert_eq!(
        count_indicator(&pair.gnu, prompt),
        expected_count,
        "Vertico's count indicator is not the expected one"
    );
    assert_eq!(
        current_candidate(&pair.gnu, prefix).as_deref(),
        expected_current,
        "Vertico's current candidate is not the expected one"
    );
}

/// Send `key` to both peers and wait for the count indicator to reach `count`,
/// then check the whole candidate window against GNU's.
///
/// The indicator is the package's own report of the selection, so waiting for
/// it is waiting for Vertico to have processed the key rather than for the
/// screen to have stopped moving.
pub(super) fn press_and_wait_for_count(
    pair: &mut PackageTuiPair,
    key: &str,
    prompt: &str,
    prefix: &str,
    count: &str,
    expected_current: Option<&str>,
) -> Result<(), String> {
    both(pair, &format!("{key} to {count}"), |session| {
        session.send_key(key);
        wait_for(
            session,
            Duration::from_secs(8),
            &format!("the count indicator to read {count}"),
            |grid| count_indicator_in(grid, prompt) == count,
        );
    })?;
    assert_candidate_state(pair, prefix, prompt, count, expected_current);
    Ok(())
}

/// The candidate window is where a grown minibuffer window puts it: its prompt
/// row is the row after the main window's mode line, and its candidates are the
/// rows after that, without a gap.
///
/// How tall the window grows is the package's decision and is pinned by the
/// frozen screens; that the window starts here is what this checks.
pub(super) fn assert_candidates_start_under_mode_line(
    pair: &PackageTuiPair,
    prefix: &str,
    prompt: &str,
) {
    for session in [&pair.gnu, &pair.neo] {
        let grid = session.text_grid();
        let prompt_row = prompt_row_in(&grid)
            .unwrap_or_else(|| panic!("{} rendered no mode line", session.name));
        assert!(
            grid[usize::from(prompt_row)].contains(prompt),
            "{}: the row under the mode line is not the prompt row:\n{}",
            session.name,
            grid.join("\n")
        );
        assert_eq!(
            candidate_rows(&grid, prefix).first().copied(),
            Some(prompt_row + 1),
            "{}: the candidates do not start directly under the prompt",
            session.name
        );
    }
}
