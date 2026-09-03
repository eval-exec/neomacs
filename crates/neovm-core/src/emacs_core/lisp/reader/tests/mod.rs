use super::*;
use crate::buffer::{CharPos0, LispCharPos1};
use crate::emacs_core::eval::{Context, DisplayHost, GuiFrameHostRequest, PopupMenuRequest};
use crate::emacs_core::print_value;
use crate::emacs_core::value::{
    ValueKind, VecLikeType, eq_value, get_string_text_properties_table_for_value,
};
use crate::test_utils::{eval_with_ldefs_boot_autoloads, runtime_startup_eval_all};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
struct RecordingPopupHost {
    shown: Arc<Mutex<Vec<PopupMenuRequest>>>,
    hidden: Arc<Mutex<usize>>,
}

impl DisplayHost for RecordingPopupHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn show_popup_menu(&mut self, menu: PopupMenuRequest) -> Result<(), String> {
        self.shown.lock().unwrap().push(menu);
        Ok(())
    }

    fn hide_popup_menu(&mut self) -> Result<(), String> {
        *self.hidden.lock().unwrap() += 1;
        Ok(())
    }
}

fn install_mouse_help_echo_snapshot_with_value(eval: &mut Context, help: Value) -> Value {
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    {
        let buf = eval.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("abc");
    }
    crate::emacs_core::textprop::builtin_put_text_property(
        eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("help-echo"),
            help,
        ],
    )
    .expect("put help-echo property");

    let frame_id = eval
        .frames
        .selected_frame()
        .map(|frame| frame.id)
        .unwrap_or_else(|| {
            eval.frames
                .create_frame("reader-help-echo", 160, 64, buf_id)
        });
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    let frame = eval.frames.get_mut(frame_id).expect("frame");
    frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
        window_id,
        text_area_left_offset: 8,
        points: vec![crate::window::DisplayPointSnapshot {
            role: crate::window::DisplayPointRole::Glyph,
            buffer_pos: crate::buffer::LispCharPos1::new(1),
            x: 0,
            y: 0,
            width: 8,
            height: 16,
            row: 0,
            col: 0,
        }],
        rows: vec![crate::window::DisplayRowSnapshot {
            row: 0,
            y: 0,
            height: 16,
            start_x: 0,
            start_col: 0,
            end_x: 0,
            end_col: 0,
            start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
            end_buffer_pos: Some(crate::buffer::LispCharPos1::new(3)),
            fringe: Default::default(),
        }],
        ..crate::window::WindowDisplaySnapshot::default()
    }]);
    Value::make_frame(frame_id.0)
}

fn install_mouse_help_echo_snapshot(eval: &mut Context, help: &str) -> Value {
    install_mouse_help_echo_snapshot_with_value(eval, Value::string(help))
}

fn bootstrap_eval_all(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

// ===================================================================
// read-from-string tests
// ===================================================================

#[test]
fn read_from_string_integer() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("42")]).unwrap();
    // Should be (42 . 2)
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            assert!(pair_car.is_fixnum());
            assert!(pair_cdr.is_fixnum());
        }
        _ => panic!("Expected cons, got {:?}", result),
    }
}

#[test]
fn read_from_string_symbol() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("hello")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            assert_eq!(pair_car.as_symbol_name(), Some("hello"));
            assert!(pair_cdr.is_fixnum());
        }
        _ => panic!("Expected cons, got {:?}", result),
    }
}

#[test]
fn read_accepts_a_nul_padded_decimal_wire_field_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let mut bytes = b"1.987500000000000e+01".to_vec();
    bytes.resize(31, 0);
    let field = Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes));

    let value = builtin_read(&mut ev, vec![field]).expect("read fixed-width decimal field");
    assert!(value.is_float());
    assert_eq!(value.as_number_f64(), Some(19.875));

    let pair = builtin_read_from_string(&mut ev, vec![field])
        .expect("read-from-string fixed-width decimal field");
    assert_eq!(pair.cons_car().as_number_f64(), Some(19.875));
    assert_eq!(pair.cons_cdr().as_fixnum(), Some(21));
}

/// `(("ical:" . "icalendar-"))`
fn ical_shorthands_alist() -> Value {
    Value::cons(
        Value::cons(Value::string("ical:"), Value::string("icalendar-")),
        Value::NIL,
    )
}

#[test]
fn read_from_string_applies_read_symbol_shorthands() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.obarray
        .set_symbol_value("read-symbol-shorthands", ical_shorthands_alist());
    let result =
        builtin_read_from_string(&mut ev, vec![Value::string("ical:error-regexp")]).unwrap();
    assert_eq!(
        result.cons_car().as_symbol_name(),
        Some("icalendar-error-regexp"),
        "read-from-string must rewrite ical: prefix via read-symbol-shorthands"
    );
}

#[test]
fn read_from_buffer_applies_read_symbol_shorthands() {
    // This is the path used by `byte-compile-file`, which reads forms from a
    // buffer whose `read-symbol-shorthands` was set buffer-local by
    // `hack-local-variables`.  Regression test for icalendar (GNU 31.0.90)
    // failing to byte-compile because `ical:` prefixes were not rewritten.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.obarray
        .set_symbol_value("read-symbol-shorthands", ical_shorthands_alist());
    let buf_id = ev.buffers.create_buffer("shorthand-read");
    {
        let buf = ev.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("ical:foo");
    }
    ev.buffers
        .goto_buffer_emacs_byte_pos(buf_id, crate::buffer::EmacsBytePos::new(0));
    let result = builtin_read(&mut ev, vec![Value::make_buffer(buf_id)]).unwrap();
    assert_eq!(
        result.as_symbol_name(),
        Some("icalendar-foo"),
        "read from buffer must rewrite ical: prefix via read-symbol-shorthands"
    );
}

#[test]
fn read_positioning_symbols_reports_character_offsets_for_multibyte_strings() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();

    let value = builtin_read_impl(&mut ev, vec![Value::string("(\"β\" foo)")], true)
        .expect("read positioning symbols from string");
    let items = list_to_vec(&value).expect("reader result is a list");

    assert_eq!(items[1].as_symbol_with_pos_pos(), Some(5));
}

#[test]
fn read_positioning_symbols_reports_absolute_lisp_positions_for_buffers() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let buf_id = ev.buffers.create_buffer("positioning-buffer-read");
    {
        let buf = ev.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("λβ foo");
        let after_prefix = buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(2));
        buf.goto_emacs_byte_pos(after_prefix);
    }

    let value = builtin_read_impl(&mut ev, vec![Value::make_buffer(buf_id)], true)
        .expect("read positioning symbol from buffer");

    assert_eq!(value.as_symbol_with_pos_pos(), Some(4));
}

#[test]
fn read_positioning_symbols_reports_relative_character_offsets_for_markers() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let buf_id = ev.buffers.create_buffer("positioning-marker-read");
    ev.buffers.get_mut(buf_id).expect("buffer").insert("β foo");
    let marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut ev.buffers,
        buf_id,
        LispCharPos1::new(2),
        false,
    );

    let value = builtin_read_impl(&mut ev, vec![marker], true)
        .expect("read positioning symbol from marker");

    assert_eq!(value.as_symbol_with_pos_pos(), Some(1));
}

#[test]
fn read_from_marker_advances_marker_without_moving_buffer_point_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let buf_id = ev.buffers.create_buffer("marker-read");
    {
        let buf = ev.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("(alpha β) tail");
    }
    ev.buffers
        .goto_buffer_emacs_byte_pos(buf_id, crate::buffer::EmacsBytePos::new(3));
    let marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut ev.buffers,
        buf_id,
        LispCharPos1::new(1),
        false,
    );

    let first = builtin_read(&mut ev, vec![marker]).expect("read list from marker stream");
    assert_eq!(print_value(&first), "(alpha β)");
    assert_eq!(
        crate::emacs_core::marker::marker_position_as_int_with_buffers(&ev.buffers, &marker)
            .expect("marker remains attached after first read"),
        10
    );
    assert_eq!(
        ev.buffers
            .get(buf_id)
            .expect("buffer")
            .point_lisp_char_pos(),
        LispCharPos1::new(4)
    );

    let second = builtin_read(&mut ev, vec![marker]).expect("read symbol from marker stream");
    assert_eq!(second.as_symbol_name(), Some("tail"));
    assert_eq!(
        crate::emacs_core::marker::marker_position_as_int_with_buffers(&ev.buffers, &marker)
            .expect("marker remains attached after second read"),
        15
    );
    assert_eq!(
        ev.buffers
            .get(buf_id)
            .expect("buffer")
            .point_lisp_char_pos(),
        LispCharPos1::new(4)
    );

    for (name, text, expected_signal, expected_data, expected_position) in [
        ("marker-read-whitespace-eof", "  ", "end-of-file", None, 3),
        ("marker-read-mid-form-eof", "(alpha", "end-of-file", None, 7),
        (
            "marker-read-invalid-syntax",
            ")",
            "invalid-read-syntax",
            Some(")"),
            2,
        ),
    ] {
        let error_buf_id = ev.buffers.create_buffer(name);
        ev.buffers
            .get_mut(error_buf_id)
            .expect("error buffer")
            .insert(text);
        let error_marker = crate::emacs_core::marker::make_registered_buffer_marker(
            &mut ev.buffers,
            error_buf_id,
            LispCharPos1::new(1),
            false,
        );
        let result = builtin_read(&mut ev, vec![error_marker]);
        let signal = match &result {
            Err(Flow::Signal(signal)) if signal.symbol_name() == expected_signal => signal,
            _ => panic!(
                "marker stream over {text:?} should signal {expected_signal}, got {result:?}"
            ),
        };
        match expected_data {
            Some(data) => {
                assert_eq!(signal.data.len(), 1);
                assert_eq!(signal.data[0].as_utf8_str(), Some(data));
            }
            None => assert!(signal.data.is_empty()),
        }
        assert_eq!(
            crate::emacs_core::marker::marker_position_as_int_with_buffers(
                &ev.buffers,
                &error_marker,
            )
            .expect("marker remains attached after reader error"),
            expected_position
        );
    }
}

#[test]
fn read_from_string_string_value() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result =
        builtin_read_from_string(&mut ev, vec![Value::string(r#""hello world""#)]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            assert_eq!(pair_car.as_utf8_str(), Some("hello world"));
            assert!(pair_cdr.is_fixnum());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_unterminated_string_signals_end_of_file_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let unterminated = builtin_read_from_string(&mut ev, vec![Value::string(r#""unterminated"#)]);
    assert!(
        matches!(unterminated, Err(Flow::Signal(ref sig)) if sig.symbol_name() == "end-of-file"),
        "GNU read-from-string signals end-of-file for an unterminated string, got {unterminated:?}"
    );

    let escape_at_eof = builtin_read_from_string(&mut ev, vec![Value::string(r#""abc\"#)]);
    assert!(
        matches!(escape_at_eof, Err(Flow::Signal(ref sig)) if sig.symbol_name() == "end-of-file"),
        "GNU read-from-string signals end-of-file for an unterminated string escape, got {escape_at_eof:?}"
    );
}

#[test]
fn read_from_string_ascii_string_literals_are_unibyte() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string(r#""hello""#)]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let text_value = result.cons_car();
            let text = text_value
                .as_lisp_string()
                .expect("reader should return a string object");
            assert!(!text.is_multibyte());
            assert_eq!(text.as_bytes(), b"hello");
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_preserves_propertized_string_literal_intervals() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(
        &mut ev,
        vec![Value::string(
            r#"#(" " 0 1 (marginalia--align t display (space :align-to (+ left 20))))"#,
        )],
    )
    .expect("read propertized string literal");
    let string = result.cons_car();
    let props = get_string_text_properties_table_for_value(string)
        .expect("reader should apply #(\"...\" START END PLIST) intervals");

    assert_eq!(
        props.get_property_at_char_pos(CharPos0::ZERO, Value::symbol("marginalia--align")),
        Some(Value::symbol("t"))
    );
    let display = props
        .get_property_at_char_pos(CharPos0::ZERO, Value::symbol("display"))
        .expect("display property should survive reader literal");
    assert!(display.is_cons());
    assert!(display.cons_car().is_symbol_named("space"));
}

#[test]
fn read_from_string_modifier_string_escapes_follow_gnu_rules() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let assert_string = |value: Value, expected: &[u8]| {
        let text = value
            .as_lisp_string()
            .expect("reader should return a string object");
        assert!(!text.is_multibyte());
        assert_eq!(text.as_bytes(), expected);
    };

    let meta = builtin_read_from_string(&mut ev, vec![Value::string(r#""\M-x""#)]).unwrap();
    assert_string(meta.cons_car(), &[0xF8]);

    let ctrl = builtin_read_from_string(&mut ev, vec![Value::string(r#""\C-x""#)]).unwrap();
    assert_string(ctrl.cons_car(), &[0x18]);

    let old_ctrl = builtin_read_from_string(&mut ev, vec![Value::string(r#""\^l""#)]).unwrap();
    assert_string(old_ctrl.cons_car(), &[0x0C]);

    let shift = builtin_read_from_string(&mut ev, vec![Value::string(r#""\S-a""#)]).unwrap();
    assert_string(shift.cons_car(), b"A");
}

#[test]
fn read_from_string_preserves_unibyte_string_literals() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let input = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'"', 0xFF, b'"',
    ]));
    let result = builtin_read_from_string(&mut ev, vec![input]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            let text = pair_car
                .as_lisp_string()
                .expect("reader should return a string object");
            assert!(!text.is_multibyte());
            assert_eq!(text.as_bytes(), &[0xFF]);
            assert_eq!(pair_cdr.as_fixnum(), Some(3));
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_preserves_valid_utf8_runs_as_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let input = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'"', 0xCE, 0xBB, b'"',
    ]));

    let result = builtin_read_from_string(&mut ev, vec![input]).unwrap();
    let text_value = result.cons_car();
    let text = text_value
        .as_lisp_string()
        .expect("reader should return a string object");

    assert!(
        !text.is_multibyte(),
        "GNU string sources preserve high bytes even when they form valid UTF-8"
    );
    assert_eq!(text.as_bytes(), &[0xCE, 0xBB]);
    assert_eq!(result.cons_cdr().as_fixnum(), Some(4));
}

#[test]
fn read_from_string_preserves_unibyte_char_literals() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let input = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'?', 0xFF,
    ]));
    let result = builtin_read_from_string(&mut ev, vec![input]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            assert_eq!(pair_car.as_fixnum(), Some(255));
            assert_eq!(pair_cdr.as_fixnum(), Some(2));
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_list() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("(+ 1 2)")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            // car should be a list (+ 1 2)
            assert!(pair_car.is_cons());
            assert!(pair_cdr.is_fixnum());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_with_start() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // "  42 rest" — start at 2
    let result =
        builtin_read_from_string(&mut ev, vec![Value::string("  42 rest"), Value::fixnum(2)])
            .unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            assert!(pair_car.is_fixnum());
            assert!(pair_cdr.is_fixnum());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_float() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("3.125")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            assert!(
                pair_car.as_float().is_some()
                    && (pair_car.as_float().unwrap() - 3.125).abs() < 1e-10
            );
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_char() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("?a")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            assert!(&pair_car.is_char());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("nil")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            assert!(pair_car.is_nil());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_t() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("t")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            assert!(pair_car.is_t());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_vector() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("[1 2 3]")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            assert!(pair_car.is_vector());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_unterminated_vector_signals_end_of_file_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("[1 2")]);
    assert!(
        matches!(result, Err(Flow::Signal(ref sig)) if sig.symbol_name() == "end-of-file"),
        "GNU read-from-string signals end-of-file for an unterminated vector, got {result:?}"
    );
}

#[test]
fn read_from_string_quoted() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("'foo")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            // Should be (quote foo) as a list
            assert!(pair_car.is_cons());
            assert!(pair_cdr.is_fixnum());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_dotted_pair() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("(a . b)")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            // car should be a dotted pair (a . b)
            assert!(pair_car.is_cons());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_keyword() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string(":test")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            assert_eq!(pair_car.as_symbol_name(), Some(":test"));
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_uninterned_symbol() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("#:test")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            match pair_car.kind() {
                ValueKind::Symbol(id) => {
                    assert_eq!(resolve_sym(id), "test");
                    assert_ne!(id, crate::emacs_core::intern::intern("test"));
                }
                other => panic!("expected uninterned symbol, got {other:?}"),
            }
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_empty_error() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("")]);
    assert!(result.is_err());
}

#[test]
fn read_from_string_whitespace_only_error() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("   ")]);
    assert!(result.is_err());
}

#[test]
fn read_from_string_multiple_forms_reads_first() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("42 99")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            assert!(pair_car.is_fixnum());
            // End position should be after "42" (position 2), not after "99"
            match pair_cdr.kind() {
                ValueKind::Fixnum(n) => assert!(n <= 3, "end pos {} should be <= 3", n),
                _ => panic!("Expected int end position"),
            }
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_with_start_and_end() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // "xxx42yyy" with start=3, end=5 -> substring "42"
    let result = builtin_read_from_string(
        &mut ev,
        vec![
            Value::string("xxx42yyy"),
            Value::fixnum(3),
            Value::fixnum(5),
        ],
    )
    .unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            assert!(pair_car.is_fixnum());
            assert!(pair_cdr.is_fixnum());
        }
        _ => panic!("Expected cons"),
    }
}

