//! mode_line_min_width_boundary_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::xdisp::*;
use neovm_display::test_support::authority::mode_line_min_width_compare_fresh_gnu_row_for_test as compare_fresh_gnu_row;

#[test]
fn numeric_padding_same_source_predecessor_matches_fresh_gnu_row() {
    compare_fresh_gnu_row(crate::test_utils::new_context, "same-source-predecessor");
}

#[test]
fn numeric_padding_equal_nonidentical_predecessor_matches_fresh_gnu_row() {
    compare_fresh_gnu_row(crate::test_utils::new_context, "other-identity-predecessor");
}
