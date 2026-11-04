use super::*;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::{ValueKind, VecLikeType};

fn expect_vector_ints(value: Value) -> Vec<i64> {
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let guard = value.as_vector_data().unwrap().clone();
            guard
                .iter()
                .map(|item| match item.kind() {
                    ValueKind::Fixnum(n) => n,
                    other => panic!("expected int in vector, got {other:?}"),
                })
                .collect()
        }
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn empty_kbd_string_returns_empty_string() {
    crate::test_utils::init_test_tracing();
    let result = parse_kbd_string("   ").expect("parse should succeed");
    assert_eq!(result.as_utf8_str(), Some(""));
}

#[test]
fn kbd_ctrl_char_returns_string() {
    crate::test_utils::init_test_tracing();
    let result = parse_kbd_string("C-a").expect("parse should succeed");
    assert_eq!(result.as_utf8_str(), Some("\u{1}"));
}

#[test]
fn kbd_ctrl_sequence_returns_string() {
    crate::test_utils::init_test_tracing();
    let result = parse_kbd_string("C-x C-f").expect("parse should succeed");
    assert_eq!(result.as_utf8_str(), Some("\u{18}\u{6}"));
}

#[test]
fn kbd_meta_char_returns_vector() {
    crate::test_utils::init_test_tracing();
    let result = parse_kbd_string("M-x").expect("parse should succeed");
    assert_eq!(expect_vector_ints(result), vec![134_217_848]);
}

#[test]
fn kbd_ctrl_meta_char_returns_vector() {
    crate::test_utils::init_test_tracing();
    let result = parse_kbd_string("C-M-a").expect("parse should succeed");
    assert_eq!(expect_vector_ints(result), vec![134_217_729]);
}

#[test]
fn kbd_named_keys_without_modifiers_return_chars() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        parse_kbd_string("NUL").expect("NUL parse").as_utf8_str(),
        Some("\0")
    );
    assert_eq!(
        parse_kbd_string("LFD").expect("LFD parse").as_utf8_str(),
        Some("\n")
    );
    assert_eq!(
        parse_kbd_string("RET").expect("RET parse").as_utf8_str(),
        Some("\r")
    );
    assert_eq!(
        parse_kbd_string("TAB").expect("TAB parse").as_utf8_str(),
        Some("\t")
    );
    assert_eq!(
        parse_kbd_string("ESC").expect("ESC parse").as_utf8_str(),
        Some("\u{1b}")
    );
    assert_eq!(
        parse_kbd_string("DEL").expect("DEL parse").as_utf8_str(),
        Some("\u{7f}")
    );
}

#[test]
fn kbd_named_keys_with_modifiers_return_modifier_encoded_ints() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        expect_vector_ints(parse_kbd_string("C-NUL").expect("C-NUL parse")),
        vec![67_108_864]
    );
    assert_eq!(
        expect_vector_ints(parse_kbd_string("C-LFD").expect("C-LFD parse")),
        vec![67_108_874]
    );
    assert_eq!(
        expect_vector_ints(parse_kbd_string("C-RET").expect("C-RET parse")),
        vec![67_108_877]
    );
    assert_eq!(
        expect_vector_ints(parse_kbd_string("C-SPC").expect("C-SPC parse")),
        vec![67_108_896]
    );
}

#[test]
fn event_modifier_domain_matches_gnu_names_and_kbd_prefixes() {
    crate::test_utils::init_test_tracing();
    let cases = [
        (EventModifier::Control, "control", "C-"),
        (EventModifier::Meta, "meta", "M-"),
        (EventModifier::Shift, "shift", "S-"),
        (EventModifier::Super, "super", "s-"),
        (EventModifier::Hyper, "hyper", "H-"),
        (EventModifier::Alt, "alt", "A-"),
    ];
    for (modifier, symbol, prefix) in cases {
        assert_eq!(EventModifier::from_lispy_symbol(symbol), Some(modifier));
        assert_eq!(modifier.lispy_symbol(), symbol);
        assert_eq!(modifier.kbd_prefix(), prefix);
    }
    assert_eq!(EventModifier::from_lispy_symbol("ctrl"), None);
    assert_eq!(EventModifier::from_lispy_symbol("command"), None);
}

#[test]
fn kbd_plain_multi_char_token_expands_into_plain_string() {
    crate::test_utils::init_test_tracing();
    let result = parse_kbd_string("f1").expect("parse should succeed");
    assert_eq!(result.as_utf8_str(), Some("f1"));
}

#[test]
fn kbd_mixed_sequence_returns_vector_with_plain_char_codes() {
    crate::test_utils::init_test_tracing();
    let result = parse_kbd_string("a M-b").expect("parse should succeed");
    assert_eq!(expect_vector_ints(result), vec![97, 134_217_826]);
}

