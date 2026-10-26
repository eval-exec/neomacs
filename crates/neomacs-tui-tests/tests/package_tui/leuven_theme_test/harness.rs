use super::super::scenario::PackageTuiPair;
use super::prelude::*;
use neomacs_tui_tests::{RawTerminalSnapshot, Snapshot, TuiSession};
use std::time::{Duration, Instant};

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
    let deadline = Instant::now() + timeout;
    loop {
        let observed = session
            .observe_at_fence()
            .expect("Leuven ordered terminal observation");
        let grid = observed.text_grid();
        if predicate(&grid) {
            return;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "{} timed out waiting for {description}:\n{}",
            session.name,
            grid.join("\n")
        );
        session.read(remaining.min(Duration::from_millis(50)));
    }
}

pub(super) fn invoke(session: &mut TuiSession, command: &str, ready: &str) {
    // Prompt entry and literal consumption share the existing input budget.
    // A successful PTY write alone does not establish minibuffer consumption.
    let input_deadline = Instant::now() + Duration::from_secs(8);
    session.send_key("M-x");
    wait_for(
        session,
        input_deadline.saturating_duration_since(Instant::now()),
        &format!("M-x prompt before {command}"),
        |grid| grid.last().is_some_and(|row| row.trim_end() == "M-x"),
    );
    session.send(command.as_bytes());
    let entered = format!("M-x {command}");
    wait_for(
        session,
        input_deadline.saturating_duration_since(Instant::now()),
        &format!("M-x literal before submitting {command}"),
        |grid| grid.last().is_some_and(|row| row.trim_end() == entered),
    );
    session.send_key("RET");
    wait_for(
        session,
        // The first real theme command cold-loads Org and reapplies its faces.
        // GNU can spend more than 12 CPU seconds there on the bounded CI load.
        Duration::from_secs(30),
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
