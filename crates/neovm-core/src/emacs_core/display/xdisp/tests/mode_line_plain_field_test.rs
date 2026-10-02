//! Direct property-free percent fields, compared with temporary segments.
//! The owned-data trace follows the borrowed-property differential helper,
//! which remains private and unchanged in its separate sibling module.

use super::*;

struct PlainFieldGuard;

impl PlainFieldGuard {
    fn set(enabled: bool, borrowed: bool) -> Self {
        set_mode_line_plain_field_for_test(Some(enabled));
        set_mode_line_prop_borrow_for_test(Some(borrowed));
        Self
    }
}

impl Drop for PlainFieldGuard {
    fn drop(&mut self) {
        set_mode_line_prop_borrow_for_test(None);
        set_mode_line_plain_field_for_test(None);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct StringTrace {
    bytes: Vec<u8>,
    multibyte: bool,
    char_len: usize,
    interval_plists: String,
}

#[derive(Debug, PartialEq, Eq)]
struct SourceTrace {
    output_start: usize,
    output_end: usize,
    source_start: usize,
    source_ordinal: usize,
    is_root: bool,
    is_input_source: bool,
    text: StringTrace,
}

#[derive(Debug, PartialEq, Eq)]
struct OutputTrace {
    text: StringTrace,
    sources: Vec<SourceTrace>,
    eval_count: i64,
    point: usize,
}

#[derive(Clone, Copy)]
enum PropertyOrder {
    Exact,
    Canonical,
}

fn string_trace(eval: &mut Context, value: Value, order: PropertyOrder) -> StringTrace {
    let string = value.as_lisp_string().expect("mode-line string");
    let bytes = string.as_bytes().to_vec();
    let multibyte = string.is_multibyte();
    let char_len = string.schars();
    // No interval borrow crosses these Lisp calls. Literal-only cases keep
    // exact plist order. Percent expansion's existing HashMap overlay order
    // is random even for two copied-baseline walks, so those comparisons sort
    // property pairs within each output run, retaining boundaries and values.
    // Original source strings always use Exact below.
    eval.set_variable("d5-plain-field-result", value);
    let inspect = match order {
        PropertyOrder::Exact => "(prin1-to-string (object-intervals d5-plain-field-result))",
        PropertyOrder::Canonical => {
            r#"(prin1-to-string
          (mapcar
           (lambda (run)
             (let ((plist (nth 2 run)) (pairs nil))
               (while plist
                 (setq pairs (cons (cons (car plist) (car (cdr plist))) pairs))
                 (setq plist (cdr (cdr plist))))
               (list (car run) (car (cdr run))
                     (apply #'append
                            (mapcar (lambda (pair) (list (car pair) (cdr pair)))
                                    (sort pairs
                                          (lambda (a b)
                                            (string-lessp (prin1-to-string (car a))
                                                          (prin1-to-string (car b))))))))))
           (object-intervals d5-plain-field-result)))"#
        }
    };
    let interval_plists = eval
        .eval_str(inspect)
        .expect("object intervals")
        .as_utf8_str()
        .expect("printed intervals")
        .to_owned();
    StringTrace {
        bytes,
        multibyte,
        char_len,
        interval_plists,
    }
}

fn output_trace(setup: &str, enabled: bool, borrowed: bool, order: PropertyOrder) -> OutputTrace {
    crate::test_utils::init_test_tracing();
    let _guard = PlainFieldGuard::set(enabled, borrowed);
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("plain-field", 800, 600, buffer_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    eval.buffers
        .get_mut(buffer_id)
        .expect("buffer")
        .insert("first\nsecond\n");
    eval.eval_str("(setq d5-plain-field-evals 0)")
        .expect("eval counter");
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'A', 0xff, b'B', b'%', b'b', b'Z', 0x80,
    ]));
    eval.set_variable("d5-plain-field-raw", raw);
    let format = eval.eval_str(setup).expect("format setup");
    eval.set_variable("d5-plain-field-format", format);
    let output = format_mode_line_for_display_with_sources(
        &mut eval,
        format,
        Value::make_window(window_id.0),
        Value::make_buffer(buffer_id),
        24,
    );
    // Fresh :eval strings are owned only by the output sidecar. Root the
    // complete sidecar before introspection calls that could collect.
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(output.value());
    for span in output.source_spans() {
        eval.push_specpdl_root(span.source());
    }
    let input_source = eval.obarray.symbol_value("d5-plain-field-source").copied();
    let mut identities = Vec::new();
    let text = string_trace(&mut eval, output.value(), order);
    let sources = output
        .source_spans()
        .iter()
        .map(|span| {
            let source = span.source();
            let source_ordinal =
                if let Some(index) = identities.iter().position(|seen| *seen == source) {
                    index
                } else {
                    identities.push(source);
                    identities.len() - 1
                };
            SourceTrace {
                output_start: span.output_start(),
                output_end: span.output_end(),
                source_start: span.source_start(),
                source_ordinal,
                is_root: source == output.value(),
                is_input_source: input_source == Some(source),
                text: string_trace(&mut eval, source, PropertyOrder::Exact),
            }
        })
        .collect();
    let eval_count = eval
        .obarray
        .symbol_value("d5-plain-field-evals")
        .copied()
        .and_then(Value::as_fixnum)
        .expect("eval counter");
    let point = eval
        .buffers
        .get(buffer_id)
        .expect("buffer")
        .point_char_pos()
        .get();
    eval.restore_specpdl_roots(root_scope);
    OutputTrace {
        text,
        sources,
        eval_count,
        point,
    }
}

