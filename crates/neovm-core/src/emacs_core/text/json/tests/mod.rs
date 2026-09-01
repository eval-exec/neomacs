use super::*;
use crate::emacs_core::value::{ValueKind, VecLikeType};
use crate::heap_types::LispString;

#[test]
fn json_option_domains_match_gnu_symbols() {
    assert_eq!(
        ObjectType::from_symbol_value(&Value::symbol("hash-table")),
        Some(ObjectType::HashTable)
    );
    assert_eq!(
        ObjectType::from_symbol_value(&Value::symbol("alist")),
        Some(ObjectType::Alist)
    );
    assert_eq!(
        ObjectType::from_symbol_value(&Value::symbol("plist")),
        Some(ObjectType::Plist)
    );
    assert_eq!(
        ArrayType::from_symbol_value(&Value::symbol("array")),
        Some(ArrayType::Vector)
    );
    assert_eq!(
        ArrayType::from_symbol_value(&Value::symbol("list")),
        Some(ArrayType::List)
    );

    assert_eq!(ObjectType::HashTable.symbol_name(), "hash-table");
    assert_eq!(ArrayType::Vector.symbol_name(), "array");
    assert_eq!(ArrayType::from_symbol_value(&Value::symbol("vector")), None);

    for (keyword, parsed) in [
        (":object-type", JsonOptionKey::ObjectType),
        (":array-type", JsonOptionKey::ArrayType),
        (":null-object", JsonOptionKey::NullObject),
        (":false-object", JsonOptionKey::FalseObject),
    ] {
        assert_eq!(
            JsonOptionKey::from_value(&Value::keyword(keyword)),
            Some(parsed)
        );
        assert_eq!(parsed.keyword(), keyword);
    }
    assert_eq!(
        JsonOptionKey::from_value(&Value::symbol("object-type")),
        None
    );
    assert_eq!(JsonOptionKey::from_value(&Value::keyword(":unknown")), None);
}

// -----------------------------------------------------------------------
// Serializer tests
// -----------------------------------------------------------------------

#[test]
fn serialize_null() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::NIL]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("{}"));
}

#[test]
fn serialize_nil_as_custom_null_object() {
    crate::test_utils::init_test_tracing();
    let result =
        builtin_json_serialize(vec![Value::NIL, Value::keyword(":null-object"), Value::NIL]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("null"));
}

#[test]
fn serialize_true() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::T]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("true"));
}

#[test]
fn serialize_false_keyword() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::keyword(":false")]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("false"));
}

#[test]
fn serialize_json_false_keyword() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::keyword(":json-false")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("json-value-p"), Value::keyword(":json-false")]
            );
        }
        other => panic!("expected wrong-type-argument json-value-p, got {:?}", other),
    }
}

#[test]
fn serialize_integer() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::fixnum(42)]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("42"));
}

#[test]
fn serialize_negative_integer() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::fixnum(-7)]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("-7"));
}

#[test]
fn serialize_float() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::make_float(3.125)]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("3.125"));
}

#[test]
fn serialize_float_whole() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::make_float(1.0)]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("1.0"));
}

#[test]
fn serialize_float_matches_gnu_dtoastr() {
    crate::test_utils::init_test_tracing();
    // Ground-truth strings captured from GNU Emacs json-serialize. The float
    // printer must match byte-for-byte, including GNU's e+NN exponent style
    // and its fixed/scientific threshold (so 1e15 is scientific but 1e7 is
    // not). This is also what neomacs number-to-string produces.
    let cases: &[(f64, &str)] = &[
        (3.125, "3.125"),
        (0.5, "0.5"),
        (100.0, "100.0"),
        (-2.5, "-2.5"),
        (1e7, "10000000.0"),
        (1e15, "1e+15"),
        (1e16, "1e+16"),
        (1e20, "1e+20"),
        (1e-4, "0.0001"),
        (1e-5, "1e-05"),
        (1e-7, "1e-07"),
        (1234567890123456789.0, "1.2345678901234568e+18"),
    ];
    for &(input, expected) in cases {
        let result = builtin_json_serialize(vec![Value::make_float(input)]).unwrap();
        assert_eq!(
            result.as_utf8_str(),
            Some(expected),
            "json-serialize of {input:?} should match GNU"
        );
    }
}