/// Regression for audit §11.6: START/END must be character indices, and
/// the returned FINAL-STRING-INDEX must be a character index too
/// (matching GNU `Fread_from_string` in `src/lread.c:2514`). Multibyte
/// chars in STRING were previously sliced as raw UTF-8 bytes, which
/// either panicked mid-codepoint or produced byte counts where elisp
/// callers expect char counts.
#[test]
fn read_from_string_multibyte_indices_are_character_based() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // "αβγ 42 δ" — eight logical characters, fourteen UTF-8 bytes.
    // Read from char index 4 (just before "42").
    let result =
        builtin_read_from_string(&mut ev, vec![Value::string("αβγ 42 δ"), Value::fixnum(4)])
            .unwrap();
    let pair_car = result.cons_car();
    let pair_cdr = result.cons_cdr();
    assert_eq!(pair_car.as_fixnum(), Some(42));
    // FINAL-STRING-INDEX should be a character index — the position
    // after "42", which is char 6 (the trailing space). If START/END
    // were treated as byte offsets, we'd see a value > 6.
    let cdr = pair_cdr.as_fixnum().expect("cdr is fixnum");
    assert!(
        (6..=7).contains(&cdr),
        "expected cdr in 6..=7 char range, got {cdr}"
    );
}

/// Negative START/END must count from the end of STRING in *characters*,
/// not bytes (audit §11.6, mirroring GNU `validate_subarray`).
#[test]
fn read_from_string_negative_indices_are_character_based() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    // "αβ 42" — five characters, seven UTF-8 bytes. Negative -2 means
    // the fourth character (the '4').
    let result =
        builtin_read_from_string(&mut ev, vec![Value::string("αβ 42"), Value::fixnum(-2)]).unwrap();
    let pair_car = result.cons_car();
    let pair_cdr = result.cons_cdr();
    assert_eq!(pair_car.as_fixnum(), Some(42));
    assert_eq!(pair_cdr.as_fixnum(), Some(5));
}

/// Out-of-range START/END is detected against the *character* count of
/// STRING, not its byte length. For "α" (1 char, 2 bytes) char index 2
/// must be rejected even though it would be a valid byte offset.
#[test]
fn read_from_string_out_of_range_uses_character_count() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("α"), Value::fixnum(2)]);
    assert!(
        result.is_err(),
        "char index 2 must be out of range for a 1-char string"
    );
}

#[test]
fn read_from_string_unibyte_indices_are_character_based() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let input = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b' ', b'4', b'2',
    ]));
    let result = builtin_read_from_string(&mut ev, vec![input, Value::fixnum(2)]).unwrap();
    let pair_car = result.cons_car();
    let pair_cdr = result.cons_cdr();
    assert_eq!(pair_car.as_fixnum(), Some(42));
    assert_eq!(pair_cdr.as_fixnum(), Some(4));
}

#[test]
fn read_from_string_unibyte_out_of_range_uses_character_count() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let input = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b' ', b'4', b'2',
    ]));
    let result = builtin_read_from_string(&mut ev, vec![input, Value::fixnum(5)]);
    assert!(
        result.is_err(),
        "char index 5 must be out of range for a 4-char unibyte string"
    );
}

// ===================================================================
// read tests
// ===================================================================

#[test]
fn read_from_string_stream() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read(&mut ev, vec![Value::string("42")]).unwrap();
    assert!(result.is_fixnum());
}

#[test]
fn read_from_string_interns_symbols_in_global_obarray() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result =
        builtin_read_from_string(&mut ev, vec![Value::string("reader-obarray-side-effect")])
            .unwrap();
    assert_eq!(
        result.cons_car().as_symbol_name(),
        Some("reader-obarray-side-effect")
    );
    assert!(
        ev.obarray()
            .intern_soft("reader-obarray-side-effect")
            .is_some()
    );
}

#[test]
fn read_nil_stream_uses_string_valued_standard_input() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("standard-input", Value::string("(LBRACE RBRACE)"));
    let result = builtin_read(&mut ev, vec![Value::NIL]).unwrap();
    assert_eq!(print_value(&result), "(LBRACE RBRACE)");
}

#[test]
fn read_no_args_uses_string_valued_standard_input() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("standard-input", Value::string("(LBRACE RBRACE)"));
    let result = builtin_read(&mut ev, vec![]).unwrap();
    assert_eq!(print_value(&result), "(LBRACE RBRACE)");
}

#[test]
fn read_interns_symbols_in_global_obarray() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result =
        builtin_read(&mut ev, vec![Value::string("reader-stream-obarray-symbol")]).unwrap();
    assert_eq!(
        result.as_symbol_name(),
        Some("reader-stream-obarray-symbol")
    );
    assert!(
        ev.obarray()
            .intern_soft("reader-stream-obarray-symbol")
            .is_some()
    );
}

#[test]
fn read_nil_stream() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read(&mut ev, vec![Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn read_no_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read(&mut ev, vec![]);
    assert!(result.is_err());
}

#[test]
fn read_rejects_extra_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read(&mut ev, vec![Value::string("a"), Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn read_non_stream_type_is_invalid_function() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read(&mut ev, vec![Value::fixnum(1)]);
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "invalid-function"),
        other => panic!("expected invalid-function signal, got {other:?}"),
    }
}

// ===================================================================
// Stub function tests
// ===================================================================

#[test]
fn read_from_minibuffer_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_minibuffer(&mut ev, vec![Value::string("Prompt: ")]);
    assert!(result.is_err());
}

#[test]
fn read_from_minibuffer_non_character_event_stays_queued_and_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::symbol("foo")]),
    );
    let result = builtin_read_from_minibuffer(&mut ev, vec![Value::string("Prompt: ")]);
    assert!(matches!(result, Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file"));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::symbol("foo")]))
    );
}

#[test]
fn read_from_minibuffer_ignores_initial_and_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_minibuffer(
        &mut ev,
        vec![Value::string("Prompt: "), Value::string("initial")],
    );
    assert!(result.is_err());
}