fn compare(setup: &str) {
    for borrowed in [false, true] {
        let temporary = output_trace(setup, false, borrowed, PropertyOrder::Canonical);
        let direct = output_trace(setup, true, borrowed, PropertyOrder::Canonical);
        assert_eq!(
            direct, temporary,
            "borrowed={borrowed}; format setup: {setup}"
        );
    }
}

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
        .text_props
        .debug_syntax_caches_consistent()
        .expect("syntax cache");
    let syntax_free_end = rendered
        .text_props
        .syntax_prop_free_run_end(
            CharPos0::new(rendered.char_len().saturating_sub(1)),
            CharPos0::new(rendered.char_len()),
        )
        .get();
    let transitions = rendered
        .min_width_transitions
        .iter()
        .map(|transition| {
            let mut padding_properties: Vec<_> = transition
                .run
                .padding_properties
                .iter()
                .map(|(name, value)| (*name, *value))
                .collect();
            padding_properties.sort_unstable_by_key(|(name, _)| name.bits());
            TransitionTrace {
                output_start: transition.output_start,
                width_spec: transition.run.width_spec,
                columns: transition.run.columns,
                padding_properties,
            }
        })
        .collect();
    RenderedTrace {
        text: rendered.text.clone(),
        intervals: rendered.text_props.interval_plist_runs_for_test(),
        mutation_tick: rendered.text_props.mutation_tick(),
        syntax_tick: rendered.text_props.syntax_prop_tick(),
        syntax_free_end,
        sources: rendered.source_spans.clone(),
        transitions,
    }
}

#[test]
fn empty_fields_preserve_ticks_intervals_sources_and_min_width_transitions() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let source = eval
        .eval_str(r#"(propertize "前A" 'face 'bold 'mouse-face 'highlight 'syntax-table '(2))"#)
        .expect("source");
    eval.set_variable("d5-plain-field-source", source);
    let min_width = eval
        .eval_str("'(display (min-width (6.0)) face italic)")
        .expect("min width");
    let mut seed = ModeLineRendered::default();
    seed.append_string_value_preserving_props(&source);
    seed.note_display_min_width_transition(min_width);
    seed.text_props
        .syntax_prop_free_run_end(CharPos0::ZERO, CharPos0::new(seed.char_len()));
    let properties = std::collections::HashMap::new();
    let mut traces = Vec::new();
    for enabled in [false, true] {
        let _guard = PlainFieldGuard::set(enabled, false);
        let mut rendered = seed.clone();
        for (text, width) in [("", 0), ("été😀", -4), ("A", 7), ("", 3), ("café", 2)] {
            append_mode_line_percent_string_spec(&mut rendered, text, &properties, width);
        }
        rendered.append_string_value_preserving_props(&source);
        traces.push(rendered_trace(&rendered));
    }
    assert_eq!(traces[1], traces[0]);
}

#[test]
fn propertized_fields_keep_graft_plist_order_and_ticks() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let source = eval
        .eval_str(r#"(propertize "AB" 'face 'bold 'help-echo "prefix")"#)
        .expect("source");
    eval.set_variable("d5-plain-field-source", source);
    let mut seed = ModeLineRendered::default();
    seed.append_string_value_preserving_props(&source);
    let properties = std::collections::HashMap::from([
        (Value::symbol("face"), Value::symbol("italic")),
        (Value::symbol("mouse-face"), Value::symbol("highlight")),
    ]);
    let mut traces = Vec::new();
    for enabled in [false, true] {
        let _guard = PlainFieldGuard::set(enabled, false);
        let mut rendered = seed.clone();
        // Both arms clone the same map/hasher, so exact plist order is stable.
        append_mode_line_percent_string_spec(&mut rendered, "éB", &properties, 7);
        append_mode_line_percent_string_spec(&mut rendered, "", &properties, 3);
        rendered.append_string_value_preserving_props(&source);
        traces.push(rendered_trace(&rendered));
    }
    assert_eq!(traces[1], traces[0]);
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

#[test]
fn eval_source_mutation_and_fresh_mouse_targets_match_temporary_segments() {
    compare(
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
