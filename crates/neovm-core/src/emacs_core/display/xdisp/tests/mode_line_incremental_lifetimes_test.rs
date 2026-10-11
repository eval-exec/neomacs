//! Independent d5 lifetime checks using the upstream incremental root owner.
//! No alternate root policy, root-slot instrumentation, or GC cache is added.

use super::*;
use crate::emacs_core::eval::save_scratch_gc_roots;

fn context() -> Context {
    let mut eval = interactive_context();
    eval.gc_stress = false;
    eval.tagged_heap.set_gc_threshold(usize::MAX);
    eval
}

fn assert_live_interval_plists(eval: &Context, value: Value) {
    let string = value.as_lisp_string().expect("formatted string");
    let Some(table) = get_string_text_properties_table_for_value(value) else {
        return;
    };
    for run in table.object_interval_plist_runs_for_char_len(CharLen::new(string.schars())) {
        let mut plist = run.plist();
        while plist.is_cons() {
            assert!(
                eval.tagged_heap.owns_heap_value_for_test(plist),
                "GC reclaimed an accumulated plist cell"
            );
            assert!(
                !plist.cons_car().is_dead(),
                "accumulated property name was swept"
            );
            let tail = plist.cons_cdr();
            if !tail.is_cons() {
                break;
            }
            assert!(
                eval.tagged_heap.owns_heap_value_for_test(tail),
                "GC reclaimed an accumulated property value cell"
            );
            assert!(
                !tail.cons_car().is_dead(),
                "accumulated property payload was swept"
            );
            plist = tail.cons_cdr();
        }
    }
}

fn display_setup(setup: &str) -> (Context, ModeLineDisplayOutput) {
    let mut eval = context();
    let buffer_id = eval.buffers.current_buffer_id().expect("buffer");
    let frame_id = eval
        .frames
        .create_frame("incremental-lifetimes", 800, 600, buffer_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    let format = eval.eval_str(setup).expect("format setup");
    let before = save_scratch_gc_roots();
    let gc = eval.gc_count;
    let output = format_mode_line_for_display_with_sources(
        &mut eval,
        format,
        Value::make_window(window_id.0),
        Value::make_buffer(buffer_id),
        24,
    );
    assert_eq!(eval.gc_count, gc + 1, "fixture must collect exactly once");
    assert_eq!(
        save_scratch_gc_roots(),
        before,
        "formatter leaked scratch roots"
    );
    assert_live_interval_plists(&eval, output.value());
    (eval, output)
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
        eval.obarray.symbol_value_copied("d5-root-evals"),
        Some(Value::fixnum(2))
    );
    let first = output
        .source_spans()
        .iter()
        .find(|span| span.output_start() == 0)
        .expect("first source");
    assert!(eval.tagged_heap.owns_heap_value_for_test(first.source()));
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
    assert!(eval.tagged_heap.owns_heap_value_for_test(payload));
    assert!(
        eval.tagged_heap
            .owns_heap_value_for_test(payload.cons_car())
    );
    assert_eq!(payload.cons_car().as_utf8_str(), Some("payload"));
}

#[test]
fn mode_line_incremental_top_format_and_face_survive_replacement_during_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = context();
    let format = eval
        .eval_str(
            "(setq d5-root-format
           (list '(:eval (progn (setq d5-root-format nil d5-root-face nil)
                               (garbage-collect) \"X\")) (copy-sequence \"Y\")))",
        )
        .expect("format");
    let face = eval
        .eval_str("(setq d5-root-face (list :foreground (copy-sequence \"red\")))")
        .expect("face");
    let before = save_scratch_gc_roots();
    let gc = eval.gc_count;
    let output =
        builtin_format_mode_line_ctx(&mut eval, vec![format, face]).expect("format-mode-line");
    assert_eq!(eval.gc_count, gc + 1);
    assert_eq!(save_scratch_gc_roots(), before);
    assert_live_interval_plists(&eval, output);
    assert_eq!(output.as_utf8_str(), Some("XY"));
    let table = get_string_text_properties_table_for_value(output).expect("properties");
    let result_face = table
        .get_property_at_char_pos(CharPos0::ZERO, Value::symbol("face"))
        .expect("default face");
    assert_eq!(result_face, face, "preserve face object identity");
    assert!(eval.tagged_heap.owns_heap_value_for_test(result_face));
    let face_value = result_face.cons_cdr().cons_car();
    assert!(eval.tagged_heap.owns_heap_value_for_test(face_value));
    assert_eq!(face_value.as_utf8_str(), Some("red"));
}

