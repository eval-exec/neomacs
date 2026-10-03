//! Numeric provenance and inherited display-run boundaries. End-user
//! behavior is compared dynamically by the separate live GNU padding oracle.
use super::super::mode_line_numeric_padding::{self, PaddingRanges};
use super::*;
use std::ffi::OsStr;

fn unmarked(ranges: &PaddingRanges, end: usize) -> Vec<std::ops::Range<usize>> {
    let mut result = Vec::new();
    ranges.for_each_unmarked_range(end, |r| result.push(r));
    result
}

#[test]
fn mode_line_numeric_padding_selector_defaults_off() {
    assert!(!mode_line_numeric_padding::parse(None));
    for v in ["", "off", "0", "false", "no", "unknown"] {
        assert!(!mode_line_numeric_padding::parse(Some(OsStr::new(v))));
    }
    for v in ["on", "1", "true", "yes", " ON "] {
        assert!(mode_line_numeric_padding::parse(Some(OsStr::new(v))));
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        assert!(!mode_line_numeric_padding::parse(Some(OsStr::from_bytes(
            b"\xff"
        ))));
    }
    let previous = mode_line_numeric_padding::force_for_test(Some(true));
    assert!(mode_line_numeric_padding::enabled());
    mode_line_numeric_padding::force_for_test(previous);
}

#[test]
fn mode_line_numeric_padding_clip_and_offset_preserve_character_ranges() {
    let mut ranges = PaddingRanges::default();
    ranges.mark(5..10);
    let clipped = ranges.clipped(7);
    assert_eq!(unmarked(&clipped, 7), vec![0..5]);
    let mut joined = PaddingRanges::default();
    joined.append_shifted(&clipped, 8);
    assert_eq!(unmarked(&joined, 19), vec![0..13, 15..19]);
}

#[test]
fn mode_line_numeric_padding_stretch_insertion_remains_unmarked() {
    let mut ranges = PaddingRanges::default();
    ranges.mark(3..8);
    ranges.mark(10..12);
    ranges.insert_unmarked(5, 2);
    assert_eq!(unmarked(&ranges, 16), vec![0..3, 5..7, 10..12, 14..16]);
    ranges.insert_unmarked(0, 1);
    assert_eq!(unmarked(&ranges, 17), vec![0..4, 6..8, 11..13, 15..17]);
}

struct PaddingPolicyGuard(Option<bool>);
impl PaddingPolicyGuard {
    fn set(value: bool) -> Self {
        Self(mode_line_numeric_padding::force_for_test(Some(value)))
    }
}
impl Drop for PaddingPolicyGuard {
    fn drop(&mut self) {
        mode_line_numeric_padding::force_for_test(self.0);
    }
}

fn width_props(eval: &mut Context, width: usize, help: &str) -> Value {
    eval.eval_str(&format!(
        r#"'(display (min-width ({width}.0)) help-echo "{help}")"#
    ))
    .expect("min-width properties")
}

#[test]
fn numeric_padding_prefix_starts_inherited_display_run_at_first_text() {
    crate::test_utils::init_test_tracing();
    let _policy = PaddingPolicyGuard::set(true);
    let mut eval = interactive_context();
    let props = width_props(&mut eval, 9, "outer");
    let next_props = width_props(&mut eval, 2, "next");
    let mut field = ModeLineRendered::default();
    append_mode_line_numeric_segment(&mut field, &ModeLineRendered::default(), 5, 0);
    field.append_rendered(&ModeLineRendered::plain("A"));
    field.apply_propertize_properties(props, ModeLineTarget::Display { columns: 80 });
    assert_eq!(field.min_width_transitions.len(), 1);
    assert_eq!(field.min_width_transitions[0].output_start, 5);
    assert!(
        field
            .text_props
            .get_property_at_char_pos(CharPos0::ZERO, Value::symbol("display"))
            .is_none()
    );
    assert!(
        field
            .text_props
            .get_property_at_char_pos(CharPos0::new(4), Value::symbol("help-echo"))
            .is_none()
    );
    assert_eq!(
        field
            .text_props
            .get_property_at_char_pos(CharPos0::new(5), Value::symbol("help-echo"))
            .and_then(|v| v.as_utf8_str().map(str::to_owned)),
        Some("outer".into())
    );
    let mut next = ModeLineRendered::plain("B");
    next.apply_propertize_properties(next_props, ModeLineTarget::Display { columns: 80 });
    field.append_rendered(&next);
    field.realize_display_min_width_transitions();
    assert_eq!(
        field.text.len(),
        15,
        "five plain pad cells + nine-column A run + B"
    );
    assert_eq!(
        unmarked(&field.numeric_padding, field.char_len()),
        vec![5..15]
    );
}

#[test]
fn numeric_padding_only_child_creates_no_inherited_display_run() {
    crate::test_utils::init_test_tracing();
    let _policy = PaddingPolicyGuard::set(true);
    let mut eval = interactive_context();
    let props = width_props(&mut eval, 9, "outer");
    let next_props = width_props(&mut eval, 2, "next");
    let mut field = ModeLineRendered::default();
    append_mode_line_numeric_segment(&mut field, &ModeLineRendered::default(), 5, 0);
    field.apply_propertize_properties(props, ModeLineTarget::Display { columns: 80 });
    assert!(field.min_width_transitions.is_empty());
    assert!(
        field
            .text_props
            .get_property_at_char_pos(CharPos0::ZERO, Value::symbol("display"))
            .is_none()
    );
    let mut next = ModeLineRendered::plain("B");
    next.apply_propertize_properties(next_props, ModeLineTarget::Display { columns: 80 });
    field.append_rendered(&next);
    field.realize_display_min_width_transitions();
    assert_eq!(
        field.text.len(),
        6,
        "a nonexistent outer run must not stretch the five pad cells"
    );
}

