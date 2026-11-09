//! Direct native percent fields, compared with temporary segments.
//! The owned-data trace follows the borrowed-property differential helper,
//! which remains private and unchanged in its separate sibling module.

use super::*;

use crate::test_utils::mode_line_plain_field_test_upper_layer_helpers::{
    OutputTrace, PropertyOrder, PropertyRead, string_trace,
};

fn compare(setup: &str) {
    crate::test_utils::mode_line_plain_field_test_upper_layer_helpers::compare(
        || neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime()),
        setup,
    )
}

fn output_trace(
    setup: &str,
    path: ModeLinePlainFieldPath,
    properties: PropertyRead,
    order: PropertyOrder,
) -> OutputTrace {
    crate::test_utils::mode_line_plain_field_test_upper_layer_helpers::output_trace(
        || neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime()),
        setup,
        path,
        properties,
        order,
    )
}

#[test]
fn plain_percent_fields_preserve_multibyte_identity_like_temporary_segments() {
    crate::test_utils::init_test_tracing();
    let properties: Vec<(Value, Value)> = Vec::new();
    for (spec, width, precision) in [
        ("·", 0, None),
        ("AB·", 7, Some(2)),
        ("A", 4, None),
        ("", 3, None),
    ] {
        let mut identities = Vec::new();
        for path in [
            ModeLinePlainFieldPath::Temporary,
            ModeLinePlainFieldPath::Direct,
        ] {
            let _guard = crate::test_utils::mode_line_plain_field_test_upper_layer_helpers::plain_field_guard_for_test(
                path,
                PropertyRead::Copied,
            );
            let mut rendered = ModeLineRendered::default();
            append_mode_line_percent_string_spec(&mut rendered, spec, &properties, width);
            if let Some(precision) = precision {
                rendered = rendered.slice_chars(precision);
            }
            let value = rendered.into_value(ModeLineFaceSpec {
                no_props: true,
                face: None,
            });
            let string = value.as_lisp_string().expect("percent field string");
            identities.push((string.is_multibyte(), string.as_bytes().to_vec()));
        }
        assert_eq!(identities[1], identities[0], "spec={spec:?}");
    }
}

#[test]
fn plain_status_numeric_coding_and_dash_fields_match_temporary_segments() {
    compare(
        r#"(progn
      (setq buffer-file-name "/tmp/é-rust.rs" mode-name "Rust")
      (list "中文:%12b|%f|%i|%I|%F|%*|%+|%&|%%|%l|%c|%C|%p|%P|%o|%q|%z|%Z|%e|%@|%s|%n"
            "%-"))"#,
    );
}

#[test]
fn raw_literal_bytes_and_percent_fields_match_temporary_segments() {
    compare(
        r#"(progn
      (setq d5-plain-field-source d5-plain-field-raw)
      (list d5-plain-field-source "é:%8l/%c/%4b!" 'd5-plain-field-source))"#,
    );
}

#[test]
fn nested_propertize_precision_and_min_width_match_temporary_segments() {
    compare(
        r#"(list
      '(:propertize "A%8l" display (min-width (12.0)) face italic)
      '(:propertize "Y%p" display (min-width (5.0)))
      '(-4 "é:%b!")
      '(12 (:propertize "%l:%c" face bold)))"#,
    );
}