#[test]
fn serialize_nan_errors() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::make_float(f64::NAN)]);
    assert!(result.is_err());
}

#[test]
fn serialize_inf_errors() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::make_float(f64::INFINITY)]);
    assert!(result.is_err());
}

#[test]
fn serialize_string() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::string("hello")]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("\"hello\""));
}

#[test]
fn serialize_string_with_escapes() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::string("a\"b\\c\ndef")]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("\"a\\\"b\\\\c\\ndef\""));
}

#[test]
fn serialize_raw_unibyte_string_rejects_non_json_bytes() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    match builtin_json_serialize(vec![raw]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("json-value-p"), raw]);
        }
        other => panic!("expected wrong-type-argument json-value-p, got {:?}", other),
    }
}

#[test]
fn serialize_ascii_unibyte_string_still_works() {
    crate::test_utils::init_test_tracing();
    let ascii = Value::heap_string(LispString::from_unibyte(b"hello".to_vec()));
    let result = builtin_json_serialize(vec![ascii]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("\"hello\""));
}

#[test]
fn serialize_multibyte_raw_byte_char_rejects_non_json_value() {
    crate::test_utils::init_test_tracing();
    let mut bytes = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let len = crate::emacs_core::emacs_char::char_string(
        crate::emacs_core::emacs_char::byte8_to_char(0xFF),
        &mut bytes,
    );
    let raw = Value::heap_string(LispString::from_emacs_bytes(bytes[..len].to_vec()));
    match builtin_json_serialize(vec![raw]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("json-value-p"), raw]);
        }
        other => panic!("expected wrong-type-argument json-value-p, got {:?}", other),
    }
}

#[test]
fn serialize_empty_vector() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::vector(vec![])]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("[]"));
}

#[test]
fn serialize_vector() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![Value::vector(vec![
        Value::fixnum(1),
        Value::string("two"),
        Value::T,
        Value::NIL,
    ])]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("[1,\"two\",true,{}]"));
}

#[test]
fn serialize_hash_table() {
    crate::test_utils::init_test_tracing();
    let ht = Value::hash_table(HashTableTest::Equal);
    let _ = ht.with_hash_table_mut(|table| {
        table.insert(
            HashKey::from_str("name"),
            Value::string("name"),
            Value::string("Alice"),
        );
    });
    let result = builtin_json_serialize(vec![ht]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("{\"name\":\"Alice\"}"));
}

#[test]
fn serialize_alist() {
    crate::test_utils::init_test_tracing();
    let alist = Value::list(vec![
        Value::cons(Value::symbol("a"), Value::fixnum(1)),
        Value::cons(Value::symbol("b"), Value::fixnum(2)),
    ]);
    let result = builtin_json_serialize(vec![alist]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("{\"a\":1,\"b\":2}"));
}

#[test]
fn serialize_plist_strips_keyword_colons() {
    crate::test_utils::init_test_tracing();
    // A plist (flat KEY VALUE …) serializes as an object; the leading colon
    // of keyword keys is stripped, matching GNU.
    let plist = Value::list(vec![
        Value::keyword(":a"),
        Value::fixnum(1),
        Value::keyword(":b"),
        Value::fixnum(2),
    ]);
    let result = builtin_json_serialize(vec![plist]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("{\"a\":1,\"b\":2}"));
}

#[test]
fn serialize_alist_keeps_first_duplicate_key() {
    crate::test_utils::init_test_tracing();
    // When a key repeats, GNU keeps the first value and drops later ones.
    let alist = Value::list(vec![
        Value::cons(Value::symbol("a"), Value::fixnum(1)),
        Value::cons(Value::symbol("a"), Value::fixnum(2)),
    ]);
    let result = builtin_json_serialize(vec![alist]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("{\"a\":1}"));
}

#[test]
fn serialize_bignum_emits_full_decimal() {
    crate::test_utils::init_test_tracing();
    // A bignum (integer beyond i64) serializes as its full decimal value,
    // round-tripping json-parse-string of the same literal.
    let big = Value::make_integer_from_str_or_zero("123456789012345678901234567890");
    let result = builtin_json_serialize(vec![big]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("123456789012345678901234567890"));
}