#[test]
fn read_from_minibuffer_rejects_non_stringish_initial_input() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result =
        builtin_read_from_minibuffer(&mut ev, vec![Value::string("Prompt: "), Value::fixnum(1)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn read_from_minibuffer_rejects_cons_initial_with_non_string_car() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let cons_initial = Value::cons(Value::fixnum(1), Value::fixnum(1));
    let result =
        builtin_read_from_minibuffer(&mut ev, vec![Value::string("Prompt: "), cons_initial]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn read_from_minibuffer_rejects_more_than_seven_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_minibuffer(
        &mut ev,
        vec![
            Value::string("Prompt: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn read_from_minibuffer_runs_setup_edit_and_exit_in_gnu_order() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer command");
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r##"(progn
                   (setq test-minibuffer-order nil)
                   (unwind-protect
                       (let ((map (make-sparse-keymap))
                             (minibuffer-setup-hook
                              (list (lambda ()
                                      (push 'setup test-minibuffer-order))))
                             (minibuffer-exit-hook
                              (list (lambda ()
                                      (push 'exit test-minibuffer-order)))))
                         (define-key map " "
                           (lambda ()
                             (interactive)
                             (push 'edit test-minibuffer-order)
                             (exit-minibuffer)))
                         (read-from-minibuffer "Prompt: " nil map)
                         (nreverse test-minibuffer-order))
                     (makunbound 'test-minibuffer-order)))"##,
        )
        .expect("read-from-minibuffer should exit normally");

    assert_eq!(format!("{result}"), "(setup edit exit)");
}

#[test]
fn read_from_minibuffer_initializes_an_unbound_history_before_setup_hooks() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer exit key");
    eval.input_rx = Some(rx);

    let result = eval
        .eval_str(
            r##"(progn
                   (setq neo-unbound-history-seen nil)
                   (makunbound 'neo-unbound-minibuffer-history)
                   (let ((map (make-sparse-keymap))
                         (minibuffer-setup-hook
                          (list
                           (lambda ()
                             (setq neo-unbound-history-seen
                                   (list
                                    (boundp 'neo-unbound-minibuffer-history)
                                    neo-unbound-minibuffer-history
                                    minibuffer-history-variable)))))
                         (minibuffer-exit-hook nil))
                     (define-key map " " #'exit-minibuffer)
                     (read-from-minibuffer
                      "Prompt: " "entry" map nil
                      'neo-unbound-minibuffer-history))
                   (list neo-unbound-history-seen
                         neo-unbound-minibuffer-history))"##,
        )
        .expect("GNU initializes an unbound history variable before setup hooks");

    assert_eq!(
        format!("{result}"),
        "((t nil neo-unbound-minibuffer-history) (\"entry\"))"
    );
}

#[test]
fn active_minibuffer_is_recorded_at_front_of_its_frame_buffer_list() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer exit key");
    eval.input_rx = Some(rx);

    let result = eval
        .eval_str(
            r##"(let ((seen nil)
                      (map (make-sparse-keymap))
                      (minibuffer-exit-hook nil))
                  (define-key map " " #'exit-minibuffer)
                  (let ((minibuffer-setup-hook
                         (list
                          (lambda ()
                            (setq seen
                                  (car (mapcar #'buffer-name
                                               (frame-parameter nil 'buffer-list))))))))
                    (read-from-minibuffer "Prompt: " nil map))
                  seen)"##,
        )
        .expect("minibuffer setup should observe frame buffer order");

    assert_eq!(format!("{result}"), r#"" *Minibuf-1*""#);
}

#[test]
fn exiting_minibuffer_records_the_restored_calling_buffer_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer exit key");
    eval.input_rx = Some(rx);

    let result = eval
        .eval_str(
            r##"(let ((map (make-sparse-keymap))
                       (minibuffer-exit-hook nil))
                   (define-key map " " #'exit-minibuffer)
                   (read-from-minibuffer "Prompt: " nil map)
                   (equal (mapcar #'buffer-name
                                  (seq-take (frame-parameter nil 'buffer-list) 2))
                          '("*scratch*" " *Minibuf-1*")))"##,
        )
        .expect("minibuffer exit should restore the caller's selection record");

    assert_eq!(result, Value::T);
}

/// GNU `read_minibuf` binds `minibuffer-default` to the DEFAULT argument for
/// the whole read (`src/minibuf.c:591`, `specbind (Qminibuffer_default,
/// defalt)`), and restores the outer value on exit.  Everything that offers the
/// default to the user reads that variable rather than the argument:
/// `next-history-element`/`M-n`, `minibuffer-default-add-function`, and
/// packages that observe the live minibuffer from `minibuffer-setup-hook`.
///
/// Neomacs threaded DEFAULT through the read but never bound the variable, so
/// the variable stayed nil throughout.
///
/// GNU oracle (emacs -Q --batch), recording `minibuffer-default` from
/// `minibuffer-setup-hook`:
///   (read-from-minibuffer "P: " nil nil nil nil "zed") => seen ((:default "zed"))
///   (completing-read "Imenu: " '("alpha" "beta") nil t nil nil "beta")
///                                                     => seen ((:default "beta"))
#[test]
fn read_from_minibuffer_binds_minibuffer_default_for_the_whole_read_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    for _ in 0..2 {
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char(' '),
        ))
        .expect("queue minibuffer exit key");
    }
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r##"(let ((map (make-sparse-keymap))
                     (seen nil))
                 (define-key map " " #'exit-minibuffer)
                 (let ((minibuffer-setup-hook
                        (list (lambda () (push minibuffer-default seen))))
                       (minibuffer-exit-hook nil))
                   (read-from-minibuffer "P: " nil map nil nil "zed")
                   (read-from-minibuffer "Q: " nil map nil nil '("one" "two")))
                 (list (nreverse seen) minibuffer-default))"##,
        )
        .expect("read-from-minibuffer should exit normally");

    assert_eq!(
        format!("{result}"),
        r#"(("zed" ("one" "two")) nil)"#,
        "DEFAULT must be visible as minibuffer-default during the read and \
         restored to the outer value afterwards"
    );
}

#[test]
fn read_from_minibuffer_runs_minibuffer_modes_to_clear_stale_locals() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue space");
    drop(tx);
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r#"(let ((minibuffer-setup-hook nil)
                  (minibuffer-exit-hook nil)
                  (mode-seen nil)
                  (setup-seen nil)
                  (inactive-seen nil)
                  (map (make-sparse-keymap)))
              (setq minibuffer-setup-hook
                    (list (lambda ()
                            (setq setup-seen (local-variable-p 'minibuffer-completion-table)))))
              (define-key map " " #'exit-minibuffer)
              (with-current-buffer (get-buffer-create " *Minibuf-1*")
                (setq-local minibuffer-completion-table '("BUGS"))
                (setq-local stale-local t))
              (let ((orig-minibuffer-mode (symbol-function 'minibuffer-mode))
                    (orig-inactive-mode (symbol-function 'minibuffer-inactive-mode)))
                (unwind-protect
                    (progn
                      (fset 'minibuffer-mode
                            (lambda ()
                              (funcall orig-minibuffer-mode)
                              (setq mode-seen (list (local-variable-p 'minibuffer-completion-table)
                                                    (local-variable-p 'stale-local)))))
                      (fset 'minibuffer-inactive-mode
                            (lambda ()
                              (funcall orig-inactive-mode)
                              (setq inactive-seen (list (local-variable-p 'minibuffer-completion-table)
                                                        (local-variable-p 'stale-local)))))
                      (read-from-minibuffer "Prompt: " nil map)
                      (list mode-seen setup-seen inactive-seen))
                  (fset 'minibuffer-mode orig-minibuffer-mode)
                  (fset 'minibuffer-inactive-mode orig-inactive-mode))))"#,
        )
        .expect("eval should return minibuffer mode observations");
    assert_eq!(format!("{result}"), "((nil nil) nil (nil nil))");
}

#[test]
fn read_from_minibuffer_uses_calling_buffers_default_directory_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue space");
    drop(tx);
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r#"(progn
                 (with-current-buffer (get-buffer-create " *Minibuf-1*")
                   (setq default-directory "/tmp/launch-directory/"))
                 (let ((default-directory "/tmp/project-root/")
                       (seen nil)
                       (map (make-sparse-keymap)))
                   (define-key map " "
                     (lambda ()
                       (interactive)
                       (setq seen default-directory)
                       (exit-minibuffer)))
                   (read-from-minibuffer "Prompt: " nil map)
                   seen))"#,
        )
        .expect("eval should return the directory visible to a minibuffer command");

    assert_eq!(format!("{result}"), r#""/tmp/project-root/""#);
}

#[test]
fn read_from_minibuffer_swallows_exit_hook_signals_after_cleanup() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer command");
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r##"(let ((map (make-sparse-keymap))
                       (minibuffer-setup-hook nil)
                       (minibuffer-exit-hook
                        (list (lambda () (error "ignored")))))
                   (define-key map " "
                     (lambda () (interactive) (exit-minibuffer)))
                   (read-from-minibuffer "Prompt: " nil map))"##,
        )
        .expect("the exit-hook signal should be swallowed after cleanup");

    assert_eq!(result.as_utf8_str(), Some(""));
}

#[test]
fn read_from_minibuffer_restores_window_splits_created_during_the_read() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer command");
    drop(tx);
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r#"(let ((read-minibuffer-restore-windows t)
                     (minibuffer-setup-hook nil)
                     (minibuffer-exit-hook nil)
                     (map (make-sparse-keymap)))
                 (define-key map " "
                   (lambda ()
                     (interactive)
                     (split-window (minibuffer-selected-window) nil 'right)
                     (exit-minibuffer)))
                 (let ((before (length (window-list))))
                   (read-from-minibuffer "Prompt: " nil map)
                   (list before (length (window-list)))))"#,
        )
        .expect("read-from-minibuffer should restore its caller's windows");

    assert_eq!(format!("{result}"), "(1 1)");
}

#[test]
fn read_from_minibuffer_keeps_window_splits_when_restoration_is_disabled() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer command");
    drop(tx);
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r#"(let ((read-minibuffer-restore-windows nil)
                     (minibuffer-setup-hook nil)
                     (minibuffer-exit-hook nil)
                     (map (make-sparse-keymap)))
                 (define-key map " "
                   (lambda ()
                     (interactive)
                     (split-window (minibuffer-selected-window) nil 'right)
                     (exit-minibuffer)))
                 (let ((before (length (window-list))))
                   (read-from-minibuffer "Prompt: " nil map)
                   (list before (length (window-list)))))"#,
        )
        .expect("read-from-minibuffer should preserve window changes by policy");

    assert_eq!(format!("{result}"), "(1 2)");
}

#[test]
fn read_from_minibuffer_setup_error_unwinds_the_complete_session() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('x'),
    ))
    .expect("keep the interactive command-loop input source live");
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r##"(let ((calling-buffer (current-buffer))
                       (calling-window (selected-window))
                       (minibuffer-setup-hook
                        (list (lambda () (error "setup failed"))))
                       (minibuffer-exit-hook nil))
                   (condition-case nil
                       (read-from-minibuffer "Prompt: ")
                     (error nil))
                   (list (eq (current-buffer) calling-buffer)
                         (eq (selected-window) calling-window)
                         (= (minibuffer-depth) 0)
                         (null (active-minibuffer-window))
                         (minibufferp (current-buffer))))"##,
        )
        .expect("the setup-hook signal should be handled after minibuffer unwind");

    assert_eq!(format!("{result}"), "(t t t t nil)");
}

#[test]
fn read_from_minibuffer_mode_error_unwinds_before_session_entry() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('x'),
    ))
    .expect("keep the interactive command-loop input source live");
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r##"(let ((calling-buffer (current-buffer))
                       (calling-window (selected-window))
                       (minibuffer-mode-hook
                        (list (lambda () (error "mode failed"))))
                       (minibuffer-setup-hook nil)
                       (minibuffer-exit-hook nil))
                   (condition-case nil
                       (read-from-minibuffer "Prompt: ")
                     (error nil))
                   (list (eq (current-buffer) calling-buffer)
                         (eq (selected-window) calling-window)
                         (= (minibuffer-depth) 0)
                         (null (active-minibuffer-window))
                         (minibufferp (current-buffer))))"##,
        )
        .expect("the mode-hook signal should be handled after minibuffer unwind");

    assert_eq!(format!("{result}"), "(t t t t nil)");
}

#[test]
fn read_from_minibuffer_converts_unibyte_initial_input_to_buffer_text() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::named(crate::keyboard::NamedKey::Return),
    ))
    .expect("queue minibuffer return");
    ev.input_rx = Some(rx);
    let args = vec![
        Value::string("Prompt: "),
        Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF])),
    ];

    let result = finish_read_from_minibuffer_in_vm_runtime(&mut ev, &args)
        .expect("read-from-minibuffer should return initial input");

    let result_text = result
        .as_lisp_string()
        .expect("minibuffer result should be a Lisp string");
    assert!(result_text.is_multibyte());
    assert_eq!(
        crate::emacs_core::builtins::lisp_string_char_codes(result_text),
        vec![crate::emacs_core::emacs_char::byte8_to_char(0xFF)],
    );
}

/// GNU `read_minibuf` treats the cdr of a cons INITIAL-CONTENTS as a
/// one-based character position, inserts the initial text, and then moves
/// point there (`src/minibuf.c:606-620, 886-891`).  This must remain a
/// character coordinate: a byte offset would put point inside `α`.
#[test]
fn read_from_minibuffer_cons_initial_places_point_at_its_one_based_character_position() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer exit key");
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r##"(let ((map (make-sparse-keymap))
                       (seen nil))
                   (define-key map " " #'exit-minibuffer)
                   (let ((minibuffer-setup-hook
                          (list (lambda ()
                                  (setq seen
                                        (list (minibuffer-contents)
                                              (- (point)
                                                 (minibuffer-prompt-end))))))))
                     (read-from-minibuffer "P: " '("αβγ" . 2) map))
                   seen)"##,
        )
        .expect("read-from-minibuffer should exit normally");

    assert_eq!(format!("{result}"), r#"("αβγ" 1)"#);
}

/// GNU only interprets a non-nil cons cdr as an explicit position.  A nil cdr
/// retains the ordinary end-of-input cursor (`src/minibuf.c:606-620`).
#[test]
fn read_from_minibuffer_cons_initial_with_nil_position_keeps_point_at_end() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer exit key");
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r##"(let ((map (make-sparse-keymap))
                       (seen nil))
                   (define-key map " " #'exit-minibuffer)
                   (let ((minibuffer-setup-hook
                          (list (lambda ()
                                  (setq seen
                                        (- (point)
                                           (minibuffer-prompt-end)))))))
                     (read-from-minibuffer "P: " '("αβγ") map))
                   seen)"##,
        )
        .expect("read-from-minibuffer should exit normally");

    assert_eq!(result, Value::fixnum(3));
}

#[test]
fn read_from_minibuffer_restores_calling_frame_after_frame_switch() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let calling_frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut ev);
    let other_buffer = ev.buffers.create_buffer("*completion-frame*");
    let other_frame = ev
        .frames
        .create_frame("completion-frame", 80, 24, other_buffer);
    assert_eq!(
        ev.frames.selected_frame().map(|frame| frame.id),
        Some(calling_frame)
    );
    ev.obarray.set_symbol_value(
        "test-minibuffer-other-frame",
        Value::make_frame(other_frame.0),
    );
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer command");
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r##"(let ((map (make-sparse-keymap))
                       (minibuffer-setup-hook nil)
                       (minibuffer-exit-hook nil))
                   (define-key map " "
                     (lambda ()
                       (interactive)
                       (select-frame test-minibuffer-other-frame)
                       (exit-minibuffer)))
                   (read-from-minibuffer "Prompt: " nil map))"##,
        )
        .expect("minibuffer read should exit normally");

    assert_eq!(result.as_utf8_str(), Some(""));
    assert_eq!(
        ev.frames.selected_frame().map(|frame| frame.id),
        Some(calling_frame),
        "GNU read_minibuf switches back to the frame that invoked the minibuffer"
    );
}

#[test]
fn read_from_minibuffer_uses_and_restores_a_separate_minibuffer_owner_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    let root_frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut ev);
    let root_minibuffer = ev
        .frames
        .get(root_frame)
        .and_then(|frame| frame.minibuffer_window)
        .expect("root frame minibuffer");
    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(root_frame.0),
        ),
        Value::cons(Value::symbol("width"), Value::fixnum(40)),
        Value::cons(Value::symbol("height"), Value::fixnum(10)),
        Value::cons(
            Value::symbol("minibuffer"),
            Value::make_window(root_minibuffer.0),
        ),
        Value::cons(Value::symbol("visibility"), Value::NIL),
    ]);
    let child = crate::emacs_core::frame::builtin_make_terminal_frame(&mut ev, vec![params])
        .expect("make child frame sharing the root minibuffer");
    let child_frame = crate::window::FrameId(child.as_frame_id().expect("child frame"));
    ev.frames.select_frame(child_frame);
    ev.obarray.set_symbol_value(
        "test-minibuffer-root-frame",
        Value::make_frame(root_frame.0),
    );
    ev.obarray.set_symbol_value(
        "test-minibuffer-child-frame",
        Value::make_frame(child_frame.0),
    );
    ev.obarray
        .set_symbol_value("test-minibuffer-owner-was-root", Value::NIL);

    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer command");
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r##"(let ((map (make-sparse-keymap))
                       (before (length (window-list test-minibuffer-root-frame)))
                       (minibuffer-setup-hook
                        (list (lambda ()
                                (setq test-minibuffer-owner-was-root
                                      (eq (window-frame
                                           (active-minibuffer-window))
                                          test-minibuffer-root-frame)))))
                       (minibuffer-exit-hook nil))
                   (define-key map " "
                     (lambda ()
                       (interactive)
                       (with-selected-frame test-minibuffer-root-frame
                         (split-window (frame-root-window)))
                       (exit-minibuffer)))
                   (read-from-minibuffer "Prompt: " nil map)
                   (list test-minibuffer-owner-was-root
                         (eq (selected-frame) test-minibuffer-child-frame)
                         before
                         (length (window-list test-minibuffer-root-frame))))"##,
        )
        .expect("shared-minibuffer-frame read should exit normally");

    assert_eq!(format!("{result}"), "(t t 1 1)");
}

#[test]
fn read_from_minibuffer_custom_keymap_lambda_sees_last_command_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue space");
    drop(tx);
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r#"(let ((seen nil)
                 (minibuffer-setup-hook nil)
                 (minibuffer-exit-hook nil)
                 (map (make-sparse-keymap)))
             (define-key map " "
               (lambda ()
                 (interactive)
                 (setq seen last-command-event)
                 (exit-minibuffer)))
             (condition-case err
                 (list (read-from-minibuffer "Prompt: " nil map)
                       seen
                       last-command-event)
               (error
                (list :error (car err) (cdr err) seen last-command-event))))"#,
        )
        .expect("eval should return condition-case payload");
    assert_eq!(format!("{result}"), r#"("" 32 32)"#);
}

#[test]
fn read_from_minibuffer_strips_properties_by_default_and_preserves_explicit_opt_in_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    for _ in 0..4 {
        tx.send(crate::keyboard::InputEvent::key_press(
            crate::keyboard::KeyEvent::char(' '),
        ))
        .expect("queue minibuffer exit key");
    }
    drop(tx);
    ev.input_rx = Some(rx);

    let result = ev
        .eval_str(
            r#"(progn
                 (defvar neo-rfm-plain-history nil)
                 (defvar neo-rfm-caller-opt-in-history nil)
                 (defvar neo-rfm-minibuffer-opt-in-history nil)
                 (defvar neo-rfm-reused-history nil)
                 (let ((neo-rfm-plain-history nil)
                       (neo-rfm-caller-opt-in-history nil)
                       (neo-rfm-minibuffer-opt-in-history nil)
                       (neo-rfm-reused-history nil)
                       (map (make-sparse-keymap)))
                   (define-key map " " #'exit-minibuffer)
                   (let* ((source (propertize "abc" 'face 'bold))
                          (plain
                           (read-from-minibuffer
                            "Plain: " source map nil 'neo-rfm-plain-history))
                          (caller-opt-in
                           (let ((minibuffer-allow-text-properties t))
                             (read-from-minibuffer
                              "Caller: " source map nil
                              'neo-rfm-caller-opt-in-history)))
                          (minibuffer-opt-in
                           (minibuffer-with-setup-hook
                               (lambda ()
                                 (setq-local minibuffer-allow-text-properties t))
                             (read-from-minibuffer
                              "Local: " source map nil
                              'neo-rfm-minibuffer-opt-in-history)))
                          (reused
                           (read-from-minibuffer
                            "Reused: " source map nil 'neo-rfm-reused-history)))
                     (list (text-properties-at 0 plain)
                           (text-properties-at 0 (car neo-rfm-plain-history))
                           (text-properties-at 0 caller-opt-in)
                           (text-properties-at
                            0 (car neo-rfm-caller-opt-in-history))
                           (text-properties-at 0 minibuffer-opt-in)
                           (text-properties-at
                            0 (car neo-rfm-minibuffer-opt-in-history))
                           (text-properties-at 0 reused)
                           (text-properties-at 0 (car neo-rfm-reused-history))))))"#,
        )
        .expect("eval should return minibuffer property observations");

    assert_eq!(
        format!("{result}"),
        "(nil nil (face bold) (face bold) (face bold) (face bold) nil nil)"
    );
}

#[test]
fn read_from_minibuffer_uses_live_prompt_field_boundary_after_prompt_rewrite_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer exit key");
    drop(tx);
    eval.input_rx = Some(rx);

    let result = eval
        .eval_str(
            r#"(progn
                 (defvar neo-live-prompt-history nil)
                 (let ((neo-live-prompt-history nil)
                       (map (make-sparse-keymap)))
                   (define-key map " " #'exit-minibuffer)
                   (let ((value
                          (minibuffer-with-setup-hook
                              (lambda ()
                                (let ((inhibit-read-only t))
                                  (delete-region
                                   (point-min) (minibuffer-prompt-end))
                                  (goto-char (point-min))
                                  (insert
                                   (propertize
                                    "(3/3) Environment: "
                                    'front-sticky t
                                    'rear-nonsticky t
                                    'field t
                                    'read-only t)))
                                (goto-char (point-max)))
                            (read-from-minibuffer
                             "Environment: " "production" map nil
                             'neo-live-prompt-history))))
                     (list value neo-live-prompt-history))))"#,
        )
        .expect("minibuffer read should finish");

    assert_eq!(format!("{result}"), r#"("production" ("production"))"#);
}

#[test]
fn read_from_minibuffer_adds_history_after_restoring_the_calling_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer exit key");
    eval.input_rx = Some(rx);

    let result = eval
        .eval_str(
            r##"(progn
                   (defvar neo-buffer-local-minibuffer-history nil)
                   (with-current-buffer
                       (get-buffer-create " *minibuffer-history-caller*")
                     (setq-local neo-buffer-local-minibuffer-history nil)
                     (let ((map (make-sparse-keymap))
                           (minibuffer-setup-hook nil)
                           (minibuffer-exit-hook nil))
                       (define-key map " " #'exit-minibuffer)
                       (list
                        (read-from-minibuffer
                         "Prompt: " "local" map nil
                         'neo-buffer-local-minibuffer-history)
                        neo-buffer-local-minibuffer-history))))"##,
        )
        .expect("history insertion should observe the restored caller buffer");

    assert_eq!(format!("{result}"), r#"("local" ("local"))"#);
}

#[test]
fn read_from_minibuffer_uses_empty_input_default_only_for_history_when_read_is_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer exit key");
    eval.input_rx = Some(rx);

    let result = eval
        .eval_str(
            r##"(progn
                   (defvar neo-empty-minibuffer-history nil)
                   (setq neo-empty-minibuffer-history nil)
                   (let ((map (make-sparse-keymap))
                         (minibuffer-setup-hook nil)
                         (minibuffer-exit-hook nil))
                     (define-key map " " #'exit-minibuffer)
                     (list
                      (read-from-minibuffer
                       "Prompt: " nil map nil
                       'neo-empty-minibuffer-history "fallback")
                      neo-empty-minibuffer-history)))"##,
        )
        .expect("empty input should return as a string and record the default");

    assert_eq!(format!("{result}"), r#"("" ("fallback"))"#);
}

#[test]
fn activate_minibuffer_window_switches_displayed_buffer_and_restores_state() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut ev);
    let minibuffer_window = ev
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.minibuffer_window)
        .expect("initial frame minibuffer window");
    let previous_selected_window = ev
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let previous_minibuffer_buffer = ev
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(minibuffer_window))
        .and_then(|window| window.buffer_id())
        .expect("inactive minibuffer buffer");

    let active_buffer = ev.buffer_manager_mut().create_buffer(" *Minibuf-1*");
    let saved = activate_minibuffer_window(
        &mut ev,
        active_buffer,
        crate::emacs_core::minibuffer::MinibufferEntryLevel::Outermost,
    )
    .expect("activate minibuffer");

    let frame = ev
        .frame_manager()
        .get(frame_id)
        .expect("frame should stay live");
    assert_eq!(frame.selected_window, minibuffer_window);
    assert_eq!(
        frame
            .find_window(minibuffer_window)
            .and_then(|window| window.buffer_id()),
        Some(active_buffer)
    );
    assert_eq!(ev.buffer_manager().current_buffer_id(), Some(active_buffer));
    assert_eq!(ev.active_minibuffer_window, Some(minibuffer_window));
    assert_eq!(
        ev.minibuffer_selected_window,
        Some(previous_selected_window)
    );

    let generation_while_active = ev.menu_bar_rebuild_generation();

    restore_minibuffer_window(&mut ev, saved);

    let frame = ev
        .frame_manager()
        .get(frame_id)
        .expect("frame should stay live");
    assert_eq!(frame.selected_window, previous_selected_window);
    assert_eq!(
        frame
            .find_window(minibuffer_window)
            .and_then(|window| window.buffer_id()),
        Some(previous_minibuffer_buffer)
    );
    assert_eq!(ev.active_minibuffer_window, None);
    assert_eq!(ev.minibuffer_selected_window, None);
    assert_ne!(
        ev.menu_bar_rebuild_generation(),
        generation_while_active,
        "GNU minibuffer_unwind restores the minibuffer buffer after the caller is selected, so wset_redisplay must raise the global menu rebuild boundary"
    );
}

#[test]
fn nested_minibuffer_keeps_the_outer_calling_window_active() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut ev);
    let calling_window = ev
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let outer_buffer = ev.buffer_manager_mut().create_buffer(" *Minibuf-1*");
    let _outer = activate_minibuffer_window(
        &mut ev,
        outer_buffer,
        crate::emacs_core::minibuffer::MinibufferEntryLevel::Outermost,
    )
    .expect("activate outer minibuffer");
    let inner_buffer = ev.buffer_manager_mut().create_buffer(" *Minibuf-2*");
    let _inner = activate_minibuffer_window(
        &mut ev,
        inner_buffer,
        crate::emacs_core::minibuffer::MinibufferEntryLevel::Recursive,
    )
    .expect("activate nested minibuffer");

    assert_eq!(
        ev.minibuffer_selected_window,
        Some(calling_window),
        "GNU preserves minibuf_selected_window when a recursive minibuffer reuses the already-selected minibuffer window"
    );
}

#[test]
fn activate_minibuffer_window_saves_the_callers_live_buffer_point() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut ev);
    let caller_window = ev
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let caller_buffer = ev
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(caller_window))
        .and_then(|window| window.buffer_id())
        .expect("caller buffer");
    let text = "one\ntwo\nthree\n";
    ev.buffer_manager_mut()
        .get_mut(caller_buffer)
        .expect("caller buffer")
        .insert(text);
    ev.buffer_manager_mut()
        .goto_buffer_emacs_byte_pos(caller_buffer, crate::buffer::EmacsBytePos::new(text.len()));
    let expected = LispCharPos1::from_one_based_usize(text.len() + 1);

    let minibuffer = ev.buffer_manager_mut().create_buffer(" *Minibuf-1*");
    let _saved = activate_minibuffer_window(
        &mut ev,
        minibuffer,
        crate::emacs_core::minibuffer::MinibufferEntryLevel::Outermost,
    )
    .expect("activate minibuffer");
    // Redisplay/text edits refresh the cached window point from its marker.
    // Saving only the cache would therefore lose the caller's live point as
    // soon as the next synchronization runs.
    ev.sync_window_positions(caller_buffer);
    let saved_point = match ev
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(caller_window))
        .expect("caller window")
    {
        crate::window::Window::Leaf { point, .. } => *point,
        crate::window::Window::Internal { .. } => panic!("caller must remain a live window"),
    };

    assert_eq!(saved_point, expected);
}

#[test]
fn expired_minibuffer_buffer_is_erased_before_restore() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut ev);
    let minibuffer_window = ev
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.minibuffer_window)
        .expect("initial frame minibuffer window");
    let previous_selected_window = ev
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let active_buffer = ev.buffer_manager_mut().create_buffer(" *Minibuf-1*");
    ev.buffer_manager_mut()
        .replace_buffer_contents(active_buffer, "M-x bury-buffer")
        .expect("install minibuffer contents");
    let saved = activate_minibuffer_window(
        &mut ev,
        active_buffer,
        crate::emacs_core::minibuffer::MinibufferEntryLevel::Outermost,
    )
    .expect("activate minibuffer");

    erase_expired_minibuffer_buffer_in_state(&mut ev.buffers, active_buffer);
    restore_minibuffer_window(&mut ev, saved);

    let active_minibuffer = ev
        .buffer_manager()
        .get(active_buffer)
        .expect("active minibuffer buffer");
    let text = active_minibuffer.buffer_substring_range(active_minibuffer.full_emacs_byte_range());
    assert_eq!(text, "");
    let frame = ev.frame_manager().get(frame_id).expect("frame");
    assert_eq!(frame.selected_window, previous_selected_window);
    assert_ne!(frame.selected_window, minibuffer_window);
}

#[test]
fn active_minibuffer_window_sync_keeps_live_buffer_point() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut ev);
    let minibuffer_window = ev
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.minibuffer_window)
        .expect("initial frame minibuffer window");
    let active_buffer = ev.buffer_manager_mut().create_buffer(" *Minibuf-1*");
    let _saved = activate_minibuffer_window(
        &mut ev,
        active_buffer,
        crate::emacs_core::minibuffer::MinibufferEntryLevel::Outermost,
    )
    .expect("activate minibuffer");

    ev.buffer_manager_mut()
        .replace_buffer_contents(active_buffer, "Eval: (+ 1 2)")
        .expect("replace active minibuffer contents");
    ev.buffer_manager_mut()
        .goto_buffer_emacs_byte_pos(active_buffer, crate::buffer::EmacsBytePos::new(13))
        .expect("move active minibuffer point");
    if let Some(crate::window::Window::Leaf { point, .. }) = ev
        .frame_manager_mut()
        .get_mut(frame_id)
        .and_then(|frame| frame.find_window_mut(minibuffer_window))
    {
        *point = LispCharPos1::from_one_based_usize(7);
    }

    let pre_buffer_point = ev
        .buffer_manager()
        .get(active_buffer)
        .expect("active minibuffer buffer")
        .point_char_pos()
        .get()
        + 1;
    assert_eq!(pre_buffer_point, 14);

    crate::emacs_core::window_cmds::remember_selected_window_point_in_state(
        &mut ev.frames,
        &mut ev.buffers,
        frame_id,
    );
    crate::emacs_core::window_cmds::sync_selected_window_buffer_in_state(
        &ev.frames,
        &mut ev.buffers,
        frame_id,
    );

    let window_point = ev
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(minibuffer_window))
        .and_then(|window| match window {
            crate::window::Window::Leaf { point, .. } => Some(*point),
            crate::window::Window::Internal { .. } => None,
        })
        .expect("minibuffer window point");
    let buffer_point = ev
        .buffer_manager()
        .get(active_buffer)
        .expect("active minibuffer buffer")
        .point_char_pos()
        .get()
        + 1;

    assert_eq!(window_point, LispCharPos1::from_one_based_usize(14));
    assert_eq!(buffer_point, 14);
}

#[test]
fn read_string_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_string(&mut ev, vec![Value::string("Prompt: ")]);
    assert!(result.is_err());
}

#[test]
fn read_string_non_character_event_stays_queued_and_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::symbol("foo")]),
    );
    let result = builtin_read_string(&mut ev, vec![Value::string("Prompt: ")]);
    assert!(matches!(result, Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file"));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::symbol("foo")]))
    );
}

#[test]
fn read_string_ignores_initial_and_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_string(
        &mut ev,
        vec![Value::string("Prompt: "), Value::string("initial")],
    );
    assert!(result.is_err());
}

