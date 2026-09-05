use super::*;
use crate::buffer::buffer::BUFFER_SLOT_BUFFER_FILE_CODING_SYSTEM;
use crate::buffer::{BufferTextBackendKind, CharPos0, LispCharPos1};
use crate::emacs_core::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::{
    StringTextPropertyRun, ValueKind, get_string_text_properties_table_for_value,
    set_string_text_properties_for_value,
};
use malachite::Integer;

fn interactive_context() -> Context {
    let mut eval = Context::new();
    eval.set_variable("noninteractive", Value::NIL);
    eval
}

fn implemented_text_backends() -> impl Iterator<Item = BufferTextBackendKind> {
    BufferTextBackendKind::implemented_variants()
}

fn convert_current_buffer_text_backend(eval: &mut Context, kind: BufferTextBackendKind) {
    let implemented_kind = kind.implemented().expect("test backend is implemented");
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    eval.buffers
        .get_mut(buffer_id)
        .expect("current buffer")
        .convert_text_backend_kind(implemented_kind);
}

fn fragment_current_buffer(eval: &mut Context, text: &str) {
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    let buffer = eval.buffers.get_mut(buffer_id).expect("current buffer");
    buffer.insert(text);

    let first_fragment = text.find('\n').unwrap_or(text.len()).min(text.len());
    buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(first_fragment));
    buffer.insert("tmp");
    buffer.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
        first_fragment,
        first_fragment + "tmp".len(),
    ));

    let second_fragment = text.len().saturating_sub(1);
    buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(second_fragment));
    buffer.insert("xx");
    buffer.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
        second_fragment,
        second_fragment + "xx".len(),
    ));

    assert_eq!(buffer.buffer_string(), text);
}

#[test]
fn test_register_bootstrap_vars_include_tab_bar_display_vars() {
    crate::test_utils::init_test_tracing();
    let mut obarray = crate::emacs_core::symbol::Obarray::new();
    register_bootstrap_vars(&mut obarray);

    assert_eq!(obarray.symbol_value("inhibit-redisplay"), Some(&Value::NIL));
    assert_eq!(
        obarray.symbol_value("auto-resize-tab-bars"),
        Some(&Value::T)
    );
    // `auto-raise-tab-bar-buttons' is a GNU `DEFVAR_BOOL' (`src/xdisp.c:38704'),
    // so it is declared by `defvar_bool::GNU_BOOL_VARIABLES' rather than here.
    assert_eq!(
        obarray.symbol_value("tab-bar-border"),
        Some(&Value::symbol("internal-border-width"))
    );
    assert_eq!(
        obarray.symbol_value("tab-bar-button-margin"),
        Some(&Value::fixnum(1))
    );
    assert_eq!(
        obarray.symbol_value("fontification-functions"),
        Some(&Value::NIL)
    );
    assert!(obarray.is_special("fontification-functions"));
    let fontification_functions = intern("fontification-functions");
    assert!(
        obarray
            .blv(fontification_functions)
            .is_some_and(|blv| blv.local_if_set)
    );
    for name in [
        "wrap-prefix",
        "line-prefix",
        "display-line-numbers",
        "display-line-numbers-width",
        "display-line-numbers-widen",
        "display-line-numbers-offset",
        "display-fill-column-indicator",
        "display-fill-column-indicator-column",
        "display-fill-column-indicator-character",
    ] {
        let id = intern(name);
        assert!(obarray.is_special(name), "{name} should be special");
        assert!(
            obarray.blv(id).is_some_and(|blv| blv.local_if_set),
            "{name} should be buffer-local-on-set"
        );
    }
    assert_eq!(
        obarray.default_value_id(intern("display-line-numbers-offset")),
        Some(&Value::fixnum(0))
    );
}

#[test]
fn display_line_numbers_assignment_is_buffer_local_on_set() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str("(setq display-line-numbers t)")
        .expect("setq display-line-numbers");
    let buffer = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(
        buffer.get_buffer_local("display-line-numbers"),
        Some(Value::T)
    );
    assert_eq!(
        eval.obarray()
            .default_value_id(intern("display-line-numbers")),
        Some(&Value::NIL)
    );
}

#[test]
fn redisplay_fontification_walks_successive_unfontified_chunks() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let target = eval.buffers.current_buffer_id().expect("current buffer");
    eval.eval_str(
        r#"
        (insert "abcdefghij")
        (setq redisplay-fontify-calls nil)
        (setq fontification-functions
              (list (lambda (start)
                      (setq redisplay-fontify-calls
                            (cons start redisplay-fontify-calls))
                      (let ((end (min (point-max) (+ start 3))))
                        (put-text-property start end 'fontified t)
                        (put-text-property start end 'face 'bold)))))
        "#,
    )
    .unwrap_or_else(|err| panic!("install chunked fontification hook: {err}"));

    let other = eval.buffer_manager_mut().create_buffer("*other*");
    eval.set_current_buffer_unrecorded(other)
        .expect("switch to other buffer");

    let outcome =
        ensure_fontified_for_redisplay(&mut eval, target, 0, 10).expect("fontify target buffer");
    assert_eq!(outcome, RedisplayFontificationOutcome::Fontified);

    assert_eq!(eval.buffers.current_buffer_id(), Some(other));
    eval.set_current_buffer_unrecorded(target)
        .expect("switch back to target");
    let result = eval
        .eval_str(
            r#"(list redisplay-fontify-calls
                     (text-property-not-all 1 (point-max) 'fontified t)
                     (get-text-property 10 'face))"#,
        )
        .expect("inspect fontification result");
    assert_eq!(
        super::super::print::print_value(&result),
        "((10 7 4 1) nil bold)"
    );
}

#[test]
fn test_format_mode_line() {
    crate::test_utils::init_test_tracing();
    let result =
        builtin_format_mode_line(vec![Value::string("test"), Value::symbol("default")]).unwrap();
    assert_eq!(result, Value::string(""));

    let result = builtin_format_mode_line(vec![
        Value::string("test"),
        Value::symbol("default"),
        Value::NIL,
    ])
    .unwrap();
    assert_eq!(result, Value::string(""));

    let err = builtin_format_mode_line(vec![
        Value::string("test"),
        Value::symbol("default"),
        Value::symbol("window"),
    ])
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    let err = builtin_format_mode_line(vec![
        Value::string("test"),
        Value::symbol("default"),
        Value::NIL,
        Value::symbol("buffer"),
    ])
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    assert!(builtin_format_mode_line(vec![]).is_err());
}

#[test]
fn test_format_mode_line_eval_optional_designators() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-format", 80, 24, buffer_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window.0 as i64;

    let ok = builtin_format_mode_line_ctx(
        &mut eval,
        vec![
            Value::string("%b"),
            Value::NIL,
            Value::fixnum(window_id),
            Value::make_buffer(buffer_id),
        ],
    )
    .unwrap();
    // %b expands to the current buffer name
    let buf_name = eval
        .buffers
        .current_buffer()
        .map(|b| b.name_runtime_string_owned())
        .unwrap_or_default();
    assert_eq!(ok, Value::string(buf_name));

    let err = builtin_format_mode_line_ctx(
        &mut eval,
        vec![Value::string("%b"), Value::NIL, Value::string("x")],
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    let err = builtin_format_mode_line_ctx(
        &mut eval,
        vec![
            Value::string("%b"),
            Value::NIL,
            Value::NIL,
            Value::string("x"),
        ],
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }
}

#[test]
fn test_format_mode_line_noninteractive_returns_empty_after_validation() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let rendered = builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%b")]).unwrap();
    assert_eq!(rendered, Value::string(""));

    let err = builtin_format_mode_line_ctx(
        &mut eval,
        vec![Value::string("%b"), Value::NIL, Value::string("x")],
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }
}

#[test]
fn test_resolve_live_window_display_context_uses_selected_window_buffer_point() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let selected_buffer_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-selected-point", 800, 600, selected_buffer_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let buffer = eval
            .buffers
            .get_mut(selected_buffer_id)
            .expect("selected window buffer");
        buffer.insert("abc\ndef\n");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(5));
    }
    let other_id = eval.buffers.create_buffer("*other*");
    eval.buffers.set_current(other_id);

    let ctx = resolve_live_window_display_context(
        &eval.frames,
        &eval.buffers,
        Some(&Value::make_window(selected_window.0)),
    )
    .expect("display context")
    .expect("selected window context");

    assert_eq!(ctx.window_point, LispCharPos1::from_one_based_usize(6));
    assert_eq!(eval.buffers.current_buffer_id(), Some(other_id));
}

#[test]
fn test_format_mode_line_eval_uses_explicit_buffer_instead_of_current_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let saved_current = eval.buffers.current_buffer_id().expect("current buffer");
    let other_id = eval.buffers.create_buffer("*other*");

    let ok = builtin_format_mode_line_ctx(
        &mut eval,
        vec![
            Value::string("%b"),
            Value::NIL,
            Value::NIL,
            Value::make_buffer(other_id),
        ],
    )
    .unwrap();

    assert_eq!(ok, Value::string("*other*"));
    assert_eq!(eval.buffers.current_buffer_id(), Some(saved_current));
}

#[test]
fn test_format_mode_line_eval_uses_window_buffer_instead_of_current_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let saved_current = eval.buffers.current_buffer_id().expect("current buffer");
    let frame_id = eval
        .frames
        .create_frame("xdisp-window", 80, 24, saved_current);
    let other_id = eval.buffers.create_buffer("*window*");
    let window_id = {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        let selected = frame.selected_window;
        let window = frame
            .find_window_mut(selected)
            .expect("selected window on frame");
        match window {
            crate::window::Window::Leaf { buffer_id, .. } => *buffer_id = other_id,
            other => panic!("expected leaf window, got {:?}", other),
        }
        selected.0 as i64
    };

    let ok = builtin_format_mode_line_ctx(
        &mut eval,
        vec![
            Value::string("%b"),
            Value::NIL,
            Value::make_window(window_id as u64),
            Value::NIL,
        ],
    )
    .unwrap();

    assert_eq!(ok, Value::string("*window*"));
    assert_eq!(eval.buffers.current_buffer_id(), Some(saved_current));
}

#[test]
fn test_format_mode_line_in_state_uses_buffer_local_symbols_and_restores_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let saved_current = eval.buffers.current_buffer_id().expect("current buffer");
    let other_id = eval.buffers.create_buffer("*mode-line*");
    eval.buffers
        .set_buffer_local_property(other_id, "mode-name", Value::string("Neo"))
        .expect("mode-name local should set");

    let rendered = format_mode_line_from_state(
        &eval.obarray,
        &[],
        &eval.frames,
        &mut eval.buffers,
        &eval.processes,
        vec![
            Value::list(vec![
                Value::string("%b "),
                Value::symbol("mode-name"),
                Value::string(" "),
                Value::symbol("mode-line-front-space"),
            ]),
            Value::NIL,
            Value::NIL,
            Value::make_buffer(other_id),
        ],
    )
    .expect("format-mode-line shared state")
    .expect("non-:eval format should stay on shared state");

    // Expected shape:
    //   "%b "                 -> "*mode-line* "        (buffer name + literal space)
    //   'mode-name            -> "Neo"                 (buffer-local value via
    //                                                   set_buffer_local_property)
    //   " "                                             (literal)
    //   'mode-line-front-space-> ""                    (unbound in bare
    //                                                   Context::new(): bindings.el
    //                                                   hasn't run, so the symbol
    //                                                   resolves to nil and GNU
    //                                                   xdisp.c:28438-28468 emits
    //                                                   nothing)
    //
    // This used to be asserted as "*mode-line* Neo  " (with two trailing spaces)
    // because the walker hardcoded mode-line-front-space to a space. That
    // short-circuit diverged from GNU and silently dropped the real symbol
    // value (e.g. bindings.el's `(:eval (if (display-graphic-p) " " "-"))`),
    // so it was removed.
    assert_eq!(rendered, Value::string("*mode-line* Neo "));
    assert_eq!(eval.buffers.current_buffer_id(), Some(saved_current));
}

#[test]
fn test_format_mode_line_eval_keeps_shared_buffer_context_around_eval_forms() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let saved_current = eval.buffers.current_buffer_id().expect("current buffer");
    let other_id = eval.buffers.create_buffer("*mode-line-eval*");
    eval.buffers
        .set_buffer_local_property(other_id, "mode-name", Value::string("Neo"))
        .expect("mode-name local should set");

    let rendered = builtin_format_mode_line_ctx(
        &mut eval,
        vec![
            Value::list(vec![
                Value::string("%b "),
                Value::list(vec![Value::symbol(":eval"), Value::symbol("mode-name")]),
            ]),
            Value::NIL,
            Value::NIL,
            Value::make_buffer(other_id),
        ],
    )
    .expect("format-mode-line eval");

    assert_eq!(rendered, Value::string("*mode-line-eval* Neo"));
    assert_eq!(eval.buffers.current_buffer_id(), Some(saved_current));
}

#[test]
fn test_format_mode_line_in_state_with_eval_keeps_shared_buffer_context_around_eval_forms() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let saved_current = eval.buffers.current_buffer_id().expect("current buffer");
    let other_id = eval.buffers.create_buffer("*mode-line-shared-eval*");
    eval.buffers
        .set_buffer_local_property(other_id, "mode-name", Value::string("Neo"))
        .expect("mode-name local should set");

    let rendered = finish_format_mode_line_in_state_with_eval(
        &eval.obarray,
        &[],
        &eval.frames,
        &mut eval.buffers,
        &eval.processes,
        &[
            Value::list(vec![
                Value::string("%b "),
                Value::list(vec![Value::symbol(":eval"), Value::symbol("mode-name")]),
            ]),
            Value::NIL,
            Value::NIL,
            Value::make_buffer(other_id),
        ],
        |form, buffers| {
            assert_eq!(*form, Value::symbol("mode-name"));
            let buffer = buffers.current_buffer().expect("mode-line buffer");
            Ok(buffer
                .get_buffer_local("mode-name")
                .expect("buffer-local mode-name"))
        },
    )
    .expect("format-mode-line shared eval");

    assert_eq!(rendered, Value::string("*mode-line-shared-eval* Neo"));
    assert_eq!(eval.buffers.current_buffer_id(), Some(saved_current));
}

#[test]
fn test_format_mode_line_preserves_raw_unibyte_literal_segments() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));

    let rendered = builtin_format_mode_line_ctx(&mut eval, vec![Value::list(vec![raw])])
        .expect("format-mode-line raw literal");
    let text = rendered
        .as_lisp_string()
        .expect("format-mode-line should return a LispString");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF]);
}

#[test]
fn test_format_mode_line_symbol_conditional_uses_only_selected_branch() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    eval.obarray.set_symbol_value("mode-line-flag", Value::T);

    let then_rendered = builtin_format_mode_line_ctx(
        &mut eval,
        vec![Value::list(vec![
            Value::symbol("mode-line-flag"),
            Value::string("then"),
            Value::list(vec![
                Value::symbol(":eval"),
                Value::list(vec![Value::symbol("error"), Value::string("boom")]),
            ]),
        ])],
    )
    .expect("format-mode-line should use then branch");

    eval.obarray.set_symbol_value("mode-line-flag", Value::NIL);
    let else_rendered = builtin_format_mode_line_ctx(
        &mut eval,
        vec![Value::list(vec![
            Value::symbol("mode-line-flag"),
            Value::list(vec![
                Value::symbol(":eval"),
                Value::list(vec![Value::symbol("error"), Value::string("boom")]),
            ]),
            Value::string("else"),
        ])],
    )
    .expect("format-mode-line should use else branch");

    assert_eq!(then_rendered, Value::string("then"));
    assert_eq!(else_rendered, Value::string("else"));
}

#[test]
fn test_format_mode_line_string_valued_symbols_render_literally() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let other_id = eval.buffers.create_buffer("*mode-line-literal*");
    eval.buffers
        .set_buffer_local_property(other_id, "mode-name", Value::string("%b"))
        .expect("mode-name local should set");

    let rendered = format_mode_line_from_state(
        &eval.obarray,
        &[],
        &eval.frames,
        &mut eval.buffers,
        &eval.processes,
        vec![
            Value::list(vec![Value::string("%b "), Value::symbol("mode-name")]),
            Value::NIL,
            Value::NIL,
            Value::make_buffer(other_id),
        ],
    )
    .expect("format-mode-line shared state")
    .expect("string-valued symbols should not require eval");

    assert_eq!(rendered, Value::string("*mode-line-literal* %b"));
}

#[test]
fn test_format_mode_line_fixnum_elements_pad_and_truncate_tail() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let other_id = eval.buffers.create_buffer("xy");

    let rendered = format_mode_line_from_state(
        &eval.obarray,
        &[],
        &eval.frames,
        &mut eval.buffers,
        &eval.processes,
        vec![
            Value::list(vec![
                Value::list(vec![Value::fixnum(5), Value::string("%b")]),
                Value::string("!"),
                Value::list(vec![Value::fixnum(-1), Value::string("%b")]),
            ]),
            Value::NIL,
            Value::NIL,
            Value::make_buffer(other_id),
        ],
    )
    .expect("format-mode-line shared state")
    .expect("fixnum elements should not require eval");

    assert_eq!(rendered, Value::string("xy   !x"));
}

#[test]
fn test_format_mode_line_percent_specs_keep_gnu_field_width_and_dash_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let other_id = eval.buffers.create_buffer("xy");

    let rendered = format_mode_line_from_state(
        &eval.obarray,
        &[],
        &eval.frames,
        &mut eval.buffers,
        &eval.processes,
        vec![
            Value::string("%5b|%-|%2*"),
            Value::NIL,
            Value::NIL,
            Value::make_buffer(other_id),
        ],
    )
    .expect("format-mode-line shared state")
    .expect("percent specs should not require eval");

    assert_eq!(rendered, Value::string("xy   |--|- "));
}

#[test]
fn test_format_mode_line_respects_risky_local_variable_for_eval_forms() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    eval.obarray.set_symbol_value(
        "unsafe-mode-line",
        Value::list(vec![
            Value::symbol(":eval"),
            Value::list(vec![Value::symbol("error"), Value::string("boom")]),
        ]),
    );
    eval.obarray.set_symbol_value(
        "trusted-mode-line",
        Value::list(vec![Value::symbol(":eval"), Value::string("ok")]),
    );
    eval.obarray
        .put_property("trusted-mode-line", "risky-local-variable", Value::T)
        .unwrap();

    let suppressed =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::symbol("unsafe-mode-line")])
            .expect("unsafe mode-line variable should be suppressed");
    let allowed = builtin_format_mode_line_ctx(&mut eval, vec![Value::symbol("trusted-mode-line")])
        .expect("trusted mode-line variable should evaluate");

    assert_eq!(suppressed, Value::string(""));
    assert_eq!(allowed, Value::string("ok"));
}

#[test]
fn test_format_mode_line_propertize_preserves_text_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let rendered = builtin_format_mode_line_ctx(
        &mut eval,
        vec![Value::list(vec![
            Value::symbol(":propertize"),
            Value::string("abc"),
            Value::symbol("face"),
            Value::symbol("bold"),
            Value::symbol("help-echo"),
            Value::string("h"),
        ])],
    )
    .expect("format-mode-line propertize");

    assert_eq!(rendered.as_utf8_str(), Some("abc"));
    assert!(rendered.is_string(), "expected string result");
    let props =
        get_string_text_properties_table_for_value(rendered).expect("mode-line text properties");
    assert_eq!(
        props.get_property_at_char_pos(CharPos0::ZERO, Value::symbol("face")),
        Some(Value::symbol("bold"))
    );
    assert_eq!(
        props.get_property_at_char_pos(CharPos0::ZERO, Value::symbol("help-echo")),
        Some(Value::string("h"))
    );
}