#[test]
fn kbd_angle_events_return_symbols() {
    crate::test_utils::init_test_tracing();
    let result = parse_kbd_string("<f1>").expect("parse should succeed");
    match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let guard = result.as_vector_data().unwrap().clone();
            assert_eq!(guard.len(), 1);
            assert_eq!(guard[0], Value::symbol("f1"));
        }
        other => panic!("expected vector, got {other:?}"),
    }

    let result = parse_kbd_string("C-<f1>").expect("parse should succeed");
    match result.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let guard = result.as_vector_data().unwrap().clone();
            assert_eq!(guard.len(), 1);
            assert_eq!(guard[0], Value::symbol("C-f1"));
        }
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn kbd_modifier_plus_multi_char_token_signals_error() {
    crate::test_utils::init_test_tracing();
    let err = parse_kbd_string("C-f1").expect_err("C-f1 should fail");
    assert_eq!(err, "C- must prefix a single character, not f1");
}

#[test]
fn kbd_modifier_chain_uses_consumed_prefix_in_error() {
    crate::test_utils::init_test_tracing();
    let err = parse_kbd_string("C-M-BS").expect_err("C-M-BS should fail");
    assert_eq!(err, "C-M- must prefix a single character, not BS");
}

#[test]
fn key_events_from_designator_accepts_kbd_string_and_vector() {
    crate::test_utils::init_test_tracing();
    // Raw strings: each character is a key event.
    // For meta-x, use Emacs unibyte encoding: 'x' | 0x80 = 0xF8
    let from_string =
        key_events_from_designator(&Value::string("\u{00f8}")).expect("decode string");
    assert_eq!(
        from_string,
        vec![KeyEvent::Char {
            code: 'x',
            ctrl: false,
            meta: true,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }]
    );

    let from_vector = key_events_from_designator(&Value::vector(vec![Value::fixnum(134_217_848)]))
        .expect("decode vector int");
    assert_eq!(from_vector, from_string);
}

#[test]
fn key_events_from_designator_preserves_raw_unibyte_meta_bytes() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xF6]));
    let events = key_events_from_designator(&raw).expect("decode raw unibyte meta-v");
    assert_eq!(
        events,
        vec![KeyEvent::Char {
            code: 'v',
            ctrl: false,
            meta: true,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }]
    );
}

#[test]
fn key_events_from_designator_decodes_symbol_events() {
    crate::test_utils::init_test_tracing();
    let events = key_events_from_designator(&Value::vector(vec![Value::symbol("C-f1")]))
        .expect("decode symbol");
    assert_eq!(
        events,
        vec![KeyEvent::Function {
            name: intern("f1"),
            ctrl: true,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }]
    );
}

#[test]
fn key_events_from_designator_decodes_event_modifier_list() {
    crate::test_utils::init_test_tracing();
    // (control ??) => Ctrl+?
    let list = Value::list(vec![Value::symbol("control"), Value::fixnum('?' as i64)]);
    let events =
        key_events_from_designator(&Value::vector(vec![list])).expect("decode modifier list");
    assert_eq!(
        events,
        vec![KeyEvent::Char {
            code: '?',
            ctrl: true,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }]
    );

    // (meta control ?a) => M-C-a
    let list = Value::list(vec![
        Value::symbol("meta"),
        Value::symbol("control"),
        Value::fixnum('a' as i64),
    ]);
    let events =
        key_events_from_designator(&Value::vector(vec![list])).expect("decode multi-modifier list");
    assert_eq!(
        events,
        vec![KeyEvent::Char {
            code: 'a',
            ctrl: true,
            meta: true,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }]
    );

    let list = Value::list(vec![
        Value::symbol("alt"),
        Value::symbol("super"),
        Value::symbol("hyper"),
        Value::symbol("shift"),
        Value::symbol("control"),
        Value::symbol("meta"),
        Value::symbol("f1"),
    ]);
    let events =
        key_events_from_designator(&Value::vector(vec![list])).expect("decode all modifiers");
    assert_eq!(
        events,
        vec![KeyEvent::Function {
            name: intern("f1"),
            ctrl: true,
            meta: true,
            shift: true,
            super_: true,
            hyper: true,
            alt: true,
        }]
    );
}

#[test]
fn key_events_from_designator_rejects_non_array_types() {
    crate::test_utils::init_test_tracing();
    let err = key_events_from_designator(&Value::fixnum(1)).expect_err("int should fail");
    match err {
        KeyDesignatorError::WrongType(v) => assert_eq!(v, Value::fixnum(1)),
        other => panic!("expected WrongType error, got {other:?}"),
    }
}