#[test]
fn serialize_nested() {
    crate::test_utils::init_test_tracing();
    let inner = Value::vector(vec![Value::fixnum(1), Value::fixnum(2)]);
    let alist = Value::list(vec![Value::cons(Value::symbol("arr"), inner)]);
    let result = builtin_json_serialize(vec![alist]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("{\"arr\":[1,2]}"));
}

#[test]
fn serialize_alist_string_key_type_error() {
    crate::test_utils::init_test_tracing();
    let alist = Value::list(vec![Value::cons(Value::string("a"), Value::fixnum(1))]);
    match builtin_json_serialize(vec![alist]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("symbolp")));
        }
        other => panic!("expected wrong-type-argument signal, got {:?}", other),
    }
}

#[test]
fn serialize_custom_false_object() {
    crate::test_utils::init_test_tracing();
    // Use nil as the false-object.
    let result = builtin_json_serialize(vec![
        Value::NIL,
        Value::keyword(":false-object"),
        Value::NIL,
    ]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("false"));
}

#[test]
fn serialize_wrong_no_args() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_serialize(vec![]);
    assert!(result.is_err());
}

#[test]
fn json_parse_buffer_advances_point_after_value() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(" 42 trailing");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let value = builtin_json_parse_buffer(&mut eval, vec![]).expect("parse buffer");
    assert_eq!(value, Value::fixnum(42));
    assert_eq!(
        eval.buffers
            .current_buffer()
            .expect("current buffer")
            .point_emacs_byte_pos()
            .get(),
        3
    );
}

#[test]
fn json_parse_string_rejects_invalid_unibyte_utf8() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(LispString::from_unibyte(vec![b'"', 0xFF, b'"']));
    match builtin_json_parse_string(vec![raw]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "json-utf8-decode-error");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(1), Value::NIL, Value::fixnum(3)]
            );
        }
        other => panic!("expected json-utf8-decode-error, got {:?}", other),
    }
}

#[test]
fn json_parse_buffer_invalid_utf8_does_not_advance_point() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.set_multibyte_value(false);
        buf.insert_lisp_string(&LispString::from_unibyte(vec![
            b'"', 0xFF, b'"', b' ', b'x',
        ]));
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    match builtin_json_parse_buffer(&mut eval, vec![]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "json-utf8-decode-error");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(1), Value::NIL, Value::fixnum(3)]
            );
        }
        other => panic!("expected json-utf8-decode-error, got {:?}", other),
    }
    assert_eq!(
        eval.buffers
            .current_buffer()
            .expect("current buffer")
            .point_emacs_byte_pos()
            .get(),
        0
    );
}

#[test]
fn json_parse_buffer_end_of_file_uses_gnu_signal_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(" ");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    match builtin_json_parse_buffer(&mut eval, vec![]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "json-end-of-file");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(1), Value::NIL, Value::fixnum(1)]
            );
        }
        other => panic!("expected json-end-of-file signal, got {:?}", other),
    }
    assert_eq!(
        eval.buffers
            .current_buffer()
            .expect("current buffer")
            .point_emacs_byte_pos()
            .get(),
        0
    );
}

#[test]
fn json_insert_writes_at_point_and_advances() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("ab");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(1));
    }

    builtin_json_insert(
        &mut eval,
        vec![Value::vector(vec![Value::fixnum(1), Value::T])],
    )
    .expect("json insert");

    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "a[1,true]b");
    assert_eq!(buf.point_emacs_byte_pos().get(), 9);
}

// -----------------------------------------------------------------------
// Parser tests
// -----------------------------------------------------------------------

#[test]
fn parse_rejects_excessive_nesting_without_stack_overflow() {
    crate::test_utils::init_test_tracing();
    // Far beyond MAX_PARSE_DEPTH: a naive recursive-descent parser would
    // overflow the stack here. We must instead signal a catchable error.
    let s: String = std::iter::repeat('[').take(MAX_PARSE_DEPTH + 50).collect();
    match builtin_json_parse_string(vec![Value::string(s)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "json-object-too-deep");
        }
        other => panic!("expected json-object-too-deep signal, got {:?}", other),
    }
}