#[test]
fn test_format_mode_line_percent_specs_preserve_source_string_text_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("fmt-prop-buffer");
    eval.buffers.set_current(buffer_id);

    let format = Value::string("%b!");
    assert!(format.is_string(), "expected string format");
    set_string_text_properties_for_value(
        format,
        vec![StringTextPropertyRun {
            start: 0,
            end: 3,
            plist: Value::list(vec![
                Value::symbol("face"),
                Value::symbol("bold"),
                Value::symbol("help-echo"),
                Value::string("h"),
            ]),
        }],
    );

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![format]).expect("format-mode-line props");

    assert_eq!(rendered.as_utf8_str(), Some("fmt-prop-buffer!"));
    if !rendered.is_string() {
        panic!("expected string result");
    };
    let props =
        get_string_text_properties_table_for_value(rendered).expect("mode-line text properties");
    assert_eq!(
        props.get_property_at_char_pos(CharPos0::ZERO, Value::symbol("face")),
        Some(Value::symbol("bold"))
    );
    assert_eq!(
        props.get_property_at_char_pos(CharPos0::ZERO, Value::symbol("help-echo")),
        Some(Value::string("h"))
    );
    let last_char = "fmt-prop-buffer".chars().count();
    assert_eq!(
        props.get_property_at_char_pos(CharPos0::new(last_char), Value::symbol("face")),
        Some(Value::symbol("bold"))
    );
    assert_eq!(
        props.get_property_at_char_pos(CharPos0::new(last_char), Value::symbol("help-echo")),
        Some(Value::string("h"))
    );
}

#[test]
fn mode_line_display_preserves_nested_literal_string_sources() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("mode-line-string-sources", 800, 600, buffer_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    let first = Value::string(" first ");
    let second = Value::string(" second ");

    let output = format_mode_line_for_display_with_sources(
        &mut eval,
        Value::list(vec![first, second]),
        Value::make_window(window_id.0),
        Value::make_buffer(buffer_id),
        80,
    );

    assert_eq!(output.value(), Value::string(" first  second "));
    assert_eq!(
        output.source_spans(),
        &[
            ModeLineDisplaySourceSpan::new(0, 7, first, 0),
            ModeLineDisplaySourceSpan::new(7, 15, second, 0),
        ]
    );
    assert_eq!(output.source_spans()[1].source_position(10), Some(3));
}

#[test]
fn mode_line_percent_field_padding_inherits_the_format_string_face() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("xy");
    eval.buffers.set_current(buffer_id);

    let format = Value::string("%12b");
    set_string_text_properties_for_value(
        format,
        vec![StringTextPropertyRun {
            start: 0,
            end: 4,
            plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
        }],
    );

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![format]).expect("format padded %b field");
    assert_eq!(rendered.as_utf8_str(), Some("xy          "));
    let props =
        get_string_text_properties_table_for_value(rendered).expect("mode-line text properties");
    assert_eq!(
        props.get_property_at_char_pos(CharPos0::new(11), Value::symbol("face")),
        Some(Value::symbol("bold")),
        "GNU applies the source format face to the complete padded %b field",
    );
}

#[test]
fn test_format_mode_line_status_specs_match_gnu_buffer_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("status-buffer");
    eval.buffers.set_current(buffer_id);
    {
        let buffer = eval.buffers.get_mut(buffer_id).expect("buffer");
        buffer.insert("abc");
        buffer.set_modified(true);
        buffer.set_buffer_local("buffer-read-only", Value::T);
    }

    let status =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%*|%+|%&")]).expect("status");
    assert_eq!(status, Value::string("%|*|*"));

    {
        let buffer = eval.buffers.get_mut(buffer_id).expect("buffer");
        buffer.set_buffer_local("buffer-read-only", Value::NIL);
        buffer.set_modified(false);
        buffer.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(1, 2));
    }

    let narrowed =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%n")]).expect("narrow");
    assert_eq!(narrowed, Value::string(" Narrow"));
}

#[test]
fn test_format_mode_line_face_argument_adds_default_face_and_merges_explicit_face() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let rendered = builtin_format_mode_line_ctx(
        &mut eval,
        vec![
            Value::list(vec![
                Value::list(vec![
                    Value::symbol(":propertize"),
                    Value::string("a"),
                    Value::symbol("face"),
                    Value::symbol("italic"),
                ]),
                Value::string("b"),
            ]),
            Value::symbol("bold"),
        ],
    )
    .expect("format-mode-line face arg");

    assert_eq!(rendered.as_utf8_str(), Some("ab"));
    assert!(rendered.is_string(), "expected string result");
    let props =
        get_string_text_properties_table_for_value(rendered).expect("mode-line text properties");
    assert_eq!(
        props.get_property_at_char_pos(CharPos0::ZERO, Value::symbol("face")),
        Some(Value::list(vec![
            Value::symbol("italic"),
            Value::symbol("bold")
        ]))
    );
    assert_eq!(
        props.get_property_at_char_pos(CharPos0::new(1), Value::symbol("face")),
        Some(Value::symbol("bold"))
    );
}

#[test]
fn test_format_mode_line_integer_face_argument_discards_text_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let rendered = builtin_format_mode_line_ctx(
        &mut eval,
        vec![
            Value::list(vec![
                Value::symbol(":propertize"),
                Value::string("abc"),
                Value::symbol("face"),
                Value::symbol("bold"),
                Value::symbol("help-echo"),
                Value::string("h"),
            ]),
            Value::fixnum(0),
        ],
    )
    .expect("format-mode-line face int");

    assert_eq!(rendered, Value::string("abc"));
    assert!(rendered.is_string(), "expected string result");
    assert!(
        get_string_text_properties_table_for_value(rendered).is_none(),
        "integer FACE arg should drop text properties"
    );
}

#[test]
fn test_format_mode_line_fixnum_padding_does_not_inherit_inner_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let rendered = builtin_format_mode_line_ctx(
        &mut eval,
        vec![Value::list(vec![
            Value::fixnum(5),
            Value::list(vec![
                Value::symbol(":propertize"),
                Value::string("x"),
                Value::symbol("face"),
                Value::symbol("bold"),
            ]),
        ])],
    )
    .expect("format-mode-line fixnum padding");

    assert_eq!(rendered.as_utf8_str(), Some("x    "));
    assert!(rendered.is_string(), "expected string result");
    let props =
        get_string_text_properties_table_for_value(rendered).expect("mode-line text properties");
    assert_eq!(
        props.get_property_at_char_pos(CharPos0::ZERO, Value::symbol("face")),
        Some(Value::symbol("bold"))
    );
    assert_eq!(
        props.get_property_at_char_pos(CharPos0::new(1), Value::symbol("face")),
        None
    );
}

#[test]
fn test_format_mode_line_recursive_depth_specs_match_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();

    eval.command_loop.recursive_depth = 4;
    let shallow =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%[|%]")]).expect("depth 3");
    assert_eq!(shallow, Value::string("[[[|]]]"));

    eval.command_loop.recursive_depth = 7;
    let deep =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%[|%]")]).expect("depth 6");
    assert_eq!(deep, Value::string("[[[... | ...]]]"));
}

#[test]
fn test_format_mode_line_size_and_process_specs_match_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("mode-line-metadata");
    eval.buffers.set_current(buffer_id);
    {
        let buffer = eval.buffers.get_mut(buffer_id).expect("buffer");
        buffer.insert(&"x".repeat(1536));
    }

    let no_process =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%i|%I|%s")]).expect("specs");
    assert_eq!(no_process, Value::string("1536|1.5k|no process"));

    eval.processes.create_process(
        "mode-line-proc".into(),
        Value::make_buffer(buffer_id),
        "cat".into(),
        vec![],
        crate::emacs_core::process::ProcessCodingSystems::gnu_make_process_initial(),
    );
    let with_process =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%i|%I|%s")]).expect("specs");
    assert_eq!(with_process, Value::string("1536|1.5k|run"));
}

#[test]
fn test_format_mode_line_column_c_and_big_c_specs_match_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("col-test");
    eval.buffers.set_current(buffer_id);
    {
        let buffer = eval.buffers.get_mut(buffer_id).expect("buffer");
        buffer.insert("abcdef");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3)); // point at column 3 (0-indexed)
    }

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%c|%C")]).expect("col specs");
    // %c = 0-indexed column (3), %C = 1-indexed column (4)
    assert_eq!(rendered, Value::string("3|4"));
}

#[test]
fn test_format_mode_line_line_and_column_at_deep_position() {
    // Exercises the zero-copy line/column derivation (prefix_line_and_column):
    // point several lines in, so the newline count is > 1 and the column span
    // starts after an interior newline.
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("line-col-deep");
    eval.buffers.set_current(buffer_id);
    {
        let buffer = eval.buffers.get_mut(buffer_id).expect("buffer");
        buffer.insert("l1\nl2\nl3\nabcXdef");
        // point after "l1\nl2\nl3\nabc" -> line 4 (three preceding newlines),
        // column 3 (0-indexed) on that line.
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new("l1\nl2\nl3\nabc".len()));
    }
    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%l|%c")]).expect("specs");
    assert_eq!(rendered, Value::string("4|3"));
}

#[test]
fn format_mode_line_line_number_starts_at_accessible_buffer_beginning() {
    // GNU xdisp.c `decode_mode_spec` initializes `%l`'s line counter at
    // BUF_BEGV, not the full buffer beginning.  Thus narrowing makes the first
    // accessible line line 1 while `%i` reports the same accessible region.
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    eval.eval_str(
        r#"(progn
  (erase-buffer)
  (insert "zero\none\ntwo\nthree\n")
  (narrow-to-region 6 20)
  (goto-char 14))"#,
    )
    .expect("create narrowed mode-line fixture");

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%l|%c|%i")]).expect("specs");

    assert_eq!(rendered, Value::string("3|0|14"));
}

#[test]
fn format_mode_line_line_uses_the_target_windows_point_not_the_buffer_point() {
    // A buffer shown in two windows: `%l` for a NON-selected window must reflect
    // THAT window's own point (GNU displays each window's mode line with the
    // buffer point set to `w->pointm`), not the selected window's live buffer
    // point. Before the fix, `%l` always used the buffer point, so a second
    // window showing the same buffer reported the selected window's line.
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.current_buffer().expect("current buffer").id;
    {
        let buffer = eval.buffers.get_mut(buffer_id).expect("buffer");
        buffer.insert("line1\nline2\nline3\nline4\n");
        // Selected window / live buffer point -> line 1.
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    let frame_id = eval
        .frames
        .create_frame("mode-line-window-point", 800, 600, buffer_id);
    let selected = eval.frames.get(frame_id).expect("frame").selected_window;
    let other = eval
        .frames
        .split_window(
            frame_id,
            selected,
            crate::window::SplitDirection::Horizontal,
            buffer_id,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("split produced a second window");
    // Put the second (non-selected) window's point on line 3 (char 13).
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        match frame.find_window_mut(other).expect("second window") {
            crate::window::Window::Leaf { point, .. } => {
                *point = LispCharPos1::from_one_based_usize(13)
            }
            leaf => panic!("expected leaf window, got {leaf:?}"),
        }
    }

    // The non-selected window's `%l` must be ITS point's line (3), not the
    // selected window's line (1).
    let other_l = builtin_format_mode_line_ctx(
        &mut eval,
        vec![Value::string("%l"), Value::NIL, Value::make_window(other.0)],
    )
    .expect("other window %l");
    assert_eq!(other_l, Value::string("3"));
}

fn format_mode_line_position_backend_trace(kind: BufferTextBackendKind) -> String {
    let mut eval = interactive_context();
    convert_current_buffer_text_backend(&mut eval, kind);
    fragment_current_buffer(&mut eval, "αβ\ncdé\nlast");
    {
        let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
        let buffer = eval.buffers.get_mut(buffer_id).expect("current buffer");
        buffer.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new("αβ\ncd".len()));
        assert_eq!(buffer.text_backend_kind(), kind);
    }

    let rendered = builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%l|%c|%C|%i")])
        .expect("format-mode-line position specs");
    rendered
        .as_utf8_str()
        .expect("position mode-line should be UTF-8")
        .to_owned()
}

#[test]
fn implemented_text_backends_match_format_mode_line_position_specs() {
    crate::test_utils::init_test_tracing();
    let baseline = format_mode_line_position_backend_trace(BufferTextBackendKind::GapBuffer);
    assert_eq!(baseline, "2|2|3|14");

    for kind in implemented_text_backends() {
        assert_eq!(
            format_mode_line_position_backend_trace(kind),
            baseline,
            "{kind:?}"
        );
    }
}

#[test]
fn test_format_mode_line_major_mode_name_spec_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("mode-test");
    eval.buffers.set_current(buffer_id);
    eval.buffers
        .set_buffer_local_property(buffer_id, "mode-name", Value::string("Emacs-Lisp"))
        .expect("set mode-name");

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%m")]).expect("mode spec");
    assert_eq!(rendered, Value::string("Emacs-Lisp"));

    // Default mode-name is "Fundamental"
    let other_id = eval.buffers.create_buffer("default-mode");
    eval.buffers.set_current(other_id);
    let default =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%m")]).expect("default mode");
    assert_eq!(default, Value::string("Fundamental"));
}

#[test]
fn test_format_mode_line_major_mode_name_preserves_raw_unibyte_value() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("raw-mode-test");
    eval.buffers.set_current(buffer_id);
    let raw_mode = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'-', b'M',
    ]));
    eval.buffers
        .set_buffer_local_property(buffer_id, "mode-name", raw_mode)
        .expect("set raw mode-name");

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%m")]).expect("mode spec");
    let text = rendered
        .as_lisp_string()
        .expect("format-mode-line should return a LispString");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF, b'-', b'M']);
}

#[test]
fn test_format_mode_line_frame_name_f_spec_prefers_title_and_preserves_raw_unibyte_value() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("frame-title-test");
    eval.buffers.set_current(buffer_id);

    let frame_id = eval.frames.create_frame("frame-name", 80, 24, buffer_id);
    let raw_title = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'-', b'F',
    ]));
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.title = raw_title;
    }

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%F")]).expect("frame title");
    let text = rendered
        .as_lisp_string()
        .expect("format-mode-line should return a LispString");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF, b'-', b'F']);
}

#[test]
fn test_format_mode_line_frame_name_f_spec_defaults_to_emacs_without_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%F")]).expect("frame name");
    assert_eq!(rendered, Value::string("Emacs"));
}

#[test]
fn test_format_mode_line_frame_name_f_spec_uses_emacs_for_gui_frame_without_explicit_name() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("frame-name-default-test");
    eval.buffers.set_current(buffer_id);

    let frame_id = eval.frames.create_frame("F1", 80, 24, buffer_id);
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("x")));
    }

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%F")]).expect("frame name");
    assert_eq!(rendered, Value::string("Emacs"));
}

#[test]
fn test_format_mode_line_remote_at_spec_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("remote-test");
    eval.buffers.set_current(buffer_id);

    // Local directory → "-"
    eval.buffers
        .set_buffer_local_property(buffer_id, "default-directory", Value::string("/home/user"))
        .expect("set local default-directory");
    let local =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%@")]).expect("local @");
    assert_eq!(local, Value::string("-"));

    // Remote (Tramp-style) directory → "@"
    eval.buffers
        .set_buffer_local_property(
            buffer_id,
            "default-directory",
            Value::string("/ssh:host:/home/user"),
        )
        .expect("set remote default-directory");
    let remote =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%@")]).expect("remote @");
    assert_eq!(remote, Value::string("@"));
}

#[test]
fn test_format_mode_line_remote_at_spec_accepts_raw_unibyte_default_directory() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("remote-raw-test");
    eval.buffers.set_current(buffer_id);
    let mut remote_dir = b"/ssh:host:/home/user".to_vec();
    remote_dir.push(0xFF);
    eval.buffers
        .set_buffer_local_property(
            buffer_id,
            "default-directory",
            Value::heap_string(crate::heap_types::LispString::from_unibyte(remote_dir)),
        )
        .expect("set raw remote default-directory");

    let remote =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%@")]).expect("remote @");
    assert_eq!(remote, Value::string("@"));
}

#[test]
fn test_format_mode_line_coding_system_z_and_big_z_specs_match_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("coding-test");
    eval.buffers.set_current(buffer_id);

    // utf-8-unix → mnemonic 'U', EOL ':'
    eval.buffers
        .set_buffer_local_property(
            buffer_id,
            "buffer-file-coding-system",
            Value::symbol("utf-8-unix"),
        )
        .expect("set coding");
    let z =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%z|%Z")]).expect("coding z");
    assert_eq!(z, Value::string("U|U:"));

    // undecided-dos → mnemonic '-', EOL '\'
    eval.buffers
        .set_buffer_local_property(
            buffer_id,
            "buffer-file-coding-system",
            Value::symbol("undecided-dos"),
        )
        .expect("set coding dos");
    let dos =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%z|%Z")]).expect("coding dos");
    assert_eq!(dos, Value::string("-|-\\"));
}

#[test]
fn test_format_mode_line_blanks_coding_mnemonics_for_unibyte_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("unibyte-coding-test");
    eval.buffers.set_current(buffer_id);
    eval.buffers
        .set_buffer_multibyte_flag(buffer_id, false)
        .expect("make test buffer unibyte");
    eval.buffers
        .set_buffer_local_property(
            buffer_id,
            "buffer-file-coding-system",
            Value::symbol("utf-8-unix"),
        )
        .expect("set coding");

    let window_system = builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%z|%Z")])
        .expect("window-system coding");
    assert_eq!(window_system, Value::string(" | :"));

    eval.frames
        .create_frame("unibyte-tty-coding-frame", 80, 24, buffer_id);
    let tty =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%z|%Z")]).expect("TTY coding");
    assert_eq!(
        tty,
        Value::string("   |   :"),
        "GNU blanks keyboard, terminal, and buffer mnemonics for an unibyte buffer"
    );
}

#[test]
fn test_format_mode_line_big_z_preserves_raw_unibyte_eol_indicator() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("coding-raw-eol-test");
    eval.buffers.set_current(buffer_id);

    eval.buffers
        .set_buffer_local_property(
            buffer_id,
            "buffer-file-coding-system",
            Value::symbol("undecided-dos"),
        )
        .expect("set coding");
    eval.obarray.set_symbol_value(
        "eol-mnemonic-dos",
        Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF])),
    );

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%Z")]).expect("coding Z");
    let text = rendered
        .as_lisp_string()
        .expect("format-mode-line should return a LispString");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[b'-', 0xFF]);
}

#[test]
fn test_format_mode_line_tty_z_uses_live_coding_manager_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("tty-coding-test");
    eval.buffers.set_current(buffer_id);
    eval.frames
        .create_frame("tty-coding-frame", 80, 24, buffer_id);

    eval.buffers
        .set_buffer_local_property(
            buffer_id,
            "buffer-file-coding-system",
            Value::symbol("utf-8-unix"),
        )
        .expect("set coding");
    eval.obarray
        .set_symbol_value("terminal-coding-system", Value::NIL);
    eval.obarray
        .set_symbol_value("keyboard-coding-system", Value::NIL);

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%z")]).expect("tty coding z");
    assert_eq!(rendered, Value::string("UUU"));
}

#[test]
fn test_format_mode_line_tty_z_uses_prefer_utf8_declared_mnemonic() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("tty-prefer-utf8-coding-test");
    eval.buffers.set_current(buffer_id);
    eval.frames
        .create_frame("tty-coding-frame", 80, 24, buffer_id);

    eval.buffers
        .set_buffer_local_property(
            buffer_id,
            "buffer-file-coding-system",
            Value::symbol("prefer-utf-8-unix"),
        )
        .expect("set coding");

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%z")]).expect("tty coding z");
    assert_eq!(rendered, Value::string("UU-"));
}

#[test]
fn test_format_mode_line_tty_z_orders_keyboard_before_terminal() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("tty-coding-order-test");
    eval.buffers.set_current(buffer_id);
    eval.frames
        .create_frame("tty-coding-frame", 80, 24, buffer_id);

    crate::emacs_core::coding::builtin_set_keyboard_coding_system(
        &mut eval.coding_systems,
        vec![Value::symbol("utf-8-unix")],
    )
    .expect("set keyboard coding");
    crate::emacs_core::coding::builtin_set_terminal_coding_system(
        &mut eval.coding_systems,
        vec![Value::symbol("no-conversion")],
    )
    .expect("set terminal coding");
    eval.buffers
        .set_buffer_local_property(
            buffer_id,
            "buffer-file-coding-system",
            Value::symbol("utf-8-unix"),
        )
        .expect("set buffer coding");

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%z")]).expect("tty coding z");
    assert_eq!(rendered, Value::string("U=U"));
}