#[test]
fn mode_line_incremental_resolved_symbol_survives_replacement_during_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = context();
    let format = eval
        .eval_str(
            "(progn
           (put 'd5-scalar-format 'risky-local-variable t)
           (setq d5-scalar-format
             (list '(:eval (progn (setq d5-scalar-format nil)
                                  (garbage-collect) \"X\"))
                   (copy-sequence \"Y\")))
           'd5-scalar-format)",
        )
        .expect("resolved format setup");
    let before = save_scratch_gc_roots();
    let gc = eval.gc_count;
    let value = builtin_format_mode_line_ctx(&mut eval, vec![format]).expect("resolved formatter");
    assert_eq!(value.as_utf8_str(), Some("XY"));
    assert_eq!(eval.gc_count, gc + 1);
    assert_eq!(
        eval.obarray.symbol_value_copied("d5-scalar-format"),
        Some(Value::NIL)
    );
    assert_eq!(save_scratch_gc_roots(), before);
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
    assert!(eval.tagged_heap.owns_heap_value_for_test(last));
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
    let mut eval = context();
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
    let obarray = eval.obarray.clone();
    let mut buffers = eval.buffers.clone();
    let frames = crate::window::FrameManager::new();
    let processes = crate::emacs_core::process::ProcessManager::new();
    let before = save_scratch_gc_roots();
    let gc = eval.gc_count;
    let mut calls = Vec::new();
    let output = finish_format_mode_line_in_state_with_eval(
        &obarray,
        &[],
        &frames,
        &mut buffers,
        &processes,
        &[format],
        |form, _| {
            calls.push(*form);
            if form.is_symbol_named("d5-state-first") {
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
                assert!(form.is_symbol_named("d5-state-second"));
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
    assert_eq!(eval.gc_count, gc + 1);
    assert_eq!(save_scratch_gc_roots(), before);
    assert_live_interval_plists(&eval, output);
    assert_eq!(output.as_utf8_str(), Some("ACB"));
    let table = get_string_text_properties_table_for_value(output).expect("properties");
    for (pos, expected) in [(0, "prefix-owner"), (1, "fresh-return-owner")] {
        let payload = table
            .get_property_at_char_pos(CharPos0::new(pos), Value::symbol("help-echo"))
            .expect("owned payload");
        assert!(eval.tagged_heap.owns_heap_value_for_test(payload));
        assert!(
            eval.tagged_heap
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
fn mode_line_incremental_split_panic_releases_accumulator_owners() {
    crate::test_utils::init_test_tracing();
    let mut eval = context();
    let format = eval
        .eval_str(
            "(setq d5-panic-format
          '((:propertize \"A\" face bold)
            (1 (:propertize ((:eval d5-panic)) face italic))))",
        )
        .expect("nested split format");
    let before = save_scratch_gc_roots();
    let mut calls = 0;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = finish_format_mode_line_in_state_with_eval(
            &eval.obarray,
            &[],
            &eval.frames,
            &mut eval.buffers,
            &eval.processes,
            &[format],
            |_, _| {
                calls += 1;
                panic!("deliberate split-state evaluator panic")
            },
        );
    }));
    assert!(result.is_err());
    assert_eq!(calls, 1, "panic must originate in the supplied evaluator");
    assert_eq!(
        save_scratch_gc_roots(),
        before,
        "Rust unwind leaked formatter roots"
    );
}

#[derive(Debug, PartialEq, Eq)]
struct Rendering {
    bytes: Vec<u8>,
    multibyte: bool,
    chars: usize,
    properties: Vec<std::collections::HashMap<Value, Value>>,
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

fn owned_output(numeric: bool) -> (Context, Value, Rendering) {
    let mut eval = context();
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
    let output = finish_format_mode_line_in_eval(&mut eval, &[Value::symbol("d5-heap-format")])
        .expect("independent heap output");
    eval.set_variable("d5-heap-output", output);
    let reference = rendering(output);
    (eval, output, reference)
}

#[test]
fn mode_line_incremental_outputs_remain_owned_by_independent_heaps() {
    crate::test_utils::init_test_tracing();
    let before = save_scratch_gc_roots();
    let (mut first, first_output, first_reference) = owned_output(false);
    let first_heap = crate::tagged::gc::current_tagged_heap_identity().expect("first heap");
    let (mut second, second_output, second_reference) = owned_output(true);
    let second_heap = crate::tagged::gc::current_tagged_heap_identity().expect("second heap");
    assert_ne!(first_heap, second_heap);
    assert_eq!(save_scratch_gc_roots(), before);
    for (eval, output, reference) in [
        (&mut first, first_output, first_reference),
        (&mut second, second_output, second_reference),
    ] {
        eval.setup_thread_locals();
        eval.eval_str("(setq d5-heap-source nil d5-heap-format nil)")
            .expect("detach inputs");
        let gc = eval.gc_count;
        eval.gc_collect_exact();
        assert_eq!(eval.gc_count, gc + 1);
        assert_live_interval_plists(eval, output);
        assert_eq!(rendering(output), reference);
    }
    assert_eq!(save_scratch_gc_roots(), before);
}