#[test]
fn parse_rejects_unescaped_control_char_in_string() {
    crate::test_utils::init_test_tracing();
    // A literal newline (0x0A) inside the quotes is not valid JSON; it must
    // be written as the escape \n. GNU signals json-parse-error here.
    match builtin_json_parse_string(vec![Value::string("\"a\nb\"")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "json-parse-error");
        }
        other => panic!("expected json-parse-error signal, got {:?}", other),
    }
}

#[test]
fn parse_rejects_malformed_surrogates() {
    crate::test_utils::init_test_tracing();
    // High surrogate with no following low surrogate, high surrogate
    // followed by a non-low escape, and a lone low surrogate are all
    // json-invalid-surrogate-error in GNU (not silent U+FFFD).
    for input in [r#""\uD800""#, r#""\uD800A""#, r#""\uDC00""#] {
        match builtin_json_parse_string(vec![Value::string(input)]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(
                    sig.symbol_name(),
                    "json-invalid-surrogate-error",
                    "input {input:?}"
                );
            }
            other => panic!("expected json-invalid-surrogate-error for {input:?}, got {other:?}"),
        }
    }
}

#[test]
fn parse_rejects_malformed_escape_sequences() {
    crate::test_utils::init_test_tracing();
    // An unknown escape (\x) and a \u with non-hex digits are both
    // json-escape-sequence-error in GNU, not the generic json-parse-error.
    for input in [r#""\x""#, r#""\uZZZZ""#] {
        match builtin_json_parse_string(vec![Value::string(input)]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(
                    sig.symbol_name(),
                    "json-escape-sequence-error",
                    "input {input:?}"
                );
            }
            other => panic!("expected json-escape-sequence-error for {input:?}, got {other:?}"),
        }
    }
}

#[test]
fn parse_large_integer_becomes_bignum_not_float() {
    crate::test_utils::init_test_tracing();
    // 10^20 exceeds i64; GNU keeps full precision as a bignum rather than
    // degrading to a lossy float.
    let result = builtin_json_parse_string(vec![Value::string("100000000000000000000")]).unwrap();
    assert!(
        matches!(result.kind(), ValueKind::Veclike(VecLikeType::Bignum)),
        "expected a bignum, got {:?}",
        result.kind()
    );
    assert_eq!(
        result.as_bignum().map(|n| n.to_string()),
        Some("100000000000000000000".to_string())
    );
}

#[test]
fn parse_out_of_range_float_signals_number_out_of_range() {
    crate::test_utils::init_test_tracing();
    // 1e999 overflows the double range; GNU signals
    // json-number-out-of-range-error instead of returning an infinite float.
    match builtin_json_parse_string(vec![Value::string("1e999")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "json-number-out-of-range-error");
        }
        other => panic!("expected json-number-out-of-range-error, got {other:?}"),
    }
}

#[test]
fn parse_null() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![Value::string("null")]);
    let val = result.unwrap();
    assert_eq!(val.as_symbol_name(), Some(":null"));
}

#[test]
fn parse_true() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![Value::string("true")]);
    assert!(result.unwrap().is_t());
}

#[test]
fn parse_false() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![Value::string("false")]);
    let val = result.unwrap();
    assert_eq!(val.as_symbol_name(), Some(":false"));
}

#[test]
fn parse_integer() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![Value::string("42")]);
    assert!(result.unwrap().is_fixnum());
}

#[test]
fn parse_negative_integer() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![Value::string("-7")]);
    assert!(result.unwrap().is_fixnum());
}

#[test]
fn parse_float() {
    crate::test_utils::init_test_tracing();
    let val = builtin_json_parse_string(vec![Value::string("3.125")]).unwrap();
    match val.kind() {
        ValueKind::Float => assert!((val.as_float().unwrap() - 3.125).abs() < 1e-10),
        _ => panic!("expected float, got {:?}", val),
    }
}

