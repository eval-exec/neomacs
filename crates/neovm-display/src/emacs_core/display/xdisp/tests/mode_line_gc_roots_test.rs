//! Accumulated mode-line intervals must remain roots across later `:eval`.

use super::*;
pub(super) use crate::test_utils::mode_line_gc_roots_test_upper_layer_helpers::assert_owned_property;
use crate::test_utils::mode_line_gc_roots_test_upper_layer_helpers::{FormatEntry, PropertySource};
use neovm_eval::emacs_core::eval::Context;

fn assert_accumulated_property_survives(
    generational: bool,
    source: PropertySource,
    entry: FormatEntry,
) {
    crate::test_utils::mode_line_gc_roots_test_upper_layer_helpers::assert_accumulated_property_survives(|| neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime()), generational, source, entry)
}

#[test]
fn gc_mode_line_propertize_spine_survives_later_eval_gen0() {
    assert_accumulated_property_survives(
        false,
        PropertySource::PropertizeElement,
        FormatEntry::LispString,
    );
}

#[test]
fn gc_mode_line_propertize_spine_survives_later_eval_gen1() {
    assert_accumulated_property_survives(
        true,
        PropertySource::PropertizeElement,
        FormatEntry::LispString,
    );
}

#[test]
fn gc_mode_line_display_propertize_spine_survives_later_eval_gen0() {
    assert_accumulated_property_survives(
        false,
        PropertySource::PropertizeElement,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_display_propertize_spine_survives_later_eval_gen1() {
    assert_accumulated_property_survives(
        true,
        PropertySource::PropertizeElement,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_parent_spine_survives_nested_eval_gen0() {
    assert_accumulated_property_survives(
        false,
        PropertySource::NestedEval,
        FormatEntry::LispString,
    );
}

#[test]
fn gc_mode_line_parent_spine_survives_nested_eval_gen1() {
    assert_accumulated_property_survives(true, PropertySource::NestedEval, FormatEntry::LispString);
}

#[test]
fn gc_mode_line_display_parent_spine_survives_nested_eval_gen0() {
    assert_accumulated_property_survives(false, PropertySource::NestedEval, FormatEntry::Display);
}

#[test]
fn gc_mode_line_display_parent_spine_survives_nested_eval_gen1() {
    assert_accumulated_property_survives(true, PropertySource::NestedEval, FormatEntry::Display);
}