#[test]
fn read_string_rejects_non_stringish_initial_input() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_string(&mut ev, vec![Value::string("Prompt: "), Value::fixnum(1)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn read_string_rejects_cons_initial_with_non_string_car() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let cons_initial = Value::cons(Value::fixnum(1), Value::fixnum(1));
    let result = builtin_read_string(&mut ev, vec![Value::string("Prompt: "), cons_initial]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn read_string_rejects_more_than_five_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_string(
        &mut ev,
        vec![
            Value::string("Prompt: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn finish_read_string_with_minibuffer_builds_expected_args() {
    crate::test_utils::init_test_tracing();
    let result = finish_read_string_with_minibuffer(
        &[
            Value::string("Prompt: "),
            Value::string("seed"),
            Value::symbol("hist"),
            Value::string("fallback"),
            Value::T,
        ],
        |minibuffer_args| {
            assert_eq!(
                minibuffer_args,
                &[
                    Value::string("Prompt: "),
                    Value::string("seed"),
                    Value::NIL,
                    Value::NIL,
                    Value::symbol("hist"),
                    Value::string("fallback"),
                    Value::T,
                ]
            );
            Ok(Value::string("result"))
        },
    )
    .unwrap();
    assert_eq!(result, Value::string("result"));
}

#[test]
fn completing_read_minibuffer_args_choose_completion_keymap_by_require_match() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.obarray.set_symbol_value(
        "minibuffer-local-completion-map",
        Value::symbol("completion-map"),
    );
    eval.obarray.set_symbol_value(
        "minibuffer-local-must-match-map",
        Value::symbol("must-match-map"),
    );

    let default_args = completing_read_minibuffer_args(
        eval.obarray(),
        &[
            Value::string("Prompt: "),
            Value::list(vec![Value::string("alpha")]),
            Value::NIL,
            Value::NIL,
            Value::string("seed"),
            Value::symbol("hist"),
            Value::string("fallback"),
            Value::T,
        ],
    );
    assert_eq!(
        default_args,
        [
            Value::string("Prompt: "),
            Value::string("seed"),
            Value::symbol("completion-map"),
            Value::NIL,
            Value::symbol("hist"),
            Value::string("fallback"),
            Value::T,
        ]
    );

    let must_match_args = completing_read_minibuffer_args(
        eval.obarray(),
        &[
            Value::string("Prompt: "),
            Value::list(vec![Value::string("alpha")]),
            Value::NIL,
            Value::T,
        ],
    );
    assert_eq!(must_match_args[2], Value::symbol("must-match-map"));
}

#[test]
fn completion_confirm_from_require_match_matches_gnu_minibuffer_setup() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        completion_confirm_from_require_match(Value::NIL),
        Value::NIL
    );
    assert_eq!(completion_confirm_from_require_match(Value::T), Value::NIL);
    assert_eq!(
        completion_confirm_from_require_match(Value::symbol("confirm")),
        Value::symbol("confirm")
    );
    assert_eq!(
        completion_confirm_from_require_match(Value::symbol("confirm-after-completion")),
        Value::symbol("confirm-after-completion")
    );
    assert_eq!(
        completion_confirm_from_require_match(Value::symbol("predicate-function")),
        Value::symbol("predicate-function")
    );
    assert_eq!(
        completion_confirm_from_require_match(Value::string("non-t-value")),
        Value::string("non-t-value")
    );
}

/// `read-number' is lisp/subr.el:3725 and has no subr (DIVERGENCES.md 152),
/// so its arms are asked of the runtime that loaded `subr.el'.  Every row was
/// measured on GNU 31.0.90 `-Q --batch' with stdin at /dev/null first
/// (tmp/pw59/gnu-readnumber2.txt), including the third HIST argument the Rust
/// subr was registered too narrow to accept.
#[test]
fn read_number_arms_match_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        crate::test_utils::runtime_startup_eval_all(
            r#"(condition-case e (read-number "Number: " 42) (error (car e)))
               (condition-case e (read-number "Number: ") (error (car e)))
               (let ((unread-command-events (list 'foo)))
                 (list (condition-case e (read-number "Number: ") (error (car e)))
                       unread-command-events))
               (condition-case e (read-number "Number: " "x") (error e))
               (condition-case e (read-number "Number: " 1.5) (error (car e)))
               (condition-case e (read-number "Number: " 42 nil nil) (error e))
               (condition-case e (read-number 123) (error e))
               (condition-case e (read-number "Number: " nil 'my-hist) (error (car e)))"#,
        ),
        vec![
            "OK end-of-file",
            "OK end-of-file",
            // The queued event is left alone; batch reads stdin.
            "OK (end-of-file (foo))",
            "OK (wrong-type-argument numberp \"x\")",
            "OK end-of-file",
            "OK (wrong-number-of-arguments (1 . 3) 4)",
            "OK (wrong-type-argument stringp 123)",
            // HIST: accepted, which the registered (1 . 2) arity refused.
            "OK end-of-file",
        ],
    );
}

#[test]
fn read_passwd_startup_is_autoloaded() {
    crate::test_utils::init_test_tracing();
    let eval = eval_with_ldefs_boot_autoloads(&["read-passwd"]);
    let function = eval
        .obarray
        .symbol_function("read-passwd")
        .expect("missing read-passwd startup function cell");
    assert!(crate::emacs_core::autoload::is_autoload_value(&function));
}

#[test]
fn read_passwd_loads_from_gnu_auth_source() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"
        (condition-case err
            (read-passwd "")
          (error (list 'err (car err))))
        (subrp (symbol-function 'read-passwd))
        "#,
    );
    assert_eq!(results[0], r#"OK (err end-of-file)"#);
    assert_eq!(results[1], "OK nil");
}

#[test]
fn read_passwd_loaded_accepts_optional_args_and_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"
        (condition-case err
            (read-passwd "" t "default")
          (error (list 'err (car err))))
        "#,
    );
    assert_eq!(results[0], r#"OK (err end-of-file)"#);
}

#[test]
fn read_passwd_loaded_rejects_non_string_prompt() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"
        (condition-case err
            (read-passwd 123)
          (error (list 'err (car err))))
        "#,
    );
    assert_eq!(results[0], r#"OK (err wrong-type-argument)"#);
}

#[test]
fn read_passwd_loaded_rejects_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"
        (condition-case err
            (read-passwd)
          (error (list 'err (car err))))
        (condition-case err
            (read-passwd "" nil nil nil)
          (error (list 'err (car err))))
        "#,
    );
    assert_eq!(results[0], r#"OK (err wrong-number-of-arguments)"#);
    assert_eq!(results[1], r#"OK (err wrong-number-of-arguments)"#);
}

#[test]
fn completing_read_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_completing_read(&mut ev, vec![Value::string("Choose: "), Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn minibuffer_input_source_distinguishes_stdin_macros_and_live_input() {
    crate::test_utils::init_test_tracing();

    let mut batch = Context::new();
    assert_eq!(
        batch.minibuffer_input_source(),
        MinibufferInputSource::StandardInput
    );

    batch.begin_executing_kbd_macro_runtime(vec![Value::fixnum(b'a' as i64)]);
    assert_eq!(
        batch.minibuffer_input_source(),
        MinibufferInputSource::CommandLoop
    );

    let mut interactive = Context::new();
    let (_input_tx, input_rx) = crossbeam_channel::unbounded();
    interactive.init_input_system(input_rx);
    assert_eq!(
        interactive.minibuffer_input_source(),
        MinibufferInputSource::CommandLoop
    );
}

#[test]
fn completing_read_uses_dynamically_bound_keyboard_macro_events_in_batch_mode() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(let ((executing-kbd-macro t)
                  (unread-command-events (append "al" '(tab return)))
                  (completion-styles '(basic)))
              (completing-read "Choose: " '("alpha" "beta")))"#,
    );

    assert_eq!(results, vec![r#"OK "alpha""#]);
}

#[test]
fn y_or_n_p_uses_dynamically_bound_keyboard_macro_answer_in_batch_mode() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(let ((executing-kbd-macro t)
                  (unread-command-events (list ?y)))
              (list (y-or-n-p "Really remove download? ")
                    unread-command-events))"#,
    );

    assert_eq!(results, vec!["OK (t nil)"]);
}

#[test]
fn read_char_consumes_executing_keyboard_macro_event_without_input_receiver() {
    crate::test_utils::init_test_tracing();
    let mut batch = Context::new();
    let prefix = vec![Value::char('C'), Value::char('c')];
    batch.set_read_command_keys(prefix.clone());
    batch.begin_executing_kbd_macro_runtime(vec![Value::char('a')]);

    let result = builtin_read_char(&mut batch, vec![]).expect("read-char");

    assert_eq!(result, Value::fixnum('a' as i64));
    assert_eq!(
        batch.read_command_keys(),
        &[prefix[0], prefix[1], Value::char('a')]
    );
}

#[test]
fn completing_read_calls_completing_read_function_before_interactive_read() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = ev
        .eval_str(
            r#"(let ((completing-read-function
                      (lambda (&rest args)
                        (cons 'called args))))
                 (completing-read "Choose: " '("alpha") nil t nil 'hist "alpha" t))"#,
        )
        .expect("custom completing-read-function should evaluate");

    let values = list_to_vec(&result).expect("result should be a proper list");
    assert_eq!(values[0], Value::symbol("called"));
    assert_eq!(values[1].as_utf8_str(), Some("Choose: "));
    assert!(values[4].is_t());
    assert_eq!(values[7].as_utf8_str(), Some("alpha"));
    assert!(values[8].is_t());
}

#[test]
fn completing_read_pads_omitted_arguments_before_calling_completing_read_function() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = ev
        .eval_str(
            r#"(let ((completing-read-function
                      (lambda (&rest args)
                        (cons (length args) args))))
                 (completing-read "Choose: " '("alpha") nil t nil 'hist "alpha"))"#,
        )
        .expect("custom completing-read-function should evaluate");

    let values = list_to_vec(&result).expect("result should be a proper list");
    assert_eq!(values.len(), 9);
    assert_eq!(values[0], Value::fixnum(8));
    assert_eq!(values[1].as_utf8_str(), Some("Choose: "));
    assert_eq!(values[2], Value::list(vec![Value::string("alpha")]));
    assert!(values[3].is_nil());
    assert!(values[4].is_t());
    assert!(values[5].is_nil());
    assert_eq!(values[6], Value::symbol("hist"));
    assert_eq!(values[7].as_utf8_str(), Some("alpha"));
    assert!(values[8].is_nil());
}

#[test]
fn completing_read_non_character_event_stays_queued_and_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::symbol("foo")]),
    );
    let result = builtin_completing_read(&mut ev, vec![Value::string("Choose: "), Value::NIL]);
    assert!(matches!(result, Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file"));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::symbol("foo")]))
    );
}

#[test]
fn completing_read_ignores_default_and_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_completing_read(
        &mut ev,
        vec![
            Value::string("Choose: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::string("fallback"),
        ],
    );
    assert!(result.is_err());
}

#[test]
fn completing_read_rejects_non_stringish_initial_input() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_completing_read(
        &mut ev,
        vec![
            Value::string("Choose: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(1),
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn completing_read_accepts_cons_initial_with_string_and_position() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let cons_initial = Value::cons(Value::string("x"), Value::fixnum(1));
    let result = builtin_completing_read(
        &mut ev,
        vec![
            Value::string("Choose: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            cons_initial,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file"
    ));
}

#[test]
fn completing_read_rejects_cons_initial_with_non_string_car() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let cons_initial = Value::cons(Value::fixnum(1), Value::fixnum(1));
    let result = builtin_completing_read(
        &mut ev,
        vec![
            Value::string("Choose: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            cons_initial,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn completing_read_rejects_cons_initial_with_non_numeric_position() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let cons_initial = Value::cons(Value::string("x"), Value::NIL);
    let result = builtin_completing_read(
        &mut ev,
        vec![
            Value::string("Choose: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            cons_initial,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn completing_read_rejects_more_than_eight_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_completing_read(
        &mut ev,
        vec![
            Value::string("Choose: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn yes_or_no_p_signals_end_of_file() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_yes_or_no_p(&mut ev, vec![Value::string("Confirm? ")]);
    assert!(result.is_err());
}

#[test]
fn yes_or_no_p_rejects_non_string_prompt() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_yes_or_no_p(&mut ev, vec![Value::fixnum(123)]);
    assert!(result.is_err());
}

#[test]
fn yes_or_no_p_rejects_extra_arg() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_yes_or_no_p(&mut ev, vec![Value::string("Confirm? "), Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn yes_or_no_p_uses_dialog_path_for_cons_last_input_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let frame_id = ev
        .frames
        .create_frame("yes-or-no-dialog", 800, 600, scratch);
    ev.frames.select_frame(frame_id);
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    let hidden = Arc::clone(&host.hidden);
    ev.set_display_host(Box::new(host));
    ev.obarray.set_symbol_value(
        "last-input-event",
        Value::list(vec![Value::symbol("dbus-event")]),
    );
    ev.obarray
        .set_symbol_value("last-nonmenu-event", Value::NIL);
    ev.obarray.set_symbol_value("use-dialog-box", Value::T);
    tx.send(crate::keyboard::InputEvent::MenuSelection { index: 0 })
        .unwrap();

    let result = builtin_yes_or_no_p(&mut ev, vec![Value::string("Confirm? ")]).unwrap();
    assert_eq!(result, Value::T);
    let shown = shown.lock().unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].frame_id, frame_id);
    assert_eq!(shown[0].title.as_deref(), Some("Confirm? "));
    assert_eq!(shown[0].entries.len(), 2);
    assert_eq!(shown[0].entries[0].label, "Yes");
    assert_eq!(shown[0].entries[1].label, "No");
    assert_eq!(*hidden.lock().unwrap(), 1);
}

#[test]
fn yes_or_no_p_respects_use_dialog_box_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "last-input-event",
        Value::list(vec![Value::symbol("dbus-event")]),
    );
    ev.obarray
        .set_symbol_value("last-nonmenu-event", Value::NIL);
    ev.obarray.set_symbol_value("use-dialog-box", Value::NIL);

    let result = builtin_yes_or_no_p(&mut ev, vec![Value::string("Confirm? ")]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file"
    ));
}

#[test]
fn yes_or_no_p_uses_dialog_before_short_answers() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    ev.set_display_host(Box::new(host));
    ev.obarray.set_symbol_value(
        "last-input-event",
        Value::list(vec![Value::symbol("dbus-event")]),
    );
    ev.obarray
        .set_symbol_value("last-nonmenu-event", Value::NIL);
    ev.obarray.set_symbol_value("use-dialog-box", Value::T);
    ev.obarray.set_symbol_value("use-short-answers", Value::T);
    tx.send(crate::keyboard::InputEvent::MenuSelection { index: 1 })
        .unwrap();

    let result = builtin_yes_or_no_p(&mut ev, vec![Value::string("Confirm? ")]).unwrap();

    assert_eq!(result, Value::NIL);
    let shown = shown.lock().unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].title.as_deref(), Some("Confirm? "));
}

#[test]
fn yes_or_no_p_ignores_unread_events_and_eofs() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(89)]),
    );
    let result = builtin_yes_or_no_p(&mut ev, vec![Value::string("Confirm? ")]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file"
    ));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::fixnum(89)]))
    );
}

#[test]
fn yes_or_no_p_unread_events_do_not_change() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(110)]),
    );
    let result = builtin_yes_or_no_p(&mut ev, vec![Value::string("Confirm? ")]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file"
    ));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::fixnum(110)]))
    );
}

#[test]
fn yes_or_no_p_rejects_invalid_character_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(48)]),
    );
    let result = builtin_yes_or_no_p(&mut ev, vec![Value::string("Confirm? ")]);
    assert!(matches!(result, Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file"));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::fixnum(48)]))
    );
}