#[test]
fn test_format_mode_line_tty_z_reads_visible_buffer_file_coding_value_without_local_flag() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("tty-coding-visible-slot-test");
    eval.buffers.set_current(buffer_id);
    eval.frames
        .create_frame("tty-coding-frame", 80, 24, buffer_id);

    eval.buffers
        .set_buffer_local_property(
            buffer_id,
            "buffer-file-coding-system",
            Value::symbol("utf-8-unix"),
        )
        .expect("set coding");
    eval.buffers
        .get_mut(buffer_id)
        .expect("buffer")
        .set_slot_local_flag(BUFFER_SLOT_BUFFER_FILE_CODING_SYSTEM, false);
    eval.obarray
        .set_symbol_value("terminal-coding-system", Value::NIL);
    eval.obarray
        .set_symbol_value("keyboard-coding-system", Value::NIL);

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%z")]).expect("tty coding z");
    assert_eq!(rendered, Value::string("UUU"));
}

#[test]
fn test_format_mode_line_tty_big_z_uses_live_coding_manager_state_and_eol_indicator() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("tty-coding-big-z-test");
    eval.buffers.set_current(buffer_id);
    eval.frames
        .create_frame("tty-coding-frame", 80, 24, buffer_id);

    eval.buffers
        .set_buffer_local_property(
            buffer_id,
            "buffer-file-coding-system",
            Value::symbol("utf-8-unix"),
        )
        .expect("set coding");
    eval.obarray
        .set_symbol_value("terminal-coding-system", Value::NIL);
    eval.obarray
        .set_symbol_value("keyboard-coding-system", Value::NIL);

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%Z")]).expect("tty coding Z");
    assert_eq!(rendered, Value::string("UUU:"));
}

#[test]
fn test_format_mode_line_propertize_display_min_width_matches_gnu_spacing() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let display = Value::list(vec![
        Value::symbol("min-width"),
        Value::list(vec![Value::make_float(6.0)]),
    ]);
    let format = Value::list(vec![
        Value::symbol(":propertize"),
        Value::string("All"),
        Value::symbol("display"),
        display,
    ]);

    let rendered = builtin_format_mode_line_ctx(&mut eval, vec![format]).expect("mode-line");
    assert_eq!(rendered, Value::string("All   "));
}

#[test]
fn mode_line_display_does_not_flush_a_terminal_min_width_run() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("mode-line-min-width", 800, 600, buffer_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    let display = Value::list(vec![
        Value::symbol("min-width"),
        Value::list(vec![Value::make_float(5.0)]),
    ]);
    let format = Value::list(vec![
        Value::symbol(":propertize"),
        Value::string("All"),
        Value::symbol("display"),
        display,
    ]);

    let rendered = format_mode_line_for_display(
        &mut eval,
        format,
        Value::make_window(window_id.0),
        Value::make_buffer(buffer_id),
        80,
    );

    assert_eq!(rendered.as_utf8_str(), Some("All"));
    let properties =
        get_string_text_properties_table_for_value(rendered).expect("display property retained");
    assert_eq!(
        properties.get_property_at_char_pos(CharPos0::ZERO, Value::symbol("display")),
        Some(display)
    );
}

#[test]
fn mode_line_display_closes_a_min_width_run_when_another_one_starts() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("mode-line-min-width-transition", 800, 600, buffer_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    let min_width = |columns: f64| {
        Value::list(vec![
            Value::symbol("min-width"),
            Value::list(vec![Value::make_float(columns)]),
        ])
    };
    let propertized = |text: &str, width: f64| {
        Value::list(vec![
            Value::symbol(":propertize"),
            Value::string(text),
            Value::symbol("display"),
            min_width(width),
        ])
    };
    let format = Value::list(vec![
        propertized("All", 5.0),
        propertized("Y", 4.0),
        Value::string("XX"),
    ]);

    let rendered = format_mode_line_for_display(
        &mut eval,
        format,
        Value::make_window(window_id.0),
        Value::make_buffer(buffer_id),
        80,
    );

    assert_eq!(rendered.as_utf8_str(), Some("All  YXX"));
}

#[test]
fn test_format_mode_line_position_o_and_q_specs() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer_id = eval.buffers.create_buffer("pos-test");
    eval.buffers.set_current(buffer_id);

    // Empty buffer → "All" for %o, "All   " (with trailing spaces) for %q (GNU compat)
    let empty =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%o|%q")]).expect("empty");
    assert_eq!(empty, Value::string("All|All   "));

    // With content and no window set, fallback covers full buffer → "All"
    {
        let buffer = eval.buffers.get_mut(buffer_id).expect("buffer");
        buffer.insert(&"x".repeat(100));
    }
    let all_visible =
        builtin_format_mode_line_ctx(&mut eval, vec![Value::string("%o|%p")]).expect("all");
    assert_eq!(all_visible, Value::string("All|All"));

    // Set up frame/window to test partial visibility.
    let frame_id = eval.frames.create_frame("pos-frame", 80, 24, buffer_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    // Window showing middle portion: start=20, simulated visible range [20..80].
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        match window {
            crate::window::Window::Leaf { window_start, .. } => {
                *window_start = LispCharPos1::from_one_based_usize(20);
            }
            other => panic!("expected leaf window, got {:?}", other),
        }
    }

    let mid = builtin_format_mode_line_ctx(
        &mut eval,
        vec![
            Value::string("%o|%p|%P"),
            Value::NIL,
            Value::make_window(selected_window.0),
        ],
    )
    .expect("mid pos");
    // %o: toppos=20 > begv=0 → not "Top"; botpos=100 >= zv=100 → "Bottom"
    // %p: botpos >= zv → pos(20) > begv(0) → "Bottom"
    // %P: botpos >= zv → toppos(20) > begv(0) → "Bottom"
    assert_eq!(mid, Value::string("Bottom|Bottom|Bottom"));

    // Window at the very start
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        match window {
            crate::window::Window::Leaf { window_start, .. } => {
                *window_start = LispCharPos1::new(0);
            }
            other => panic!("expected leaf window, got {:?}", other),
        }
    }
    let at_top = builtin_format_mode_line_ctx(
        &mut eval,
        vec![
            Value::string("%o|%p"),
            Value::NIL,
            Value::make_window(selected_window.0),
        ],
    )
    .expect("top pos");
    // window_start=0 and window_end(=zv)=100 >= zv → All
    assert_eq!(at_top, Value::string("All|All"));
}

#[test]
fn test_format_mode_line_percent_specs_use_window_buffer_and_completed_window_end() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let target_id = eval.buffers.create_buffer("window-target");
    {
        let buffer = eval.buffers.get_mut(target_id).expect("target buffer");
        buffer.insert(&"x".repeat(100));
    }
    let other_id = eval.buffers.create_buffer("other-buffer");
    {
        let buffer = eval.buffers.get_mut(other_id).expect("other buffer");
        buffer.insert(&"y".repeat(1000));
    }
    let frame_id = eval.frames.create_frame("pos-frame", 80, 24, target_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let target = eval.buffers.get(target_id).expect("target buffer");
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        match window {
            crate::window::Window::Leaf { window_start, .. } => {
                *window_start = LispCharPos1::from_one_based_usize(20);
                window.set_window_end_from_positions(
                    LispCharPos1::from_one_based_usize(
                        target.point_max_char_pos().get().saturating_add(1),
                    ),
                    target.point_max_emacs_byte_pos(),
                    target.point_max_char_pos().to_lisp(),
                    target.point_max_emacs_byte_pos(),
                    0,
                );
            }
            other => panic!("expected leaf window, got {:?}", other),
        }
    }
    eval.buffers.set_current(other_id);

    let rendered = builtin_format_mode_line_ctx(
        &mut eval,
        vec![
            Value::string("%o|%p|%P"),
            Value::NIL,
            Value::make_window(selected_window.0),
        ],
    )
    .expect("mode-line percent specs");

    assert_eq!(rendered, Value::string("Bottom|Bottom|Bottom"));
}

#[test]
fn test_invisible_p() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let err = builtin_invisible_p(&mut eval, vec![Value::fixnum(0)]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "args-out-of-range"),
        other => panic!("expected args-out-of-range, got {:?}", other),
    }
    let result = builtin_invisible_p(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert!(result.is_nil());

    let result = builtin_invisible_p(&mut eval, vec![Value::symbol("invisible")]).unwrap();
    assert!(result.is_truthy());

    let result = builtin_invisible_p(&mut eval, vec![Value::fixnum(-1)]).unwrap();
    assert!(result.is_truthy());

    let result = builtin_invisible_p(&mut eval, vec![Value::NIL]).unwrap();
    assert!(result.is_nil());

    let result = builtin_invisible_p(&mut eval, vec![Value::string("x")]).unwrap();
    assert!(result.is_truthy());

    let result = builtin_invisible_p(&mut eval, vec![Value::make_float(1.5)]).unwrap();
    assert!(result.is_truthy());
}

#[test]
fn test_line_pixel_height() {
    crate::test_utils::init_test_tracing();
    let result = builtin_line_pixel_height(vec![]).unwrap();
    assert_eq!(result, Value::fixnum(1));
}

#[test]
fn test_window_text_pixel_size() {
    crate::test_utils::init_test_tracing();
    let result = builtin_window_text_pixel_size(vec![]).unwrap();
    if result.is_cons() {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        assert_eq!(pair_car, Value::fixnum(0));
        assert_eq!(pair_cdr, Value::fixnum(0));
    } else {
        panic!("expected cons");
    }
}

#[test]
fn test_window_text_pixel_size_arg_validation() {
    crate::test_utils::init_test_tracing();
    let err = builtin_window_text_pixel_size(vec![Value::fixnum(1)]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    let err = builtin_window_text_pixel_size(vec![Value::NIL, Value::symbol("x")]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    let err = builtin_window_text_pixel_size(vec![Value::NIL, Value::NIL, Value::symbol("x")])
        .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    assert!(builtin_window_text_pixel_size(vec![Value::NIL, Value::T]).is_ok());
    assert!(
        builtin_window_text_pixel_size(vec![
            Value::NIL,
            Value::cons(Value::fixnum(1), Value::fixnum(0)),
        ])
        .is_ok()
    );
    let positive_big = Value::bignum(Integer::from(1u64) << 100u32);
    assert!(builtin_window_text_pixel_size(vec![Value::NIL, positive_big]).is_ok());
    assert!(
        builtin_window_text_pixel_size(vec![
            Value::NIL,
            Value::cons(positive_big, Value::fixnum(0)),
        ])
        .is_ok()
    );
    assert!(
        builtin_window_text_pixel_size(vec![Value::NIL, Value::fixnum(1), positive_big]).is_ok()
    );
    let err = builtin_window_text_pixel_size(vec![
        Value::NIL,
        Value::cons(Value::fixnum(1), Value::symbol("bad-offset")),
    ])
    .unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("fixnump"), Value::symbol("bad-offset")]
            );
        }
        other => panic!("expected wrong-type-argument integerp, got {:?}", other),
    }

    // X-LIMIT / Y-LIMIT / MODE / PIXELWISE are accepted without strict type checks.
    assert!(
        builtin_window_text_pixel_size(vec![
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::symbol("x"),
            Value::symbol("y"),
            Value::symbol("z"),
            Value::symbol("m"),
        ])
        .is_ok()
    );
}

#[test]
fn test_window_text_pixel_size_from_t_starts_at_first_non_empty_line() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-test", 80, 24, buf_id);
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
    }
    {
        let buffer = eval.buffers.get_mut(buf_id).expect("buffer");
        buffer.insert("\n\n  abc\n");
    }
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window.0 as i64;

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![Value::make_window(selected_window as u64), Value::T],
    )
    .unwrap();
    assert!(result.is_cons(), "expected cons, got {:?}", result.kind());
    assert_eq!(result.cons_car(), Value::fixnum(5));
    assert_eq!(result.cons_cdr(), Value::fixnum(1));
}

#[test]
fn test_window_text_pixel_size_eval_window_validation() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-test", 80, 24, buf_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window.0 as i64;

    let ok = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![Value::make_window(selected_window as u64)],
    )
    .unwrap();
    match ok.kind() {
        ValueKind::Cons => {}
        other => panic!("expected cons return, got {other:?}"),
    }

    let err =
        builtin_window_text_pixel_size_ctx(&mut eval, vec![Value::fixnum(999_999)]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }
}

#[test]
fn test_window_text_pixel_size_tty_frame_uses_char_cell_metrics() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-test", 80, 24, buf_id);
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
    }
    {
        let buffer = eval.buffers.get_mut(buf_id).expect("buffer");
        buffer.insert("tiny\n");
    }
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window.0 as i64;

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::fixnum(24),
            Value::T,
        ],
    )
    .unwrap();
    assert!(result.is_cons(), "expected cons, got {:?}", result.kind());
    assert_eq!(result.cons_car(), Value::fixnum(4));
    assert_eq!(result.cons_cdr(), Value::fixnum(2));
}

#[test]
fn window_text_pixel_size_counts_tty_wrapped_screen_rows() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-wrap-test", 10, 24, buf_id);
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
    }
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("123456789012\nnext\n");
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window.0 as u64;

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window),
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::fixnum(24),
            Value::T,
        ],
    )
    .expect("measure wrapped TTY buffer text");

    assert_eq!(result.cons_cdr(), Value::fixnum(4));
}

/// GNU `window_text_pixel_size` starts the ordinary display iterator
/// (`src/xdisp.c:11712-12042`), so its height uses the iterator's typed
/// `line_wrap` policy.  With `truncate-lines` non-nil a long logical line is
/// one screen row; it must not be divided by the TTY body width.
#[test]
fn window_text_pixel_size_honors_tty_truncate_lines() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-truncate-test", 10, 24, buf_id);
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
    }
    eval.buffers
        .set_buffer_local_property(buf_id, "truncate-lines", Value::T)
        .expect("enable truncation in measured buffer");
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("123456789012345678901234567890\nnext");
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window.0 as u64;

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window),
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::fixnum(24),
            Value::T,
        ],
    )
    .expect("measure truncated TTY buffer text");

    // Two logical text lines plus the requested mode line.
    assert_eq!(result.cons_cdr(), Value::fixnum(3));
}

#[test]
fn test_window_text_pixel_size_ctx_coerces_bignum_positions_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-test", 80, 24, buf_id);
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
    }
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("abc\n");
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window.0 as i64;
    let positive_big = Value::bignum(Integer::from(1u64) << 100u32);

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![Value::make_window(selected_window as u64), positive_big],
    )
    .expect("GNU clips bignum FROM with fix_position");
    assert!(result.is_cons());

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::fixnum(1),
            positive_big,
        ],
    )
    .expect("GNU clips bignum TO with fix_position");
    assert!(result.is_cons());
}

#[test]
fn test_window_text_pixel_size_matches_gnu_trailing_line_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-test", 80, 24, buf_id);
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
    }
    {
        let buffer = eval.buffers.get_mut(buf_id).expect("buffer");
        buffer.insert("hello\nworld\n\n");
    }
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window.0 as i64;

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![Value::make_window(selected_window as u64)],
    )
    .unwrap();
    assert!(result.is_cons(), "expected cons, got {:?}", result.kind());
    assert_eq!(result.cons_car(), Value::fixnum(5));
    assert_eq!(result.cons_cdr(), Value::fixnum(3));

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::fixnum(24),
            Value::T,
        ],
    )
    .unwrap();
    assert!(result.is_cons(), "expected cons, got {:?}", result.kind());
    assert_eq!(result.cons_car(), Value::fixnum(5));
    assert_eq!(
        result.cons_cdr(),
        Value::fixnum(3),
        "TO=t trims trailing blank lines before adding the mode line"
    );
}

#[test]
fn test_window_text_pixel_size_uses_char_positions_for_multibyte_range() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-test", 80, 24, buf_id);
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
    }
    {
        let buffer = eval.buffers.get_mut(buf_id).expect("buffer");
        buffer.insert("α\nwide");
    }
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window.0 as i64;

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::fixnum(3),
            Value::fixnum(5),
        ],
    )
    .unwrap();
    assert!(result.is_cons(), "expected cons, got {:?}", result.kind());
    assert_eq!(result.cons_car(), Value::fixnum(2));
    assert_eq!(result.cons_cdr(), Value::fixnum(1));
}

/// Build a context with a single TTY frame (char cell 1x1) whose selected
/// window shows the current buffer, used by the `display`-spec measurement
/// tests below.  Returns the selected-window id as a fixnum-able integer.
fn pixel_size_tty_context() -> (Context, i64) {
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-display-spec", 200, 24, buf_id);
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
    }
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window.0 as i64;
    (eval, selected_window)
}

/// A space carrying `display (space :align-to 80)` must measure as if the text
/// after it starts at column 80 — not as a single character column.  Mirrors
/// marginalia's right-aligned annotation (which uses align-to to pad the line).
#[test]
fn test_window_text_pixel_size_honors_display_align_to_column() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    // "AB" + space + "XY": the space is at column 2; align-to 80 pushes "XY" to
    // columns 80,81 so the widest column reached is 82.
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("AB XY");
    // Put `display (space :align-to 80)` on the single space (1-based pos 3..4).
    let spec = Value::list(vec![
        Value::symbol("space"),
        Value::symbol(":align-to"),
        Value::fixnum(80),
    ]);
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(4),
            Value::symbol("display"),
            spec,
        ],
    )
    .expect("put display align-to property");

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![Value::make_window(selected_window as u64)],
    )
    .expect("window-text-pixel-size");
    assert!(result.is_cons(), "expected cons, got {:?}", result.kind());
    // char_width == 1.0, so pixels == columns.  Without the fix this would be
    // ~5 (2 chars + 1 space + 2 chars); with align-to honored it is 82.
    assert_eq!(
        result.cons_car(),
        Value::fixnum(82),
        "align-to 80 should make the line ~82 columns wide, not ~5"
    );
}

/// `display (space :align-to (+ left N))` is the form marginalia actually emits
/// (default `marginalia-align` is `left`).  `left` resolves to text-area column
/// 0, so the target column is N.
#[test]
fn test_window_text_pixel_size_honors_display_align_to_plus_left() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("AB XY");
    let spec = Value::list(vec![
        Value::symbol("space"),
        Value::symbol(":align-to"),
        Value::list(vec![
            Value::symbol("+"),
            Value::symbol("left"),
            Value::fixnum(80),
        ]),
    ]);
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(4),
            Value::symbol("display"),
            spec,
        ],
    )
    .expect("put display align-to (+ left N) property");

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![Value::make_window(selected_window as u64)],
    )
    .expect("window-text-pixel-size");
    assert_eq!(result.cons_car(), Value::fixnum(82));
}

/// `display (space :width 20)` advances the running column by 20.
#[test]
fn test_window_text_pixel_size_honors_display_space_width() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    // "AB" (cols 0,1) + width-20 space (cols 2..22) + "XY" (cols 22,23) => 24.
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("AB XY");
    let spec = Value::list(vec![
        Value::symbol("space"),
        Value::symbol(":width"),
        Value::fixnum(20),
    ]);
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(4),
            Value::symbol("display"),
            spec,
        ],
    )
    .expect("put display :width property");

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![Value::make_window(selected_window as u64)],
    )
    .expect("window-text-pixel-size");
    assert_eq!(
        result.cons_car(),
        Value::fixnum(24),
        ":width 20 should add 20 columns (2 + 20 + 2), not 1 (2 + 1 + 2)"
    );
}

