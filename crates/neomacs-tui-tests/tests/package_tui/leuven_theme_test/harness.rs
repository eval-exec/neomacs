use super::super::scenario::PackageTuiPair;
use super::prelude::*;
use neomacs_tui_tests::{RawTerminalSnapshot, Snapshot, TuiSession};
use std::time::Duration;

pub(super) fn lifecycle_report(pair: &PackageTuiPair, gnu: bool) -> String {
    let grid = if gnu {
        pair.gnu.text_grid()
    } else {
        pair.neo.text_grid()
    };
    grid.into_iter()
        .map(|row| row.trim_end().to_owned())
        .filter(|row| REPORT_PREFIXES.iter().any(|prefix| row.starts_with(prefix)))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn wait_for<F>(
    session: &mut TuiSession,
    timeout: Duration,
    description: &str,
    predicate: F,
) where
    F: Fn(&[String]) -> bool,
{
    session.read_until(timeout, |grid| predicate(grid));
    let grid = session.text_grid();
    assert!(
        predicate(&grid),
        "{} timed out waiting for {description}:\n{}",
        session.name,
        grid.join("\n")
    );
}

pub(super) fn invoke(session: &mut TuiSession, command: &str, ready: &str) {
    session.send_keys("M-x");
    wait_for(
        session,
        Duration::from_secs(8),
        &format!("M-x prompt before {command}"),
        |grid| grid.iter().any(|row| row.contains("M-x")),
    );
    session.send(command.as_bytes());
    session.send_keys("RET");
    wait_for(
        session,
        Duration::from_secs(12),
        &format!("{command} readiness marker {ready:?}"),
        |grid| grid.iter().any(|row| row.contains(ready)),
    );
}

pub(super) fn invoke_both(pair: &mut PackageTuiPair, command: &str, ready: &str) {
    invoke(&mut pair.gnu, command, ready);
    invoke(&mut pair.neo, command, ready);
}

pub(super) fn ansi_rows(session: &TuiSession, needles: &[&str]) -> String {
    let grid = session.text_grid();
    needles
        .iter()
        .map(|needle| {
            let row = grid
                .iter()
                .position(|contents| contents.contains(needle))
                .unwrap_or_else(|| {
                    panic!(
                        "{} never rendered {needle:?}:\n{}",
                        session.name,
                        grid.join("\n")
                    )
                }) as u16;
            RawTerminalSnapshot::capture_rows(session.screen(), row..row + 1).ansi_grid()
        })
        .collect::<Vec<_>>()
        .join("")
}

pub(super) fn assert_rendered_rows(
    pair: &PackageTuiPair,
    label: &str,
    needles: &[&str],
    expected: impl Snapshot,
    neo_mismatches: &mut Vec<String>,
) -> String {
    let gnu = ansi_rows(&pair.gnu, needles);
    let neo = ansi_rows(&pair.neo, needles);
    expected.assert_snapshot(&gnu);
    record_neo_mismatch(neo_mismatches, label, &neo, &gnu);
    gnu
}

pub(super) fn record_neo_mismatch(mismatches: &mut Vec<String>, label: &str, neo: &str, gnu: &str) {
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU: {gnu:?}\nNeo: {neo:?}"));
    }
}

pub(super) fn exact_row(session: &TuiSession, needle: &str) -> String {
    session
        .text_grid()
        .into_iter()
        .find(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("{} did not render {needle:?}", session.name))
        .trim_end()
        .to_owned()
}