#[test]
fn yes_or_no_p_rejects_nil_prompt() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_yes_or_no_p(&mut ev, vec![Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn finish_yes_or_no_p_with_minibuffer_retries_until_valid_answer() {
    crate::test_utils::init_test_tracing();
    let mut prompts = Vec::new();
    let mut answers = [Value::string("maybe"), Value::string(" no ")].into_iter();
    let result = finish_yes_or_no_p_with_minibuffer(&[Value::string("Confirm?")], |args| {
        prompts.push(args[0].as_utf8_str().unwrap().to_owned());
        Ok(answers.next().expect("enough answers"))
    })
    .unwrap();
    assert_eq!(result, Value::NIL);
    assert_eq!(
        prompts,
        vec![
            "Confirm? (yes or no) ".to_string(),
            "Confirm? (yes or no) ".to_string()
        ]
    );
}

#[test]
fn input_pending_p_returns_nil_without_events() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_input_pending_p(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn input_pending_p_returns_t_with_unread_events() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_input_pending_p(&mut ev, vec![]).unwrap();
    assert_eq!(result, Value::T);
}

#[test]
fn input_pending_p_returns_t_with_non_nil_requeued_input_method_events() {
    crate::test_utils::init_test_tracing();

    for queue in [
        "unread-post-input-method-events",
        "unread-input-method-events",
    ] {
        let mut ev = Context::new();
        ev.obarray
            .set_symbol_value(queue, Value::symbol("pending-input-method-event"));
        let result = builtin_input_pending_p(&mut ev, vec![]).unwrap();
        assert_eq!(result, Value::T, "{queue} must count as pending input");
    }
}

#[test]
fn input_pending_p_uses_dynamic_unread_command_events_binding() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = ev
        .eval_str("(let ((unread-command-events nil)) (input-pending-p))")
        .unwrap();
    assert!(result.is_nil());
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::fixnum(97)]))
    );
}

#[test]
fn input_pending_p_returns_nil_for_non_list_unread_command_events() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray
        .set_symbol_value("unread-command-events", Value::fixnum(7));
    let result = builtin_input_pending_p(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn input_pending_p_accepts_optional_check_timers_arg() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::symbol("foo")]),
    );
    let result = builtin_input_pending_p(&mut ev, vec![Value::symbol("timers")]).unwrap();
    assert_eq!(result, Value::T);
}

#[test]
fn input_pending_p_returns_t_with_host_keypress() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .expect("queue keypress");
    ev.input_rx = Some(rx);

    let result = builtin_input_pending_p(&mut ev, vec![]).unwrap();
    assert_eq!(result, Value::T);

    let event = ev.read_char().expect("keypress should remain available");
    assert_eq!(event, Value::fixnum('a' as i64));
}

#[test]
fn input_pending_p_ignores_focus_events_by_default() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::Focus {
        focused: true,
        emacs_frame_id: 0,
    })
    .expect("queue focus event");
    ev.input_rx = Some(rx);

    let result = builtin_input_pending_p(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn input_pending_p_ignores_deferred_switch_frame_event_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let switch_frame = Value::list(vec![Value::symbol("switch-frame"), Value::fixnum(7)]);
    ev.command_loop
        .keyboard
        .set_unread_selection_event(switch_frame);

    let pending = builtin_input_pending_p(&mut ev, vec![]).unwrap();

    assert!(
        pending.is_nil(),
        "GNU input-pending-p does not include unread_switch_frame"
    );
    assert_eq!(
        ev.read_char()
            .expect("deferred switch-frame remains readable"),
        switch_frame
    );
}

#[test]
fn input_pending_p_filters_configured_low_level_special_events_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.queue_special_event(Value::list(vec![Value::symbol("help-echo"), Value::NIL]));
    ev.queue_special_event(Value::list(vec![
        Value::symbol("select-window"),
        Value::fixnum(7),
    ]));

    let pending = builtin_input_pending_p(&mut ev, vec![]).unwrap();

    assert!(
        pending.is_nil(),
        "configured low-level maintenance events must not preempt idle work"
    );
}

#[test]
fn input_pending_p_ignores_mouse_move_without_track_mouse() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::MouseMove {
        x: 10.0,
        y: 20.0,
        modifiers: crate::keyboard::Modifiers::none(),
        target_frame_id: 0,
    })
    .expect("queue mouse move");
    ev.input_rx = Some(rx);

    let result = builtin_input_pending_p(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn input_pending_p_reports_mouse_move_with_track_mouse() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value("track-mouse", Value::T);
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::MouseMove {
        x: 10.0,
        y: 20.0,
        modifiers: crate::keyboard::Modifiers::none(),
        target_frame_id: 0,
    })
    .expect("queue mouse move");
    ev.input_rx = Some(rx);

    let result = builtin_input_pending_p(&mut ev, vec![]).unwrap();
    assert_eq!(result, Value::T);
}

#[test]
fn read_char_skips_mouse_move_without_track_mouse() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::MouseMove {
        x: 10.0,
        y: 20.0,
        modifiers: crate::keyboard::Modifiers::none(),
        target_frame_id: 0,
    })
    .expect("queue mouse move");
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .expect("queue keypress");
    ev.input_rx = Some(rx);

    let result = ev.read_char().expect("keypress should remain readable");
    assert_eq!(result, Value::fixnum('a' as i64));
}

#[test]
fn read_char_returns_mouse_move_with_track_mouse() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value("track-mouse", Value::T);
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::MouseMove {
        x: 10.0,
        y: 20.0,
        modifiers: crate::keyboard::Modifiers::none(),
        target_frame_id: 0,
    })
    .expect("queue mouse move");
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .expect("queue keypress");
    ev.input_rx = Some(rx);

    let result = ev.read_char().expect("mouse movement should be readable");
    let slots = crate::emacs_core::value::list_to_vec(&result).expect("mouse movement event");
    assert_eq!(slots[0], Value::symbol("mouse-movement"));

    let next = ev.read_char().expect("keypress should remain readable");
    assert_eq!(next, Value::fixnum('a' as i64));
}

#[test]
fn read_char_mouse_move_updates_mouse_position_even_without_track_mouse() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::MouseMove {
        x: 24.0,
        y: 40.0,
        modifiers: crate::keyboard::Modifiers::none(),
        target_frame_id: 0,
    })
    .expect("queue mouse move");
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .expect("queue keypress");
    ev.input_rx = Some(rx);

    let result = ev.read_char().expect("keypress should remain readable");
    assert_eq!(result, Value::fixnum('a' as i64));

    let pixel = crate::emacs_core::builtins::symbols::builtin_mouse_pixel_position(&mut ev, vec![])
        .expect("mouse-pixel-position should succeed");
    if !pixel.is_cons() {
        panic!("expected dotted mouse pixel position");
    };
    let _outer_car = pixel.cons_car();
    let outer_cdr = pixel.cons_cdr();
    if !outer_cdr.is_cons() {
        panic!("expected inner cons");
    };
    let inner_car = outer_cdr.cons_car();
    let inner_cdr = outer_cdr.cons_cdr();
    assert_eq!(inner_car, Value::fixnum(24));
    assert_eq!(inner_cdr, Value::fixnum(40));
}

#[test]
fn input_pending_p_ignores_internal_help_echo_events() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let frame = install_mouse_help_echo_snapshot(&mut ev, "tip");
    crate::emacs_core::builtins::builtin_display_update_for_mouse_movement(
        &mut ev,
        vec![frame, Value::fixnum(12), Value::fixnum(4)],
    )
    .expect("display update should succeed");

    let result = builtin_input_pending_p(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn display_update_for_mouse_movement_shows_help_echo_via_read_char() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let frame = install_mouse_help_echo_snapshot(&mut ev, "tip");
    let (_tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    crate::emacs_core::builtins::builtin_display_update_for_mouse_movement(
        &mut ev,
        vec![frame, Value::fixnum(12), Value::fixnum(4)],
    )
    .expect("display update should succeed");

    let result = ev
        .read_char_with_timeout(Some(Duration::ZERO))
        .expect("read-char should consume help-echo");
    assert!(result.is_none());
    assert_eq!(ev.current_message_text(), Some("tip".to_string()));
}

#[test]
fn display_update_for_mouse_movement_clears_help_echo_when_leaving_region() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let frame = install_mouse_help_echo_snapshot(&mut ev, "tip");
    let (_tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    crate::emacs_core::builtins::builtin_display_update_for_mouse_movement(
        &mut ev,
        vec![frame, Value::fixnum(12), Value::fixnum(4)],
    )
    .expect("display update should succeed");
    ev.read_char_with_timeout(Some(Duration::ZERO))
        .expect("read-char should consume help-echo");
    assert_eq!(ev.current_message_text(), Some("tip".to_string()));

    crate::emacs_core::builtins::builtin_display_update_for_mouse_movement(
        &mut ev,
        vec![frame, Value::fixnum(12), Value::fixnum(40)],
    )
    .expect("display update should succeed");
    ev.read_char_with_timeout(Some(Duration::ZERO))
        .expect("read-char should consume help-echo clear");
    assert_eq!(ev.current_message_text(), None);
}

#[test]
fn display_update_for_mouse_movement_respects_help_echo_inhibit_substitution() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let help = Value::string("\\[save-buffer]");
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut ev,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("help-echo-inhibit-substitution"),
            Value::T,
            help,
        ],
    )
    .expect("put help-echo-inhibit-substitution property");
    let frame = install_mouse_help_echo_snapshot_with_value(&mut ev, help);
    let (_tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    crate::emacs_core::builtins::builtin_display_update_for_mouse_movement(
        &mut ev,
        vec![frame, Value::fixnum(12), Value::fixnum(4)],
    )
    .expect("display update should succeed");

    let result = ev
        .read_char_with_timeout(Some(Duration::ZERO))
        .expect("read-char should consume help-echo");
    assert!(result.is_none());
    assert_eq!(
        ev.current_message_text(),
        Some("\\[save-buffer]".to_string())
    );
}

#[test]
fn display_update_for_mouse_movement_runs_mouse_fixup_before_echo_message() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let frame = install_mouse_help_echo_snapshot(&mut ev, "tip");
    let (_tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    ev.eval_str(
        r#"(fset 'mouse-fixup-help-message
                  (lambda (msg) (concat "fixed:" msg)))"#,
    )
    .expect("install mouse-fixup-help-message");

    crate::emacs_core::builtins::builtin_display_update_for_mouse_movement(
        &mut ev,
        vec![frame, Value::fixnum(12), Value::fixnum(4)],
    )
    .expect("display update should succeed");

    let result = ev
        .read_char_with_timeout(Some(Duration::ZERO))
        .expect("read-char should consume help-echo");
    assert!(result.is_none());
    assert_eq!(ev.current_message_text(), Some("fixed:tip".to_string()));
}

#[test]
fn display_update_for_mouse_movement_runs_mouse_fixup_without_input_receiver() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let frame = install_mouse_help_echo_snapshot(&mut ev, "tip");

    ev.eval_str(
        r#"(fset 'mouse-fixup-help-message
                  (lambda (msg) (concat "fixed:" msg)))"#,
    )
    .expect("install mouse-fixup-help-message");

    crate::emacs_core::builtins::builtin_display_update_for_mouse_movement(
        &mut ev,
        vec![frame, Value::fixnum(12), Value::fixnum(4)],
    )
    .expect("display update should succeed");

    let result = ev
        .read_char_with_timeout(Some(Duration::ZERO))
        .expect("read-char should consume help-echo");
    assert!(result.is_none());
    assert_eq!(ev.current_message_text(), Some("fixed:tip".to_string()));
}

#[test]
fn display_update_for_mouse_movement_preserves_raw_unibyte_help_echo() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let frame = install_mouse_help_echo_snapshot_with_value(&mut ev, raw);
    let (_tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    crate::emacs_core::builtins::builtin_display_update_for_mouse_movement(
        &mut ev,
        vec![frame, Value::fixnum(12), Value::fixnum(4)],
    )
    .expect("display update should succeed");

    let result = ev
        .read_char_with_timeout(Some(Duration::ZERO))
        .expect("read-char should consume help-echo");
    assert!(result.is_none());
    let expected = crate::emacs_core::builtins::lisp_string_to_runtime_string(raw);
    assert_eq!(ev.current_message_text(), Some(expected));
}

#[test]
fn display_update_for_mouse_movement_runs_mouse_fixup_before_show_help_function() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let frame = install_mouse_help_echo_snapshot(&mut ev, "tip");
    let (_tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    ev.eval_str(
        r#"(progn
             (setq show-help-collected nil)
             (fset 'mouse-fixup-help-message
                   (lambda (msg) (concat "fixed:" msg)))
             (setq show-help-function
                   (lambda (msg) (setq show-help-collected msg))))"#,
    )
    .expect("install help fixup/show-help-function");

    crate::emacs_core::builtins::builtin_display_update_for_mouse_movement(
        &mut ev,
        vec![frame, Value::fixnum(12), Value::fixnum(4)],
    )
    .expect("display update should succeed");

    let result = ev
        .read_char_with_timeout(Some(Duration::ZERO))
        .expect("read-char should consume help-echo");
    assert!(result.is_none());
    let value = ev
        .eval_str("show-help-collected")
        .expect("read show-help-collected");
    assert_eq!(value.as_utf8_str(), Some("fixed:tip"));
}

#[test]
fn read_char_mouse_move_sets_help_echo_even_without_track_mouse() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    install_mouse_help_echo_snapshot(&mut ev, "tip");
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::MouseMove {
        x: 12.0,
        y: 4.0,
        modifiers: crate::keyboard::Modifiers::none(),
        target_frame_id: 0,
    })
    .expect("queue mouse move");
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .expect("queue keypress");
    ev.input_rx = Some(rx);

    let result = ev.read_char().expect("keypress should remain readable");
    assert_eq!(result, Value::fixnum('a' as i64));
    assert_eq!(ev.current_message_text(), None);
}

#[test]
fn input_pending_p_check_timers_does_not_run_timer_when_input_is_already_pending() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (setq input-pending-timer-fired nil)
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6))))
             (fset 'input-pending-timer-callback
                   (lambda () (setq input-pending-timer-fired 'done))))"#,
    )
    .expect("install input-pending-p timer setup");
    // A due GNU timer vector on `timer-list` ([TRIGGERED HIGH LOW USECS
    // REPEAT FN ARGS IDLE PSECS integral]) — time 0 = due since the epoch.
    ev.set_variable(
        "timer-list",
        Value::list(vec![Value::vector(vec![
            Value::NIL,
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::NIL,
            Value::symbol("input-pending-timer-callback"),
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
        ])]),
    );

    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('a'),
    ))
    .expect("queue keypress");
    ev.input_rx = Some(rx);

    let result = builtin_input_pending_p(&mut ev, vec![Value::T]).unwrap();
    assert_eq!(result, Value::T);
    assert!(
        ev.eval_symbol("input-pending-timer-fired")
            .expect("timer callback flag")
            .is_nil()
    );

    let event = ev.read_char().expect("keypress should remain available");
    assert_eq!(event, Value::fixnum('a' as i64));
}

#[test]
fn input_pending_p_reloads_event_filter_after_timer_callbacks() {
    fn change_filter_and_queue_event(ctx: &mut Context) -> EvalResult {
        ctx.set_variable("input-pending-p-filter-events", Value::NIL);
        ctx.queue_special_event(Value::list(vec![
            Value::symbol("select-window"),
            Value::fixnum(7),
        ]));
        Ok(Value::NIL)
    }

    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.register_subr(crate::emacs_core::subr::SubrSpec::fixed0(
        "input-pending-filter-timer-callback",
        change_filter_and_queue_event,
    ));
    ev.eval_str(
        r#"(progn
             (setq input-pending-p-filter-events t)
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6)))))"#,
    )
    .expect("install input-pending-p filter timer setup");
    ev.set_variable(
        "timer-list",
        Value::list(vec![Value::vector(vec![
            Value::NIL,
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(0),
            Value::NIL,
            Value::symbol("input-pending-filter-timer-callback"),
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
        ])]),
    );

    let pending = builtin_input_pending_p(&mut ev, vec![Value::T]).unwrap();

    assert_eq!(
        pending,
        Value::T,
        "GNU reads input-pending-p-filter-events after running due timers"
    );
}

#[test]
fn input_pending_p_returns_t_when_quit_flag_is_set() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_quit_flag_value(Value::T);
    let result = builtin_input_pending_p(&mut ev, vec![]).unwrap();
    assert_eq!(result, Value::T);
}

