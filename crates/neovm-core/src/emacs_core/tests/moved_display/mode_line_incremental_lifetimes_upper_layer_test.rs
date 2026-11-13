//! mode_line_incremental_lifetimes_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::xdisp::*;
use crate::window::DisplayFrameManagerExt;
use neovm_display::test_support::authority::mode_line_incremental_lifetimes_context_for_test as context;
use neovm_display::test_utils::mode_line_incremental_lifetimes_test_upper_layer_helpers::assert_live_interval_plists;
use neovm_edit::buffer::CharPos0;
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::save_scratch_gc_roots;
use neovm_value::value::StringTextPropertyRun;
use neovm_value::value::Value;
use neovm_value::value::get_string_text_properties_table_for_value;
use neovm_value::value::set_string_text_properties_for_value;
#[derive(Debug, PartialEq, Eq)]
struct Rendering {
    bytes: Vec<u8>,
    multibyte: bool,
    chars: usize,
    properties: Vec<std::collections::HashMap<Value, Value>>,
}

fn display_setup(setup: &str) -> (Context, ModeLineDisplayOutput) {
    let mut eval = context(crate::test_utils::new_context);
    let buffer_id = eval.edit.buffers.current_buffer_id().expect("buffer");
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("incremental-lifetimes", 800, 600, buffer_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let format = eval.eval_str(setup).expect("format setup");
    let before = save_scratch_gc_roots();
    let gc = eval.gc_count_for_test();
    let output = format_mode_line_for_display_with_sources(
        &mut eval,
        format,
        Value::make_window(window_id.0),
        Value::make_buffer(buffer_id),
        24,
    );
    assert_eq!(
        eval.gc_count_for_test(),
        gc + 1,
        "fixture must collect exactly once"
    );
    assert_eq!(
        save_scratch_gc_roots(),
        before,
        "formatter leaked scratch roots"
    );
    assert_live_interval_plists(&eval, output.value());
    (eval, output)
}

fn owned_output(numeric: bool) -> (Context, Value, Rendering) {
    let mut eval = context(crate::test_utils::new_context);
    let prefix = if numeric {
        "(list 30 d5-heap-source)"
    } else {
        "(list d5-heap-source \"界\")"
    };
    let setup = format!(
        "(progn
          (setq d5-heap-source (propertize (copy-sequence \"%b|%%|%m\")
                  'face 'bold 'help-echo (list (copy-sequence \"payload\"))))
          (put 'd5-heap-format 'risky-local-variable t)
          (setq d5-heap-format {prefix}))"
    );
    eval.eval_str(&setup).expect("independent heap format");
    let output = neovm_display::test_support::authority::finish_format_mode_line_in_eval_for_test(
        &mut eval,
        &[Value::symbol("d5-heap-format")],
    )
    .expect("independent heap output");
    eval.set_variable("d5-heap-output", output);
    let reference = rendering(output);
    (eval, output, reference)
}

fn rendering(value: Value) -> Rendering {
    let string = value.as_lisp_string().expect("formatted string");
    let table = get_string_text_properties_table_for_value(value);
    Rendering {
        bytes: string.as_bytes().to_vec(),
        multibyte: string.multibyte(),
        chars: string.schars(),
        properties: (0..string.schars())
            .map(|position| {
                table
                    .as_ref()
                    .map(|table| table.get_properties_at_char_pos(CharPos0::new(position)))
                    .unwrap_or_default()
            })
            .collect(),
    }
}

#[test]
fn mode_line_incremental_fresh_source_and_nested_payload_survive_sibling_gc() {
    crate::test_utils::init_test_tracing();
    let (eval, output) = display_setup(
        "(progn (setq d5-root-evals 0)
          (setq d5-root-format
            (list '(:eval (progn (setq d5-root-evals (1+ d5-root-evals))
                                (propertize (copy-sequence \"A\") 'face 'bold
                                  'help-echo (list (copy-sequence \"payload\")))))
                  '(:eval (progn (setq d5-root-evals (1+ d5-root-evals))
                                (garbage-collect) \"B\")))))",
    );
    assert_eq!(output.value().as_utf8_str(), Some("AB"));
    assert_eq!(
        eval.obarray_view().symbol_value_copied("d5-root-evals"),
        Some(Value::fixnum(2))
    );
    let first = output
        .source_spans()
        .iter()
        .find(|span| span.output_start() == 0)
        .expect("first source");
    assert!(eval.tagged_heap().owns_heap_value_for_test(first.source()));
    assert_eq!(first.source().as_utf8_str(), Some("A"));
    assert_ne!(
        first.source(),
        output.value(),
        "retain original source identity"
    );
    let table = get_string_text_properties_table_for_value(output.value()).expect("properties");
    let payload = table
        .get_property_at_char_pos(CharPos0::ZERO, Value::symbol("help-echo"))
        .expect("nested payload");
    assert!(eval.tagged_heap().owns_heap_value_for_test(payload));
    assert!(
        eval.tagged_heap()
            .owns_heap_value_for_test(payload.cons_car())
    );
    assert_eq!(payload.cons_car().as_utf8_str(), Some("payload"));
}

#[test]
fn mode_line_incremental_current_tail_survives_ancestor_detach_and_gc() {
    crate::test_utils::init_test_tracing();
    let (eval, output) = display_setup(
        "(setq d5-root-format
          (list (copy-sequence \"A\")
                '(:eval (progn (setcdr d5-root-format nil)
                               (garbage-collect) \"B\"))
                (propertize (copy-sequence \"C\") 'face 'bold)))",
    );
    // The GNU live iterator has already advanced to the second cons. Detaching
    // the first cons's cdr preserves the current tail and its final element.
    assert_eq!(output.value().as_utf8_str(), Some("ABC"));
    let table = get_string_text_properties_table_for_value(output.value()).expect("properties");
    assert_eq!(
        table.get_property_at_char_pos(CharPos0::new(2), Value::symbol("face")),
        Some(Value::symbol("bold"))
    );
    let last = output.source_spans().last().expect("last source").source();
    assert!(eval.tagged_heap().owns_heap_value_for_test(last));
    assert_eq!(last.as_utf8_str(), Some("C"));
}

#[test]
fn mode_line_incremental_propertize_cached_cdr_survives_mutation_and_gc() {
    crate::test_utils::init_test_tracing();
    let (_eval, output) = display_setup(
        "(progn
          (setq d5-root-child
            (list :propertize
                  (list (copy-sequence \"C\")
                        '(:eval (progn (setcdr d5-root-child nil)
                                       (garbage-collect) \"B\")))
                  'face 'italic))
          (setq d5-root-format
            (list (propertize (copy-sequence \"A\") 'face 'bold)
                  d5-root-child)))",
    );
    assert_eq!(output.value().as_utf8_str(), Some("ACB"));
    let table = get_string_text_properties_table_for_value(output.value()).expect("properties");
    assert_eq!(
        table.get_property_at_char_pos(CharPos0::ZERO, Value::symbol("face")),
        Some(Value::symbol("bold"))
    );
    for pos in [1, 2] {
        assert_eq!(
            table.get_property_at_char_pos(CharPos0::new(pos), Value::symbol("face")),
            Some(Value::symbol("italic"))
        );
    }
}

#[test]
fn mode_line_incremental_split_fresh_format_survives_nested_context_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = context(crate::test_utils::new_context);
    let format = eval
        .eval_str(
            "(setq d5-state-format
          (list (propertize (copy-sequence \"A\") 'help-echo
                            (list (copy-sequence \"prefix-owner\")))
                '(:eval d5-state-first)))",
        )
        .expect("rooted prefix and split format");
    // Independent manager copies permit collection through the real owning
    // Context without aliasing fields borrowed by the compatibility callback.
    // Every Value still belongs to that same active heap and mutator thread.
    let obarray = (*eval.obarray_view()).clone();
    let mut buffers = eval.edit.buffers.clone();
    let frames = crate::window::FrameManager::new();
    let processes = neovm_eval::emacs_core::eval::data::process_state::ProcessManager::new();
    let before = save_scratch_gc_roots();
    let gc = eval.gc_count_for_test();
    let mut calls = Vec::new();
    let output = neovm_display::test_support::authority::finish_format_mode_line_in_state_with_eval_for_test(
        neovm_eval::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &obarray,
        ),
        &[],
        &frames,
        &mut buffers,
        &processes,
        &[format],
        |form, _| {
            calls.push(*form);
            if form.is_bare_symbol_named("d5-state-first") {
                let fresh = Value::string("C");
                set_string_text_properties_for_value(
                    fresh,
                    vec![StringTextPropertyRun {
                        start: 0,
                        end: 1,
                        plist: Value::list(vec![
                            Value::symbol("help-echo"),
                            Value::list(vec![Value::string("fresh-return-owner")]),
                        ]),
                    }],
                );
                Ok(Value::list(vec![
                    fresh,
                    Value::list(vec![
                        Value::symbol(":eval"),
                        Value::symbol("d5-state-second"),
                    ]),
                ]))
            } else {
                assert!(form.is_bare_symbol_named("d5-state-second"));
                eval.gc_collect_exact();
                Ok(Value::string("B"))
            }
        },
    )
    .expect("nested split callback");
    assert_eq!(
        calls,
        vec![
            Value::symbol("d5-state-first"),
            Value::symbol("d5-state-second")
        ]
    );
    assert_eq!(eval.gc_count_for_test(), gc + 1);
    assert_eq!(save_scratch_gc_roots(), before);
    assert_live_interval_plists(&eval, output);
    assert_eq!(output.as_utf8_str(), Some("ACB"));
    let table = get_string_text_properties_table_for_value(output).expect("properties");
    for (pos, expected) in [(0, "prefix-owner"), (1, "fresh-return-owner")] {
        let payload = table
            .get_property_at_char_pos(CharPos0::new(pos), Value::symbol("help-echo"))
            .expect("owned payload");
        assert!(eval.tagged_heap().owns_heap_value_for_test(payload));
        assert!(
            eval.tagged_heap()
                .owns_heap_value_for_test(payload.cons_car())
        );
        assert_eq!(payload.cons_car().as_utf8_str(), Some(expected));
    }
    assert!(
        table
            .get_properties_at_char_pos(CharPos0::new(2))
            .is_empty()
    );
}

#[test]
fn mode_line_incremental_outputs_remain_owned_by_independent_heaps() {
    crate::test_utils::init_test_tracing();
    let before = save_scratch_gc_roots();
    let (mut first, first_output, first_reference) = owned_output(false);
    let first_heap = neovm_value::tagged::gc::current_tagged_heap_identity().expect("first heap");
    let (mut second, second_output, second_reference) = owned_output(true);
    let second_heap = neovm_value::tagged::gc::current_tagged_heap_identity().expect("second heap");
    assert_ne!(first_heap, second_heap);
    assert_eq!(save_scratch_gc_roots(), before);
    for (eval, output, reference) in [
        (&mut first, first_output, first_reference),
        (&mut second, second_output, second_reference),
    ] {
        eval.setup_thread_locals();
        eval.eval_str("(setq d5-heap-source nil d5-heap-format nil)")
            .expect("detach inputs");
        let gc = eval.gc_count_for_test();
        eval.gc_collect_exact();
        assert_eq!(eval.gc_count_for_test(), gc + 1);
        assert_live_interval_plists(eval, output);
        assert_eq!(rendering(output), reference);
    }
    assert_eq!(save_scratch_gc_roots(), before);
}