#[test]
fn parse_float_exponent() {
    crate::test_utils::init_test_tracing();
    let val = builtin_json_parse_string(vec![Value::string("1.5e2")]).unwrap();
    match val.kind() {
        ValueKind::Float => assert!((val.as_float().unwrap() - 150.0).abs() < 1e-10),
        _ => panic!("expected float, got {:?}", val),
    }
}

#[test]
fn parse_zero() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![Value::string("0")]);
    assert!(result.unwrap().is_fixnum());
}

#[test]
fn parse_string_simple() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![Value::string("\"hello\"")]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("hello"));
}

#[test]
fn parse_raw_unibyte_input_signals_instead_of_panicking() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    let result = builtin_json_parse_string(vec![raw]);
    assert!(result.is_err());
}

#[test]
fn parse_string_with_escapes() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![Value::string("\"a\\\"b\\\\c\\nd\"")]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("a\"b\\c\nd"));
}

#[test]
fn parse_string_unicode_escape() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![Value::string("\"\\u0041\"")]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("A"));
}

#[test]
fn parse_string_surrogate_pair() {
    crate::test_utils::init_test_tracing();
    // U+1F600 (grinning face) = \uD83D\uDE00
    let result = builtin_json_parse_string(vec![Value::string("\"\\uD83D\\uDE00\"")]);
    let val = result.unwrap();
    assert_eq!(val.as_utf8_str(), Some("\u{1F600}"));
}

#[test]
fn parse_empty_array() {
    crate::test_utils::init_test_tracing();
    let val = builtin_json_parse_string(vec![Value::string("[]")]).unwrap();
    match val.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            assert!(val.as_vector_data().unwrap().is_empty())
        }
        _ => panic!("expected vector, got {:?}", val),
    }
}

#[test]
fn parse_array() {
    crate::test_utils::init_test_tracing();
    let val = builtin_json_parse_string(vec![Value::string("[1, 2, 3]")]).unwrap();
    match val.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = val.as_vector_data().unwrap().clone();
            assert_eq!(items.len(), 3);
            assert!(items[0].is_fixnum());
            assert!(items[1].is_fixnum());
            assert!(items[2].is_fixnum());
        }
        _ => panic!("expected vector, got {:?}", val),
    }
}

#[test]
fn parse_array_as_list() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![
        Value::string("[1, 2]"),
        Value::keyword(":array-type"),
        Value::symbol("list"),
    ]);
    let val = result.unwrap();
    let items = list_to_vec(&val).expect("should be a list");
    assert_eq!(items.len(), 2);
    assert!(items[0].is_fixnum());
    assert!(items[1].is_fixnum());
}

#[test]
fn parse_empty_object() {
    crate::test_utils::init_test_tracing();
    let val = builtin_json_parse_string(vec![Value::string("{}")]).unwrap();
    match val.kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = val.as_hash_table().unwrap();
            assert!(table.data.is_empty());
        }
        _ => panic!("expected hash-table, got {:?}", val),
    }
}

#[test]
fn parse_object_hash_table() {
    crate::test_utils::init_test_tracing();
    let val = builtin_json_parse_string(vec![Value::string("{\"a\": 1, \"b\": 2}")]).unwrap();
    match val.kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = val.as_hash_table().unwrap();
            assert_eq!(table.data.len(), 2);
            assert_eq!(table.key_snapshots().count(), 2);
            assert_eq!(
                table
                    .data
                    .get(&HashKey::from_str("a"))
                    .map(|v: &Value| v.as_fixnum()),
                Some(Some(1))
            );
            assert_eq!(
                table
                    .data
                    .get(&HashKey::from_str("b"))
                    .map(|v: &Value| v.as_fixnum()),
                Some(Some(2))
            );
            assert!(matches!(
                table.key_snapshot(&HashKey::from_str("a")),
                Some(key) if (*key).as_utf8_str() == Some("a")
            ));
            assert!(matches!(
                table.key_snapshot(&HashKey::from_str("b")),
                Some(key) if (*key).as_utf8_str() == Some("b")
            ));
        }
        _ => panic!("expected hash-table, got {:?}", val),
    }
}

