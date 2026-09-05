#![cfg(unix)]
//! Regression test for issue #170: Doom's list-valued `line-prefix` must
//! center buffer text in the public TTY character grid.

use crate::support;

use neomacs_tui_tests::TuiSession;
use std::time::Duration;
use support::*;

const TARGET_ROWS: u16 = 34;
const TARGET_COLS: u16 = 248;
const SENTINEL: &str = "ISSUE-170-CENTERED";

fn sentinel_position(session: &TuiSession) -> Option<(usize, usize)> {
    session
        .text_grid()
        .iter()
        .enumerate()
        .find_map(|(row, text)| text.find(SENTINEL).map(|column| (row, column)))
}

#[test]
fn doom_space_line_prefix_centers_buffer_text() {
    let (mut gnu, mut neo) = boot_pair("");
    resize_both(&mut gnu, &mut neo, TARGET_ROWS, TARGET_COLS);
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // Doom's dashboard computes one prefix for every inserted group of lines:
    //
    //   (space :align-to (- center (/ width 2)))
    //
    // The prefix is installed directly as the `line-prefix` and
    // `indent-prefix` text properties; it is a display spec, not a string.
    let half_width = SENTINEL.chars().count() / 2;
    let expression = format!(
        r#"(progn (switch-to-buffer (get-buffer-create "*issue-170*")) (erase-buffer) (insert "{SENTINEL}\n") (add-text-properties (point-min) (point-max) '(line-prefix (space :align-to (- center {half_width})) indent-prefix (space :align-to (- center {half_width})))) (goto-char (point-min)))"#
    );
    eval_expression(&mut gnu, &mut neo, &expression);

    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.contains(SENTINEL))
    });
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    let expected_column = (usize::from(TARGET_COLS) - SENTINEL.chars().count()) / 2;
    let gnu_position = sentinel_position(&gnu).expect("GNU should display the sentinel");
    let neo_position = sentinel_position(&neo).expect("Neomacs should display the sentinel");

    if neo_position != gnu_position {
        dump_pair_grids("issue #170 Doom display-spec prefix", &gnu, &neo);
    }

    assert_eq!(
        gnu_position.1, expected_column,
        "GNU oracle should place the sentinel at Doom's computed center"
    );
    assert_eq!(
        neo_position, gnu_position,
        "Neomacs should honor Doom's display-spec prefix in its public TTY grid"
    );
    assert_pair_exact_display("doom_space_line_prefix_centers_buffer_text", &gnu, &neo);
}
