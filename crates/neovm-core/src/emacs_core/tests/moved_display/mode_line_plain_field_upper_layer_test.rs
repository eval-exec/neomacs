//! mode_line_plain_field_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::xdisp::*;
use neovm_display::test_support::authority::{
    mode_line_plain_field_compare_for_test as compare,
    xdisp_interactive_context_for_test as interactive_context,
};
use neovm_display::test_utils::mode_line_plain_field_test_upper_layer_helpers::PropertyRead;
use neovm_edit::buffer::CharPos0;
use neovm_value::value::Value;
#[derive(Debug, PartialEq, Eq)]
struct TransitionTrace {
    output_start: usize,
    width_spec: Value,
    columns: usize,
    padding_properties: Vec<(Value, Value)>,
}

#[derive(Debug, PartialEq, Eq)]
struct RenderedTrace {
    text: Vec<u32>,
    intervals: Vec<(usize, usize, Vec<(Value, Value)>)>,
    mutation_tick: u64,
    syntax_tick: u64,
    syntax_free_end: usize,
    sources: Vec<ModeLineDisplaySourceSpan>,
    transitions: Vec<TransitionTrace>,
}

fn rendered_trace(rendered: &ModeLineRendered) -> RenderedTrace {
    rendered
        .text_props()
        .debug_syntax_caches_consistent()
        .expect("syntax cache");
    let syntax_free_end = rendered
        .text_props()
        .syntax_prop_free_run_end(
            CharPos0::new(rendered.char_len().saturating_sub(1)),
            CharPos0::new(rendered.char_len()),
        )
        .get();
    let transitions = rendered
        .min_width_transitions()
        .iter()
        .map(|transition| {
            let mut padding_properties: Vec<_> = transition
                .run()
                .padding_properties()
                .iter()
                .map(|(name, value)| (*name, *value))
                .collect();
            padding_properties.sort_unstable_by_key(|(name, _)| name.bits());
            TransitionTrace {
                output_start: transition.output_start(),
                width_spec: transition.run().width_spec(),
                columns: transition.run().columns(),
                padding_properties,
            }
        })
        .collect();
    RenderedTrace {
        text: rendered.text().to_vec(),
        intervals: rendered.text_props().interval_plist_runs_for_test(),
        mutation_tick: rendered.text_props().mutation_tick(),
        syntax_tick: rendered.text_props().syntax_prop_tick(),
        syntax_free_end,
        sources: rendered.source_spans().to_vec(),
        transitions,
    }
}

#[test]
fn empty_fields_preserve_ticks_intervals_sources_and_min_width_transitions() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context(crate::test_utils::new_context);
    let source = eval
        .eval_str(r#"(propertize "前A" 'face 'bold 'mouse-face 'highlight 'syntax-table '(2))"#)
        .expect("source");
    eval.set_variable("d5-plain-field-source", source);
    let min_width = eval
        .eval_str("'(display (min-width (6.0)) face italic)")
        .expect("min width");
    let mut seed = ModeLineRendered::default();
    neovm_display::test_support::authority::mode_line_rendered_append_string_value_preserving_props(
        &mut seed, &source,
    );
    neovm_display::test_support::authority::mode_line_rendered_note_display_min_width_transition(
        &mut seed, min_width,
    );
    seed.text_props()
        .syntax_prop_free_run_end(CharPos0::ZERO, CharPos0::new(seed.char_len()));
    let properties: Vec<(Value, Value)> = Vec::new();
    let mut traces = Vec::new();
    for path in [
        ModeLinePlainFieldPath::Temporary,
        ModeLinePlainFieldPath::Direct,
    ] {
        let _guard = neovm_display::test_support::authority::plain_field_guard_for_test(
            path,
            PropertyRead::Copied,
        );
        let mut rendered = seed.clone();
        for (text, width) in [("", 0), ("été😀", -4), ("A", 7), ("", 3), ("café", 2)] {
            neovm_display::test_support::authority::append_mode_line_percent_string_spec(
                &mut rendered,
                text,
                &properties,
                width,
            );
        }
        neovm_display::test_support::authority::mode_line_rendered_append_string_value_preserving_props(&mut rendered, &source);
        traces.push(rendered_trace(&rendered));
    }
    assert_eq!(traces[1], traces[0]);
}