/// Reproduces the live vertico-posframe width bug.  Vertico stores the
/// candidate list in the `before-string` of a zero-length overlay anchored at
/// `point-max` (see vertico.el `vertico--display-candidates`:
/// `(make-overlay (point-max) (point-max) ...)` + `overlay-put ... 'before-string`).
/// Each candidate line carries marginalia's `display (space :align-to (+ left N))`
/// to right-pad the annotation.  posframe's `fit-frame-to-buffer` measures this
/// minibuffer with `window-text-pixel-size`, so the measured width MUST include
/// the aligned annotation — otherwise the child frame is too narrow and the
/// marginalia description is pushed off-screen.
///
/// Before the fix, `region_text_metrics_with_display` never processes the
/// *before-string* of an overlay anchored at the scan end (`point-max`): the
/// scan loop only visits positions `scan < end`, and the end-of-scan handler
/// processes *after-strings* only.  So the wide candidate line is invisible to
/// the measurement and the width collapses to the bare minibuffer prompt width.
#[test]
fn test_window_text_pixel_size_measures_overlay_before_string_at_point_max() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    // The minibuffer text: a short prompt "M-x ".  point-max is right after it.
    eval.buffers.get_mut(buf_id).expect("buffer").insert("M-x ");
    let point_max = eval
        .buffers
        .get(buf_id)
        .expect("buffer")
        .accessible_char_region()
        .end()
        .get() as i64
        + 1; // 1-based char pos of point-max

    // Build the candidate before-string exactly like vertico:
    //   "\ncand1<space>desc1\ncand2<space>desc2"
    // where each <space> carries `display (space :align-to (+ left 40))`, which
    // pushes the description to column 40 (the marginalia annotation alignment).
    let s = "\ndescribe-function desc-a\nfind-file          desc-b";
    let before_string = Value::string(s);
    // Positions of the two alignment spaces (0-based char offsets into the
    // string): right after "describe-function" and after "find-file".
    let align_pos1 = s.find("describe-function ").unwrap() + "describe-function".len();
    let align_pos2 = s.rfind("find-file").unwrap() + "find-file".len();
    let spec = || {
        Value::list(vec![
            Value::symbol("space"),
            Value::symbol(":align-to"),
            Value::list(vec![
                Value::symbol("+"),
                Value::symbol("left"),
                Value::fixnum(40),
            ]),
        ])
    };
    set_string_text_properties_for_value(
        before_string,
        vec![
            StringTextPropertyRun {
                start: align_pos1,
                end: align_pos1 + 1,
                plist: Value::list(vec![Value::symbol("display"), spec()]),
            },
            StringTextPropertyRun {
                start: align_pos2,
                end: align_pos2 + 1,
                plist: Value::list(vec![Value::symbol("display"), spec()]),
            },
        ],
    );

    // Zero-length overlay at point-max with the candidate `before-string`,
    // mirroring `(make-overlay (point-max) (point-max) nil t t)`.
    let overlay = crate::emacs_core::buffer::builtin_make_overlay(
        &mut eval,
        vec![
            Value::fixnum(point_max),
            Value::fixnum(point_max),
            Value::NIL,
            Value::T,
            Value::T,
        ],
    )
    .expect("make candidates overlay");
    crate::emacs_core::buffer::builtin_overlay_put(
        &mut eval,
        vec![overlay, Value::symbol("before-string"), before_string],
    )
    .expect("set before-string");

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![Value::make_window(selected_window as u64)],
    )
    .expect("window-text-pixel-size");
    let width = result.cons_car().as_int().expect("width");
    // char_width == 1.0, so pixels == columns.  With align-to honored, each
    // candidate line reaches column 40 + len("desc-a") == 46.  Without the fix
    // the before-string at point-max is skipped entirely and width is ~4 (the
    // "M-x " prompt only).
    assert!(
        width >= 46,
        "candidate before-string at point-max must be measured (with its \
         align-to annotation): expected width >= 46, got {width}"
    );
}

/// Plain text with no width-affecting `display` property must be unchanged: a
/// `display` STRING/other spec we do not model falls through to per-char column
/// counting (the covered char still counts as one column).
#[test]
fn test_window_text_pixel_size_plain_text_unchanged_by_fix() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("hello\nworld!");
    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![Value::make_window(selected_window as u64)],
    )
    .expect("window-text-pixel-size");
    // Widest line is "world!" => 6 columns.
    assert_eq!(result.cons_car(), Value::fixnum(6));
    assert_eq!(result.cons_cdr(), Value::fixnum(2));
}

#[test]
fn window_text_pixel_size_cons_from_reports_offset_start() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\nc\nd\ne\n");

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(11), Value::fixnum(-3)),
            Value::fixnum(11),
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(1), Value::fixnum(3), Value::fixnum(5),])
    );
}

#[test]
fn window_text_pixel_size_clips_to_before_applying_from_offset() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\nc\nd\ne\n");

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(11), Value::fixnum(-3)),
            Value::fixnum(1),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(1), Value::fixnum(3), Value::fixnum(5),])
    );
}

#[test]
fn window_text_pixel_size_keeps_row_height_when_offset_moves_past_to() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\nc\nd\ne\n");

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(1), Value::fixnum(3)),
            Value::fixnum(3),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(0), Value::fixnum(1), Value::fixnum(7),])
    );
}

#[test]
fn window_text_pixel_size_negative_offset_at_clipped_to_has_zero_height() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\nc\n");

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(1), Value::fixnum(-1)),
            Value::fixnum(1),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(0), Value::fixnum(0), Value::fixnum(1),])
    );
}

#[test]
fn window_text_pixel_size_offset_to_trailing_empty_row_has_zero_height() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\nc\n");

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(1), Value::fixnum(3)),
            Value::fixnum(1),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(0), Value::fixnum(0), Value::fixnum(7),])
    );
}

#[test]
fn window_text_pixel_size_offset_in_empty_buffer_has_zero_height() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(1), Value::fixnum(1)),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(0), Value::fixnum(0), Value::fixnum(1),])
    );
}

#[test]
fn window_text_pixel_size_zero_cons_offset_preserves_pair_shape() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\n");

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(5), Value::fixnum(0)),
            Value::fixnum(5),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(result, Value::cons(Value::fixnum(0), Value::fixnum(0)));
}

#[test]
fn window_text_pixel_size_positive_cons_offset_moves_forward() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\nc\nd\ne\n");

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(1), Value::fixnum(3)),
            Value::fixnum(11),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(7),])
    );
}

#[test]
fn window_text_pixel_size_positive_subrow_offset_stays_on_current_row() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .char_height = 16.0;
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\nc\n");

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(1), Value::fixnum(1)),
            Value::fixnum(7),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(1), Value::fixnum(48), Value::fixnum(1),])
    );
}

#[test]
fn window_text_pixel_size_offset_uses_live_row_pixel_heights() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .char_height = 16.0;
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\nc\n");

    let row = |row, y, height, start, end| crate::window::DisplayRowSnapshot {
        row,
        y,
        height,
        start_buffer_pos: Some(crate::buffer::LispCharPos1::new(start)),
        end_buffer_pos: Some(crate::buffer::LispCharPos1::new(end)),
        ..Default::default()
    };
    let window_id = crate::window::WindowId(selected_window as u64);
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id,
            rows: vec![
                row(0, 0, 10, 1, 2),
                row(1, 10, 30, 3, 4),
                row(2, 40, 10, 5, 6),
            ],
            ..Default::default()
        }]);

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(1), Value::fixnum(15)),
            Value::fixnum(7),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(1), Value::fixnum(32), Value::fixnum(3),])
    );
}

#[test]
fn window_text_pixel_size_rejects_snapshot_after_overlay_change() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .char_height = 16.0;
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\nc\n");
    let buffer_modiff = eval.buffers.get(buf_id).expect("buffer").modified_tick();
    let overlay_modiff = eval
        .buffers
        .get(buf_id)
        .expect("buffer")
        .overlay_modified_tick();

    let row = |row, y, height, start, end| crate::window::DisplayRowSnapshot {
        row,
        y,
        height,
        start_buffer_pos: Some(crate::buffer::LispCharPos1::new(start)),
        end_buffer_pos: Some(crate::buffer::LispCharPos1::new(end)),
        ..Default::default()
    };
    let window_id = crate::window::WindowId(selected_window as u64);
    let frame_id = eval.frames.selected_frame().expect("selected frame").id;
    let layout_freshness = eval
        .window_display_snapshot_freshness(frame_id, window_id, buf_id)
        .expect("freshness token");
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id,
            rows: vec![
                row(0, 0, 10, 1, 2),
                row(1, 10, 30, 3, 4),
                row(2, 40, 10, 5, 6),
            ],
            buffer_modiff: Some(buffer_modiff),
            layout_freshness: Some(layout_freshness),
            ..Default::default()
        }]);

    eval.eval_str(
        "(let ((overlay (make-overlay 1 2)))\
           (overlay-put overlay 'help-echo \"changed\"))",
    )
    .expect("mutate overlay state after snapshot");
    let buffer = eval.buffers.get(buf_id).expect("buffer");
    assert_eq!(buffer.modified_tick(), buffer_modiff);
    assert_ne!(buffer.overlay_modified_tick(), overlay_modiff);

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(1), Value::fixnum(15)),
            Value::fixnum(7),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(1), Value::fixnum(48), Value::fixnum(1),])
    );
}

#[test]
fn window_text_pixel_size_rejects_snapshot_after_narrowing() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .char_height = 16.0;
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\nc\n");
    let buffer_modiff = eval.buffers.get(buf_id).expect("buffer").modified_tick();

    let row = |row, y, start, end| crate::window::DisplayRowSnapshot {
        row,
        y,
        height: 10,
        start_buffer_pos: Some(crate::buffer::LispCharPos1::new(start)),
        end_buffer_pos: Some(crate::buffer::LispCharPos1::new(end)),
        ..Default::default()
    };
    let window_id = crate::window::WindowId(selected_window as u64);
    let frame_id = eval.frames.selected_frame().expect("selected frame").id;
    let layout_freshness = eval
        .window_display_snapshot_freshness(frame_id, window_id, buf_id)
        .expect("freshness token");
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id,
            rows: vec![row(0, 0, 1, 2), row(1, 10, 3, 4), row(2, 20, 5, 6)],
            buffer_modiff: Some(buffer_modiff),
            layout_freshness: Some(layout_freshness),
            ..Default::default()
        }]);

    eval.eval_str("(narrow-to-region 3 7)")
        .expect("narrow after snapshot");
    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_none()
    );

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(3), Value::fixnum(-1)),
            Value::fixnum(7),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(1), Value::fixnum(32), Value::fixnum(3),])
    );
}

#[test]
fn retained_display_rows_reject_window_system_changes() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers.get_mut(buf_id).expect("buffer").insert("a\n");
    let window_id = crate::window::WindowId(selected_window as u64);
    let frame_id = eval.frames.selected_frame().expect("selected frame").id;
    let layout_freshness = eval
        .window_display_snapshot_freshness(frame_id, window_id, buf_id)
        .expect("freshness token");
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id,
            layout_freshness: Some(layout_freshness),
            ..Default::default()
        }]);
    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_some()
    );

    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .set_window_system(Some(Value::symbol("neo")));

    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_none()
    );
}

#[test]
fn retained_display_rows_reject_window_margin_changes() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let window_id = crate::window::WindowId(selected_window as u64);
    let frame_id = eval.frames.selected_frame().expect("selected frame").id;
    let layout_freshness = eval
        .window_display_snapshot_freshness(frame_id, window_id, buf_id)
        .expect("freshness token");
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id,
            layout_freshness: Some(layout_freshness),
            ..Default::default()
        }]);
    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_some()
    );

    eval.eval_str("(set-window-margins (selected-window) 2 1)")
        .expect("change window margins after snapshot");

    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_none()
    );
}

#[test]
fn retained_display_rows_reject_window_display_table_changes() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let window_id = crate::window::WindowId(selected_window as u64);
    let frame_id = eval.frames.selected_frame().expect("selected frame").id;
    let display_table = eval
        .eval_str("(make-char-table 'display-table)")
        .expect("make display table");
    eval.frames
        .set_window_display_table(window_id, display_table);
    let layout_freshness = eval
        .window_display_snapshot_freshness(frame_id, window_id, buf_id)
        .expect("freshness token");
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id,
            layout_freshness: Some(layout_freshness),
            ..Default::default()
        }]);
    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_some()
    );

    crate::emacs_core::chartable::builtin_set_char_table_range(
        vec![
            display_table,
            Value::fixnum('a' as i64),
            Value::vector(vec![Value::fixnum('A' as i64)]),
        ],
        None,
    )
    .expect("mutate installed display table in place");
    assert_eq!(
        eval.frames.window_display_table(window_id).bits(),
        display_table.bits()
    );

    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_none()
    );
}

#[test]
fn retained_display_rows_reject_window_face_filter_parameter_changes() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let window_id = crate::window::WindowId(selected_window as u64);
    let frame_id = eval.frames.selected_frame().expect("selected frame").id;
    let layout_freshness = eval
        .window_display_snapshot_freshness(frame_id, window_id, buf_id)
        .expect("freshness token");
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id,
            layout_freshness: Some(layout_freshness),
            ..Default::default()
        }]);
    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_some()
    );

    eval.eval_str("(set-window-parameter (selected-window) 'indent-bars-whr 'right)")
        .expect("change a :window face-filter parameter after snapshot");

    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_none()
    );
}

#[test]
fn retained_display_rows_reject_restored_window_parameter_state() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let window_id = crate::window::WindowId(selected_window as u64);
    let frame_id = eval.frames.selected_frame().expect("selected frame").id;
    let saved_window = eval
        .frames
        .selected_frame()
        .expect("selected frame")
        .selected_window()
        .expect("selected window")
        .clone();

    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .selected_window_mut()
        .expect("selected window")
        .parameters_mut()
        .push((Value::symbol("indent-bars-whr"), Value::symbol("right")));
    let layout_freshness = eval
        .window_display_snapshot_freshness(frame_id, window_id, buf_id)
        .expect("freshness token");
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id,
            layout_freshness: Some(layout_freshness),
            ..Default::default()
        }]);
    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_some()
    );

    let mut restored_window = saved_window;
    restored_window
        .parameters_mut()
        .push((Value::symbol("indent-bars-whr"), Value::symbol("left")));
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .root_window = restored_window;

    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_none()
    );
}

#[test]
fn window_text_pixel_size_preserves_pixel_progress_beyond_live_rows() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .char_height = 16.0;
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\nc\nd\n");

    let row = |row, y, start, end| crate::window::DisplayRowSnapshot {
        row,
        y,
        height: 5,
        start_buffer_pos: Some(crate::buffer::LispCharPos1::new(start)),
        end_buffer_pos: Some(crate::buffer::LispCharPos1::new(end)),
        ..Default::default()
    };
    let window_id = crate::window::WindowId(selected_window as u64);
    eval.frames
        .selected_frame_mut()
        .expect("selected frame")
        .commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id,
            rows: vec![row(0, 0, 1, 2), row(1, 5, 3, 4), row(2, 10, 5, 6)],
            ..Default::default()
        }]);

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(1), Value::fixnum(15)),
            Value::fixnum(9),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(1), Value::fixnum(16), Value::fixnum(7),])
    );
}

#[test]
fn window_text_pixel_size_forward_offset_stays_on_final_unterminated_row() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers.get_mut(buf_id).expect("buffer").insert("abc");

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(1), Value::fixnum(1)),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(3), Value::fixnum(1), Value::fixnum(1),])
    );
}

#[test]
fn window_text_pixel_size_cons_offset_counts_wrapped_rows() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert(&format!("{}\nb\nc\n", "a".repeat(400)));

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(406), Value::fixnum(-3)),
            Value::fixnum(406),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(2), Value::fixnum(3), Value::fixnum(399),])
    );
}

/// GNU's display iterator resolves every `face' property run while measuring
/// text for `window-text-pixel-size'.  A missing named face is therefore a
/// log-only diagnostic even when no redisplay callback is installed (the path
/// used by `fit-window-to-buffer' in batch mode).
#[test]
fn window_text_pixel_size_logs_invalid_named_face_property_runs() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("prefix alpha attrs layered");
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(8),
            Value::fixnum(13),
            Value::symbol("face"),
            Value::symbol("missing-face"),
        ],
    )
    .expect("put missing face property");
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(14),
            Value::fixnum(19),
            Value::symbol("face"),
            Value::list(vec![Value::keyword(":weight"), Value::symbol("bold")]),
        ],
    )
    .expect("put valid face attribute plist");
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(20),
            Value::fixnum(27),
            Value::symbol("face"),
            Value::list(vec![
                Value::symbol("bold"),
                Value::symbol("other-missing-face"),
            ]),
        ],
    )
    .expect("put layered face references");

    builtin_window_text_pixel_size_ctx(&mut eval, vec![Value::make_window(selected_window as u64)])
        .expect("window-text-pixel-size");

    let messages_id = eval
        .buffers
        .find_buffer_by_name("*Messages*")
        .expect("face diagnostic should create *Messages*");
    assert_eq!(
        eval.buffers
            .get(messages_id)
            .expect("*Messages* live")
            .buffer_string(),
        "Invalid face reference: missing-face\nInvalid face reference: other-missing-face\n"
    );
}

fn window_text_pixel_size_backend_trace(kind: BufferTextBackendKind) -> (i64, i64) {
    let mut eval = interactive_context();
    convert_current_buffer_text_backend(&mut eval, kind);
    fragment_current_buffer(&mut eval, "αx\nb\tc\nlast\n\n");
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-backend-pixels", 80, 24, buf_id);
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.char_width = 2.0;
        frame.char_height = 3.0;
        frame.font_pixel_size = 3.0;
        frame.set_window_system(None);
    }
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window.0 as i64;

    let size = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![Value::make_window(selected_window as u64)],
    )
    .expect("window-text-pixel-size");
    assert!(size.is_cons(), "expected cons, got {:?}", size.kind());
    (
        size.cons_car().as_int().expect("pixel width integer"),
        size.cons_cdr().as_int().expect("pixel height integer"),
    )
}

#[test]
fn implemented_text_backends_match_window_text_pixel_size_metrics() {
    crate::test_utils::init_test_tracing();
    let baseline = window_text_pixel_size_backend_trace(BufferTextBackendKind::GapBuffer);
    assert_eq!(baseline, (18, 12));

    for kind in implemented_text_backends() {
        assert_eq!(
            window_text_pixel_size_backend_trace(kind),
            baseline,
            "{kind:?}"
        );
    }
}

#[test]
fn test_pos_visible_in_window_p() {
    crate::test_utils::init_test_tracing();
    let result = builtin_pos_visible_in_window_p(vec![Value::fixnum(1)]).unwrap();
    assert!(result.is_nil());

    let result = builtin_pos_visible_in_window_p(vec![Value::fixnum(100), Value::symbol("window")])
        .unwrap_err();
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data[0], Value::symbol("window-live-p"));
        }
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    let result =
        builtin_pos_visible_in_window_p(vec![Value::symbol("left"), Value::fixnum(1)]).unwrap_err();
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data[0], Value::symbol("window-live-p"));
        }
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    let result = builtin_pos_visible_in_window_p(vec![Value::symbol("left")]).unwrap_err();
    match result {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data[0], Value::symbol("integer-or-marker-p"));
        }
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    let result =
        builtin_pos_visible_in_window_p(vec![Value::fixnum(1), Value::NIL, Value::fixnum(1)])
            .unwrap();
    assert!(result.is_nil());
}

#[test]
fn test_pos_visible_in_window_p_eval_window_validation() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let err = builtin_pos_visible_in_window_p_ctx(&mut eval, vec![Value::NIL, Value::string("x")])
        .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    let err = builtin_pos_visible_in_window_p_ctx(
        &mut eval,
        vec![Value::symbol("left"), Value::fixnum(1)],
    )
    .unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data[0], Value::symbol("window-live-p"));
        }
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    let ok = builtin_pos_visible_in_window_p_ctx(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert!(ok.is_nil());
}

#[test]
fn test_pos_visible_in_window_p_eval_returns_partial_geometry_for_live_window() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    eval.set_variable("noninteractive", Value::NIL);
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-pos", 160, 64, buf_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("abc\ndef\nghi\n");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(4));
    }
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        match window {
            crate::window::Window::Leaf {
                window_start,
                point,
                ..
            } => {
                *window_start = LispCharPos1::ONE;
                *point = LispCharPos1::from_one_based_usize(5);
            }
            other => panic!("expected leaf window, got {:?}", other),
        }
    }

    let result = builtin_pos_visible_in_window_p_ctx(
        &mut eval,
        vec![
            Value::fixnum(5),
            Value::make_window(selected_window.0),
            Value::T,
        ],
    )
    .unwrap();
    assert_eq!(super::super::print::print_value(&result), "(0 16)");
}

#[test]
fn pos_visible_in_new_live_window_falls_back_when_active_presentation_predates_it() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let original_buffer = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-new-window", 800, 600, original_buffer);
    let original_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame
            .prepare_and_activate_display_presentation_for_test(
                crate::window::geometry::PresentationId::new(1),
                vec![crate::window::WindowDisplaySnapshot {
                    window_id: original_window,
                    ..Default::default()
                }],
            )
            .expect("initial presentation");
    }

    // Model the real Issue #249 ordering: help-mode creates a live window,
    // then asks whether its point is visible before the asynchronous GUI has
    // presented a frame containing that new window.
    let help_buffer = eval.buffers.create_buffer("*Disabled Command*");
    let help_window = eval
        .frames
        .split_window(
            frame_id,
            original_window,
            crate::window::SplitDirection::Horizontal,
            help_buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("help window");

    let result = builtin_pos_visible_in_window_p_ctx(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::make_window(help_window.0),
            Value::T,
        ],
    )
    .expect("a live window must not expose presentation-lag as a Lisp error");

    assert_eq!(super::super::print::print_value(&result), "(0 0)");
}

