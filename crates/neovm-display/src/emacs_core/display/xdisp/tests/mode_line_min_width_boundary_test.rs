//! Direct-display regressions with rows collected from a fresh live GNU PTY
//! run. Only terminal fill after the final `|` was removed from the fixture.
use super::*;
use crate::test_utils::mode_line_numeric_padding_shared_helpers::PaddingPolicyGuard;

use crate::test_utils::mode_line_min_width_boundary_test_upper_layer_helpers::{
    GNU_ROWS, expected, format_for,
};

fn compare_fresh_gnu_row(kind: &str) {
    crate::test_utils::mode_line_min_width_boundary_test_upper_layer_helpers::compare_fresh_gnu_row(
        || neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime()),
        kind,
    )
}

#[test]
fn numeric_padding_mode_name_boundary_matches_fresh_gnu_row() {
    compare_fresh_gnu_row("mode-name");
}

#[test]
fn numeric_padding_frame_name_boundary_matches_fresh_gnu_row() {
    compare_fresh_gnu_row("frame-name");
}

#[test]
fn numeric_padding_eol_indicator_boundary_matches_fresh_gnu_row() {
    compare_fresh_gnu_row("eol-indicator");
}

#[test]
fn numeric_padding_other_source_nonzero_literal_matches_fresh_gnu_row() {
    compare_fresh_gnu_row("other-source-literal");
}
