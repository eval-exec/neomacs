//! mode_line_gc_roots_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::xdisp::*;
use neovm_display::test_support::authority::mode_line_gc_assert_accumulated_property_survives_for_test as assert_accumulated_property_survives;
use neovm_display::test_utils::mode_line_gc_roots_test_upper_layer_helpers::{
    FormatEntry, PropertySource,
};
use neovm_eval::emacs_core::eval::Context;

#[test]
fn gc_mode_line_copied_spine_survives_later_eval_gen0() {
    assert_accumulated_property_survives(
        crate::test_utils::new_context,
        false,
        PropertySource::CopiedString,
        FormatEntry::LispString,
    );
}

#[test]
fn gc_mode_line_copied_spine_survives_later_eval_gen1() {
    assert_accumulated_property_survives(
        crate::test_utils::new_context,
        true,
        PropertySource::CopiedString,
        FormatEntry::LispString,
    );
}

#[test]
fn gc_mode_line_display_copied_spine_survives_later_eval_gen0() {
    assert_accumulated_property_survives(
        crate::test_utils::new_context,
        false,
        PropertySource::CopiedString,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_display_copied_spine_survives_later_eval_gen1() {
    assert_accumulated_property_survives(
        crate::test_utils::new_context,
        true,
        PropertySource::CopiedString,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_display_fresh_eval_source_survives_later_eval_gen0() {
    assert_accumulated_property_survives(
        crate::test_utils::new_context,
        false,
        PropertySource::FreshEvalString,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_display_fresh_eval_source_survives_later_eval_gen1() {
    assert_accumulated_property_survives(
        crate::test_utils::new_context,
        true,
        PropertySource::FreshEvalString,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_display_detached_tail_is_not_rendered_gen0() {
    assert_accumulated_property_survives(
        crate::test_utils::new_context,
        false,
        PropertySource::DetachedFormatElements,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_display_detached_tail_is_not_rendered_gen1() {
    assert_accumulated_property_survives(
        crate::test_utils::new_context,
        true,
        PropertySource::DetachedFormatElements,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_detached_tail_is_not_rendered_gen0() {
    assert_accumulated_property_survives(
        crate::test_utils::new_context,
        false,
        PropertySource::DetachedFormatElements,
        FormatEntry::LispString,
    );
}

#[test]
fn gc_mode_line_detached_tail_is_not_rendered_gen1() {
    assert_accumulated_property_survives(
        crate::test_utils::new_context,
        true,
        PropertySource::DetachedFormatElements,
        FormatEntry::LispString,
    );
}
