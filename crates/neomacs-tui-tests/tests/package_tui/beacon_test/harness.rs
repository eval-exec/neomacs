use super::super::scenario::PackageTuiPair;
use expect_test::Expect;
// Re-exported so the sibling modules reach the shared vocabulary the same way
// they reach this module's own helpers.
pub(super) use neomacs_tui_tests::package_harness::{
    both, catch_phase, exact_row, invoke, wait_for,
};
use neomacs_tui_tests::{RawTerminalSnapshot, TuiSession};
use std::time::{Duration, Instant};

pub(super) fn push_rows(
    pair: &PackageTuiPair,
    marker: &str,
    gnu: &mut Vec<String>,
    neo: &mut Vec<String>,
) {
    gnu.push(exact_row(&pair.gnu, marker));
    neo.push(exact_row(&pair.neo, marker));
}

pub(super) fn record_pair(
    pair: &PackageTuiPair,
    label: &str,
    marker: &str,
    expected: Expect,
    mismatches: &mut Vec<String>,
) {
    let gnu = exact_row(&pair.gnu, marker);
    let neo = exact_row(&pair.neo, marker);
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU: {gnu}\nNeo: {neo}"));
    }
    expected.assert_eq(&gnu);
}

pub(super) fn styled_span(
    session: &TuiSession,
    needle: &str,
    offset: usize,
    width: usize,
) -> String {
    let grid = session.text_grid();
    let row = grid
        .iter()
        .position(|contents| contents.contains(needle))
        .unwrap_or_else(|| {
            panic!(
                "{} did not render {needle:?}\n{}",
                session.name,
                grid.join("\n")
            )
        }) as u16;
    let column = grid[row as usize]
        .find(needle)
        .expect("located needle row must retain the needle")
        + offset;
    let mut snapshot = RawTerminalSnapshot::capture_rows(session.screen(), row..row + 1);
    snapshot.rows[0].cells = snapshot.rows[0].cells[column..column + width].to_vec();
    snapshot.ansi_grid()
}