#[test]
fn test_pos_visible_in_window_p_noninteractive_returns_nil_like_gnu_batch() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    eval.set_variable("noninteractive", Value::T);
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-batch-pos", 160, 64, buf_id);
    // GNU `pos_visible_p` answers nil because the --batch frame is the
    // INITIAL frame (`FRAME_INITIAL_P`), not because `noninteractive` is t:
    // model the batch condition the way GNU carries it.
    eval.frames.get_mut(frame_id).expect("frame").initial = true;
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("abc\ndef\n");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(1));
    }

    let implicit = builtin_pos_visible_in_window_p_ctx(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert!(implicit.is_nil());

    let explicit = builtin_pos_visible_in_window_p_ctx(
        &mut eval,
        vec![Value::fixnum(1), Value::make_window(selected_window.0)],
    )
    .unwrap();
    assert!(explicit.is_nil());
}

#[test]
/// The snapshot arm answers with the rows the snapshot actually holds.
///
/// This test used to run the same scenario with NO committed redisplay cache
/// and pin `(16 1 16 0)` -- a geometry approximation, not a matrix. GNU
/// `Fwindow_line_height` returns nil there (src/window.c:2082-2089), so that
/// case now belongs to
/// `window_line_height_refuses_every_line_form_without_a_current_matrix_like_gnu`
/// and this one keeps what it was named for by publishing a real snapshot.
fn test_window_line_height_eval_returns_live_gui_row_metrics() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-line-height", 160, 64, buf_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("abc\ndef\nghi\n");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(4));
    }
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        match window {
            crate::window::Window::Leaf {
                window_start,
                point,
                ..
            } => {
                *window_start = LispCharPos1::ONE;
                *point = LispCharPos1::from_one_based_usize(5);
            }
            other => panic!("expected leaf window, got {:?}", other),
        }
    }
    {
        let row = |index: i64, start: usize, end: usize| crate::window::DisplayRowSnapshot {
            row: index,
            y: index * 16,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 48,
            end_col: 3,
            start_buffer_pos: Some(crate::buffer::LispCharPos1::from_one_based_usize(start)),
            end_buffer_pos: Some(crate::buffer::LispCharPos1::from_one_based_usize(end)),
            fringe: Default::default(),
        };
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: selected_window,
            rows: vec![row(0, 1, 5), row(1, 5, 9), row(2, 9, 13), row(3, 13, 13)],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let first = builtin_window_line_height(
        &mut eval,
        vec![Value::fixnum(0), Value::make_window(selected_window.0)],
    )
    .unwrap();
    let third = builtin_window_line_height(
        &mut eval,
        vec![Value::fixnum(2), Value::make_window(selected_window.0)],
    )
    .unwrap();
    let last = builtin_window_line_height(
        &mut eval,
        vec![Value::fixnum(-1), Value::make_window(selected_window.0)],
    )
    .unwrap();
    assert_eq!(super::super::print::print_value(&first), "(16 0 0 0)");
    assert_eq!(super::super::print::print_value(&third), "(16 2 32 0)");
    assert_eq!(super::super::print::print_value(&last), "(16 3 48 0)");
}

#[test]
fn test_window_line_height_eval_uses_exact_chrome_rows() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-line-height-chrome", 160, 80, buf_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame
            .prepare_and_activate_display_presentation_for_test(
                crate::window::geometry::PresentationId::new(1),
                vec![crate::window::WindowDisplaySnapshot {
                    window_id: selected_window,
                    tab_line_height: 5,
                    header_line_height: 7,
                    mode_line_height: 9,
                    rows: vec![
                        crate::window::DisplayRowSnapshot {
                            row: 0,
                            y: 0,
                            height: 5,
                            start_x: 0,
                            start_col: 0,
                            end_x: 20,
                            end_col: 2,
                            start_buffer_pos: None,
                            end_buffer_pos: None,
                            fringe: Default::default(),
                        },
                        crate::window::DisplayRowSnapshot {
                            row: 1,
                            y: 5,
                            height: 7,
                            start_x: 0,
                            start_col: 0,
                            end_x: 20,
                            end_col: 2,
                            start_buffer_pos: None,
                            end_buffer_pos: None,
                            fringe: Default::default(),
                        },
                        crate::window::DisplayRowSnapshot {
                            row: 2,
                            y: 12,
                            height: 11,
                            start_x: 0,
                            start_col: 0,
                            end_x: 20,
                            end_col: 2,
                            start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                            end_buffer_pos: Some(crate::buffer::LispCharPos1::new(3)),
                            fringe: Default::default(),
                        },
                        crate::window::DisplayRowSnapshot {
                            row: 3,
                            y: 23,
                            height: 9,
                            start_x: 0,
                            start_col: 0,
                            end_x: 20,
                            end_col: 2,
                            start_buffer_pos: None,
                            end_buffer_pos: None,
                            fringe: Default::default(),
                        },
                    ],
                    ..crate::window::WindowDisplaySnapshot::default()
                }],
            )
            .expect("presented geometry");
    }

    let tab = builtin_window_line_height(
        &mut eval,
        vec![
            Value::symbol("tab-line"),
            Value::make_window(selected_window.0),
        ],
    )
    .unwrap();
    let header = builtin_window_line_height(
        &mut eval,
        vec![
            Value::symbol("header-line"),
            Value::make_window(selected_window.0),
        ],
    )
    .unwrap();
    let mode = builtin_window_line_height(
        &mut eval,
        vec![
            Value::symbol("mode-line"),
            Value::make_window(selected_window.0),
        ],
    )
    .unwrap();

    assert_eq!(super::super::print::print_value(&tab), "(5 0 0 0)");
    assert_eq!(super::super::print::print_value(&header), "(7 0 0 0)");
    assert_eq!(super::super::print::print_value(&mode), "(9 0 23 0)");
}

#[test]
fn test_window_line_height_eval_reports_text_rows_relative_to_text_area() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-line-height-text-origin", 160, 80, buf_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: selected_window,
            tab_line_height: 5,
            header_line_height: 7,
            rows: vec![
                crate::window::DisplayRowSnapshot {
                    row: 0,
                    y: 0,
                    height: 5,
                    start_x: 0,
                    start_col: 0,
                    end_x: 20,
                    end_col: 2,
                    start_buffer_pos: None,
                    end_buffer_pos: None,
                    fringe: Default::default(),
                },
                crate::window::DisplayRowSnapshot {
                    row: 1,
                    y: 5,
                    height: 7,
                    start_x: 0,
                    start_col: 0,
                    end_x: 20,
                    end_col: 2,
                    start_buffer_pos: None,
                    end_buffer_pos: None,
                    fringe: Default::default(),
                },
                crate::window::DisplayRowSnapshot {
                    row: 2,
                    y: 12,
                    height: 11,
                    start_x: 0,
                    start_col: 0,
                    end_x: 20,
                    end_col: 2,
                    start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                    end_buffer_pos: Some(crate::buffer::LispCharPos1::new(3)),
                    fringe: Default::default(),
                },
                crate::window::DisplayRowSnapshot {
                    row: 3,
                    y: 23,
                    height: 13,
                    start_x: 0,
                    start_col: 0,
                    end_x: 20,
                    end_col: 2,
                    start_buffer_pos: Some(crate::buffer::LispCharPos1::new(4)),
                    end_buffer_pos: Some(crate::buffer::LispCharPos1::new(6)),
                    fringe: Default::default(),
                },
            ],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let first = builtin_window_line_height(
        &mut eval,
        vec![Value::fixnum(0), Value::make_window(selected_window.0)],
    )
    .unwrap();
    let last = builtin_window_line_height(
        &mut eval,
        vec![Value::fixnum(-1), Value::make_window(selected_window.0)],
    )
    .unwrap();

    assert_eq!(super::super::print::print_value(&first), "(11 0 0 0)");
    assert_eq!(super::super::print::print_value(&last), "(13 1 11 0)");
}

#[test]
fn test_posn_at_point_eval_uses_exact_redisplay_snapshot() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-posn", 160, 64, buf_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("abcdef\n");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(4));
    }
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        let window = frame
            .find_window_mut(selected_window)
            .expect("selected window");
        match window {
            crate::window::Window::Leaf {
                window_start,
                point,
                ..
            } => {
                *window_start = LispCharPos1::ONE;
                *point = LispCharPos1::from_one_based_usize(5);
            }
            other => panic!("expected leaf window, got {:?}", other),
        }
        frame
            .prepare_and_activate_display_presentation_for_test(
                crate::window::geometry::PresentationId::new(1),
                vec![crate::window::WindowDisplaySnapshot {
                    window_id: selected_window,
                    regions: crate::window::PresentedWindowRegions {
                        outer: neomacs_display_protocol::types::Rect::new(
                            144.0, 24.0, 800.0, 600.0,
                        ),
                        text_body: neomacs_display_protocol::types::Rect::new(
                            168.0, 41.0, 760.0, 560.0,
                        ),
                        ..Default::default()
                    },
                    regions_materialized: true,
                    text_area_left_offset: 999,
                    header_line_height: 88,
                    tab_line_height: 99,
                    points: vec![crate::window::DisplayPointSnapshot {
                        role: crate::window::DisplayPointRole::Glyph,
                        buffer_pos: crate::buffer::LispCharPos1::new(5),
                        x: 72,
                        y: 999,
                        width: 7,
                        height: 17,
                        row: 99,
                        col: 9,
                    }],
                    body_rows: vec![crate::window::PresentedBodyRowSnapshot {
                        output_row: 99,
                        body_row: 2,
                        body_y: 34,
                    }],
                    rows: vec![crate::window::DisplayRowSnapshot {
                        row: 1,
                        y: 18,
                        height: 30,
                        start_x: 0,
                        start_col: 0,
                        end_x: 0,
                        end_col: 0,
                        start_buffer_pos: Some(crate::buffer::LispCharPos1::new(5)),
                        end_buffer_pos: Some(crate::buffer::LispCharPos1::new(5)),
                        fringe: Default::default(),
                    }],
                    ..crate::window::WindowDisplaySnapshot::default()
                }],
            )
            .expect("presented geometry");
    }

    let result = builtin_posn_at_point(
        &mut eval,
        vec![Value::fixnum(5), Value::make_window(selected_window.0)],
    )
    .unwrap();
    assert_eq!(
        super::super::print::print_value(&result),
        "(#<window 1> 5 (72 . 34) 0 nil 5 (9 . 2) nil (0 . 0) (7 . 17))"
    );
}

#[test]
fn test_posn_at_point_reports_text_area_relative_y_below_window_chrome() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-posn-chrome", 800, 600, buf_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("completion");
    }
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: selected_window,
            header_line_height: 5,
            tab_line_height: 17,
            points: vec![crate::window::DisplayPointSnapshot {
                role: crate::window::DisplayPointRole::Glyph,
                buffer_pos: crate::buffer::LispCharPos1::new(1),
                x: 54,
                y: 313,
                width: 7,
                height: 17,
                row: 17,
                col: 0,
            }],
            rows: vec![crate::window::DisplayRowSnapshot {
                row: 17,
                y: 313,
                height: 17,
                start_x: 54,
                start_col: 0,
                end_x: 61,
                end_col: 1,
                start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                end_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                fringe: Default::default(),
            }],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let result = builtin_posn_at_point(
        &mut eval,
        vec![Value::fixnum(1), Value::make_window(selected_window.0)],
    )
    .expect("posn-at-point");

    assert_eq!(
        super::super::print::print_value(&result),
        "(#<window 1> 1 (54 . 291) 0 nil 1 (0 . 15) nil (0 . 0) (7 . 17))"
    );
}

#[test]
fn posn_at_point_recomputes_a_terminal_window_redisplay_has_not_drawn_yet() {
    // Ledger 201, row 1.  GNU answers `posn-at-point` with no redisplay at all:
    // `Fposn_at_point` goes through `Fpos_visible_in_window_p` ->
    // `pos_visible_p`, which runs `start_display` from `w->start` and
    // `move_it_to` on every call (src/xdisp.c:1772-1774).  The only glyph-matrix
    // read on that whole path fills in the WIDTH/HEIGHT cell and is guarded,
    // `else { *width = *height = 0; }` (src/dispnew.c:6394-6420).  Measured on
    // GNU 31.0.90 over a pty, cold:
    //   pos=83 posn=(83 (20 . 0) 0 nil 83 (20 . 0) nil (0 . 0) (0 . 0))
    // and warm the same call answers `... (1 . 0)`.  The unpopulated matrix
    // costs GNU one cell of ten, and nothing else.
    //
    // This port answered nil for ALL 144 cold posn probes of ledger 201's sweep
    // because it read only the retained redisplay snapshot.  The frontend's
    // synchronous single-window layout seam -- the one `(window-end WINDOW t)`
    // already uses -- is this port's `start_display`, so ask it.
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-posn-cold", 800, 600, buf_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    eval.frames.get_mut(frame_id).expect("frame").initial = false;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("completion");
    }
    // No `commit_redisplay_cache_for_test`: redisplay has never run for this
    // window, which is exactly the state a `-l` script sees before the command
    // loop's first redisplay.
    assert!(
        eval.frames
            .get(frame_id)
            .expect("frame")
            .redisplay_snapshot(window_id)
            .is_none(),
        "the fixture must start with no retained rows, or it measures the warm path"
    );

    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = std::rc::Rc::clone(&calls);
    eval.install_window_layout_query(move |_eval, queried_frame, queried_window| {
        assert_eq!(queried_frame, frame_id);
        assert_eq!(queried_window, window_id);
        observed.set(observed.get() + 1);
        crate::window::WindowLayoutQueryOutcome::Ready(crate::window::WindowLayoutQuery::new(
            crate::buffer::LispCharPos1::ONE,
            Some(crate::window::WindowDisplaySnapshot {
                window_id,
                points: vec![crate::window::DisplayPointSnapshot {
                    role: crate::window::DisplayPointRole::Glyph,
                    buffer_pos: crate::buffer::LispCharPos1::new(1),
                    x: 54,
                    y: 17,
                    width: 7,
                    height: 17,
                    row: 1,
                    col: 0,
                }],
                rows: vec![crate::window::DisplayRowSnapshot {
                    row: 1,
                    y: 17,
                    height: 17,
                    start_x: 54,
                    start_col: 0,
                    end_x: 61,
                    end_col: 1,
                    start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                    end_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                    fringe: Default::default(),
                }],
                ..crate::window::WindowDisplaySnapshot::default()
            }),
        ))
    });

    let result = builtin_posn_at_point(
        &mut eval,
        vec![Value::fixnum(1), Value::make_window(window_id.0)],
    )
    .expect("posn-at-point");

    assert_eq!(
        super::super::print::print_value(&result),
        "(#<window 1> 1 (54 . 17) 0 nil 1 (0 . 1) nil (0 . 0) (7 . 17))",
        "a window redisplay has not drawn is recomputed, not reported as invisible"
    );
    assert_eq!(
        calls.get(),
        1,
        "one row walk answers the query, as GNU's single move_it_to does"
    );
}

#[test]
fn gui_posn_at_point_uses_next_presented_glyph_only_within_the_same_body_row() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer = eval.buffers.current_buffer().expect("buffer").id;
    let frame_id = eval.frames.create_frame("posn-hidden", 800, 600, buffer);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    eval.buffers
        .get_mut(buffer)
        .expect("buffer")
        .insert("abcdefghijklmnopqrstuvwxyz\n");
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame
            .prepare_and_activate_display_presentation_for_test(
                crate::window::geometry::PresentationId::new(1),
                vec![crate::window::WindowDisplaySnapshot {
                    window_id,
                    regions: crate::window::PresentedWindowRegions {
                        outer: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
                        text_body: neomacs_display_protocol::types::Rect::new(
                            0.0, 0.0, 800.0, 600.0,
                        ),
                        ..Default::default()
                    },
                    regions_materialized: true,
                    points: vec![
                        crate::window::DisplayPointSnapshot {
                            role: crate::window::DisplayPointRole::Glyph,
                            buffer_pos: crate::buffer::LispCharPos1::new(10),
                            x: 24,
                            y: 18,
                            width: 8,
                            height: 16,
                            row: 0,
                            col: 3,
                        },
                        crate::window::DisplayPointSnapshot {
                            role: crate::window::DisplayPointRole::Glyph,
                            buffer_pos: crate::buffer::LispCharPos1::new(14),
                            x: 56,
                            y: 18,
                            width: 8,
                            height: 16,
                            row: 0,
                            col: 7,
                        },
                        crate::window::DisplayPointSnapshot {
                            role: crate::window::DisplayPointRole::Glyph,
                            buffer_pos: crate::buffer::LispCharPos1::new(20),
                            x: 0,
                            y: 34,
                            width: 8,
                            height: 16,
                            row: 1,
                            col: 0,
                        },
                    ],
                    body_rows: vec![
                        crate::window::PresentedBodyRowSnapshot {
                            output_row: 0,
                            body_row: 0,
                            body_y: 18,
                        },
                        crate::window::PresentedBodyRowSnapshot {
                            output_row: 1,
                            body_row: 1,
                            body_y: 34,
                        },
                    ],
                    ..Default::default()
                }],
            )
            .expect("presented geometry");
    }

    let hidden_on_row = builtin_posn_at_point(
        &mut eval,
        vec![Value::fixnum(12), Value::make_window(window_id.0)],
    )
    .expect("hidden position query");
    let gap_between_rows = builtin_posn_at_point(
        &mut eval,
        vec![Value::fixnum(17), Value::make_window(window_id.0)],
    )
    .expect("between-row position query");

    assert_eq!(
        super::super::print::print_value(&hidden_on_row),
        format!(
            "(#<window {}> 14 (56 . 18) 0 nil 14 (7 . 0) nil (0 . 0) (8 . 16))",
            window_id.0
        )
    );
    assert!(
        gap_between_rows.is_nil(),
        "a missing position between two presented rows must remain invisible"
    );
}

#[test]
fn tty_posn_at_x_y_uses_the_named_live_grid_approximation() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-posn-xy-chrome", 160, 80, buf_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("abcdef\n");
    }
    let result = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(30),
            Value::fixnum(40),
            Value::make_window(selected_window.0),
            Value::NIL,
        ],
    )
    .expect("posn-at-x-y");

    assert_eq!(
        super::super::print::print_value(&result),
        "(#<window 1> 8 (30 . 40) 0 nil 8 (3 . 2) nil (30 . 8) (0 . 0))"
    );
}

#[test]
fn posn_at_x_y_uses_one_presented_transform_for_text_window_and_frame_coordinates() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer = eval.buffers.current_buffer().expect("buffer").id;
    let frame_id = eval.frames.create_frame("posn-presented", 800, 600, buffer);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame
            .prepare_and_activate_display_presentation_for_test(
                crate::window::geometry::PresentationId::new(1),
                vec![crate::window::WindowDisplaySnapshot {
                    window_id,
                    regions: crate::window::PresentedWindowRegions {
                        outer: neomacs_display_protocol::types::Rect::new(
                            144.0, 24.0, 800.0, 600.0,
                        ),
                        text_body: neomacs_display_protocol::types::Rect::new(
                            168.0, 41.0, 760.0, 560.0,
                        ),
                        ..Default::default()
                    },
                    regions_materialized: true,
                    text_area_left_offset: 999,
                    points: vec![crate::window::DisplayPointSnapshot {
                        role: crate::window::DisplayPointRole::Glyph,
                        buffer_pos: crate::buffer::LispCharPos1::ONE,
                        x: 72,
                        y: 999,
                        width: 7,
                        height: 17,
                        row: 99,
                        col: 9,
                    }],
                    body_rows: vec![crate::window::PresentedBodyRowSnapshot {
                        output_row: 99,
                        body_row: 2,
                        body_y: 34,
                    }],
                    ..Default::default()
                }],
            )
            .expect("presented geometry");
    }

    let cases = [
        vec![
            Value::fixnum(72),
            Value::fixnum(51),
            Value::make_window(window_id.0),
            Value::NIL,
        ],
        vec![
            Value::fixnum(96),
            Value::fixnum(51),
            Value::make_window(window_id.0),
            Value::T,
        ],
        vec![
            Value::fixnum(240),
            Value::fixnum(75),
            Value::make_frame(frame_id.0),
            Value::NIL,
        ],
    ];
    for args in cases {
        let result = builtin_posn_at_x_y(&mut eval, args).expect("presented coordinate query");
        assert_eq!(
            super::super::print::print_value(&result),
            format!(
                "(#<window {}> 1 (72 . 34) 0 nil 1 (9 . 2) nil (0 . 0) (7 . 17))",
                window_id.0
            )
        );
    }

    assert!(
        eval.frames
            .get_mut(frame_id)
            .expect("frame")
            .retire_display_presentation(crate::window::geometry::PresentationId::new(1))
    );
    assert!(
        builtin_posn_at_x_y(
            &mut eval,
            vec![
                Value::fixnum(72),
                Value::fixnum(51),
                Value::make_window(window_id.0),
            ],
        )
        .is_err(),
        "GUI coordinates must not fall back to live-window approximation"
    );
}