#[test]
fn input_pending_p_rejects_more_than_one_arg() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_input_pending_p(&mut ev, vec![Value::NIL, Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn discard_input_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_discard_input(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn discard_input_clears_unread_command_events() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_discard_input(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::NIL)
    );
}

#[test]
fn discard_input_uses_dynamic_unread_command_events_binding() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = ev
        .eval_str("(let ((unread-command-events (list 98))) (discard-input) unread-command-events)")
        .unwrap();
    assert!(result.is_nil());
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::fixnum(97)]))
    );
}

#[test]
fn discard_input_rejects_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_discard_input(&mut ev, vec![Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn current_input_mode_returns_batch_tuple() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_current_input_mode(&mut ev, vec![]).unwrap();
    assert_eq!(
        result,
        Value::list(vec![Value::T, Value::NIL, Value::T, Value::fixnum(7)])
    );
}

#[test]
fn current_input_mode_rejects_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_current_input_mode(&mut ev, vec![Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn set_input_mode_toggles_interrupt_only() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let _ = builtin_set_input_mode(
        &mut ev,
        vec![Value::NIL, Value::T, Value::NIL, Value::fixnum(65)],
    )
    .unwrap();
    assert_eq!(
        builtin_current_input_mode(&mut ev, vec![]).unwrap(),
        Value::list(vec![Value::NIL, Value::NIL, Value::T, Value::fixnum(65)])
    );

    let _ = builtin_set_input_mode(
        &mut ev,
        vec![Value::symbol("x"), Value::NIL, Value::NIL, Value::NIL],
    )
    .unwrap();
    assert_eq!(
        builtin_current_input_mode(&mut ev, vec![]).unwrap(),
        Value::list(vec![Value::T, Value::NIL, Value::T, Value::fixnum(65)])
    );
}

#[test]
fn set_input_mode_rejects_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let too_few = builtin_set_input_mode(&mut ev, vec![Value::NIL, Value::NIL]);
    assert!(matches!(
        too_few,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));

    let too_many = builtin_set_input_mode(
        &mut ev,
        vec![Value::NIL, Value::NIL, Value::NIL, Value::NIL, Value::NIL],
    );
    assert!(matches!(
        too_many,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn set_input_mode_accepts_three_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_set_input_mode(&mut ev, vec![Value::NIL, Value::T, Value::T])
        .expect("set-input-mode should accept 3 args");
    assert!(result.is_nil());
    assert_eq!(
        builtin_current_input_mode(&mut ev, vec![]).unwrap(),
        Value::list(vec![Value::NIL, Value::NIL, Value::T, Value::fixnum(7)])
    );
}

#[test]
fn set_input_interrupt_mode_toggles_interrupt_state() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let _ = builtin_set_input_interrupt_mode(&mut ev, vec![Value::NIL]).unwrap();
    assert_eq!(
        builtin_current_input_mode(&mut ev, vec![]).unwrap(),
        Value::list(vec![Value::NIL, Value::NIL, Value::T, Value::fixnum(7)])
    );
    let _ = builtin_set_input_interrupt_mode(&mut ev, vec![Value::symbol("x")]).unwrap();
    assert_eq!(
        builtin_current_input_mode(&mut ev, vec![]).unwrap(),
        Value::list(vec![Value::T, Value::NIL, Value::T, Value::fixnum(7)])
    );
}

#[test]
fn set_input_interrupt_mode_rejects_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_set_input_interrupt_mode(&mut ev, vec![Value::NIL, Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn set_input_meta_mode_accepts_one_arg_and_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_set_input_meta_mode(vec![Value::NIL]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn set_input_meta_mode_accepts_optional_terminal_arg() {
    crate::test_utils::init_test_tracing();
    let result = builtin_set_input_meta_mode(vec![Value::symbol("encoded"), Value::NIL]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn set_input_meta_mode_rejects_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let result = builtin_set_input_meta_mode(vec![]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
    let result = builtin_set_input_meta_mode(vec![Value::NIL, Value::NIL, Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn set_output_flow_control_accepts_one_arg_and_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_set_output_flow_control(vec![Value::T]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn set_output_flow_control_accepts_two_args_and_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_set_output_flow_control(vec![Value::T, Value::NIL]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn set_output_flow_control_rejects_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let result = builtin_set_output_flow_control(vec![]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn set_quit_char_accepts_one_arg_and_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_set_quit_char(&mut ev, vec![Value::fixnum(65)]).unwrap();
    assert!(result.is_nil());
    assert_eq!(
        builtin_current_input_mode(&mut ev, vec![]).unwrap(),
        Value::list(vec![Value::T, Value::NIL, Value::T, Value::fixnum(65)])
    );
}

#[test]
fn set_quit_char_rejects_non_ascii_values() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_set_quit_char(&mut ev, vec![Value::fixnum(0o401)]);
    assert!(matches!(result, Err(Flow::Signal(sig)) if sig.symbol_name() == "error"));
}

#[test]
fn set_quit_char_rejects_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_set_quit_char(&mut ev, vec![]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn waiting_for_user_input_p_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_waiting_for_user_input_p(vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn waiting_for_user_input_p_eval_tracks_runtime_flag() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.set_waiting_for_user_input(true);
    let result = builtin_waiting_for_user_input_p_ctx(&mut eval, vec![]).unwrap();
    assert!(result.is_t());
}

#[test]
fn waiting_for_user_input_p_rejects_args() {
    crate::test_utils::init_test_tracing();
    let result = builtin_waiting_for_user_input_p(vec![Value::NIL]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn read_char_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_char(&mut ev, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn read_char_rejects_non_string_prompt() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_char(&mut ev, vec![Value::fixnum(123)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn read_char_consumes_unread_command_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_read_char(&mut ev, vec![]).unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(ev.recent_input_events(), &[Value::fixnum(97)]);
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
}

#[test]
fn read_char_with_seconds_does_not_set_command_keys_when_empty() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result =
        builtin_read_char(&mut ev, vec![Value::NIL, Value::NIL, Value::fixnum(0)]).unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(ev.read_command_keys(), &[]);
}

#[test]
fn read_char_with_nil_seconds_sets_command_keys_when_empty() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_read_char(&mut ev, vec![Value::NIL, Value::NIL, Value::NIL]).unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
}

#[test]
fn read_char_with_interactive_timeout_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);

    let start = std::time::Instant::now();
    let result = builtin_read_char(
        &mut ev,
        vec![Value::NIL, Value::NIL, Value::make_float(0.01)],
    )
    .unwrap();
    drop(tx);

    assert!(result.is_nil());
    assert!(start.elapsed() < std::time::Duration::from_millis(250));
}

#[test]
fn read_char_with_timeout_waits_without_input_source() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let start = std::time::Instant::now();
    let result = builtin_read_char(
        &mut ev,
        vec![Value::NIL, Value::NIL, Value::make_float(0.02)],
    )
    .unwrap();

    assert!(result.is_nil());
    assert!(start.elapsed() >= std::time::Duration::from_millis(10));
    assert!(start.elapsed() < std::time::Duration::from_millis(500));
}

#[test]
fn read_char_preserves_existing_command_keys_context() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_read_command_keys(vec![Value::fixnum(97)]);
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(98)]),
    );
    let result =
        builtin_read_char(&mut ev, vec![Value::NIL, Value::NIL, Value::fixnum(0)]).unwrap();
    assert_eq!(result.as_int(), Some(98));
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
}

#[test]
fn read_char_host_quit_char_returns_event_and_sets_quit_flag() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char_with_mods('g', crate::keyboard::Modifiers::ctrl()),
    ))
    .expect("queue C-g");
    ev.input_rx = Some(rx);

    let result = builtin_read_char(&mut ev, vec![]).unwrap();
    assert_eq!(result, Value::fixnum(7));
    assert_eq!(ev.quit_flag_value(), Value::T);
}

#[test]
fn read_char_signals_error_on_non_character_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::symbol("foo")]),
    );
    let result = builtin_read_char(&mut ev, vec![]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "error"
                && sig.data == vec![Value::string("Non-character input-event")]
    ));
    assert_eq!(ev.recent_input_events(), &[Value::symbol("foo")]);
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::symbol("foo")]))
    );
}

#[test]
fn read_char_non_character_truncates_unread_tail_to_offending_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::symbol("foo"), Value::fixnum(97)]),
    );
    let result = builtin_read_char(&mut ev, vec![]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "error"
                && sig.data == vec![Value::string("Non-character input-event")]
    ));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::symbol("foo")]))
    );
    assert_eq!(ev.recent_input_events(), &[Value::symbol("foo")]);
}

#[test]
fn read_char_consumes_character_event_and_preserves_tail() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97), Value::symbol("foo")]),
    );
    let result = builtin_read_char(&mut ev, vec![]).unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::symbol("foo")]))
    );
}

#[test]
fn read_char_rejects_more_than_three_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_char(
        &mut ev,
        vec![
            Value::string("key: "),
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn read_key_consumes_unread_command_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_read_key(&mut ev, vec![]).unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
}

#[test]
fn read_key_rejects_non_string_prompt() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_key(&mut ev, vec![Value::fixnum(123)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-type-argument"
    ));
}

#[test]
fn read_key_accepts_second_optional_arg() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_read_key(&mut ev, vec![Value::string("key: "), Value::fixnum(1)]).unwrap();
    assert_eq!(result.as_int(), Some(97));
}

#[test]
fn read_key_rejects_more_than_two_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_key(
        &mut ev,
        vec![Value::string("key: "), Value::NIL, Value::fixnum(123)],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn read_key_returns_non_integer_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let event = Value::symbol("f");
    ev.obarray
        .set_symbol_value("unread-command-events", Value::list(vec![event]));
    let result = builtin_read_key(&mut ev, vec![Value::string("key: ")]).unwrap();
    assert_eq!(result, event);
    assert_eq!(ev.read_command_keys(), std::slice::from_ref(&event));
}

#[test]
fn read_key_consumes_unread_character_and_keeps_tail() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let event = Value::symbol("foo");
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![event, Value::fixnum(97)]),
    );
    let result = builtin_read_key(&mut ev, vec![Value::string("key: ")]).unwrap();
    assert_eq!(result, event);
    assert_eq!(ev.read_command_keys(), std::slice::from_ref(&event));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::fixnum(97)]))
    );
}

#[test]
fn read_key_consumes_character_event_and_preserves_tail() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let event = Value::symbol("foo");
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97), event]),
    );
    let result = builtin_read_key(&mut ev, vec![Value::string("key: ")]).unwrap();
    assert_eq!(result.as_int(), Some(97));
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![event]))
    );
}

fn due_gnu_timer(callback: &str) -> Value {
    let when = std::time::SystemTime::now()
        .checked_sub(Duration::from_millis(1))
        .unwrap_or(std::time::UNIX_EPOCH)
        .duration_since(std::time::UNIX_EPOCH)
        .expect("timer deadline should not precede unix epoch");
    let secs = when.as_secs() as i64;

    Value::vector(vec![
        Value::NIL,
        Value::fixnum(secs >> 16),
        Value::fixnum(secs & 0xFFFF),
        Value::fixnum(when.subsec_micros() as i64),
        Value::NIL,
        Value::symbol(callback),
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
    ])
}

#[test]
fn read_event_noninteractive_no_input_waits_for_timers() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (condition-case nil
                         (apply (aref timer 5) (aref timer 6))
                       (error nil))))
             (fset 'read-event-timeout
                   (lambda () (throw 'read-event-timeout-tag 'timed-out))))"#,
    )
    .expect("install timer-event-handler and timeout callback");
    ev.set_variable(
        "timer-list",
        Value::list(vec![due_gnu_timer("read-event-timeout")]),
    );

    let result = ev.eval_str(
        r#"(catch 'read-event-timeout-tag
             (read-event)
             'read-returned)"#,
    );

    assert_eq!(
        crate::emacs_core::format_eval_result(&result),
        "OK timed-out"
    );
}

#[test]
fn read_key_sequence_noninteractive_no_input_waits_for_timers() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (condition-case nil
                         (apply (aref timer 5) (aref timer 6))
                       (error nil))))
             (fset 'read-key-sequence-timeout
                   (lambda () (throw 'read-key-sequence-timeout-tag 'timed-out))))"#,
    )
    .expect("install timer-event-handler and timeout callback");
    ev.set_variable(
        "timer-list",
        Value::list(vec![due_gnu_timer("read-key-sequence-timeout")]),
    );

    let result = ev.eval_str(
        r#"(catch 'read-key-sequence-timeout-tag
             (read-key-sequence "key: ")
             'read-returned)"#,
    );

    assert_eq!(
        crate::emacs_core::format_eval_result(&result),
        "OK timed-out"
    );
}

#[test]
fn read_key_sequence_consumes_unread_command_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_read_key_sequence(&mut ev, vec![Value::string("key: ")]).unwrap();
    assert!(result.is_string() && result.as_utf8_str() == Some("a"));
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
}

#[test]
fn read_key_sequence_consumes_non_character_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let event = Value::symbol("f");
    ev.obarray
        .set_symbol_value("unread-command-events", Value::list(vec![event]));
    let result = builtin_read_key_sequence(&mut ev, vec![Value::string("key: ")]).unwrap();
    match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = result.as_vector_data().unwrap().clone();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0], event);
        }
        other => panic!("expected vector event payload, got {other:?}"),
    }
    assert_eq!(ev.read_command_keys(), std::slice::from_ref(&event));
}

#[test]
fn read_key_sequence_consumes_non_character_event_and_preserves_tail() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let event = Value::symbol("foo");
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![event, Value::fixnum(97)]),
    );
    let result = builtin_read_key_sequence(&mut ev, vec![Value::string("key: ")]).unwrap();
    match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = result.as_vector_data().unwrap().clone();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0], event);
        }
        other => panic!("expected vector event payload, got {other:?}"),
    }
    assert_eq!(ev.read_command_keys(), std::slice::from_ref(&event));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::fixnum(97)]))
    );
}

#[test]
fn read_key_sequence_consumes_character_and_preserves_tail() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let event = Value::symbol("foo");
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97), event]),
    );
    let result = builtin_read_key_sequence(&mut ev, vec![Value::string("key: ")]).unwrap();
    assert!(result.is_string() && result.as_utf8_str() == Some("a"));
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![event]))
    );
}

#[test]
fn read_key_sequence_accepts_nil_prompt() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_read_key_sequence(&mut ev, vec![Value::NIL]).unwrap();
    assert!(result.is_string() && result.as_utf8_str() == Some("a"));
}

#[test]
fn read_key_sequence_treats_host_quit_char_as_ordinary_input() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char_with_mods('g', crate::keyboard::Modifiers::ctrl()),
    ))
    .expect("queue C-g");
    ev.input_rx = Some(rx);

    let result = builtin_read_key_sequence(&mut ev, vec![Value::string("key: ")]).unwrap();
    assert!(result.is_string() && result.as_utf8_str() == Some("\u{7}"));
    assert!(ev.quit_flag_value().is_nil());
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(7)]);
}

#[test]
fn read_key_sequence_rejects_more_than_six_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_read_key_sequence(
        &mut ev,
        vec![
            Value::string("key: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn read_key_sequence_vector_noninteractive_no_input_waits_for_timers() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.eval_str(
        r#"(progn
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (condition-case nil
                         (apply (aref timer 5) (aref timer 6))
                       (error nil))))
             (fset 'read-key-sequence-vector-timeout
                   (lambda () (throw 'read-key-sequence-vector-timeout-tag 'timed-out))))"#,
    )
    .expect("install timer-event-handler and timeout callback");
    ev.set_variable(
        "timer-list",
        Value::list(vec![due_gnu_timer("read-key-sequence-vector-timeout")]),
    );

    let result = ev.eval_str(
        r#"(catch 'read-key-sequence-vector-timeout-tag
             (read-key-sequence-vector "key: ")
             'read-returned)"#,
    );

    assert_eq!(
        crate::emacs_core::format_eval_result(&result),
        "OK timed-out"
    );
}

#[derive(Default)]
struct BlockingKeySequenceRuntime {
    unread: VecDeque<Value>,
    read_command_keys: Vec<Value>,
    blocking_keys: Vec<Value>,
    last_options: Option<crate::keyboard::ReadKeySequenceOptions>,
}

impl KeyboardInputRuntime for BlockingKeySequenceRuntime {
    fn pop_unread_command_event(&mut self) -> Option<Value> {
        self.unread.pop_front()
    }

    fn peek_unread_command_event(&self) -> Option<Value> {
        self.unread.front().copied()
    }

    fn replace_unread_command_event_with_singleton(&mut self, event: Value) {
        self.unread.clear();
        self.unread.push_back(event);
    }

    fn record_input_event(&mut self, _event: Value) {}

    fn record_nonmenu_input_event(&mut self, _event: Value) {}

    fn set_read_command_keys(&mut self, keys: Vec<Value>) {
        self.read_command_keys = keys;
    }

    fn clear_read_command_keys(&mut self) {
        self.read_command_keys.clear();
    }

    fn read_command_keys(&self) -> &[Value] {
        &self.read_command_keys
    }

    fn has_input_receiver(&self) -> bool {
        true
    }

    fn is_executing_keyboard_macro(&self) -> bool {
        false
    }

    fn read_char_blocking(&mut self) -> Result<Value, Flow> {
        unreachable!("read-char should not be used in this test runtime")
    }

    fn read_char_with_timeout(
        &mut self,
        _timeout: Option<std::time::Duration>,
        _tty_input_decoding: crate::keyboard::TtyInputDecoding,
    ) -> Result<Option<Value>, Flow> {
        unreachable!("read-char should not be used in this test runtime")
    }

