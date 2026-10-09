use std::time::Duration;

use expect_test::expect_file;
use neomacs_tui_tests::RawTerminalSnapshot;

use super::{COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, VERTICO_MELPA_PIN};

use super::scenario::{DisplayCheckpoint, PackageTuiScenario, PairTimeout, ReadinessCheckpoint};

mod annotations;
mod extended_command;
mod grid_separator;
mod growing_window;
mod harness;
mod multiform;
mod multiform_buffer;
mod narrow_terminal;
mod overflow;
mod prelude;
mod repeat_and_sort;
mod resize;
mod selection;
mod short_terminal;
mod sort_function;
mod thin_font_metrics;
mod truncation;

use harness::candidate_rows;
use prelude::VERTICO_TUI_PRELUDE;

#[test]
fn vertico_multiform_renders_the_grid_and_flat_layouts() {
    multiform::run();
}

#[test]
fn vertico_overflow_scrolls_the_candidate_window_and_counts_candidates() {
    overflow::run();
}

#[test]
fn vertico_repeat_replays_the_last_session() {
    repeat_and_sort::run();
}

#[test]
fn vertico_resize_reflows_the_candidate_window() {
    resize::run();
}

#[test]
fn vertico_selection_moves_the_highlight_and_accepts_by_key() {
    selection::run();
}

#[test]
fn vertico_sort_function_set_at_the_keyboard_orders_the_candidates() {
    sort_function::run();
}

#[test]
fn vertico_grid_renders_the_packages_default_separator() {
    grid_separator::run();
}

#[test]
fn vertico_multiform_buffer_shows_the_candidates_in_a_window() {
    multiform_buffer::run();
}

#[test]
fn vertico_narrow_terminal_scrolls_the_window_above_the_candidates() {
    narrow_terminal::run();
}

#[test]
fn vertico_short_terminal_takes_rows_from_the_candidate_window() {
    short_terminal::run();
}

#[test]
fn vertico_truncates_candidates_at_the_windows_last_column() {
    truncation::run();
}

#[test]
fn vertico_annotates_candidates_where_the_function_pads_them() {
    annotations::run();
}

#[test]
fn vertico_completes_command_names_through_m_x() {
    extended_command::run();
}

#[test]
fn vertico_grows_the_minibuffer_window_with_its_candidates() {
    growing_window::run();
}

#[test]
fn vertico_real_minibuffer_candidates_and_selection_match_gnu_grid() {
    let oracle = CachedMelpaOracle::new(VERTICO_MELPA_PIN, "vertico.el")
        .expect("prepare revision-pinned Vertico source")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare exact Compat dependency")
        .with_prelude(VERTICO_TUI_PRELUDE);
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("*scratch*"));
    let mut pair = PackageTuiScenario::new("vertico-minibuffer", oracle.prepared_packages())
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                "initial scratch buffer",
                PairTimeout::per_editor(Duration::from_secs(15), Duration::from_secs(20)),
            ),
            ready,
        )
        .expect("spawn ready package TUI pair");

    pair.send_keys_both("C-x b");
    for session in [&mut pair.gnu, &mut pair.neo] {
        session.read_until(Duration::from_secs(8), |grid| {
            grid.iter().any(|row| row.contains("Switch to buffer"))
        });
    }
    pair.send_both(b"project-");
    for session in [&mut pair.gnu, &mut pair.neo] {
        session.read_until(Duration::from_secs(8), |grid| {
            candidate_rows(grid, "project-").len() >= 3
        });
    }

    let gnu_rows = candidate_rows(&pair.gnu.text_grid(), "project-");
    let neo_rows = candidate_rows(&pair.neo.text_grid(), "project-");
    assert_eq!(neo_rows, gnu_rows, "Vertico candidate rows differ from GNU");
    let gnu_snapshot = RawTerminalSnapshot::capture_full_screen(pair.gnu.screen());

    let expected_ansi_grid = expect_file!["snapshots/expected_ansi_grid.ansi"];
    expected_ansi_grid.assert_eq(&gnu_snapshot.ansi_grid());
    let expected_plain_grid = expect_file!["snapshots/expected_plain_grid.plain"];
    expected_plain_grid.assert_eq(&gnu_snapshot.plain_grid());

    pair.assert_display(DisplayCheckpoint::new("Vertico full-screen terminal state"));
    pair.assert_display(DisplayCheckpoint::raw_terminal(
        "Vertico full-screen terminal wire state",
    ));

    pair.send_both(b"beta");
    pair.send_key_both("RET");
    for session in [&mut pair.gnu, &mut pair.neo] {
        session.read_until(Duration::from_secs(8), |grid| {
            grid.iter().any(|row| row.contains("BETA BUFFER"))
        });
        assert!(
            session
                .text_grid()
                .iter()
                .any(|row| row.contains("BETA BUFFER")),
            "{} did not select the real project-beta buffer",
            session.name
        );
    }
}
