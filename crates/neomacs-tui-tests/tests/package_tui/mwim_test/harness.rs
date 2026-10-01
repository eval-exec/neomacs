use super::super::scenario::PackageTuiPair;
use neomacs_tui_tests::TuiSession;
// Re-exported so the sibling modules reach the shared vocabulary the same way
// they reach this module's own helpers.
pub(super) use neomacs_tui_tests::package_harness::{
    both, catch_phase, exact_row, invoke, wait_for,
};

pub(super) fn record_rows(
    pair: &PackageTuiPair,
    marker: &str,
    gnu_transcript: &mut Vec<String>,
    neo_transcript: &mut Vec<String>,
) {
    gnu_transcript.push(exact_row(&pair.gnu, marker));
    neo_transcript.push(exact_row(&pair.neo, marker));
}

pub(super) fn visual_grid(session: &TuiSession) -> String {
    let grid = session.text_grid();
    let (_, cols) = session.screen_size();
    [
        "alpha beta",
        "delta epsilon",
        "theta iota",
        "lambda mu",
        "wide 界",
        "maple spruce",
        "willow aspen",
    ]
    .into_iter()
    .map(|needle| {
        let row = grid
            .iter()
            .position(|contents| contents.contains(needle))
            .unwrap_or_else(|| {
                panic!(
                    "{} did not render visual row {needle:?}:\n{}",
                    session.name,
                    grid.join("\n")
                )
            }) as u16;
        session
            .screen()
            .contents_between(row, cols - 24, row, cols)
            .trim_end()
            .to_owned()
    })
    .enumerate()
    .map(|(index, row)| format!("MWIM-GRID-{} {row:?}", index + 1))
    .collect::<Vec<_>>()
    .join("\n")
}