    fn read_key_sequence_blocking(
        &mut self,
        options: crate::keyboard::ReadKeySequenceOptions,
    ) -> Result<(Vec<Value>, Value), Flow> {
        self.last_options = Some(options);
        Ok((self.blocking_keys.clone(), Value::NIL))
    }

    fn symbol_value_or_nil(&self, _name: &str) -> Value {
        Value::NIL
    }
}

#[test]
fn read_key_sequence_vector_interactive_runtime_returns_blocking_sequence() {
    crate::test_utils::init_test_tracing();
    let mut runtime = BlockingKeySequenceRuntime {
        blocking_keys: vec![Value::fixnum(97), Value::symbol("f1")],
        ..Default::default()
    };
    let result = finish_read_key_sequence_vector_interactive_in_runtime(
        &mut runtime,
        crate::keyboard::ReadKeySequenceOptions::default(),
    )
    .expect("vector read");
    assert_eq!(
        result,
        Value::vector(vec![Value::fixnum(97), Value::symbol("f1")])
    );
}

#[test]
fn read_key_sequence_interactive_runtime_passes_prompt_options() {
    crate::test_utils::init_test_tracing();
    let mut runtime = BlockingKeySequenceRuntime {
        blocking_keys: vec![Value::fixnum(97)],
        ..Default::default()
    };
    let result = finish_read_key_sequence_interactive_in_runtime(
        &mut runtime,
        crate::keyboard::ReadKeySequenceOptions::new(Value::string("Prompt> "), false, true, true),
    )
    .expect("interactive read");
    assert_eq!(result, Value::string("a"));
    assert_eq!(
        runtime.last_options,
        Some(crate::keyboard::ReadKeySequenceOptions::new(
            Value::string("Prompt> "),
            false,
            true,
            true,
        ))
    );
}

#[test]
fn read_key_sequence_vector_consumes_unread_command_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_read_key_sequence_vector(&mut ev, vec![Value::string("key: ")]).unwrap();
    match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = result.as_vector_data().unwrap().clone();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].as_int(), Some(97));
        }
        other => panic!("expected vector, got {other:?}"),
    }
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
}

#[test]
fn read_key_sequence_vector_consumes_non_character_event() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let event = Value::symbol("x");
    ev.obarray
        .set_symbol_value("unread-command-events", Value::list(vec![event]));
    let result = builtin_read_key_sequence_vector(&mut ev, vec![Value::string("key: ")]).unwrap();
    match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = result.as_vector_data().unwrap().clone();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0], event);
        }
        other => panic!("expected vector event payload, got {other:?}"),
    }
    assert_eq!(ev.read_command_keys(), std::slice::from_ref(&event));
}

#[test]
fn read_key_sequence_vector_consumes_non_character_event_and_preserves_tail() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let event = Value::symbol("bar");
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![event, Value::fixnum(97)]),
    );
    let result = builtin_read_key_sequence_vector(&mut ev, vec![Value::string("key: ")]).unwrap();
    match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = result.as_vector_data().unwrap().clone();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0], event);
        }
        other => panic!("expected vector, got {other:?}"),
    }
    assert_eq!(ev.read_command_keys(), std::slice::from_ref(&event));
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![Value::fixnum(97)]))
    );
}

#[test]
fn read_key_sequence_vector_consumes_character_and_preserves_tail() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let event = Value::symbol("bar");
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97), event]),
    );
    let result = builtin_read_key_sequence_vector(&mut ev, vec![Value::string("key: ")]).unwrap();
    match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = result.as_vector_data().unwrap().clone();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].as_int(), Some(97));
        }
        other => panic!("expected vector, got {other:?}"),
    }
    assert_eq!(ev.read_command_keys(), &[Value::fixnum(97)]);
    assert_eq!(
        ev.obarray.symbol_value("unread-command-events"),
        Some(&Value::list(vec![event]))
    );
}

#[test]
fn read_key_sequence_vector_accepts_nil_prompt() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_read_key_sequence_vector(&mut ev, vec![Value::NIL]).unwrap();
    match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = result.as_vector_data().unwrap().clone();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].as_int(), Some(97));
        }
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn read_key_sequence_vector_blocks_for_interactive_input_when_receiver_present() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char('!'),
    ))
    .expect("queue !");
    drop(tx);
    ev.input_rx = Some(rx);

    let result = builtin_read_key_sequence_vector(&mut ev, vec![Value::string("key: ")])
        .expect("interactive read-key-sequence-vector should block for input");
    match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = result.as_vector_data().unwrap().clone();
            assert_eq!(items, vec![Value::fixnum('!' as i64)]);
        }
        other => panic!("expected vector, got {other:?}"),
    }
    assert_eq!(ev.read_command_keys(), &[Value::fixnum('!' as i64)]);
}

#[test]
fn read_key_sequence_vector_rejects_more_than_six_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.obarray.set_symbol_value(
        "unread-command-events",
        Value::list(vec![Value::fixnum(97)]),
    );
    let result = builtin_read_key_sequence_vector(
        &mut ev,
        vec![
            Value::string("key: "),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

// ===================================================================
// with-output-to-string tests
// ===================================================================

#[test]
fn with_output_to_string_captures_print_output() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let result = ev
        .eval_str(r#"(with-output-to-string (princ "a") (prin1 '(1 2)) (print "x"))"#)
        .unwrap();
    assert_eq!(result.as_utf8_str(), Some("a(1 2)\n\"x\"\n"));
}

#[test]
fn with_output_to_string_keeps_explicit_destination_working() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let result = ev
        .eval_str(
            r#"(with-temp-buffer
             (let ((buf (current-buffer)))
               (with-output-to-string
                 (princ "captured")
                 (princ " to-buf" buf))
               (buffer-string)))"#,
        )
        .unwrap();
    assert_eq!(result.as_utf8_str(), Some(" to-buf"));
}

// ===================================================================
// Edge case / integration tests
// ===================================================================

#[test]
fn read_from_string_nested_list() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("((a b) (c d))")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            assert!(pair_car.is_cons());
            assert!(pair_cdr.is_fixnum());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_with_leading_whitespace() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("   42")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            assert!(pair_car.is_fixnum());
            // End position should be 5 (after "   42")
            assert!(pair_cdr.is_fixnum());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_negative_number() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("-7")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            assert!(&pair_car.is_fixnum());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_wrong_type() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::fixnum(42)]);
    assert!(result.is_err());
}

#[test]
fn read_from_string_no_args() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![]);
    assert!(result.is_err());
}

#[test]
fn read_from_string_hash_syntax() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("#xff")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            assert!(pair_car.is_fixnum());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_c_style_hex_token_is_symbol_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let result = builtin_read_from_string(&mut ev, vec![Value::string("0xc0")]).unwrap();
    assert_eq!(result.cons_cdr(), Value::fixnum(4));
    let symbol = result.cons_car();
    assert_eq!(
        symbol.as_symbol_name(),
        Some("0xc0"),
        "GNU reads ordinary 0x-prefixed tokens as symbols, got {symbol:?}"
    );

    let hash_x = builtin_read_from_string(&mut ev, vec![Value::string("#xC0")]).unwrap();
    assert_eq!(hash_x.cons_car(), Value::fixnum(192));

    let hash_radix = builtin_read_from_string(&mut ev, vec![Value::string("#16rC0")]).unwrap();
    assert_eq!(hash_radix.cons_car(), Value::fixnum(192));
}

#[test]
fn read_from_string_hash_space_payload_matches_oracle() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("# ")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(sig.data, vec![Value::string("# ")]);
        }
        other => panic!("expected invalid-read-syntax, got {other:?}"),
    }
}

#[test]
fn read_from_string_hash_unknown_dispatch_payload_matches_oracle() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let result = builtin_read_from_string(&mut ev, vec![Value::string("#a")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(sig.data, vec![Value::string("#a")]);
        }
        other => panic!("expected invalid-read-syntax, got {other:?}"),
    }

    let result = builtin_read_from_string(&mut ev, vec![Value::string("#0")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(sig.data, vec![Value::string("#0")]);
        }
        other => panic!("expected invalid-read-syntax, got {other:?}"),
    }
}

#[test]
fn read_from_string_hash_radix_missing_digits_payload_matches_oracle() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("#x")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(sig.data, vec![Value::string("integer, radix 16")]);
        }
        other => panic!("expected invalid-read-syntax, got {other:?}"),
    }
}

#[test]
fn read_from_string_hash_radix_n_syntax_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let result = builtin_read_from_string(&mut ev, vec![Value::string("#36rZ")])
        .expect("#36rZ should read as radix-36 integer");
    assert_eq!(result.cons_car(), Value::fixnum(35));

    let result = builtin_read_from_string(&mut ev, vec![Value::string("#2r2")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(sig.data, vec![Value::string("integer, radix 2")]);
        }
        other => panic!("expected invalid-read-syntax, got {other:?}"),
    }
}

#[test]
fn read_from_string_hash_radix_trailing_invalid_digit_errors_like_gnu() {
    // GNU `read_integer' (src/lread.c:2944) keeps consuming alphanumeric
    // characters; an alphanumeric that is not a valid digit for the radix
    // (e.g. `g' after `#x1') poisons the whole token, so `#x1g' signals
    // `(invalid-read-syntax "integer, radix 16")' rather than reading `1'
    // and leaving `g' for the next form (oracle test cx27).
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    for (src, radix) in [("#x1g", 16), ("#o18", 8), ("#b12", 2)] {
        match builtin_read_from_string(&mut ev, vec![Value::string(src)]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "invalid-read-syntax", "src={src}");
                assert_eq!(
                    sig.data,
                    vec![Value::string(format!("integer, radix {radix}"))],
                    "src={src}"
                );
            }
            other => panic!("expected invalid-read-syntax for {src}, got {other:?}"),
        }
    }

    // A valid token followed by a non-alphanumeric terminator still reads fine.
    let result = builtin_read_from_string(&mut ev, vec![Value::string("#xff)")])
        .expect("#xff) should read as 255");
    assert_eq!(result.cons_car(), Value::fixnum(255));
}

#[test]
fn read_from_string_circular_cons_label_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let result = builtin_read_from_string(&mut ev, vec![Value::string("#1=(a . #1#)")])
        .expect("circular cons read label should read");
    let value = result.cons_car();
    assert!(value.is_cons());
    assert_eq!(value.cons_car(), Value::symbol("a"));
    assert_eq!(value.cons_cdr(), value);

    let printed = ev
        .eval_str(
            r##"(let ((print-circle t)
                     (x (read "#1=(a . #1#)")))
                 (list (consp x) (eq x (cdr x)) (prin1-to-string x)))"##,
        )
        .expect("circular read/print expression should evaluate");
    assert!(printed.is_cons());
    assert_eq!(printed.cons_car(), Value::T);
    let rest = printed.cons_cdr();
    assert_eq!(rest.cons_car(), Value::T);
    assert_eq!(
        rest.cons_cdr().cons_car().as_utf8_str(),
        Some("#1=(a . #1#)")
    );
}

#[test]
fn read_from_string_read_label_identity_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let list = builtin_read_from_string(&mut ev, vec![Value::string("#1=(nil)")])
        .expect("structurally-equal placeholder must not be treated as self-reference")
        .cons_car();
    assert!(list.is_cons());
    assert!(list.cons_car().is_nil());
    assert!(list.cons_cdr().is_nil());

    let vector = builtin_read_from_string(&mut ev, vec![Value::string("#1=[#1#]")])
        .expect("GNU accepts circular vectors through read-label substitution")
        .cons_car();
    assert!(vector.is_vector());
    let slots = vector.as_vector_data().expect("vector slots");
    assert_eq!(slots.len(), 1);
    assert!(eq_value(&slots[0], &vector));

    let direct_self = builtin_read_from_string(&mut ev, vec![Value::string("#1=#1#")]);
    match direct_self {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(sig.data, vec![Value::string("nonsensical self-reference")]);
        }
        other => panic!("expected invalid-read-syntax, got {other:?}"),
    }
}

#[test]
fn read_from_string_hash_s_without_list_payload_matches_oracle() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("#s")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(sig.data, vec![Value::string("#s")]);
        }
        other => panic!("expected invalid-read-syntax, got {other:?}"),
    }
}

#[test]
fn read_from_string_hash_s_followed_by_non_paren_includes_consumed_char() {
    // GNU `read1` case 's' consumes the next character before checking it, so
    // the `invalid-read-syntax` text includes the offending char.  Verified
    // against the oracle:
    //   (read "#s[foo 1]") => (invalid-read-syntax "#s[")
    //   (read "#s5")       => (invalid-read-syntax "#s5")
    //   (read "#sf")       => (invalid-read-syntax "#sf")
    crate::test_utils::init_test_tracing();
    for (input, expected) in [("#s[foo 1]", "#s["), ("#s5", "#s5"), ("#sf", "#sf")] {
        let mut ev = Context::new();
        let result = builtin_read_from_string(&mut ev, vec![Value::string(input)]);
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "invalid-read-syntax");
                assert_eq!(
                    sig.data,
                    vec![Value::string(expected)],
                    "input {input:?} should report {expected:?}"
                );
            }
            other => panic!("expected invalid-read-syntax for {input:?}, got {other:?}"),
        }
    }
}

#[test]
fn read_from_string_unmatched_close_paren_payload_matches_oracle() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string(")")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(sig.data, vec![Value::string(")")]);
        }
        other => panic!("expected invalid-read-syntax, got {other:?}"),
    }
}

#[test]
fn read_from_string_char_literal_requires_gnu_emacs_delimiter() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("?child")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(sig.data, vec![Value::string("?")]);
        }
        other => panic!("expected invalid-read-syntax, got {other:?}"),
    }
}

#[test]
fn read_from_string_hash_skip_without_length_signals_eof() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let result = builtin_read_from_string(&mut ev, vec![Value::string("#@")]);
    assert!(
        matches!(result, Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file" && sig.data.is_empty())
    );

    let result = builtin_read_from_string(&mut ev, vec![Value::string("#@x")]);
    assert!(
        matches!(result, Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file" && sig.data.is_empty())
    );
}

#[test]
fn read_from_string_hash_skip_with_payload_signals_eof() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let result = builtin_read_from_string(&mut ev, vec![Value::string("#@0x")]);
    assert!(
        matches!(result, Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file" && sig.data.is_empty())
    );

    let result = builtin_read_from_string(&mut ev, vec![Value::string("#@4data42")]);
    assert!(
        matches!(result, Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file" && sig.data.is_empty())
    );
}

#[test]
fn read_from_string_hash_skip_zero_zero_reads_nil_and_skips_to_end() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let result = builtin_read_from_string(&mut ev, vec![Value::string("#@00abc")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            assert!(result.cons_car().is_nil());
            assert_eq!(result.cons_cdr().as_fixnum(), Some(7));
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_hash_dollar_uses_load_file_name() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("load-file-name", Value::string("/tmp/reader-probe.elc"));
    let result = builtin_read_from_string(&mut ev, vec![Value::string("#$")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            assert_eq!(pair_car.as_utf8_str(), Some("/tmp/reader-probe.elc"));
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_hash_dollar_defaults_to_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("#$")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            assert!(pair_car.is_nil());
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_hash_skip_then_hash_dollar_signals_eof() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("load-file-name", Value::string("/tmp/reader-skip.elc"));
    let result = builtin_read_from_string(&mut ev, vec![Value::string("#@4data#$")]);
    assert!(
        matches!(result, Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file" && sig.data.is_empty())
    );
}

#[test]
fn read_from_string_hash_hash_reads_empty_symbol() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("##")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            assert_eq!(pair_car.as_symbol_name(), Some(""));
            assert_eq!(pair_cdr, Value::fixnum(2));
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_escaped_hash_hash_reads_literal_symbol() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("\\#\\#")]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            assert_eq!(pair_car.as_symbol_name(), Some("##"));
            assert_eq!(pair_cdr, Value::fixnum(4));
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_hash_skip_bytes_signals_eof() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let result = builtin_read_from_string(&mut ev, vec![Value::string("#@4data42 rest")]);
    assert!(
        matches!(result, Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file" && sig.data.is_empty())
    );
}

#[test]
fn read_from_string_hash_bracket_end_position() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let input = "#[(x) \"\\bT\\207\" [x] 1 (#$ . 83)] tail";
    let expected_end = input.find(" tail").unwrap() as i64;
    let result = builtin_read_from_string(&mut ev, vec![Value::string(input)]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let _pair_car = result.cons_car();
            let pair_cdr = result.cons_cdr();
            assert_eq!(pair_cdr, Value::fixnum(expected_end));
        }
        _ => panic!("Expected cons"),
    }
}

#[test]
fn read_from_string_hash_table_literal_returns_hash_table() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let input =
        "#s(hash-table size 3 test equal purecopy t unknown-key ignored data (\"a\" 1 \"b\" 2))";
    let result = builtin_read_from_string(&mut ev, vec![Value::string(input)]).unwrap();
    if !result.is_cons() {
        panic!("Expected cons");
    };
    let pair_car = result.cons_car();
    let _pair_cdr = result.cons_cdr();
    if !&pair_car.is_hash_table() {
        panic!("expected hash table object");
    };
    let table = pair_car.as_hash_table().unwrap();
    assert!(matches!(table.test, HashTableTest::Equal));
    // GNU lread.c:hash_table_from_plist ignores the printed `size' field and
    // passes `:size' derived from the number of DATA pairs to make-hash-table.
    assert_eq!(table.size, 2);
    assert_eq!(table.data.len(), 2);
    assert_eq!(table.key_snapshots().count(), 2);
    assert_eq!(
        table.data.get(&HashKey::from_str("a")).copied(),
        Some(Value::fixnum(1))
    );
    assert_eq!(
        table.data.get(&HashKey::from_str("b")).copied(),
        Some(Value::fixnum(2))
    );
}

#[test]
fn read_from_string_hash_table_literal_errors_match_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    let result = builtin_read_from_string(&mut ev, vec![Value::string("#s(hash-table data (a))")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Hash table data length is odd")]
            );
        }
        other => panic!("expected hash table data length error, got {other:?}"),
    }

    let result = builtin_read_from_string(&mut ev, vec![Value::string("#s(hash-table data . a)")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(sig.data, vec![Value::string(".")]);
        }
        other => panic!("expected invalid dotted #s syntax, got {other:?}"),
    }

    let result = builtin_read_from_string(
        &mut ev,
        vec![Value::string("#s(hash-table test bogus data (a 1))")],
    );
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![
                    Value::string("Invalid hash table test"),
                    Value::symbol("bogus")
                ]
            );
        }
        other => panic!("expected invalid hash table test error, got {other:?}"),
    }
}