#[test]
fn frame_relative_posn_at_x_y_rejects_new_surface_area_outside_stale_presentation() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer = eval.buffers.current_buffer().expect("buffer").id;
    let frame_id = eval.frames.create_frame("posn-expose", 800, 600, buffer);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame
            .prepare_and_activate_display_presentation_for_test(
                crate::window::geometry::PresentationId::new(1),
                vec![crate::window::WindowDisplaySnapshot {
                    window_id,
                    regions: crate::window::PresentedWindowRegions {
                        outer: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
                        text_body: neomacs_display_protocol::types::Rect::new(
                            0.0, 0.0, 800.0, 600.0,
                        ),
                        ..Default::default()
                    },
                    regions_materialized: true,
                    points: vec![crate::window::DisplayPointSnapshot {
                        role: crate::window::DisplayPointRole::Glyph,
                        buffer_pos: crate::buffer::LispCharPos1::ONE,
                        x: 790,
                        y: 10,
                        width: 8,
                        height: 16,
                        row: 0,
                        col: 99,
                    }],
                    body_rows: vec![crate::window::PresentedBodyRowSnapshot {
                        output_row: 0,
                        body_row: 0,
                        body_y: 10,
                    }],
                    ..Default::default()
                }],
            )
            .expect("presented geometry");
        // The native surface clock advances before evaluator redisplay publishes
        // replacement geometry.  Presentation #1 remains 800x600.
        frame.resize_pixelwise(1975, 1214);
    }

    let inside = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(790),
            Value::fixnum(10),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("inside presentation query");
    let expose = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(1000),
            Value::fixnum(10),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("expose query");

    assert!(
        !inside.is_nil(),
        "old content remains authoritative inside its extent"
    );
    assert!(
        expose.is_nil(),
        "new surface area outside the active presentation is expose, not a glyph hit"
    );
}

#[test]
fn test_posn_at_x_y_eval_uses_exact_redisplay_snapshot() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-posn-xy", 160, 64, buf_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("abcdef\n");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(4));
    }
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: selected_window,
            text_area_left_offset: 8,
            points: vec![crate::window::DisplayPointSnapshot {
                role: crate::window::DisplayPointRole::Glyph,
                buffer_pos: crate::buffer::LispCharPos1::new(5),
                x: 24,
                y: 18,
                width: 21,
                height: 30,
                row: 1,
                col: 3,
            }],
            rows: vec![crate::window::DisplayRowSnapshot {
                row: 1,
                y: 18,
                height: 30,
                start_x: 0,
                start_col: 0,
                end_x: 0,
                end_col: 0,
                start_buffer_pos: Some(crate::buffer::LispCharPos1::new(5)),
                end_buffer_pos: Some(crate::buffer::LispCharPos1::new(5)),
                fringe: Default::default(),
            }],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let text_relative = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(30),
            Value::fixnum(20),
            Value::make_window(selected_window.0),
            Value::NIL,
        ],
    )
    .unwrap();
    // Ledger 205: the `(X . Y)` cell is the CLICK, not the resolved glyph's
    // origin. `make_lispy_position` fills it before any position lookup runs --
    // `xret = mx - window_box_left (w, TEXT_AREA)`, `yret = wy -
    // WINDOW_TAB_LINE_HEIGHT (w) - WINDOW_HEADER_LINE_HEIGHT (w)`
    // (src/keyboard.c:5882-5883) -- and `posn-col-row` divides the frame's
    // character cell out of it (lisp/subr.el:2053-2090). This fixture asked
    // about (30, 20) and the glyph it lands on starts at (24, 18); GNU answers
    // the query, so the pin was 24/18 and is now 30/20. The `(COL . ROW)` cell
    // stays `(3 . 1)`: GNU's after-EOL column count adds
    // `(to_x - x1) / WINDOW_FRAME_COLUMN_WIDTH` (src/dispnew.c:6428-6430), and
    // 6 pixels short of a 21-pixel glyph is no extra column.
    assert_eq!(
        super::super::print::print_value(&text_relative),
        "(#<window 1> 5 (30 . 20) 0 nil 5 (3 . 1) nil (0 . 0) (21 . 30))"
    );

    let whole_window = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(38),
            Value::fixnum(20),
            Value::make_window(selected_window.0),
            Value::T,
        ],
    )
    .unwrap();
    // The WHOLE-window form asks about x = 38, which is the same text-area
    // column 30 once the 8-pixel text-area offset is taken off -- GNU takes it
    // off in `Fposn_at_x_y` itself (`window_box_left_offset` is added only when
    // WHOLE is nil, src/keyboard.c:13041-13046) -- so both forms answer the
    // same click.
    assert_eq!(
        super::super::print::print_value(&whole_window),
        "(#<window 1> 5 (30 . 20) 0 nil 5 (3 . 1) nil (0 . 0) (21 . 30))"
    );
}

/// One terminal-shaped text row of a fixture snapshot: a single position at
/// the row's own start, which is all these tests need to tell "the text area
/// answered" from "nothing answered".
fn fixture_text_row(
    row: i64,
    y: i64,
    pos: i64,
) -> (
    crate::window::DisplayPointSnapshot,
    crate::window::DisplayRowSnapshot,
) {
    (
        crate::window::DisplayPointSnapshot {
            role: crate::window::DisplayPointRole::Glyph,
            buffer_pos: crate::buffer::LispCharPos1::new(pos),
            x: 0,
            y,
            width: 8,
            height: 16,
            row,
            col: 0,
        },
        crate::window::DisplayRowSnapshot {
            row,
            y,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(crate::buffer::LispCharPos1::new(pos)),
            end_buffer_pos: Some(crate::buffer::LispCharPos1::new(pos)),
            fringe: Default::default(),
        },
    )
}

/// A chrome row: no buffer position, and an extent wide enough that a click
/// inside the window lands on one of its glyphs.
fn fixture_chrome_row(row: i64, y: i64, width: i64) -> crate::window::DisplayRowSnapshot {
    crate::window::DisplayRowSnapshot {
        row,
        y,
        height: 16,
        start_x: 0,
        start_col: 0,
        end_x: width,
        end_col: width / 8,
        start_buffer_pos: None,
        end_buffer_pos: None,
        fringe: Default::default(),
    }
}

#[test]
fn posn_at_x_y_on_the_mode_line_answers_the_mode_line() {
    // Ledger 209, ledger 205's residual 2. GNU's `make_lispy_position` asks
    // `window_from_coordinates (f, mx, my, &part, ...)` FIRST
    // (src/keyboard.c:5793) and branches on ON_MODE_LINE before any buffer
    // position is looked up (src/keyboard.c:5888-5905), where this port went
    // straight to a row lookup and answered nothing.
    //
    // Measured, GNU Emacs 31.0.90, 80x24 pty, `scripts/below-content-audit.el`
    // warm, buffer "abcdef\nghijkl\n" in a window whose body is 21 rows:
    //
    //   two-line|past.x0   (nil (0 . 21) (0 . 21) mode-line)
    //   two-line|past.x5   (nil (5 . 21) (5 . 21) mode-line)
    //
    // `posn-point` is nil because GNU sets `textpos = -1` for this branch
    // (src/keyboard.c:5900), and `posn-actual-col-row`'s ROW is the mode-line
    // row's own index relative to the matrix's first TEXT row, which is the
    // number of text rows (src/dispnew.c:6460).
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-posn-mode-line", 640, 384, buf_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let (point, row) = fixture_text_row(0, 0, 1);
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: selected_window,
            mode_line_height: 16,
            points: vec![point],
            rows: vec![row, fixture_chrome_row(22, 352, 640)],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let on_mode_line = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::fixnum(352),
            Value::make_window(selected_window.0),
        ],
    )
    .expect("posn-at-x-y");
    assert_eq!(
        super::super::print::print_value(&on_mode_line),
        "(#<window 1> mode-line (0 . 352) 0 nil nil (0 . 22) nil (0 . 0) (8 . 16))",
        "a mode-line coordinate answers the mode line, not nothing"
    );

    // Five columns in, on the same row: GNU's column is the index of the glyph
    // under X (src/dispnew.c:6465-6470), which grows with the click.
    let five_columns_in = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(40),
            Value::fixnum(352),
            Value::make_window(selected_window.0),
        ],
    )
    .expect("posn-at-x-y");
    assert_eq!(
        super::super::print::print_value(&five_columns_in),
        "(#<window 1> mode-line (40 . 352) 0 nil nil (5 . 22) nil (0 . 0) (8 . 16))"
    );

    // The row above it is still the text area, so the classification decides
    // WHERE the mode-line arm fires rather than swallowing the window.
    let above_it = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::make_window(selected_window.0),
        ],
    )
    .expect("posn-at-x-y");
    assert_eq!(
        super::super::print::print_value(&above_it),
        "(#<window 1> 1 (0 . 0) 0 nil 1 (0 . 0) nil (0 . 0) (8 . 16))"
    );
}

#[test]
fn posn_at_x_y_at_y_zero_of_a_window_with_a_header_line_answers_row_minus_one() {
    // Ledger 209. `posn-at-x-y`'s Y is WINDOW-relative and GNU's own doc string
    // says so -- "Note that the text area includes the header-line and the
    // tab-line of the window" (src/keyboard.c:13011-13013) -- so Y = 0 in a
    // window with a header line IS the header line.
    //
    // The ROW it reports is -1, and that is not arbitrary: `mode_line_string`
    // answers `row - MATRIX_FIRST_TEXT_ROW (w->current_matrix)`
    // (src/dispnew.c:6460) and the header-line row sits immediately above the
    // first text row. Measured, GNU Emacs 31.0.90, 80x24 pty, warm:
    //
    //   header-line|r0.x0    (nil (0 . 0)  (0 . -1) header-line)
    //   header-line|r0.x40   (nil (40 . 0) (40 . -1) header-line)
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-posn-header-line", 640, 384, buf_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let (point, row) = fixture_text_row(1, 16, 1);
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: selected_window,
            header_line_height: 16,
            mode_line_height: 16,
            points: vec![point],
            rows: vec![
                fixture_chrome_row(0, 0, 640),
                row,
                fixture_chrome_row(22, 352, 640),
            ],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let on_header_line = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::make_window(selected_window.0),
        ],
    )
    .expect("posn-at-x-y");
    assert_eq!(
        super::super::print::print_value(&on_header_line),
        "(#<window 1> header-line (0 . 0) 0 nil nil (0 . -1) nil (0 . 0) (8 . 16))"
    );

    // And the row below it is the first line of TEXT, on the same snapshot, so
    // the header-line arm states WHERE it fires rather than swallowing the
    // window. Both of its cells are text-area-relative and both are zero:
    // GNU's `yret = wy - WINDOW_TAB_LINE_HEIGHT (w) -
    // WINDOW_HEADER_LINE_HEIGHT (w)` (src/keyboard.c:5883) turns 16 into 0, and
    // `posn-actual-col-row`'s ROW is `it.vpos` (src/dispnew.c:6433), which
    // counts from the first TEXT row and so is 0 as well. Measured, GNU Emacs
    // 31.0.90, 80x24 pty, warm: `header-line|r1.x0` answers
    // `(1 (0 . 0) (0 . 0) nil)`.
    let first_text_row = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::fixnum(16),
            Value::make_window(selected_window.0),
        ],
    )
    .expect("posn-at-x-y");
    assert_eq!(
        super::super::print::print_value(&first_text_row),
        "(#<window 1> 1 (0 . 0) 0 nil 1 (0 . 0) nil (0 . 0) (8 . 16))"
    );
}

#[test]
fn posn_at_x_y_past_a_window_with_no_mode_line_resolves_the_window_below_it() {
    // Ledger 209, ledger 205's residual 3. `Fposn_at_x_y` converts a WINDOW
    // argument into FRAME pixels and hands them to `make_lispy_position`
    // (src/keyboard.c:13036-13052); the window the caller named is an ORIGIN
    // for that conversion, not the answer. `window_from_coordinates` walks the
    // frame's windows -- the minibuffer window included, because it is the root
    // window's `next` sibling and `foreach_window` follows `w->next`
    // (src/window.c:8965-8992) -- and decides.
    //
    // Measured, GNU Emacs 31.0.90, 80x24 pty, warm: a window with
    // `mode-line-format` nil has a 22-row body, and `(posn-at-x-y 0 22 WIN)`
    // answers `(1 (0 . 0) (0 . 0) nil)` -- the MINIBUFFER window's only
    // position, with the minibuffer's own coordinates.
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-posn-no-mode-line", 640, 384, buf_id);
    let (selected_window, minibuffer_window) = {
        let frame = eval.frames.get(frame_id).expect("frame");
        (
            frame.selected_window,
            frame
                .minibuffer_leaf
                .as_ref()
                .expect("a frame has a minibuffer window")
                .id(),
        )
    };
    {
        let (body_point, body_row) = fixture_text_row(0, 0, 1);
        let (mini_point, mini_row) = fixture_text_row(0, 0, 1);
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![
            crate::window::WindowDisplaySnapshot {
                window_id: selected_window,
                points: vec![body_point],
                rows: vec![body_row],
                ..crate::window::WindowDisplaySnapshot::default()
            },
            crate::window::WindowDisplaySnapshot {
                window_id: minibuffer_window,
                points: vec![mini_point],
                rows: vec![mini_row],
                ..crate::window::WindowDisplaySnapshot::default()
            },
        ]);
    }

    // The root window is 368 pixels tall (23 rows of 16) with no mode line, so
    // a Y of 368 is one row past its own body and belongs to the minibuffer.
    let past_the_body = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::fixnum(368),
            Value::make_window(selected_window.0),
        ],
    )
    .expect("posn-at-x-y");
    assert_eq!(
        super::super::print::print_value(&past_the_body),
        "(#<window 2> 1 (0 . 0) 0 nil 1 (0 . 0) nil (0 . 0) (8 . 16))",
        "the window the caller named is the origin of the conversion, not the answer"
    );

    // The falsifiable half: inside the named window's own body the answer is
    // still that window's, so the re-resolution states WHERE it fires.
    let inside_the_body = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::fixnum(352),
            Value::make_window(selected_window.0),
        ],
    )
    .expect("posn-at-x-y");
    assert_eq!(
        super::super::print::print_value(&inside_the_body),
        "(#<window 1> 1 (0 . 352) 0 nil 1 (0 . 0) nil (0 . 0) (8 . 16))"
    );
}

#[test]
fn posn_at_x_y_outside_every_window_answers_the_frame() {
    // Ledger 209. When no window of the frame owns the coordinate,
    // `window_from_coordinates` returns nil and `make_lispy_position` falls
    // into its frame branch (src/keyboard.c:6059-6075): the posn names the
    // FRAME, carries the click, and stops -- a four-element list, which is why
    // `posn-actual-col-row` (`(nth 6 ...)`, lisp/subr.el:2103-2116) is nil
    // while `posn-col-row`, derived from `posn-x-y`, still answers.
    //
    // Measured, GNU Emacs 31.0.90, 80x24 pty, warm, one row below the
    // minibuffer window: `minibuffer|past.x0` answers
    // `(nil (0 . 24) nil nil)` for (posn-point, posn-col-row,
    // posn-actual-col-row, posn-area).
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("xdisp-posn-off-frame", 640, 384, buf_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let (point, row) = fixture_text_row(0, 0, 1);
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: selected_window,
            points: vec![point],
            rows: vec![row],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let below_the_frame = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::fixnum(400),
            Value::make_window(selected_window.0),
        ],
    )
    .expect("posn-at-x-y");
    let elements =
        crate::emacs_core::value::list_to_vec(&below_the_frame).expect("a posn is a proper list");
    assert_eq!(
        elements.len(),
        4,
        "the frame branch stops after the timestamp: {}",
        super::super::print::print_value(&below_the_frame)
    );
    assert!(
        elements[0].as_frame_id().is_some(),
        "the posn names the frame: {}",
        super::super::print::print_value(&below_the_frame)
    );
    assert!(elements[1].is_nil(), "no area, and no buffer position");
    assert_eq!(
        super::super::print::print_value(&elements[2]),
        "(0 . 400)",
        "the click reaches the posn in frame pixels"
    );
}

#[test]
fn test_posn_at_x_y_batch_uses_selected_window_without_snapshot() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let scratch = eval.buffers.current_buffer_id().expect("scratch buffer");
    eval.frames
        .create_frame("xdisp-batch-posn", 80, 25, scratch);
    let temp = eval.buffers.create_buffer(" *temp*");
    eval.set_current_buffer_unrecorded(temp)
        .expect("switch to temp buffer");
    {
        let buffer = eval.buffers.current_buffer_mut().expect("temp buffer");
        buffer.insert("hello world\nsecond line");
    }

    let at_point = builtin_posn_at_point(&mut eval, vec![Value::fixnum(3)]).unwrap();
    let at_xy = builtin_posn_at_x_y(&mut eval, vec![Value::fixnum(0), Value::fixnum(0)]).unwrap();

    assert!(at_point.is_nil());
    assert_eq!(
        super::super::print::print_value(&at_xy),
        "(#<window 1> 1 (0 . 0) 0 nil 1 (0 . 0) nil (0 . 0) (0 . 0))"
    );
}

#[test]
fn tty_right_divider_parameter_does_not_change_geometry_or_border_hit() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();

    let result = eval
        .eval_str(
            r##"(progn
                  (split-window-right)
                  (let* ((left (frame-first-window))
                         (before-right (nth 2 (window-inside-pixel-edges left))))
                    (set-frame-parameter nil 'right-divider-width 8)
                    (redisplay t)
                    (let* ((after-right (nth 2 (window-inside-pixel-edges left)))
                           (y (+ 10 (nth 1 (window-inside-pixel-edges left)))))
                      (list before-right
                            after-right
                            (frame-parameter nil 'right-divider-width)
                            (frame-right-divider-width)
                            (window-right-divider-width left)
                            (posn-area (posn-at-x-y after-right y nil t))))))"##,
        )
        .expect("issue 299 terminal divider probe");

    assert_eq!(
        super::super::print::print_value(&result),
        "(39 39 8 0 0 vertical-line)"
    );
}

#[test]
fn test_posn_at_x_y_batch_wraps_long_visual_lines_like_gnu_tty() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let scratch = eval.buffers.current_buffer_id().expect("scratch buffer");
    let frame_id = eval
        .frames
        .create_frame("xdisp-batch-wrap-posn", 80 * 8, 25 * 16, scratch);
    {
        let buffer = eval.buffers.current_buffer_mut().expect("scratch buffer");
        buffer.insert(&"x".repeat(500));
    }
    let (char_width, char_height) = {
        let frame = eval.frames.get(frame_id).expect("frame");
        (frame.char_width as i64, frame.char_height as i64)
    };

    let at_xy = builtin_posn_at_x_y(
        &mut eval,
        vec![
            Value::fixnum(3 * char_width),
            Value::fixnum(2 * char_height),
        ],
    )
    .unwrap();

    assert_eq!(
        super::super::print::print_value(&at_xy),
        format!(
            "(#<window 1> 162 ({} . {}) 0 nil 162 (3 . 2) nil (0 . 0) (0 . 0))",
            3 * char_width,
            2 * char_height
        )
    );
}