#[test]
fn propertized_fields_keep_graft_plist_order_and_ticks() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context(crate::test_utils::new_context);
    let source = eval
        .eval_str(r#"(propertize "AB" 'face 'bold 'help-echo "prefix")"#)
        .expect("source");
    eval.set_variable("d5-plain-field-source", source);
    let mut seed = ModeLineRendered::default();
    neovm_display::test_support::authority::mode_line_rendered_append_string_value_preserving_props(
        &mut seed, &source,
    );
    let properties = vec![
        (Value::symbol("face"), Value::symbol("italic")),
        (Value::symbol("mouse-face"), Value::symbol("highlight")),
    ];
    let mut traces = Vec::new();
    for path in [
        ModeLinePlainFieldPath::Temporary,
        ModeLinePlainFieldPath::Direct,
    ] {
        let _guard = neovm_display::test_support::authority::plain_field_guard_for_test(
            path,
            PropertyRead::Copied,
        );
        let mut rendered = seed.clone();
        // Borrowed iteration and the cloned reference retain the same map order.
        neovm_display::test_support::authority::append_mode_line_percent_string_spec(
            &mut rendered,
            "éB",
            &properties,
            7,
        );
        neovm_display::test_support::authority::append_mode_line_percent_string_spec(
            &mut rendered,
            "",
            &properties,
            3,
        );
        neovm_display::test_support::authority::mode_line_rendered_append_string_value_preserving_props(&mut rendered, &source);
        traces.push(rendered_trace(&rendered));
    }
    assert_eq!(traces[1], traces[0]);
}

#[test]
fn eval_source_mutation_and_fresh_mouse_targets_match_temporary_segments() {
    compare(
        crate::test_utils::new_context,
        r#"(progn
      (setq d5-plain-field-source "A%12bZ")
      (list d5-plain-field-source
            '(:eval (progn
                      (setq d5-plain-field-evals (1+ d5-plain-field-evals))
                      (put-text-property 0 (length d5-plain-field-source) 'face 'italic d5-plain-field-source)
                      (put-text-property 0 (length d5-plain-field-source) 'help-echo "after" d5-plain-field-source)
                      d5-plain-field-source))
            '(:eval (progn
                      (setq d5-plain-field-evals (1+ d5-plain-field-evals))
                      (propertize (copy-sequence "E%l!") 'help-echo "fresh"
                                  'local-map '(keymap (mouse-1 . ignore)))))
            "%-"))"#,
    );
}

#[test]
fn native_fields_keep_rooted_grafts_and_syntax_ticks_across_collection() {
    crate::test_utils::init_test_tracing();
    static_assertions::const_assert_eq!(std::mem::size_of::<ModeLinePlainFieldPath>(), 1);
    let mut eval = interactive_context(crate::test_utils::new_context);
    let source = eval
        .eval_str(r#"(propertize "前A" 'face 'bold 'syntax-table '(2))"#)
        .expect("source");
    eval.set_variable("d5-plain-field-source", source);
    let syntax = eval.eval_str("'(3)").expect("syntax property");
    eval.set_variable("d5-plain-field-syntax", syntax);
    for properties in [
        Vec::new(),
        vec![(Value::symbol("face"), Value::symbol("italic"))],
        vec![
            (Value::symbol("face"), Value::symbol("italic")),
            (Value::symbol("syntax-table"), syntax),
        ],
    ] {
        let mut traces = Vec::new();
        for path in [
            ModeLinePlainFieldPath::Temporary,
            ModeLinePlainFieldPath::Direct,
        ] {
            let _guard = neovm_display::test_support::authority::plain_field_guard_for_test(
                path,
                PropertyRead::Copied,
            );
            let _roots = neovm_display::test_support::authority::scratch_roots_for_test();
            let mut rendered = ModeLineRendered::default();
            neovm_display::test_support::authority::mode_line_rendered_append_string_value_preserving_props(&mut rendered, &source);
            neovm_display::test_support::authority::mode_line_rendered_root_for_walk(&mut rendered);
            for (text, width) in [("", 0), ("abc", 7), ("été😀", -1), ("", 3), ("é", 1)] {
                neovm_display::test_support::authority::append_mode_line_percent_string_spec(
                    &mut rendered,
                    text,
                    &properties,
                    width,
                );
                eval.eval_str("(garbage-collect)").expect("collection");
                neovm_display::test_support::authority::mode_line_rendered_append_string_value_preserving_props(&mut rendered, &source);
            }
            traces.push(rendered_trace(&rendered));
        }
        assert_eq!(traces[1], traces[0]);
    }
}