#[test]
fn parse_object_as_alist() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![
        Value::string("{\"x\": 10}"),
        Value::keyword(":object-type"),
        Value::symbol("alist"),
    ]);
    let val = result.unwrap();
    let items = list_to_vec(&val).expect("should be a list");
    assert_eq!(items.len(), 1);
    // Each item should be (key . value).
    match items[0].kind() {
        ValueKind::Cons => {
            let pair_car = items[0].cons_car();
            let pair_cdr = items[0].cons_cdr();
            assert_eq!(pair_car, Value::symbol("x"));
            assert!(pair_cdr.is_fixnum());
        }
        _other => panic!("expected cons, got {:?}", items[0]),
    }
}

#[test]
fn parse_object_as_plist() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![
        Value::string("{\"key\": 42}"),
        Value::keyword(":object-type"),
        Value::symbol("plist"),
    ]);
    let val = result.unwrap();
    let items = list_to_vec(&val).expect("should be a list");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].as_symbol_name(), Some(":key"));
    assert!(items[1].is_fixnum());
}

#[test]
fn parse_nested() {
    crate::test_utils::init_test_tracing();
    let json = r#"{"arr": [1, {"nested": true}], "val": null}"#;
    let result = builtin_json_parse_string(vec![Value::string(json)]);
    assert!(result.is_ok());
}

#[test]
fn parse_custom_null_object() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![
        Value::string("null"),
        Value::keyword(":null-object"),
        Value::NIL,
    ]);
    assert!(result.unwrap().is_nil());
}

#[test]
fn parse_custom_false_object() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![
        Value::string("false"),
        Value::keyword(":false-object"),
        Value::keyword(":json-false"),
    ]);
    let val = result.unwrap();
    assert_eq!(val.as_symbol_name(), Some(":json-false"));
}

#[test]
fn parse_trailing_content_error() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![Value::string("42 extra")]);
    assert!(result.is_err());
}

#[test]
fn parse_trailing_content_reports_character_position_like_gnu() {
    crate::test_utils::init_test_tracing();
    match builtin_json_parse_string(vec![Value::string("\"é\"x")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "json-trailing-content");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(1), Value::NIL, Value::fixnum(4)]
            );
        }
        other => panic!("expected json-trailing-content, got {:?}", other),
    }
}

#[test]
fn parse_empty_string_error() {
    crate::test_utils::init_test_tracing();
    match builtin_json_parse_string(vec![Value::string("")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "json-end-of-file");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(1), Value::NIL, Value::fixnum(0)]
            );
        }
        other => panic!("expected json-end-of-file signal, got {:?}", other),
    }
}

#[test]
fn parse_end_of_file_reports_character_position_like_gnu() {
    crate::test_utils::init_test_tracing();
    match builtin_json_parse_string(vec![Value::string("\"é")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "json-end-of-file");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(1), Value::NIL, Value::fixnum(2)]
            );
        }
        other => panic!("expected json-end-of-file signal, got {:?}", other),
    }
}

#[test]
fn parse_invalid_json_error() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![Value::string("{bad}")]);
    assert!(result.is_err());
}

#[test]
fn parse_wrong_type_argument() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![Value::fixnum(42)]);
    assert!(result.is_err());
}

#[test]
fn parse_no_args() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![]);
    assert!(result.is_err());
}

// -----------------------------------------------------------------------
// Round-trip tests
// -----------------------------------------------------------------------

#[test]
fn round_trip_integer() {
    crate::test_utils::init_test_tracing();
    let serialized = builtin_json_serialize(vec![Value::fixnum(123)]).unwrap();
    let parsed = builtin_json_parse_string(vec![serialized]).unwrap();
    assert!(parsed.is_fixnum());
}

#[test]
fn round_trip_string() {
    crate::test_utils::init_test_tracing();
    let original = Value::string("hello \"world\"\ntest");
    let serialized = builtin_json_serialize(vec![original]).unwrap();
    let parsed = builtin_json_parse_string(vec![serialized]).unwrap();
    assert_eq!(parsed.as_utf8_str(), Some("hello \"world\"\ntest"));
}