#[test]
fn numeric_padding_gaps_keep_same_identity_and_survive_precision() {
    crate::test_utils::init_test_tracing();
    let _policy = PaddingPolicyGuard::set(true);
    let mut eval = interactive_context();
    let props = width_props(&mut eval, 9, "outer");
    let next_props = width_props(&mut eval, 2, "next");
    let mut field = ModeLineRendered::plain("A");
    append_mode_line_numeric_segment(&mut field, &ModeLineRendered::default(), 3, 0);
    field.append_rendered(&ModeLineRendered::plain("B"));
    append_mode_line_numeric_segment(&mut field, &ModeLineRendered::default(), 4, 0);
    let mut field = field.slice_chars(7);
    field.apply_propertize_properties(props, ModeLineTarget::Display { columns: 80 });
    assert_eq!(
        field
            .min_width_transitions
            .iter()
            .map(|t| t.output_start)
            .collect::<Vec<_>>(),
        vec![0, 4]
    );
    assert_eq!(
        field.min_width_transitions[0].run.width_spec,
        field.min_width_transitions[1].run.width_spec
    );
    let mut next = ModeLineRendered::plain("C");
    next.apply_propertize_properties(next_props, ModeLineTarget::Display { columns: 80 });
    field.append_rendered(&next);
    field.realize_display_min_width_transitions();
    assert_eq!(
        field.text.len(),
        10,
        "same identity spans its raw numeric gap without restarting"
    );
    assert_eq!(
        unmarked(&field.numeric_padding, field.char_len()),
        vec![0..1, 4..5, 7..10]
    );
}

fn render_numeric_format(setup: &str) -> (Context, ModeLineDisplayOutput) {
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.current_buffer_id().expect("buffer");
    let frame_id = eval
        .frames
        .create_frame("numeric-padding-string-boundary", 800, 600, buffer_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    let format = eval.eval_str(setup).expect("format setup");
    let output = format_mode_line_for_display_with_sources(
        &mut eval,
        format,
        Value::make_window(window_id.0),
        Value::make_buffer(buffer_id),
        80,
    );
    (eval, output)
}

#[test]
fn numeric_padding_following_plain_string_closes_min_width_like_gnu() {
    crate::test_utils::init_test_tracing();
    let _policy = PaddingPolicyGuard::set(true);
    // These rows come from the live GNU direct-redisplay padding oracle. Its
    // final ordinary string closes the last min-width run; EOF does not.
    for (setup, expected, synthetic_stretch) in [
        (
            r#"'("P[" (:propertize ((5 "") "A")
                 display (min-width (9.0)) help-echo "outer")
                 (:propertize "B" display (min-width (2.0)) help-echo "next") "]")"#,
            "P[     A        B ]",
            17usize,
        ),
        (
            r#"'("P[" (:propertize (5 "")
                 display (min-width (9.0)) help-echo "outer")
                 (:propertize "B" display (min-width (2.0)) help-echo "next") "]")"#,
            "P[     B ]",
            8usize,
        ),
        (
            r#"'("P[" (:propertize (-7 ("A" (3 "") "B" (4 "")))
                 display (min-width (9.0)) help-echo "outer")
                 (:propertize "C" display (min-width (2.0)) help-echo "next") "]")"#,
            "P[A   B    C ]",
            12usize,
        ),
    ] {
        let (_eval, output) = render_numeric_format(setup);
        assert_eq!(output.value().as_utf8_str(), Some(expected));
        let properties =
            get_string_text_properties_table_for_value(output.value()).expect("display properties");
        assert!(
            properties
                .get_property_at_char_pos(
                    CharPos0::new(synthetic_stretch),
                    Value::symbol("display")
                )
                .is_none()
        );
        assert_eq!(
            properties
                .get_property_at_char_pos(
                    CharPos0::new(synthetic_stretch),
                    Value::symbol("help-echo")
                )
                .and_then(|value| value.as_utf8_str().map(str::to_owned)),
            Some("next".to_owned())
        );
        assert!(
            output
                .source_spans()
                .iter()
                .all(|span| synthetic_stretch < span.output_start()
                    || synthetic_stretch >= span.output_end()),
            "GNU min-width stretch has no Lisp-string source"
        );
        let closing = output.source_spans().last().expect("closing string source");
        assert_eq!(closing.source().as_utf8_str(), Some("]"));
        assert_eq!(closing.output_start(), expected.chars().count() - 1);
    }
}

#[test]
fn numeric_padding_terminal_min_width_string_stays_unflushed_like_gnu() {
    crate::test_utils::init_test_tracing();
    let _policy = PaddingPolicyGuard::set(true);
    let (_eval, output) =
        render_numeric_format(r#"'("P[" (:propertize "B" display (min-width (2.0))))"#);
    assert_eq!(output.value().as_utf8_str(), Some("P[B"));
}

#[test]
fn numeric_padding_synthetic_gap_keeps_min_width_active_until_next_string() {
    crate::test_utils::init_test_tracing();
    let _policy = PaddingPolicyGuard::set(true);
    let (_eval, output) =
        render_numeric_format(r#"'("P[" (:propertize "B" display (min-width (4.0))) (3 "") "]")"#);
    assert_eq!(output.value().as_utf8_str(), Some("P[B   ]"));
}