#[test]
fn read_buffer_hash_table_literal_returns_hash_table() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf_id = ev.buffers.create_buffer(" *reader-hash-table*");
    {
        let buf = ev.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("#s(hash-table size 3 test equal data (\"a\" 1 \"b\" 2))");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }
    let value = builtin_read(&mut ev, vec![Value::make_buffer(buf_id)]).expect("read from buffer");
    if !value.is_hash_table() {
        panic!("expected hash table object");
    };
    let table = value.as_hash_table().unwrap();
    assert!(matches!(table.test, HashTableTest::Equal));
    assert_eq!(table.size, 2);
    assert_eq!(table.data.len(), 2);
    assert_eq!(
        table.data.get(&HashKey::from_str("a")).copied(),
        Some(Value::fixnum(1))
    );
    assert_eq!(
        table.data.get(&HashKey::from_str("b")).copied(),
        Some(Value::fixnum(2))
    );
}

#[test]
fn read_from_buffer_advances_point_across_multiple_forms() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf_id = ev.buffers.create_buffer(" *reader-multi*");
    let source = "(setq reader-first 1)\n(setq reader-second 2)\n";
    {
        let buf = ev.buffers.get_mut(buf_id).expect("buffer");
        buf.insert(source);
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let first = builtin_read(&mut ev, vec![Value::make_buffer(buf_id)]).expect("first form");
    ev.eval_value(&first).expect("first eval");
    let after_first = ev
        .buffers
        .get(buf_id)
        .expect("buffer")
        .point_char_pos()
        .get();
    assert!(after_first > 0, "first read should advance point");

    let second = builtin_read(&mut ev, vec![Value::make_buffer(buf_id)]).expect("second form");
    ev.eval_value(&second).expect("second eval");
    let after_second = ev
        .buffers
        .get(buf_id)
        .expect("buffer")
        .point_char_pos()
        .get();
    assert_eq!(
        after_second,
        source.len() - 1,
        "second read should stop after the form, leaving trailing whitespace unread"
    );

    let eof = builtin_read(&mut ev, vec![Value::make_buffer(buf_id)]);
    assert!(matches!(eof, Err(Flow::Signal(sig)) if sig.symbol_name() == "end-of-file"));
    assert_eq!(
        ev.buffers
            .get(buf_id)
            .expect("buffer")
            .point_char_pos()
            .get(),
        source.len(),
        "EOF read should consume trailing whitespace like GNU Emacs"
    );
    assert_eq!(
        ev.obarray.symbol_value("reader-first").cloned(),
        Some(Value::fixnum(1))
    );
    assert_eq!(
        ev.obarray.symbol_value("reader-second").cloned(),
        Some(Value::fixnum(2))
    );
}

#[test]
fn read_from_unibyte_buffer_preserves_unibyte_string_literals() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf_id = ev.buffers.create_buffer(" *reader-unibyte*");
    {
        let buf = ev.buffers.get_mut(buf_id).expect("buffer");
        buf.set_multibyte_value(false);
        buf.insert_lisp_string(&crate::heap_types::LispString::from_unibyte(vec![
            b'"', 0xFF, b'"',
        ]));
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let value = builtin_read(&mut ev, vec![Value::make_buffer(buf_id)]).expect("read from buffer");
    let text = value
        .as_lisp_string()
        .expect("reader should return a string object");
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF]);
    assert_eq!(
        ev.buffers
            .get(buf_id)
            .expect("buffer")
            .point_emacs_byte_pos()
            .get(),
        3
    );
}

#[test]
fn read_from_unibyte_buffer_preserves_valid_utf8_runs_as_bytes() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf_id = ev.buffers.create_buffer(" *reader-unibyte-utf8-run*");
    {
        let buf = ev.buffers.get_mut(buf_id).expect("buffer");
        buf.set_multibyte_value(false);
        buf.insert_lisp_string(&crate::heap_types::LispString::from_unibyte(vec![
            b'"', 0xCE, 0xBB, b'"',
        ]));
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let value = builtin_read(&mut ev, vec![Value::make_buffer(buf_id)]).expect("read from buffer");
    let text = value
        .as_lisp_string()
        .expect("reader should return a string object");

    assert!(
        !text.is_multibyte(),
        "GNU unibyte buffer sources expose each high byte as BYTE8"
    );
    assert_eq!(text.as_bytes(), &[0xCE, 0xBB]);
    assert_eq!(
        ev.buffers
            .get(buf_id)
            .expect("buffer")
            .point_emacs_byte_pos()
            .get(),
        4
    );
}

#[test]
fn read_from_buffer_preserves_string_literals_during_eval() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf_id = ev.buffers.create_buffer(" *reader-string-eval*");
    {
        let buf = ev.buffers.get_mut(buf_id).expect("buffer");
        buf.insert(r#"(progn (setq reader-string nil) (setq reader-string "abc") reader-string)"#);
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let form = builtin_read(&mut ev, vec![Value::make_buffer(buf_id)]).expect("read form");
    let result = ev.eval_value(&form).expect("eval form");
    assert_eq!(result.as_utf8_str(), Some("abc"));
}

#[test]
fn read_from_buffer_incomplete_list_signals_source_buffer_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf_id = ev.buffers.create_buffer(" *reader-incomplete-list*");
    {
        let buf = ev.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("(progn (list 1 2)");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let result = builtin_read(&mut ev, vec![Value::make_buffer(buf_id)]);
    assert!(matches!(
        result,
        Err(Flow::Signal(sig))
            if sig.symbol_name() == "end-of-file"
                && sig.data == vec![Value::make_buffer(buf_id)]
    ));
}

#[test]
fn read_from_buffer_invalid_read_syntax_reports_line_and_column_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf_id = ev.buffers.create_buffer(" *reader-invalid-syntax*");
    {
        let buf = ev.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("?child");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let result = builtin_read(&mut ev, vec![Value::make_buffer(buf_id)]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(
                sig.data,
                vec![Value::string("?"), Value::fixnum(1), Value::fixnum(2)]
            );
        }
        other => panic!("expected invalid-read-syntax, got {other:?}"),
    }
}

#[test]
fn read_from_buffer_unmatched_close_paren_reports_post_consumption_column_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf_id = ev.buffers.create_buffer(" *reader-invalid-close-paren*");
    {
        let buf = ev.buffers.get_mut(buf_id).expect("buffer");
        buf.insert(")");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let result = builtin_read(&mut ev, vec![Value::make_buffer(buf_id)]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(
                sig.data,
                vec![Value::string(")"), Value::fixnum(1), Value::fixnum(1)]
            );
        }
        other => panic!("expected invalid-read-syntax, got {other:?}"),
    }
}

#[test]
fn read_from_buffer_invalid_hash_dispatch_reports_post_consumption_column_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf_id = ev.buffers.create_buffer(" *reader-invalid-hash-dispatch*");
    {
        let buf = ev.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("#t");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let result = builtin_read(&mut ev, vec![Value::make_buffer(buf_id)]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(
                sig.data,
                vec![Value::string("#t"), Value::fixnum(1), Value::fixnum(2)]
            );
        }
        other => panic!("expected invalid-read-syntax, got {other:?}"),
    }
}

#[test]
fn read_from_buffer_empty_dotted_list_reports_post_dot_column_like_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf_id = ev.buffers.create_buffer(" *reader-invalid-empty-dot*");
    {
        let buf = ev.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("(. 1)");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let result = builtin_read(&mut ev, vec![Value::make_buffer(buf_id)]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-read-syntax");
            assert_eq!(
                sig.data,
                vec![Value::string("."), Value::fixnum(1), Value::fixnum(2)]
            );
        }
        other => panic!("expected invalid-read-syntax, got {other:?}"),
    }
}

#[test]
fn read_from_string_hash_bracket_preserves_vector() {
    crate::test_utils::init_test_tracing();
    // GNU verified: `(type-of (car (read-from-string "#[...]")))` is
    // `byte-code-function`, not `vector`. Mirror GNU here — the
    // bytecode literal reader is supposed to round-trip back to a
    // bytecode object.
    let mut ev = Context::new();
    let input = "#[nil \"\\300\\207\" [0] 1]";
    let result = builtin_read_from_string(&mut ev, vec![Value::string(input)]).unwrap();
    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            assert!(
                pair_car.is_bytecode(),
                "expected byte-code-function, got {:?}",
                pair_car.kind()
            );
        }
        other => panic!("Expected cons from read-from-string, got {other:?}"),
    }
}

#[test]
fn read_from_string_hash_dollar_inside_dotted_pair_uses_load_file_name() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    ev.set_variable("load-file-name", Value::string("/tmp/reader-dotted.elc"));
    let result = builtin_read_from_string(&mut ev, vec![Value::string("(#$ . 83)")]).unwrap();

    match result.kind() {
        ValueKind::Cons => {
            let pair_car = result.cons_car();
            let _pair_cdr = result.cons_cdr();
            if !pair_car.is_cons() {
                panic!("expected dotted pair");
            };
            let data_car = pair_car.cons_car();
            let data_cdr = pair_car.cons_cdr();
            assert_eq!(data_car.as_utf8_str(), Some("/tmp/reader-dotted.elc"));
            assert_eq!(data_cdr.as_int(), Some(83));
        }
        other => panic!("Expected cons from read-from-string, got {other:?}"),
    }
}

/// GNU `string_to_number` (`src/lread.c`) lexes a digit sequence with a single
/// trailing "." (no fractional digits and no exponent) as an INTEGER, not a
/// float. The trailing dot is an integer terminator: `(read "5.")` => 5, and a
/// magnitude that overflows a fixnum (`"1000000000000000000000."`) becomes a
/// bignum integer rather than `1e+21`.
///   GNU oracle: (list (read "5.") (type-of (read "5."))) => (5 integer)
///               (read "1000000000000000000000.")        => 1000000000000000000000
#[test]
fn read_from_string_trailing_dot_integer_is_not_a_float() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    // "5." is the integer 5, not the float 5.0.
    let result = builtin_read_from_string(&mut ev, vec![Value::string("5.")]).unwrap();
    let value = result.cons_car();
    assert!(
        value.is_integer() && !value.is_float(),
        "GNU reads \"5.\" as an integer, got {value:?}"
    );
    assert_eq!(value.as_int(), Some(5));

    // Signed trailing-dot integers: GNU reads "-5." => -5, "+5." => 5.
    let neg = builtin_read_from_string(&mut ev, vec![Value::string("-5.")]).unwrap();
    assert!(
        neg.cons_car().is_integer() && !neg.cons_car().is_float(),
        "GNU reads \"-5.\" as an integer, got {:?}",
        neg.cons_car()
    );
    assert_eq!(neg.cons_car().as_int(), Some(-5));
    let pos = builtin_read_from_string(&mut ev, vec![Value::string("+5.")]).unwrap();
    assert!(
        pos.cons_car().is_integer() && !pos.cons_car().is_float(),
        "GNU reads \"+5.\" as an integer, got {:?}",
        pos.cons_car()
    );
    assert_eq!(pos.cons_car().as_int(), Some(5));

    // A magnitude wider than a fixnum becomes a bignum integer, not 1e+21.
    let big =
        builtin_read_from_string(&mut ev, vec![Value::string("1000000000000000000000.")]).unwrap();
    let big_value = big.cons_car();
    assert!(
        big_value.is_integer() && !big_value.is_float(),
        "GNU reads \"1000000000000000000000.\" as a bignum integer, got {big_value:?}"
    );
    assert_eq!(print_value(&big_value), "1000000000000000000000");

    // Sanity: genuine floats still read as floats.
    let real_float = builtin_read_from_string(&mut ev, vec![Value::string("5.0")]).unwrap();
    assert!(
        real_float.cons_car().is_float(),
        "\"5.0\" must still read as a float"
    );
    let exp_float = builtin_read_from_string(&mut ev, vec![Value::string("5e0")]).unwrap();
    assert!(
        exp_float.cons_car().is_float(),
        "\"5e0\" must still read as a float"
    );
    let dot_exp_float = builtin_read_from_string(&mut ev, vec![Value::string("5.e0")]).unwrap();
    assert!(
        dot_exp_float.cons_car().is_float(),
        "\"5.e0\" must still read as a float"
    );
    let lead_dot_float = builtin_read_from_string(&mut ev, vec![Value::string(".5")]).unwrap();
    assert!(
        lead_dot_float.cons_car().is_float(),
        "\".5\" must still read as a float"
    );
}

/// Inside a string literal, GNU's reader drops `\<SPC>` and `\<LF>` entirely
/// (whitespace/line continuation): `read_string_literal` in `src/lread.c` has
///   case ' ': case '\n': ... continue;
/// `\<TAB>` and `\<CR>` are NOT dropped — they fall through to
/// `read_char_escape` and keep the literal char.
///   GNU oracle: (length (read "\"a\\ b\"")) => 2   ;; backslash-space
///               (length (read "\"a\\\nb\"")) => 2  ;; backslash-newline
///               (length (read "\"a\\\tb\"")) => 3  ;; backslash-tab kept
#[test]
fn read_from_string_backslash_space_is_a_continuation_escape() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();

    // "a\ b" => "ab" (the backslash-space is dropped) => length 2.
    let space = builtin_read_from_string(&mut ev, vec![Value::string("\"a\\ b\"")]).unwrap();
    let space_str = space.cons_car();
    assert_eq!(
        space_str.as_utf8_str(),
        Some("ab"),
        "GNU drops backslash-space in a string literal"
    );

    // Backslash-newline is also dropped (pre-existing behavior, kept).
    let newline = builtin_read_from_string(&mut ev, vec![Value::string("\"a\\\nb\"")]).unwrap();
    assert_eq!(
        newline.cons_car().as_utf8_str(),
        Some("ab"),
        "GNU drops backslash-newline in a string literal"
    );

    // Backslash-tab is NOT dropped by GNU: the literal tab is kept => length 3.
    let tab = builtin_read_from_string(&mut ev, vec![Value::string("\"a\\\tb\"")]).unwrap();
    assert_eq!(
        tab.cons_car().as_utf8_str(),
        Some("a\tb"),
        "GNU keeps backslash-tab (length 3) in a string literal"
    );
}

/// GNU `read_minibuf` clears the echo area on ENTRY, not on exit.
///
/// `clear_message (1, 1)` sits at `src/minibuf.c:894`, after the prompt and any
/// initial input have been installed and immediately before
/// `bset_keymap (current_buffer, map)` and `run_hook (Qminibuffer_setup_hook)`
/// (`src/minibuf.c:895`, `:900`). Both echo-area slots go with it -- the current
/// message (`echo_area_buffer[0]`) and the last displayed one
/// (`echo_area_buffer[1]`) -- so by the time any Lisp in the session runs,
/// `current-message' is nil.
///
/// Measured on GNU Emacs 31.0.90 under a pty at 80x24
/// (`scripts/l217-minibuffer-message-probe.el'), four sessions, every one:
///   in-setup-hook current-message=nil
/// This port carried the standing message into the hook in all four. The
/// message did disappear by the time the session returned, which is why ledger
/// 215 -- whose probe left the minibuffer by throwing out of the setup hook --
/// saw a survivor and this test would not have: the divergence is at entry.
#[test]
fn read_from_minibuffer_clears_the_echo_message_on_entry_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(crate::keyboard::InputEvent::key_press(
        crate::keyboard::KeyEvent::char(' '),
    ))
    .expect("queue minibuffer exit key");
    drop(tx);
    ev.input_rx = Some(rx);

    // `message' writes to stderr rather than the echo area in a batch context
    // (GNU `message3_frame_nolog' takes the `FRAME_INITIAL_P' arm,
    // src/xdisp.c:12621), so install the standing message directly -- this is
    // the state a real session is in when the user invokes a command that
    // reads from the minibuffer.
    ev.set_current_message(Some(crate::heap_types::LispString::from_utf8(
        "l217 standing message",
    )));
    assert_eq!(
        ev.current_message_text().as_deref(),
        Some("l217 standing message"),
        "the standing message must be installed before the session starts"
    );

    let result = ev
        .eval_str(
            r#"(let ((map (make-sparse-keymap))
                  (seen 'unset))
              (define-key map " " #'exit-minibuffer)
              (let ((minibuffer-setup-hook (list (lambda () (setq seen (current-message)))))
                    (minibuffer-exit-hook nil))
                (read-from-minibuffer "P: " nil map))
              (list seen (current-message)))"#,
        )
        .expect("read-from-minibuffer should exit normally");

    assert_eq!(
        format!("{result}"),
        "(nil nil)",
        "GNU clear_message (1, 1) at src/minibuf.c:894 runs BEFORE \
         minibuffer-setup-hook, so the standing message must already be gone \
         inside the hook"
    );
}