#[test]
fn round_trip_array() {
    crate::test_utils::init_test_tracing();
    let original = Value::vector(vec![Value::fixnum(1), Value::string("two"), Value::T]);
    let serialized = builtin_json_serialize(vec![original]).unwrap();
    let parsed = builtin_json_parse_string(vec![serialized]).unwrap();
    match parsed.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = parsed.as_vector_data().unwrap().clone();
            assert_eq!(items.len(), 3);
            assert!(items[0].is_fixnum());
            assert_eq!(items[1].as_utf8_str(), Some("two"));
            assert!(items[2].is_t());
        }
        _ => panic!("expected vector"),
    }
}

#[test]
fn round_trip_object() {
    crate::test_utils::init_test_tracing();
    let ht = Value::hash_table(HashTableTest::Equal);
    let _ = ht.with_hash_table_mut(|table| {
        table.insert(
            HashKey::from_str("key"),
            Value::string("key"),
            Value::fixnum(99),
        );
    });
    let serialized = builtin_json_serialize(vec![ht]).unwrap();
    let parsed = builtin_json_parse_string(vec![serialized]).unwrap();
    match parsed.kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = parsed.as_hash_table().unwrap();
            assert_eq!(
                table
                    .data
                    .get(&HashKey::from_str("key"))
                    .map(|v: &Value| v.as_fixnum()),
                Some(Some(99))
            );
        }
        _ => panic!("expected hash-table"),
    }
}

// -----------------------------------------------------------------------
// String encoding edge cases
// -----------------------------------------------------------------------

#[test]
fn encode_control_chars() {
    crate::test_utils::init_test_tracing();
    let s = "a\x00b\x01c";
    let encoded = json_encode_string(s);
    assert_eq!(encoded, "\"a\\u0000b\\u0001c\"");
}

#[test]
fn encode_backspace_formfeed() {
    crate::test_utils::init_test_tracing();
    let s = "\x08\x0C";
    let encoded = json_encode_string(s);
    assert_eq!(encoded, "\"\\b\\f\"");
}

#[test]
fn parse_large_integer_keeps_bignum_precision() {
    crate::test_utils::init_test_tracing();
    // Integer too large for i64 must become a bignum, not a lossy float
    // (matching GNU, which never degrades integer precision).
    let val = builtin_json_parse_string(vec![Value::string("99999999999999999999")]).unwrap();
    assert!(
        matches!(val.kind(), ValueKind::Veclike(VecLikeType::Bignum)),
        "expected bignum for large integer, got {:?}",
        val.kind()
    );
    assert_eq!(
        val.as_bignum().map(|n| n.to_string()),
        Some("99999999999999999999".to_string())
    );
}

#[test]
fn serialize_symbol_key_in_alist() {
    crate::test_utils::init_test_tracing();
    let alist = Value::list(vec![Value::cons(
        Value::symbol("name"),
        Value::string("test"),
    )]);
    let result = builtin_json_serialize(vec![alist]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("{\"name\":\"test\"}"));
}

#[test]
fn parse_whitespace_around_values() {
    crate::test_utils::init_test_tracing();
    let result = builtin_json_parse_string(vec![Value::string("  {  \"a\"  :  1  }  ")]);
    assert!(result.is_ok());
}

#[test]
fn parse_deeply_nested() {
    crate::test_utils::init_test_tracing();
    let json = "[[[[[[1]]]]]]";
    let result = builtin_json_parse_string(vec![Value::string(json)]);
    assert!(result.is_ok());
}

// -----------------------------------------------------------------------
// GNU error-shape parity for json-parse-string (bug group native-json #4)
//
// GNU `json_signal_error` always signals the integer triple
// `(LINE nil POS)`, never a human-readable string.  Premature end of input
// must signal `json-end-of-file` (a subtype of json-parse-error), and POS
// is the character offset of the cursor *after* the offending character was
// consumed (e.g. `[1,2,]` reports the `]` at position 6).  Oracle values
// below were produced with `emacs --batch`.
// -----------------------------------------------------------------------

/// Assert that parsing `input` signals condition `cond` with the GNU
/// `(LINE nil POS)` integer triple, never a string message.
fn assert_parse_signal(input: &str, cond: &str, line: i64, pos: i64) {
    match builtin_json_parse_string(vec![Value::string(input)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), cond, "condition for {input:?}");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(line), Value::NIL, Value::fixnum(pos)],
                "error data for {input:?}",
            );
        }
        other => panic!("expected {cond} signal for {input:?}, got {other:?}"),
    }
}