#[test]
fn test_posn_at_point_eval_returns_nil_outside_visible_snapshot_span() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("xdisp-posn-offscreen", 160, 64, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("abcdefghijklmnopqrstuvwxyz\n");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: selected_window,
            text_area_left_offset: 8,
            points: vec![
                crate::window::DisplayPointSnapshot {
                    role: crate::window::DisplayPointRole::Glyph,
                    buffer_pos: crate::buffer::LispCharPos1::new(10),
                    x: 24,
                    y: 18,
                    width: 8,
                    height: 16,
                    row: 0,
                    col: 2,
                },
                crate::window::DisplayPointSnapshot {
                    role: crate::window::DisplayPointRole::Glyph,
                    buffer_pos: crate::buffer::LispCharPos1::new(14),
                    x: 56,
                    y: 18,
                    width: 8,
                    height: 16,
                    row: 0,
                    col: 6,
                },
            ],
            rows: vec![crate::window::DisplayRowSnapshot {
                row: 0,
                y: 18,
                height: 16,
                start_x: 0,
                start_col: 0,
                end_x: 0,
                end_col: 0,
                start_buffer_pos: Some(crate::buffer::LispCharPos1::new(10)),
                end_buffer_pos: Some(crate::buffer::LispCharPos1::new(14)),
                fringe: Default::default(),
            }],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let before = builtin_posn_at_point(
        &mut eval,
        vec![Value::fixnum(5), Value::make_window(selected_window.0)],
    )
    .unwrap();
    let after = builtin_posn_at_point(
        &mut eval,
        vec![Value::fixnum(20), Value::make_window(selected_window.0)],
    )
    .unwrap();
    let hidden_gap = builtin_posn_at_point(
        &mut eval,
        vec![Value::fixnum(12), Value::make_window(selected_window.0)],
    )
    .unwrap();

    assert!(
        before.is_nil(),
        "expected offscreen position before span to be nil, got {before:?}"
    );
    assert!(
        after.is_nil(),
        "expected offscreen position after span to be nil, got {after:?}"
    );
    assert_eq!(
        super::super::print::print_value(&hidden_gap),
        "(#<window 1> 14 (56 . 18) 0 nil 14 (6 . 0) nil (0 . 0) (8 . 16))"
    );
}

#[test]
fn test_posn_at_point_eval_returns_nil_for_positions_missing_entire_visible_row() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("xdisp-posn-missing-row", 160, 96, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("abcdef\n");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: selected_window,
            text_area_left_offset: 8,
            points: vec![
                crate::window::DisplayPointSnapshot {
                    role: crate::window::DisplayPointRole::Glyph,
                    buffer_pos: crate::buffer::LispCharPos1::new(1),
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 16,
                    row: 0,
                    col: 0,
                },
                crate::window::DisplayPointSnapshot {
                    role: crate::window::DisplayPointRole::Glyph,
                    buffer_pos: crate::buffer::LispCharPos1::new(4),
                    x: 0,
                    y: 18,
                    width: 8,
                    height: 16,
                    row: 1,
                    col: 0,
                },
            ],
            rows: vec![
                crate::window::DisplayRowSnapshot {
                    row: 0,
                    y: 0,
                    height: 16,
                    start_x: 0,
                    start_col: 0,
                    end_x: 0,
                    end_col: 0,
                    start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                    end_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                    fringe: Default::default(),
                },
                crate::window::DisplayRowSnapshot {
                    row: 1,
                    y: 18,
                    height: 16,
                    start_x: 0,
                    start_col: 0,
                    end_x: 0,
                    end_col: 0,
                    start_buffer_pos: Some(crate::buffer::LispCharPos1::new(4)),
                    end_buffer_pos: Some(crate::buffer::LispCharPos1::new(4)),
                    fringe: Default::default(),
                },
            ],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let missing = builtin_posn_at_point(
        &mut eval,
        vec![Value::fixnum(2), Value::make_window(selected_window.0)],
    )
    .unwrap();
    assert!(
        missing.is_nil(),
        "expected missing position between visible rows to be nil, got {missing:?}"
    );
}

#[test]
fn test_vertical_motion_eval_uses_live_redisplay_rows() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("xdisp-vertical-motion-rows", 160, 96, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert(&"a\n".repeat(100));
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: selected_window,
            rows: vec![
                crate::window::DisplayRowSnapshot {
                    row: 0,
                    y: 0,
                    height: 16,
                    start_x: 0,
                    start_col: 0,
                    end_x: 8,
                    end_col: 1,
                    start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                    end_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                    fringe: Default::default(),
                },
                crate::window::DisplayRowSnapshot {
                    row: 1,
                    y: 16,
                    height: 16,
                    start_x: 0,
                    start_col: 0,
                    end_x: 8,
                    end_col: 1,
                    start_buffer_pos: Some(crate::buffer::LispCharPos1::new(40)),
                    end_buffer_pos: Some(crate::buffer::LispCharPos1::new(40)),
                    fringe: Default::default(),
                },
                crate::window::DisplayRowSnapshot {
                    row: 2,
                    y: 32,
                    height: 16,
                    start_x: 0,
                    start_col: 0,
                    end_x: 8,
                    end_col: 1,
                    start_buffer_pos: Some(crate::buffer::LispCharPos1::new(80)),
                    end_buffer_pos: Some(crate::buffer::LispCharPos1::new(80)),
                    fringe: Default::default(),
                },
            ],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let result = eval
        .eval_str("(progn (goto-char 1) (list (vertical-motion 2) (point)))")
        .unwrap();
    assert_eq!(super::super::print::print_value(&result), "(2 80)");
}

#[test]
fn redisplay_point_relative_motion_reports_failure_without_inventing_a_start() {
    let mut eval = interactive_context();

    assert_eq!(
        eval.redisplay_start_before_point_by_display_rows(
            crate::buffer::BufferId(u64::MAX),
            crate::window::WindowId(u64::MAX),
            CharPos0::new(42),
            10,
        ),
        None,
        "a failed display-row motion must let redisplay preserve its semantic viewport"
    );
}

#[test]
fn test_vertical_motion_eval_uses_live_redisplay_goal_column() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("xdisp-vertical-motion-column", 160, 96, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert(&"a\n".repeat(100));
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: selected_window,
            points: vec![
                crate::window::DisplayPointSnapshot {
                    role: crate::window::DisplayPointRole::Glyph,
                    buffer_pos: crate::buffer::LispCharPos1::new(40),
                    x: 0,
                    y: 16,
                    width: 8,
                    height: 16,
                    row: 1,
                    col: 0,
                },
                crate::window::DisplayPointSnapshot {
                    role: crate::window::DisplayPointRole::Glyph,
                    buffer_pos: crate::buffer::LispCharPos1::new(43),
                    x: 24,
                    y: 16,
                    width: 8,
                    height: 16,
                    row: 1,
                    col: 3,
                },
                crate::window::DisplayPointSnapshot {
                    role: crate::window::DisplayPointRole::Glyph,
                    buffer_pos: crate::buffer::LispCharPos1::new(45),
                    x: 40,
                    y: 16,
                    width: 8,
                    height: 16,
                    row: 1,
                    col: 5,
                },
            ],
            rows: vec![
                crate::window::DisplayRowSnapshot {
                    row: 0,
                    y: 0,
                    height: 16,
                    start_x: 0,
                    start_col: 0,
                    end_x: 8,
                    end_col: 1,
                    start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                    end_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                    fringe: Default::default(),
                },
                crate::window::DisplayRowSnapshot {
                    row: 1,
                    y: 16,
                    height: 16,
                    start_x: 0,
                    start_col: 0,
                    end_x: 48,
                    end_col: 6,
                    start_buffer_pos: Some(crate::buffer::LispCharPos1::new(40)),
                    end_buffer_pos: Some(crate::buffer::LispCharPos1::new(45)),
                    fringe: Default::default(),
                },
            ],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let result = eval
        .eval_str("(progn (goto-char 1) (list (vertical-motion (cons 3 1)) (point)))")
        .unwrap();
    assert_eq!(super::super::print::print_value(&result), "(1 43)");
}

/// A goal column past everything a row draws lands on the row's END boundary,
/// not on its last glyph.
///
/// `end-of-visual-line` is `(vertical-motion (cons (window-width) 0))`
/// (lisp/simple.el:8558), and GNU reaches the goal column with
/// `move_it_in_display_line (&it, ZV, first_x + to_x, MOVE_TO_X)`
/// (src/indent.c:2540). `move_it_in_display_line_to` stops either at the first
/// glyph that reaches the goal x or, when the goal is past the whole row, where
/// the DISPLAY LINE ends: at the newline it refuses to consume. So on a
/// newline-terminated row the answer is the newline's own position, one column
/// past the last glyph -- verified under GNU on a 24-column `visual-line-mode`
/// window, where every screen line answers `next-screen-line-start - 1`.
///
/// The fixture mirrors what the layout engine really publishes for such a row:
/// `end_buffer_pos` is the terminating NEWLINE (see
/// `overlay_string_newline_leaves_row_bounds_on_the_anchor_boundary` in
/// neomacs-layout-engine, where "hello world\nsecond\n" yields bounds
/// `(1, 12)` and `(13, 19)`), while `points` carry only drawn glyphs -- the
/// newline draws none.
#[test]
fn test_vertical_motion_goal_column_past_row_end_lands_on_the_row_end_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("xdisp-vertical-motion-row-end", 160, 96, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("hello world\nsecond\n");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    {
        // "hello world" is 11 glyphs at columns 0..=10; the newline at position
        // 12 draws nothing, so the row's pen stops at column 11.
        let points = (0..11)
            .map(|index| crate::window::DisplayPointSnapshot {
                role: crate::window::DisplayPointRole::Glyph,
                buffer_pos: crate::buffer::LispCharPos1::new(1 + index),
                x: 8 * index as i64,
                y: 0,
                width: 8,
                height: 16,
                row: 0,
                col: index as i64,
            })
            .collect::<Vec<_>>();
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            window_id: selected_window,
            points,
            rows: vec![crate::window::DisplayRowSnapshot {
                row: 0,
                y: 0,
                height: 16,
                start_x: 0,
                start_col: 0,
                end_x: 88,
                end_col: 11,
                start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                end_buffer_pos: Some(crate::buffer::LispCharPos1::new(12)),
                fringe: Default::default(),
            }],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    // A goal column beyond the row: GNU stops at the newline (position 12).
    let past_end = eval
        .eval_str("(progn (goto-char 3) (list (vertical-motion (cons 40 0)) (point)))")
        .unwrap();
    assert_eq!(super::super::print::print_value(&past_end), "(0 12)");

    // A goal column inside the row still selects the glyph at that column.
    let inside = eval
        .eval_str("(progn (goto-char 3) (list (vertical-motion (cons 6 0)) (point)))")
        .unwrap();
    assert_eq!(super::super::print::print_value(&inside), "(0 7)");
}

#[test]
fn test_move_point_visually() {
    crate::test_utils::init_test_tracing();
    for direction in [1_i64, 0, -1, 2] {
        let err = builtin_move_point_visually(vec![Value::fixnum(direction)]).unwrap_err();
        match err {
            Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "args-out-of-range"),
            other => panic!("expected args-out-of-range, got {:?}", other),
        }
    }

    let err = builtin_move_point_visually(vec![Value::char('a')]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "args-out-of-range"),
        other => panic!("expected args-out-of-range, got {:?}", other),
    }

    let err = builtin_move_point_visually(vec![Value::symbol("left")]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }
}

#[test]
fn test_lookup_image_map() {
    crate::test_utils::init_test_tracing();
    let result = builtin_lookup_image_map(vec![
        Value::symbol("map"),
        Value::fixnum(10),
        Value::fixnum(20),
    ])
    .unwrap();
    assert!(result.is_nil());

    let err = builtin_lookup_image_map(vec![
        Value::symbol("image"),
        Value::string("x"),
        Value::symbol("y"),
    ])
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    let err = builtin_lookup_image_map(vec![
        Value::symbol("image"),
        Value::fixnum(1),
        Value::symbol("y"),
    ])
    .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    let result =
        builtin_lookup_image_map(vec![Value::NIL, Value::fixnum(1), Value::string("y")]).unwrap();
    assert!(result.is_nil());

    let err = builtin_lookup_image_map(vec![]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments, got {:?}", other),
    }
}

#[test]
fn test_current_bidi_paragraph_direction() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = builtin_current_bidi_paragraph_direction(&mut eval, vec![]).unwrap();
    assert_eq!(result, Value::symbol("left-to-right"));

    let result = builtin_current_bidi_paragraph_direction(
        &mut eval,
        vec![Value::make_buffer(crate::buffer::BufferId(1))],
    )
    .unwrap();
    assert_eq!(result, Value::symbol("left-to-right"));

    let err = builtin_current_bidi_paragraph_direction(&mut eval, vec![Value::symbol("buffer")])
        .unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }
}

#[test]
fn test_bidi_resolved_levels() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_bidi_resolved_levels(vec![]).unwrap().is_nil());
    assert!(
        builtin_bidi_resolved_levels(vec![Value::NIL])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_bidi_resolved_levels(vec![Value::fixnum(0)])
            .unwrap()
            .is_nil()
    );

    let err = builtin_bidi_resolved_levels(vec![Value::T]).unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("fixnump"), Value::T]);
        }
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }
}

#[test]
fn test_bidi_find_overridden_directionality() {
    crate::test_utils::init_test_tracing();
    assert!(
        builtin_bidi_find_overridden_directionality(vec![
            Value::string("abc"),
            Value::fixnum(0),
            Value::string("x"),
        ])
        .unwrap()
        .is_nil()
    );
    assert!(
        builtin_bidi_find_overridden_directionality(vec![
            Value::NIL,
            Value::fixnum(0),
            Value::string("x"),
        ])
        .unwrap()
        .is_nil()
    );
    assert!(
        builtin_bidi_find_overridden_directionality(vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::NIL,
        ])
        .unwrap()
        .is_nil()
    );

    let third_arg_err = builtin_bidi_find_overridden_directionality(vec![
        Value::string("abc"),
        Value::fixnum(0),
        Value::fixnum(3),
    ])
    .unwrap_err();
    match third_arg_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(3)]);
        }
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }

    let region_arg_err =
        builtin_bidi_find_overridden_directionality(vec![Value::NIL, Value::fixnum(2), Value::NIL])
            .unwrap_err();
    match region_arg_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("integer-or-marker-p"), Value::NIL]
            );
        }
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }
}

#[test]
fn test_move_to_window_line() {
    crate::test_utils::init_test_tracing();
    // Without a selected frame, move-to-window-line should signal an error.
    let mut ev = crate::emacs_core::Context::new();
    for arg in [Value::fixnum(1), Value::fixnum(0), Value::symbol("left")] {
        let err = builtin_move_to_window_line(&mut ev, vec![arg]).unwrap_err();
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "error");
            }
            other => panic!("expected error signal, got {:?}", other),
        }
    }
}

#[test]
fn test_tool_bar_height() {
    crate::test_utils::init_test_tracing();
    let result = builtin_tool_bar_height(vec![]).unwrap();
    assert_eq!(result, Value::fixnum(0));

    let result = builtin_tool_bar_height(vec![Value::symbol("frame")]).unwrap();
    assert_eq!(result, Value::fixnum(0));
}

#[test]
fn test_tool_bar_height_eval_frame_validation() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-test", 80, 24, buf_id);

    {
        let frame = eval
            .frames
            .get_mut(frame_id)
            .expect("xdisp test frame should exist");
        frame.char_height = 17.0;
        frame.set_parameter(Value::symbol("tool-bar-lines"), Value::fixnum(2));
        frame.sync_tool_bar_height_from_parameters();
    }

    let result =
        builtin_tool_bar_height_ctx(&mut eval, vec![Value::fixnum(frame_id.0 as i64)]).unwrap();
    assert_eq!(result, Value::fixnum(2));

    let pixelwise =
        builtin_tool_bar_height_ctx(&mut eval, vec![Value::fixnum(frame_id.0 as i64), Value::T])
            .unwrap();
    assert_eq!(pixelwise, Value::fixnum(34));

    let err = builtin_tool_bar_height_ctx(&mut eval, vec![Value::string("x")]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }
}

#[test]
fn test_tab_bar_height() {
    crate::test_utils::init_test_tracing();
    let result = builtin_tab_bar_height(vec![]).unwrap();
    assert_eq!(result, Value::fixnum(0));

    let result = builtin_tab_bar_height(vec![Value::symbol("frame")]).unwrap();
    assert_eq!(result, Value::fixnum(0));
}

#[test]
fn test_tab_bar_height_eval_frame_validation() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval.frames.create_frame("xdisp-test", 80, 24, buf_id);

    let result =
        builtin_tab_bar_height_ctx(&mut eval, vec![Value::fixnum(frame_id.0 as i64)]).unwrap();
    assert_eq!(result, Value::fixnum(0));

    let err = builtin_tab_bar_height_ctx(&mut eval, vec![Value::string("x")]).unwrap_err();
    match err {
        Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "wrong-type-argument"),
        other => panic!("expected wrong-type-argument, got {:?}", other),
    }
}

#[test]
fn test_tab_bar_height_eval_reflects_tab_bar_lines_and_pixels() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let frame_id = super::super::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval.frames.get_mut(frame_id).expect("selected frame");
        frame.char_height = 20.0;
    }
    super::super::frame::builtin_modify_frame_parameters(
        &mut eval,
        vec![
            Value::fixnum(frame_id.0 as i64),
            Value::list(vec![Value::cons(
                Value::symbol("tab-bar-lines"),
                Value::fixnum(1),
            )]),
        ],
    )
    .unwrap();

    let lines =
        builtin_tab_bar_height_ctx(&mut eval, vec![Value::fixnum(frame_id.0 as i64)]).unwrap();
    assert_eq!(lines, Value::fixnum(1));

    let pixels =
        builtin_tab_bar_height_ctx(&mut eval, vec![Value::fixnum(frame_id.0 as i64), Value::T])
            .unwrap();
    assert_eq!(pixels, Value::fixnum(20));

    let frame = eval.frames.get(frame_id).expect("selected frame");
    assert_eq!(frame.tab_bar_height, 20);
}

#[test]
fn test_line_number_display_width() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();

    let result = crate::emacs_core::indent::line_number_display_width(&mut eval, vec![]).unwrap();
    assert_eq!(result, Value::fixnum(0));

    let frame_id = super::super::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .char_width = 8.0;
    {
        let buffer = eval.buffers.current_buffer_mut().expect("current buffer");
        buffer.set_buffer_local("display-line-numbers", Value::T);
        for _ in 0..100 {
            buffer.insert("x\n");
        }
    }

    let result = crate::emacs_core::indent::line_number_display_width(&mut eval, vec![]).unwrap();
    assert_eq!(result, Value::fixnum(3));

    let result =
        crate::emacs_core::indent::line_number_display_width(&mut eval, vec![Value::T]).unwrap();
    assert_eq!(result, Value::fixnum(40));

    let result = crate::emacs_core::indent::line_number_display_width(
        &mut eval,
        vec![Value::symbol("columns")],
    )
    .unwrap();
    match result.kind() {
        ValueKind::Float => assert_eq!(result.xfloat(), 5.0),
        other => panic!("expected float, got {other:?}"),
    }
}

#[test]
fn line_number_display_width_uses_byte_newline_count_not_char_pos_scan() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();

    let frame_id = super::super::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .char_width = 8.0;
    {
        let buffer = eval.buffers.current_buffer_mut().expect("current buffer");
        buffer.set_buffer_local("display-line-numbers", Value::T);
        for _ in 0..2_000 {
            buffer.insert("é journal line\n");
        }
    }

    crate::buffer::buffer_text::reset_char_pos_to_emacs_byte_pos_call_count();
    let result = crate::emacs_core::indent::line_number_display_width(&mut eval, vec![]).unwrap();

    assert_eq!(result, Value::fixnum(4));
    assert_eq!(
        crate::buffer::buffer_text::char_pos_to_emacs_byte_pos_call_count(),
        0,
        "line-number width must use byte-level newline counting, not per-character byte conversion"
    );
}

#[test]
fn test_long_line_optimizations_p() {
    crate::test_utils::init_test_tracing();
    let result = builtin_long_line_optimizations_p(vec![]).unwrap();
    assert!(result.is_nil());
}

// Test wrong arity errors
#[test]
fn test_wrong_arity() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_line_pixel_height(vec![Value::fixnum(1)]).is_err());
    {
        let mut ev = crate::emacs_core::Context::new();
        assert!(builtin_invisible_p(&mut ev, vec![]).is_err());
    }
    assert!(builtin_move_point_visually(vec![]).is_err());
    assert!(builtin_lookup_image_map(vec![Value::fixnum(1), Value::fixnum(2)]).is_err());
    {
        let mut ev = crate::emacs_core::Context::new();
        assert!(builtin_move_to_window_line(&mut ev, vec![]).is_err());
    }
}

