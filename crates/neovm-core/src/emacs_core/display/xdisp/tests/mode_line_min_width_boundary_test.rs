//! Direct-display regressions with rows collected from a fresh live GNU PTY
//! run. Only terminal fill after the final `|` was removed from the fixture.
use super::mode_line_numeric_padding_test::PaddingPolicyGuard;
use super::*;

const GNU_ROWS: &str = include_str!("mode_line_min_width_boundary_gnu_rows.tsv");

fn expected(kind: &str) -> &str {
    GNU_ROWS
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .find_map(|(name, row)| (name == kind).then_some(row))
        .expect("fresh GNU case exists")
}

fn format_for(kind: &str) -> &'static str {
    match kind {
        "mode-name" => r#"'((:propertize "AB" display (min-width (10.0))) "%m" "|")"#,
        "frame-name" => r#"'((:propertize "AB" display (min-width (10.0))) "%F" "|")"#,
        "eol-indicator" => r#"'((:propertize "AB" display (min-width (10.0))) "%Z" "|")"#,
        "other-source-literal" => {
            r#"'((:propertize "AB" display (min-width (10.0))) "%l rest" "|")"#
        }
        "same-source-predecessor" => {
            r#"(let* ((width (list 10.0))
                       (display (list 'min-width width))
                       (source (copy-sequence "%m rest")))
                  (put-text-property 0 2 'display display source)
                  (list (list :propertize "AB" 'display display) source "|"))"#
        }
        "other-identity-predecessor" => {
            r#"(let* ((width (list 10.0))
                       (display (list 'min-width width))
                       (source (copy-sequence "%m rest")))
                  (put-text-property 0 2 'display (list 'min-width (list 10.0)) source)
                  (list (list :propertize "AB" 'display display) source "|"))"#
        }
        _ => panic!("unknown GNU boundary case"),
    }
}

fn compare_fresh_gnu_row(kind: &str) {
    crate::test_utils::init_test_tracing();
    let _policy = PaddingPolicyGuard::set(true);
    let mut eval = interactive_context();
    utf8_locale_terminal(&mut eval);
    let buffer_id = eval.buffers.current_buffer_id().expect("buffer");
    let frame_id = eval.frames.create_frame("D5-F", 800, 600, buffer_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    eval.buffers
        .set_buffer_local_property(buffer_id, "mode-name", Value::string("MODE"))
        .expect("set mode name");
    eval.buffers
        .set_buffer_local_property(
            buffer_id,
            "buffer-file-coding-system",
            Value::symbol("utf-8-unix"),
        )
        .expect("set buffer coding");
    eval.obarray
        .set_symbol_value("eol-mnemonic-unix", Value::string("EOL"));
    let format = eval.eval_str(format_for(kind)).expect("GNU format setup");
    let output = format_mode_line_for_display_with_sources(
        &mut eval,
        format,
        Value::make_window(window_id.0),
        Value::make_buffer(buffer_id),
        120,
    );
    assert_eq!(output.value().as_utf8_str(), Some(expected(kind)), "{kind}");
    let closing = output.source_spans().last().expect("closing source");
    assert_eq!(closing.source().as_utf8_str(), Some("|"));
    assert_eq!(closing.output_start(), expected(kind).chars().count() - 1);
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

#[test]
fn numeric_padding_same_source_predecessor_matches_fresh_gnu_row() {
    compare_fresh_gnu_row("same-source-predecessor");
}

#[test]
fn numeric_padding_equal_nonidentical_predecessor_matches_fresh_gnu_row() {
    compare_fresh_gnu_row("other-identity-predecessor");
}