#[test]
fn parse_premature_eof_signals_json_end_of_file_with_triple() {
    crate::test_utils::init_test_tracing();
    // `{` opens an object then input ends; GNU: (json-end-of-file 1 nil 1).
    assert_parse_signal("{", "json-end-of-file", 1, 1);
    // Empty / whitespace-only input also reaches EOF while reading the value.
    assert_parse_signal("", "json-end-of-file", 1, 0);
    assert_parse_signal("  ", "json-end-of-file", 1, 2);
    // EOF inside arrays and objects at various stages.
    assert_parse_signal("[", "json-end-of-file", 1, 1);
    assert_parse_signal("[1,2", "json-end-of-file", 1, 4);
    assert_parse_signal("[1,", "json-end-of-file", 1, 3);
    assert_parse_signal("{\"a\":1", "json-end-of-file", 1, 6);
    assert_parse_signal("{\"a\":1,", "json-end-of-file", 1, 7);
    // Unterminated string.
    assert_parse_signal("\"ab", "json-end-of-file", 1, 3);
}

#[test]
fn parse_error_data_is_integer_triple_with_one_based_position() {
    crate::test_utils::init_test_tracing();
    // Trailing comma: GNU reports the `]` at position 6, not a string.
    assert_parse_signal("[1,2,]", "json-parse-error", 1, 6);
    // A bare unexpected character is reported just past it.
    assert_parse_signal("[x]", "json-parse-error", 1, 2);
    assert_parse_signal("nul", "json-parse-error", 1, 3);
    // Missing separators.
    assert_parse_signal("[1,2 3]", "json-parse-error", 1, 6);
    assert_parse_signal("{\"a\" 1}", "json-parse-error", 1, 6);
    // Trailing content after a complete value (subtype of json-parse-error).
    assert_parse_signal("[1,2] x", "json-trailing-content", 1, 7);
}

#[test]
fn parse_error_position_counts_multibyte_chars_not_bytes() {
    crate::test_utils::init_test_tracing();
    // `é` is two bytes but one character; GNU's position is a character
    // offset, so the bad `é` value is reported at position 2.
    assert_parse_signal("[é,]", "json-parse-error", 1, 2);
}

// -----------------------------------------------------------------------
// json-serialize returns a unibyte raw-UTF-8 string (bug group #5)
// and rejects Inf/NaN with a plain `error` (bug group #6).
// -----------------------------------------------------------------------

#[test]
fn serialize_returns_unibyte_raw_utf8_string() {
    crate::test_utils::init_test_tracing();
    // GNU: (json-serialize ["é"]) is a 6-byte UNIBYTE string ["é"] where é
    // is its raw 2-byte UTF-8 sequence, and multibyte-string-p is nil.
    let value = Value::vector(vec![Value::string("é")]);
    let result = builtin_json_serialize(vec![value]).unwrap();
    let s = result
        .as_lisp_string()
        .expect("json-serialize must return a string");
    assert!(
        !s.is_multibyte(),
        "json-serialize must return a unibyte string"
    );
    assert_eq!(
        s.as_bytes(),
        b"[\"\xc3\xa9\"]",
        "expected raw UTF-8 bytes for [\"é\"]"
    );
    assert_eq!(s.as_bytes().len(), 6, "GNU reports length 6");
}

#[test]
fn serialize_inf_and_nan_signal_plain_error_with_offending_float() {
    crate::test_utils::init_test_tracing();
    // GNU: (json-serialize (/ 1.0 0.0)) =>
    //   (error "JSON does not allow Inf or NaN" 1.0e+INF)
    for f in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        let float = Value::make_float(f);
        match builtin_json_serialize(vec![float]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error", "condition for {f}");
                assert_eq!(
                    sig.data,
                    vec![Value::string("JSON does not allow Inf or NaN"), float,],
                    "error data for {f}",
                );
            }
            other => panic!("expected plain error for {f}, got {other:?}"),
        }
    }
}