// Test optional args
#[test]
fn test_optional_args() {
    crate::test_utils::init_test_tracing();
    // format-mode-line allows 1-4 args
    assert!(builtin_format_mode_line(vec![]).is_err());
    assert!(builtin_format_mode_line(vec![Value::string("fmt")]).is_ok());
    assert!(
        builtin_format_mode_line(vec![
            Value::string("fmt"),
            Value::symbol("face"),
            Value::NIL,
            Value::NIL,
        ])
        .is_ok()
    );
    assert!(
        builtin_format_mode_line(vec![
            Value::string("fmt"),
            Value::symbol("face"),
            Value::symbol("window"),
            Value::symbol("buffer"),
            Value::symbol("extra"),
        ])
        .is_err()
    );

    // window-text-pixel-size allows 0-7 args
    assert!(builtin_window_text_pixel_size(vec![]).is_ok());
    assert!(
        builtin_window_text_pixel_size(vec![
            Value::NIL,
            Value::fixnum(1),
            Value::fixnum(100),
            Value::fixnum(500),
            Value::fixnum(300),
            Value::symbol("mode"),
            Value::symbol("pixelwise"),
        ])
        .is_ok()
    );
    assert!(builtin_window_text_pixel_size(vec![Value::fixnum(1); 8]).is_err());
}

/// `display_prop_replacing_p` must match GNU 31.0.50's `display_prop_intangible_p`
/// taxonomy (verified by driving the real binary's command loop): replacing
/// specs make text intangible; modifying-only specs do not; media specs replace
/// only on a window (GUI) frame. The classification args are irrelevant — only
/// the spec head/shape matters — so minimal forms are used.
#[test]
fn display_prop_replacing_p_matches_gnu_taxonomy() {
    fn head(name: &str) -> Value {
        Value::list(vec![Value::symbol(name)])
    }
    let sym = Value::symbol;

    // Replacing, frame-independent.
    assert!(display_prop_replacing_p(Value::string("=>"), false));
    assert!(display_prop_replacing_p(Value::string(""), false)); // empty string
    assert!(display_prop_replacing_p(head("space"), false));
    assert!(display_prop_replacing_p(head("left-fringe"), false));
    assert!(display_prop_replacing_p(head("right-fringe"), false));
    // ((margin LOCATION) "x")
    assert!(display_prop_replacing_p(
        Value::list(vec![
            Value::list(vec![sym("margin"), sym("left-margin")]),
            Value::string("x"),
        ]),
        false,
    ));
    // List of specs containing a string: ((raise 1) "=>")
    assert!(display_prop_replacing_p(
        Value::list(vec![
            Value::list(vec![sym("raise"), Value::fixnum(1)]),
            Value::string("=>"),
        ]),
        false,
    ));
    // Vector of specs.
    assert!(display_prop_replacing_p(
        Value::vector(vec![Value::string("=>")]),
        false
    ));
    // (disable-eval "=>")
    assert!(display_prop_replacing_p(
        Value::list(vec![sym("disable-eval"), Value::string("=>")]),
        false,
    ));

    // Modifying-only: never replacing.
    for name in ["height", "raise", "slice", "space-width", "min-width"] {
        assert!(
            !display_prop_replacing_p(head(name), false),
            "({name} …) must be modifying, not replacing"
        );
    }
    // List of only-modifying specs.
    assert!(!display_prop_replacing_p(
        Value::list(vec![head("raise"), head("height")]),
        false,
    ));
    // (when nil "=>") is disabled; (when t "=>") parses to the rest list and is
    // not itself a string, matching GNU (both non-replacing).
    assert!(!display_prop_replacing_p(
        Value::list(vec![sym("when"), Value::NIL, Value::string("=>")]),
        false,
    ));
    assert!(!display_prop_replacing_p(
        Value::list(vec![sym("when"), Value::T, Value::string("=>")]),
        false,
    ));

    // Media specs replace text only on a window (GUI) frame.
    for name in ["image", "xwidget", "video", "webkit"] {
        assert!(
            !display_prop_replacing_p(head(name), false),
            "({name} …) does not replace on a tty frame"
        );
        assert!(
            display_prop_replacing_p(head(name), true),
            "({name} …) replaces on a GUI frame"
        );
    }
}

// ---------------------------------------------------------------------------
// neomacs--frame-snapshot
// ---------------------------------------------------------------------------

fn snapshot_string(value: Value) -> String {
    crate::emacs_core::emacs_char::to_utf8_lossy(
        value.as_lisp_string().expect("string result").as_bytes(),
    )
}

#[test]
fn frame_snapshot_errors_without_display_hook() {
    let mut eval = Context::new();
    assert!(
        eval.eval_str("(neomacs--frame-snapshot)").is_err(),
        "must signal in batch mode (no frontend hook installed)"
    );
}

#[test]
fn frame_snapshot_forwards_request_to_installed_hook() {
    use crate::emacs_core::xdisp::{SnapshotFormat, SnapshotTarget};
    use std::cell::RefCell;
    use std::rc::Rc;

    let mut eval = Context::new();
    let seen: Rc<RefCell<Vec<(SnapshotTarget, SnapshotFormat)>>> = Rc::default();
    let sink = seen.clone();
    eval.frame_snapshot_fn = Some(Box::new(move |_eval, request| {
        sink.borrow_mut().push((request.target, request.format));
        Ok("SNAP".to_string())
    }));

    let value = eval
        .eval_str("(neomacs--frame-snapshot)")
        .expect("selected/text snapshot");
    assert_eq!(snapshot_string(value), "SNAP");
    eval.eval_str("(neomacs--frame-snapshot t 'json)")
        .expect("all/json snapshot");
    eval.eval_str("(neomacs--frame-snapshot nil 'text-faces)")
        .expect("selected/text-faces snapshot");

    assert_eq!(
        *seen.borrow(),
        vec![
            (SnapshotTarget::Selected, SnapshotFormat::Text),
            (SnapshotTarget::All, SnapshotFormat::Json),
            (SnapshotTarget::Selected, SnapshotFormat::TextFaces),
        ]
    );
}

#[test]
fn frame_snapshot_rejects_bad_format_and_dead_frame() {
    let mut eval = Context::new();
    eval.frame_snapshot_fn = Some(Box::new(|_, _| Ok(String::new())));
    assert!(
        eval.eval_str("(neomacs--frame-snapshot nil 'bogus)")
            .is_err(),
        "unknown format symbol must signal"
    );
    assert!(
        eval.eval_str("(neomacs--frame-snapshot 999999)").is_err(),
        "dead frame id must signal"
    );
    assert!(
        eval.eval_str("(neomacs--frame-snapshot \"x\")").is_err(),
        "non-frame FRAME arg must signal wrong-type-argument"
    );
}

#[test]
fn write_frame_snapshot_writes_file_and_returns_t() {
    let mut eval = Context::new();
    eval.frame_snapshot_fn = Some(Box::new(|_, _| Ok("SNAPSHOT-CONTENT".to_string())));
    let path = std::env::temp_dir().join(format!(
        "neomacs-frame-snapshot-test-{}.txt",
        std::process::id()
    ));
    let form = format!(
        "(neomacs--write-frame-snapshot {:?} nil 'text)",
        path.to_str().expect("utf8 temp path")
    );
    let value = eval.eval_str(&form).expect("write snapshot");
    assert!(value.is_t());
    assert_eq!(
        std::fs::read_to_string(&path).expect("artifact readable"),
        "SNAPSHOT-CONTENT"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn line_number_anchor_counts_from_recent_line_and_survives_edits_below() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let text = "line\n".repeat(2000);
    eval.eval_str(&format!(
        // `switch-to-buffer' is lisp/window.el:9558 and has no subr any more
        // (DIVERGENCES.md 154); this test only needs the buffer current, which
        // is `set-buffer' (src/buffer.c:2416) -- one of the three C primitives
        // GNU's `switch-to-buffer' body calls.
        "(progn (set-buffer (get-buffer-create \"anchor-test\")) (insert {:?}))",
        text
    ))
    .expect("setup");
    let point_at = |eval: &Context, charpos: usize| {
        let buf = eval.buffers.current_buffer().expect("buffer");
        buf.char_pos_to_emacs_byte_pos_clamped(crate::buffer::CharPos0::new(charpos))
    };
    let buf = eval.buffers.current_buffer().expect("buffer");
    // First computation seeds the anchor at point's line (5000 chars / 5 = 1000 newlines).
    let p = point_at(&eval, 5000);
    assert_eq!(
        super::prefix_line_and_column(buf, buf.accessible_emacs_byte_region(), p).line,
        1001
    );
    assert!(buf.line_number_anchor.get().is_some(), "anchor seeded");
    // Simulate the accepted-frame ack, then an edit BELOW the anchor: the
    // anchor stays valid and the count stays exact.
    buf.reset_unchanged_region();
    eval.eval_str("(progn (goto-char 9000) (insert \"x\"))")
        .expect("edit below");
    let buf = eval.buffers.current_buffer().expect("buffer");
    let p = point_at(&eval, 5000);
    assert_eq!(
        super::prefix_line_and_column(buf, buf.accessible_emacs_byte_region(), p).line,
        1001
    );
    // An edit ABOVE the anchor invalidates it: a newline inserted at the
    // start must shift the count, never show a stale number.
    eval.eval_str("(progn (goto-char 1) (insert \"\\n\"))")
        .expect("edit above");
    let buf = eval.buffers.current_buffer().expect("buffer");
    let p = point_at(&eval, 5001);
    assert_eq!(
        super::prefix_line_and_column(buf, buf.accessible_emacs_byte_region(), p).line,
        1002
    );
}

/// Build a frame whose single window has a committed redisplay snapshot with
/// three text rows, and register two fringe bitmaps so index->symbol resolves.
///
/// Expectations below are measured against GNU Emacs (src/fringe.c
/// `Ffringe_bitmaps_at_pos`): every row that redisplay laid out answers a
/// three-element list, and only a position on no row at all answers nil.
fn fringe_bitmaps_at_pos_fixture() -> (Context, crate::window::FrameId) {
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    eval.buffers
        .get_mut(buf_id)
        .expect("current buffer")
        .insert("alpha\nbeta\ngamma\n");
    let frame_id = eval
        .frames
        .create_frame("xdisp-fringe-bitmaps-at-pos", 160, 80, buf_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;

    let bitmap = |bits: u16| crate::emacs_core::builtins::fringe_bitmap::FringeBitmap {
        bits: vec![bits],
        height: 1,
        width: 8,
        period: 0,
        align: crate::emacs_core::builtins::fringe_bitmap::FringeBitmapAlign::Center,
        face: None,
    };
    let left = eval
        .fringe_bitmaps
        .define(intern("p39-left"), None, bitmap(255));
    let right = eval
        .fringe_bitmaps
        .define(intern("p39-right"), None, bitmap(129));

    let row = |row: i64, start: usize, end: usize, fringe: crate::window::RowFringeBitmaps| {
        crate::window::DisplayRowSnapshot {
            row,
            y: row * 10,
            height: 10,
            start_x: 0,
            start_col: 0,
            end_x: 20,
            end_col: 5,
            start_buffer_pos: Some(LispCharPos1::new(start as i64)),
            end_buffer_pos: Some(LispCharPos1::new(end as i64)),
            fringe,
        }
    };
    let frame = eval.frames.get_mut(frame_id).expect("frame");
    frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
        window_id,
        rows: vec![
            row(0, 1, 6, crate::window::RowFringeBitmaps::default()),
            row(
                1,
                7,
                11,
                crate::window::RowFringeBitmaps {
                    left: Some(crate::window::FringeBitmapIndex(left as u16)),
                    right: None,
                    overlay_arrow: crate::window::RowOverlayArrowBitmap::Absent,
                },
            ),
            row(
                2,
                12,
                17,
                crate::window::RowFringeBitmaps {
                    left: None,
                    right: Some(crate::window::FringeBitmapIndex(right as u16)),
                    overlay_arrow: crate::window::RowOverlayArrowBitmap::Bitmap(
                        crate::window::FringeBitmapIndex(left as u16),
                    ),
                },
            ),
        ],
        ..Default::default()
    }]);
    (eval, frame_id)
}

fn fringe_bitmaps_at_pos_call(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    builtin_fringe_bitmaps_at_pos(eval, args)
}

/// The `(LEFT RIGHT OVERLAY)` triple as three values, asserting the shape GNU
/// documents (exactly three elements) along the way.
fn fringe_triple(value: Value) -> [Value; 3] {
    let mut rest = value;
    let mut out = [Value::NIL; 3];
    for slot in &mut out {
        assert!(
            !rest.is_nil(),
            "fringe-bitmaps-at-pos must return 3 elements"
        );
        *slot = rest.cons_car();
        rest = rest.cons_cdr();
    }
    assert!(rest.is_nil(), "fringe-bitmaps-at-pos must return exactly 3");
    out
}

#[test]
fn test_fringe_bitmaps_at_pos_reports_gnu_left_right_overlay_triple() {
    crate::test_utils::init_test_tracing();
    let (mut eval, _frame_id) = fringe_bitmaps_at_pos_fixture();

    // A laid-out row with no bitmaps is `(nil nil nil)`, never nil: GNU only
    // answers nil when no row contains POS.
    let bare = fringe_bitmaps_at_pos_call(&mut eval, vec![Value::fixnum(1)])
        .expect("bare row answers a triple");
    assert_eq!(fringe_triple(bare), [Value::NIL, Value::NIL, Value::NIL]);

    let left_sym = Value::from_sym_id(intern("p39-left"));
    let right_sym = Value::from_sym_id(intern("p39-right"));
    let left = fringe_bitmaps_at_pos_call(&mut eval, vec![Value::fixnum(7)])
        .expect("left-fringe row answers a triple");
    assert_eq!(fringe_triple(left), [left_sym, Value::NIL, Value::NIL]);

    // The overlay arrow occupies its own slot, so it is reported beside the
    // right-fringe bitmap rather than displacing it.
    let right_and_arrow = fringe_bitmaps_at_pos_call(&mut eval, vec![Value::fixnum(12)])
        .expect("right-fringe row answers a triple");
    assert_eq!(
        fringe_triple(right_and_arrow),
        [Value::NIL, right_sym, left_sym]
    );
}

#[test]
fn test_fringe_bitmaps_at_pos_defaults_pos_to_point_in_selected_window() {
    crate::test_utils::init_test_tracing();
    let (mut eval, _frame_id) = fringe_bitmaps_at_pos_fixture();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let buffer = eval.buffers.get_mut(buf_id).expect("current buffer");
    // The fixture text is ASCII, so char and byte offsets coincide; Lisp
    // position 7 is the start of the second row.
    buffer.set_point_anchor(crate::buffer::TextPositionAnchor::new(
        CharPos0::new(6),
        crate::buffer::EmacsBytePos::new(6),
    ));

    let defaulted =
        fringe_bitmaps_at_pos_call(&mut eval, Vec::new()).expect("nil POS defaults to point");
    assert_eq!(
        fringe_triple(defaulted),
        [
            Value::from_sym_id(intern("p39-left")),
            Value::NIL,
            Value::NIL
        ]
    );
}

#[test]
fn test_fringe_bitmaps_at_pos_returns_nil_for_a_position_no_row_displays() {
    crate::test_utils::init_test_tracing();
    let (mut eval, _frame_id) = fringe_bitmaps_at_pos_fixture();
    // Position 18 is inside the buffer but past the last row of the snapshot,
    // which is GNU's `row_containing_pos` miss.
    let missing = fringe_bitmaps_at_pos_call(&mut eval, vec![Value::fixnum(18)])
        .expect("an undisplayed position answers nil, not an error");
    assert!(missing.is_nil(), "expected nil for an undisplayed position");
}

#[test]
fn test_fringe_bitmaps_at_pos_signals_args_out_of_range_beyond_accessible_portion() {
    crate::test_utils::init_test_tracing();
    let (mut eval, _frame_id) = fringe_bitmaps_at_pos_fixture();
    let error = fringe_bitmaps_at_pos_call(&mut eval, vec![Value::fixnum(9999)])
        .expect_err("POS past ZV signals like GNU's args_out_of_range");
    let rendered = format!("{error:?}");
    assert!(
        rendered.contains("args-out-of-range") || rendered.contains("ArgsOutOfRange"),
        "expected args-out-of-range, got {rendered}"
    );
}

/// `window-line-height` answers from a current matrix or not at all.
///
/// GNU `Fwindow_line_height` (src/window.c:2048) has exactly one source, the
/// window's current glyph matrix, and it refuses before reading anything else:
///
/// ```c
///   /* Fail if current matrix is not up-to-date.  */
///   if (!w->window_end_valid
///       || windows_or_buffers_changed
///       || b->clip_changed
///       || b->prevent_redisplay_optimizations_p
///       || window_outdated (w))
///     return Qnil;
/// ```
/// (src/window.c:2082-2089), and its docstring says what nil means: "Return nil
/// if window display is not up-to-date.  In that case, use
/// `pos-visible-in-window-p' to obtain the information."
///
/// This port had a second source: a geometry approximation
/// (`resolve_live_window_display_context`) that invented a row height, vpos and
/// ypos for a window with no redisplay snapshot. Measured against GNU Emacs
/// 31.0.90 under a pty at 80x24 (`scripts/l217-window-line-height-probe.el'),
/// that approximation answered `(1 0 0 0)` for the mini-window's LINE nil and
/// LINE 0 in five states where GNU returns nil -- ten probe lines, all of them
/// a number offered in place of GNU's "the display is not up to date".
///
/// `pos-visible-in-window-p` keeps the approximation, because GNU's own
/// `pos_visible_p` does not consult the matrix: it runs `move_it_to`. That
/// asymmetry is GNU's, and it is the one the docstring above points at.
#[test]
fn window_line_height_refuses_every_line_form_without_a_current_matrix_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("l217-no-current-matrix", 160, 64, buf_id);
    let selected_window = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("abc\ndef\nghi\n");
    }

    for line in [
        Value::NIL,
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(-1),
        Value::symbol("tab-line"),
        Value::symbol("header-line"),
        Value::symbol("mode-line"),
    ] {
        let got = builtin_window_line_height(
            &mut eval,
            vec![line, Value::make_window(selected_window.0)],
        )
        .unwrap();
        assert_eq!(
            super::super::print::print_value(&got),
            "nil",
            "window-line-height must refuse LINE {} when the window has no \
             current matrix (GNU src/window.c:2082-2089)",
            super::super::print::print_value(&line)
        );
    }
}

/// GNU runs the `recenter:` placement inside `redisplay_window`, after
/// `set_buffer_internal_1 (XBUFFER (w->contents))` (src/xdisp.c:20532-20535,
/// emacs-31.0.90), so the display iterator and every text-property probe it
/// makes read the window's buffer. This port's redisplay runs with whatever
/// buffer Lisp left current -- the active minibuffer while a completion UI
/// is up -- so the placement scan must select the window's buffer itself
/// and hand the caller's buffer back afterwards.
#[test]
fn redisplay_start_before_point_scans_the_window_buffer_not_the_current_one() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let window_buffer = eval.buffers.current_buffer_id().expect("current buffer");
    let text: String = (0..600).map(|index| format!("line {index:03}\n")).collect();
    eval.buffers
        .get_mut(window_buffer)
        .expect("window buffer")
        .insert(&text);
    let frame_id = eval
        .frames
        .create_frame("recenter-window-buffer", 80, 24, window_buffer);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;

    // A four-character buffer is current, as " *Minibuf-1*" is while `M-x`
    // completes; every position the scan probes lies beyond its end.
    let tiny = eval.buffers.create_buffer(" *tiny*");
    eval.switch_current_buffer(tiny)
        .expect("select the tiny buffer");
    eval.buffers
        .get_mut(tiny)
        .expect("tiny buffer")
        .insert("M-x ");

    let line = |index: usize| {
        text.find(&format!("line {index:03}\n"))
            .expect("line present")
    };
    let point = CharPos0::new(line(500) + 3);

    assert_eq!(
        eval.redisplay_start_before_point_by_display_rows(window_buffer, window_id, point, 0),
        Some(CharPos0::new(line(500))),
        "zero rows above point is the start of point's own screen line"
    );
    assert_eq!(
        eval.redisplay_start_before_point_by_display_rows(window_buffer, window_id, point, 2),
        Some(CharPos0::new(line(498))),
        "two rows above point on unwrapped lines is two source lines up"
    );
    assert_eq!(
        eval.buffers.current_buffer_id(),
        Some(tiny),
        "the placement scan restores the buffer redisplay was entered with"
    );
}