pub(super) fn repeated_styled_spans(session: &TuiSession, needle: &str, width: usize) -> String {
    let grid = session.text_grid();
    let row = grid
        .iter()
        .position(|contents| contents.matches(needle).count() == 2)
        .unwrap_or_else(|| {
            panic!(
                "{} did not render two occurrences of {needle:?}\n{}",
                session.name,
                grid.join("\n")
            )
        }) as u16;
    let columns = grid[row as usize]
        .match_indices(needle)
        .map(|(column, _)| column)
        .collect::<Vec<_>>();
    assert_eq!(
        columns.len(),
        2,
        "expected exactly two rendered occurrences"
    );
    let snapshot = RawTerminalSnapshot::capture_rows(session.screen(), row..row + 1);
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let mut occurrence = snapshot.clone();
            occurrence.rows[0].cells = occurrence.rows[0].cells[*column..*column + width].to_vec();
            format!(
                "B359-WINDOW-CELLS-{} {}",
                index + 1,
                occurrence.ansi_grid().trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn record_style(
    pair: &PackageTuiPair,
    label: &str,
    needle: &str,
    offset: usize,
    width: usize,
    expected: Expect,
    mismatches: &mut Vec<String>,
) {
    let gnu = styled_span(&pair.gnu, needle, offset, width);
    let neo = styled_span(&pair.neo, needle, offset, width);
    record_captured_style(label, gnu, neo, expected, mismatches);
}

pub(super) fn record_captured_style(
    label: &str,
    gnu: String,
    neo: String,
    expected: Expect,
    mismatches: &mut Vec<String>,
) {
    if neo != gnu {
        mismatches.push(format!("{label} style differs\nGNU: {gnu:?}\nNeo: {neo:?}"));
    }
    expected.assert_eq(&gnu);
}

pub(super) fn emitted_truecolor_cells(session: &TuiSession) -> Vec<(u16, u16, u16, char)> {
    let output = session.recent_output();
    let mut cells = Vec::new();
    let mut cursor = 0;
    while cursor + 2 < output.len() {
        let Some(relative_start) = output[cursor..]
            .windows(2)
            .position(|bytes| bytes == b"\x1b[")
        else {
            break;
        };
        let start = cursor + relative_start + 2;
        let Some(relative_end) = output[start..]
            .iter()
            .position(|byte| (0x40..=0x7e).contains(byte))
        else {
            break;
        };
        let end = start + relative_end;
        if output[end] != b'm' {
            cursor = end + 1;
            continue;
        }
        let parameters = String::from_utf8_lossy(&output[start..end])
            .split(';')
            .filter_map(|value| value.parse::<u16>().ok())
            .collect::<Vec<_>>();
        if let Some(index) = parameters
            .windows(2)
            .position(|parameters| parameters == [48, 2])
            && index + 4 < parameters.len()
            && end + 1 < output.len()
            && output[end + 1].is_ascii_graphic()
        {
            cells.push((
                parameters[index + 2],
                parameters[index + 3],
                parameters[index + 4],
                output[end + 1] as char,
            ));
        }
        cursor = end + 1;
    }
    cells
}

pub(super) fn emitted_styled_span(session: &TuiSession, text: &str) -> String {
    let expected_chars = text.chars().collect::<Vec<_>>();
    let cells = emitted_truecolor_cells(session);
    let observed = cells
        .windows(expected_chars.len())
        .find(|window| {
            window
                .iter()
                .map(|(_, _, _, character)| *character)
                .eq(expected_chars.iter().copied())
        })
        .unwrap_or_else(|| {
            panic!(
                "{} did not emit a truecolor span for {text:?}: {cells:?}",
                session.name
            )
        });
    let mut rendered = String::new();
    for (red, green, blue, character) in observed {
        rendered.push_str(&format!("\x1b[0;48;2;{red};{green};{blue}m{character}"));
    }
    rendered.push_str("\x1b[0m\n");
    rendered
}

pub(super) fn cell_span_background_state(
    session: &TuiSession,
    needle: &str,
    offset: usize,
    width: usize,
) -> Option<bool> {
    let grid = session.text_grid();
    let row = grid.iter().position(|contents| contents.contains(needle))?;
    let column = grid[row].find(needle).map(|start| start + offset)?;
    let control_background = session
        .screen()
        .cell(row as u16, (column + width + 1) as u16)
        .expect("Beacon styled-span control cell must exist")
        .bgcolor();
    Some((column..column + width).any(|col| {
        session
            .screen()
            .cell(row as u16, col as u16)
            .is_some_and(|cell| cell.bgcolor() != control_background)
    }))
}

pub(super) fn wait_for_pair_background(
    pair: &mut PackageTuiPair,
    needle: &str,
    offset: usize,
    width: usize,
    present: bool,
) -> Option<(String, String)> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let target = Some(present);
    let mut gnu_seen = cell_span_background_state(&pair.gnu, needle, offset, width) == target;
    let mut neo_seen = cell_span_background_state(&pair.neo, needle, offset, width) == target;
    let mut gnu_style = gnu_seen.then(|| styled_span(&pair.gnu, needle, offset, width));
    let mut neo_style = neo_seen.then(|| styled_span(&pair.neo, needle, offset, width));
    while Instant::now() < deadline {
        if gnu_seen && neo_seen {
            return present.then(|| {
                (
                    gnu_style.expect("present GNU background must capture its raw style"),
                    neo_style.expect("present Neo background must capture its raw style"),
                )
            });
        }
        if !gnu_seen {
            pair.gnu.read(Duration::from_millis(60));
            gnu_seen = cell_span_background_state(&pair.gnu, needle, offset, width) == target;
            if present && gnu_seen {
                gnu_style = Some(styled_span(&pair.gnu, needle, offset, width));
            }
        }
        if !neo_seen {
            pair.neo.read(Duration::from_millis(60));
            neo_seen = cell_span_background_state(&pair.neo, needle, offset, width) == target;
            if present && neo_seen {
                neo_style = Some(styled_span(&pair.neo, needle, offset, width));
            }
        }
    }
    panic!(
        "Beacon peers never reached background present={present}; GNU={:?} Neo={:?}\nGNU grid:\n{}\nNeo grid:\n{}\nGNU output: {:?}\nNeo output: {:?}",
        cell_span_background_state(&pair.gnu, needle, offset, width),
        cell_span_background_state(&pair.neo, needle, offset, width),
        pair.gnu.text_grid().join("\n"),
        pair.neo.text_grid().join("\n"),
        String::from_utf8_lossy(pair.gnu.recent_output()),
        String::from_utf8_lossy(pair.neo.recent_output())
    );
}

pub(super) fn wait_for_pair_marker(pair: &mut PackageTuiPair, marker: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        let gnu = pair.gnu.text_grid().iter().any(|row| row.contains(marker));
        let neo = pair.neo.text_grid().iter().any(|row| row.contains(marker));
        if gnu && neo {
            return;
        }
        pair.gnu.read(Duration::from_millis(60));
        pair.neo.read(Duration::from_millis(60));
    }
    panic!(
        "Beacon peers never rendered {marker:?}; GNU:\n{}\nNeo:\n{}",
        pair.gnu.text_grid().join("\n"),
        pair.neo.text_grid().join("\n")
    );
}
