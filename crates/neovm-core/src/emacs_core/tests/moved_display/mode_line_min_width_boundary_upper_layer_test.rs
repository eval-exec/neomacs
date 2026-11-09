//! mode_line_min_width_boundary_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::xdisp::*;
use neovm_display::test_utils::mode_line_min_width_boundary_test_upper_layer_helpers::{
    compare_fresh_gnu_row, expected, format_for,
};

#[test]
fn numeric_padding_same_source_predecessor_matches_fresh_gnu_row() {
    compare_fresh_gnu_row(crate::test_utils::new_context, "same-source-predecessor");
}

#[test]
fn numeric_padding_equal_nonidentical_predecessor_matches_fresh_gnu_row() {
    compare_fresh_gnu_row(crate::test_utils::new_context, "other-identity-predecessor");
}
