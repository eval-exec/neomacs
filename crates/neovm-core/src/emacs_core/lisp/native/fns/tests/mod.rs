use super::*;
use crate::buffer::LispCharPos1;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print;

/// Test helper: create a minimal eval context for widget-apply tests.
fn test_eval_ctx() -> crate::emacs_core::eval::Context {
    crate::emacs_core::eval::Context::new()
}

/// Test helper that calls an evaluator builtin and keeps the
/// context alive for the remainder of the test. Previously the
/// `call_fns_builtin!` macro created a short-lived `Context::new()`
/// inside its block expression and returned the builtin's result;
/// the context was then dropped at the end of the expression,
/// destroying the tagged heap and leaving the returned Value
/// pointing at freed memory. `.as_str()` on the stale Value hit
/// `BUG: StringObj header.kind = VecLike` from `tagged/value.rs`.
///
/// Each call to this helper allocates a boxed `Context` in a
/// thread-local so the returned Value's heap memory lives until
/// the test function returns.
macro_rules! call_fns_builtin {
    ($builtin:ident, $args:expr) => {{
        use std::cell::RefCell;
        thread_local! {
            static TEST_CTX: RefCell<Option<Box<Context>>> = const { RefCell::new(None) };
        }
        TEST_CTX.with(|slot| {
            let mut new_ctx = Box::new(Context::new());
            let result = $builtin(&mut new_ctx, $args);
            // Replace any prior held context (previous test calls
            // in the same thread) — the new one owns the heap
            // that holds the returned Value.
            *slot.borrow_mut() = Some(new_ctx);
            result
        })
    }};
}

// ---- Base64 standard ----

#[test]
fn base64_encode_empty() {
    crate::test_utils::init_test_tracing();
    let r = builtin_base64_encode_string(vec![Value::string(""), Value::T]).unwrap();
    assert_eq!(r.as_utf8_str(), Some(""));
}

#[test]
fn base64_encode_hello() {
    crate::test_utils::init_test_tracing();
    let r = builtin_base64_encode_string(vec![Value::string("Hello"), Value::T]).unwrap();
    assert_eq!(r.as_utf8_str(), Some("SGVsbG8="));
}

#[test]
fn base64_encode_padding_1() {
    crate::test_utils::init_test_tracing();
    // "a" -> "YQ=="
    let r = builtin_base64_encode_string(vec![Value::string("a"), Value::T]).unwrap();
    assert_eq!(r.as_utf8_str(), Some("YQ=="));
}

#[test]
fn base64_encode_padding_2() {
    crate::test_utils::init_test_tracing();
    // "ab" -> "YWI="
    let r = builtin_base64_encode_string(vec![Value::string("ab"), Value::T]).unwrap();
    assert_eq!(r.as_utf8_str(), Some("YWI="));
}

#[test]
fn base64_encode_no_padding_3() {
    crate::test_utils::init_test_tracing();
    // "abc" -> "YWJj" (no padding needed)
    let r = builtin_base64_encode_string(vec![Value::string("abc"), Value::T]).unwrap();
    assert_eq!(r.as_utf8_str(), Some("YWJj"));
}

#[test]
fn base64_roundtrip() {
    crate::test_utils::init_test_tracing();
    let original = "The quick brown fox jumps over the lazy dog";
    let encoded = builtin_base64_encode_string(vec![Value::string(original), Value::T]).unwrap();
    let decoded = builtin_base64_decode_string(vec![encoded]).unwrap();
    assert_eq!(decoded.as_utf8_str(), Some(original));
}

#[test]
fn base64_decode_invalid() {
    crate::test_utils::init_test_tracing();
    // Invalid base64 now signals an error (matching GNU Emacs)
    let r = builtin_base64_decode_string(vec![Value::string("!!!!")]);
    assert!(r.is_err());
}

#[test]
fn base64_decode_string_ignore_invalid() {
    crate::test_utils::init_test_tracing();
    let decoded =
        builtin_base64_decode_string(vec![Value::string("!!!!"), Value::NIL, Value::T]).unwrap();
    let decoded = decoded.as_lisp_string().unwrap();
    assert_eq!(decoded.as_bytes(), b"");
    assert!(!decoded.is_multibyte());
}

#[test]
fn base64_decode_string_rejects_malformed_padding_like_gnu() {
    crate::test_utils::init_test_tracing();
    for input in ["Zg=", "Zm9vYmE", "Zm9vYmFy=", "Zg=Zg="] {
        let decoded = builtin_base64_decode_string(vec![Value::string(input)]);
        assert!(decoded.is_err(), "{input} should signal invalid base64");
    }
}

#[test]
fn base64_encode_string_rejects_multibyte_non_ascii_like_gnu() {
    crate::test_utils::init_test_tracing();
    let encoded = builtin_base64_encode_string(vec![Value::string("é"), Value::T]);
    match encoded {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "Multibyte character in data for base64 encoding"
                )]
            );
        }
        other => panic!("expected multibyte base64 error, got {other:?}"),
    }
}

#[test]
fn base64_encode_string_preserves_unibyte_raw_bytes_like_gnu() {
    crate::test_utils::init_test_tracing();
    let input = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xE9]));
    let encoded = builtin_base64_encode_string(vec![input, Value::T]).unwrap();
    assert_eq!(encoded.as_utf8_str(), Some("6Q=="));
}

// ---- Base64 URL ----

#[test]
fn base64url_encode_no_pad() {
    crate::test_utils::init_test_tracing();
    let r = builtin_base64url_encode_string(vec![Value::string("a"), Value::T]).unwrap();
    // URL-safe, no padding
    assert_eq!(r.as_utf8_str(), Some("YQ"));
}

#[test]
fn base64url_encode_with_pad() {
    crate::test_utils::init_test_tracing();
    let r = builtin_base64url_encode_string(vec![Value::string("a")]).unwrap();
    assert_eq!(r.as_utf8_str(), Some("YQ=="));
}

#[test]
fn base64url_roundtrip() {
    crate::test_utils::init_test_tracing();
    let original = "Hello+World/Foo";
    let encoded = builtin_base64url_encode_string(vec![Value::string(original), Value::T]).unwrap();
    let decoded = builtin_base64_decode_string(vec![encoded, Value::T]).unwrap();
    assert_eq!(decoded.as_utf8_str(), Some(original));
}

#[test]
fn base64url_decode_basic() {
    crate::test_utils::init_test_tracing();
    let decoded = builtin_base64url_decode_string(vec![Value::string("YQ")]).unwrap();
    assert_eq!(decoded.as_utf8_str(), Some("a"));
}

#[test]
fn base64url_decode_invalid() {
    crate::test_utils::init_test_tracing();
    let decoded = builtin_base64url_decode_string(vec![Value::string("!!!!")]).unwrap();
    assert!(decoded.is_nil());
}

#[test]
fn base64url_decode_ignore_invalid() {
    crate::test_utils::init_test_tracing();
    let decoded = builtin_base64url_decode_string(vec![Value::string("!!!!"), Value::T]).unwrap();
    let decoded = decoded.as_lisp_string().unwrap();
    assert_eq!(decoded.as_bytes(), b"");
    assert!(!decoded.is_multibyte());
}

#[test]
fn base64url_uses_dash_underscore() {
    crate::test_utils::init_test_tracing();
    let input = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let std_enc = builtin_base64_encode_string(vec![input, Value::T]).unwrap();
    let input = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let url_enc = builtin_base64url_encode_string(vec![input, Value::NIL]).unwrap();
    assert_eq!(std_enc.as_utf8_str(), Some("/w=="));
    assert_eq!(url_enc.as_utf8_str(), Some("_w=="));
}

#[test]
fn base64url_encode_string_rejects_multibyte_non_ascii_like_gnu() {
    crate::test_utils::init_test_tracing();
    let encoded = builtin_base64url_encode_string(vec![Value::string("é"), Value::T]);
    match encoded {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "Multibyte character in data for base64 encoding"
                )]
            );
        }
        other => panic!("expected multibyte base64 error, got {other:?}"),
    }
}

#[test]
fn base64_region_eval_encode_decode_roundtrip() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("Hi");
    }

    let encoded = builtin_base64_encode_region(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)])
        .expect("encode region should succeed");
    assert_eq!(encoded, Value::fixnum(4));
    let encoded_text = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .buffer_string();
    assert_eq!(encoded_text, "SGk=");

    let decoded = builtin_base64_decode_region(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)])
        .expect("decode region should succeed");
    assert_eq!(decoded, Value::fixnum(2));
    let decoded_text = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .buffer_string();
    assert_eq!(decoded_text, "Hi");
}

#[test]
fn base64_region_eval_swapped_bounds_and_url_encoding() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("ab");
    }

    let encoded = builtin_base64url_encode_region(
        &mut eval,
        vec![Value::fixnum(3), Value::fixnum(1), Value::T],
    )
    .expect("url encode region should succeed");
    assert_eq!(encoded, Value::fixnum(3));
    let encoded_text = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .buffer_string();
    assert_eq!(encoded_text, "YWI");
}

#[test]
fn base64_region_eval_accepts_marker_bounds() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("abc");
    }
    let start = crate::emacs_core::marker::make_marker_value(
        Some(buffer_id),
        Some(crate::buffer::LispCharPos1::new(2)),
        false,
    );
    let end = crate::emacs_core::marker::make_marker_value(
        Some(buffer_id),
        Some(crate::buffer::LispCharPos1::new(4)),
        false,
    );

    let encoded = builtin_base64_encode_region(&mut eval, vec![start, end, Value::T])
        .expect("encode region should accept markers");
    assert_eq!(encoded, Value::fixnum(4));
    let encoded_text = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .buffer_string();
    assert_eq!(encoded_text, "aYmM=");
}

#[test]
fn base64_region_preserves_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.set_multibyte_value(false);
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert_lisp_string(&crate::heap_types::LispString::from_unibyte(vec![0xFF]));
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    }

    let encoded = builtin_base64_encode_region(&mut eval, vec![Value::fixnum(1), Value::fixnum(2)])
        .expect("encode raw-byte region should succeed");
    assert_eq!(encoded, Value::fixnum(4));
    let encoded_text = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .buffer_string();
    assert_eq!(encoded_text, "/w==");

    let decoded = builtin_base64_decode_region(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)])
        .expect("decode raw-byte region should succeed");
    assert_eq!(decoded, Value::fixnum(1));
    let decoded_text = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .buffer_substring_lisp_string_range(crate::buffer::EmacsByteRange::from_usize(0, 1));
    assert_eq!(decoded_text.as_bytes(), &[0xFF]);
    assert!(!decoded_text.is_multibyte());
}

#[test]
fn base64_decode_region_noerror_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("%%");
    }

    let ignored = builtin_base64_decode_region(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(3), Value::NIL, Value::T],
    )
    .expect("noerror decode should succeed");
    assert_eq!(ignored, Value::fixnum(0));
    let emptied = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .buffer_string();
    assert_eq!(emptied, "");

    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("%%");
    }
    let strict = builtin_base64_decode_region(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]);
    match strict {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Invalid base64 data")]);
        }
        other => panic!("expected invalid base64 signal, got {other:?}"),
    }
    let unchanged = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .buffer_string();
    assert_eq!(unchanged, "%%");
}

#[test]
fn base64_region_eval_error_shapes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("Hi");
    }

    let type_error = builtin_base64_encode_region(
        &mut eval,
        vec![Value::symbol("x"), Value::fixnum(2), Value::T],
    );
    match type_error {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("integer-or-marker-p"), Value::symbol("x")]
            );
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }

    let range_error =
        builtin_base64_encode_region(&mut eval, vec![Value::fixnum(0), Value::fixnum(2)]);
    match range_error {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data.len(), 3);
            assert!(sig.data[0].is_buffer());
            assert_eq!(sig.data[1], Value::fixnum(0));
            assert_eq!(sig.data[2], Value::fixnum(2));
        }
        other => panic!("expected args-out-of-range, got {other:?}"),
    }
}

/// Bug 1: decoding base64 whose bytes are 0x80-0xFF into a *multibyte* buffer
/// must store each raw byte as its two-byte eight-bit internal encoding, not
/// as a raw byte (which is not a valid multibyte lead byte and previously
/// panicked in `string_char_unchecked`).
///
/// GNU oracle:
///   (with-temp-buffer (insert "/w==")
///     (base64-decode-region (point-min) (point-max))
///     (append (buffer-string) nil))  => (4194303)
#[test]
fn base64_decode_region_multibyte_eight_bit_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        assert!(
            buf.get_multibyte(),
            "default temp buffer should be multibyte"
        );
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("/w==");
    }

    // "/w==" decodes to the single byte 0xFF.
    let decoded = builtin_base64_decode_region(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)])
        .expect("decode into multibyte buffer should not panic");
    // The number of inserted *characters* is 1 (one eight-bit char).
    assert_eq!(decoded, Value::fixnum(1));

    let buf = eval.buffers.current_buffer().expect("current buffer");
    // The buffer now holds exactly one character.
    let text = buf.buffer_substring_lisp_string_range(buf.full_emacs_byte_range());
    assert!(
        text.is_multibyte(),
        "decoded text must stay in the multibyte buffer's representation"
    );
    assert_eq!(text.schars(), 1, "exactly one eight-bit character");
    // The internal bytes are the two-byte eight-bit encoding of 0xFF
    // (`str_to_multibyte([0xFF])` == [0xC1, 0xBF]), and it decodes back to
    // the eight-bit character 0x3FFFFF (4194303) like GNU.
    assert_eq!(text.as_bytes(), &[0xC1, 0xBF]);
    let (ch, len) = crate::emacs_core::emacs_char::string_char(text.as_bytes());
    assert_eq!(len, 2);
    assert_eq!(ch, 4194303);
}

/// Bug 2a: encoding a region of a multibyte buffer containing a genuine
/// (non-eight-bit) multibyte character must signal GNU's error instead of
/// silently encoding the internal UTF-8 bytes.
///
/// GNU oracle:
///   (with-temp-buffer (insert "héllo")
///     (base64-encode-region (point-min) (point-max)) ...)
///   => (error "Multibyte character in data for base64 encoding")
#[test]
fn base64_encode_region_rejects_multibyte_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        assert!(buf.get_multibyte());
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("héllo");
    }

    let result = builtin_base64_encode_region(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|v| v.as_utf8_str()),
                Some("Multibyte character in data for base64 encoding")
            );
        }
        other => panic!("expected multibyte base64 error, got {other:?}"),
    }
}

/// Bug 2b: a multibyte string whose characters are eight-bit raw bytes must
/// encode the underlying raw bytes, not their internal UTF-8 expansion.
///
/// GNU oracle:
///   (base64-encode-string (string-to-multibyte (unibyte-string 200 201)))
///   => "yMk="
#[test]
fn base64_encode_string_multibyte_eight_bit_like_gnu() {
    crate::test_utils::init_test_tracing();
    // (string-to-multibyte (unibyte-string 200 201)) is a multibyte string
    // holding two eight-bit characters; its internal bytes are
    // str_to_multibyte([200, 201]).
    let internal = crate::emacs_core::emacs_char::str_to_multibyte(&[200, 201]);
    let input = Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(internal));
    assert!(input.as_lisp_string().unwrap().is_multibyte());
    let encoded = builtin_base64_encode_string(vec![input, Value::T]).unwrap();
    assert_eq!(encoded.as_utf8_str(), Some("yMk="));
}

/// Bug 2c: a final full 76-character line must not get a trailing line
/// separator; GNU only inserts separators *between* lines.
///
/// GNU oracle:
///   (length (base64-encode-string (make-string 57 ?A))) => 76
#[test]
fn base64_encode_string_no_trailing_newline_on_full_line_like_gnu() {
    crate::test_utils::init_test_tracing();
    let input = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![b'A'; 57]));
    let encoded = builtin_base64_encode_string(vec![input]).unwrap();
    let s = encoded.as_utf8_str().expect("string result");
    assert_eq!(s.len(), 76, "exactly one full line, no trailing newline");
    assert!(
        !s.contains('\n'),
        "a single full line must contain no separators"
    );

    // A 58-byte input (20 groups) wraps once, between the two lines.
    let input2 = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![b'A'; 58]));
    let encoded2 = builtin_base64_encode_string(vec![input2]).unwrap();
    let s2 = encoded2.as_utf8_str().expect("string result");
    assert_eq!(s2.len(), 81);
    assert_eq!(s2.matches('\n').count(), 1);
    assert!(!s2.ends_with('\n'), "the separator is between lines only");
}

// ---- MD5 ----

#[test]
fn md5_empty() {
    crate::test_utils::init_test_tracing();
    let r = call_fns_builtin!(builtin_md5, vec![Value::string("")]).unwrap();
    assert_eq!(r.as_utf8_str(), Some("d41d8cd98f00b204e9800998ecf8427e"));
}

#[test]
fn md5_hello() {
    crate::test_utils::init_test_tracing();
    // md5("Hello") = 8b1a9953c4611296a827abf8c47804d7
    let r = call_fns_builtin!(builtin_md5, vec![Value::string("Hello")]).unwrap();
    assert_eq!(r.as_utf8_str(), Some("8b1a9953c4611296a827abf8c47804d7"));
}

#[test]
fn md5_abc() {
    crate::test_utils::init_test_tracing();
    let r = call_fns_builtin!(builtin_md5, vec![Value::string("abc")]).unwrap();
    assert_eq!(r.as_utf8_str(), Some("900150983cd24fb0d6963f7d28e17f72"));
}

#[test]
fn md5_fox() {
    crate::test_utils::init_test_tracing();
    let r = call_fns_builtin!(
        builtin_md5,
        vec![Value::string("The quick brown fox jumps over the lazy dog")]
    )
    .unwrap();
    assert_eq!(r.as_utf8_str(), Some("9e107d9d372bb6826bd81d3542a419d6"));
}

#[test]
fn md5_string_range_errors() {
    crate::test_utils::init_test_tracing();
    match call_fns_builtin!(
        builtin_md5,
        vec![Value::string("abc"), Value::fixnum(2), Value::fixnum(1)]
    ) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(
                sig.data,
                vec![Value::string("abc"), Value::fixnum(2), Value::fixnum(1)]
            );
        }
        other => panic!("expected args-out-of-range signal, got {other:?}"),
    }
}

#[test]
fn md5_string_index_type_error() {
    crate::test_utils::init_test_tracing();
    match call_fns_builtin!(
        builtin_md5,
        vec![Value::string("abc"), Value::T, Value::fixnum(1)]
    ) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("integerp")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn md5_invalid_object_errors() {
    crate::test_utils::init_test_tracing();
    match call_fns_builtin!(builtin_md5, vec![Value::NIL]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|v| v.as_utf8_str()),
                Some("Invalid object argument")
            );
            assert_eq!(sig.data.get(1), Some(&Value::string("nil")));
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn md5_unknown_coding_system_errors() {
    crate::test_utils::init_test_tracing();
    match call_fns_builtin!(
        builtin_md5,
        vec![
            Value::string("abc"),
            Value::NIL,
            Value::NIL,
            Value::symbol("no-such"),
        ]
    ) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "coding-system-error");
            assert_eq!(sig.data, vec![Value::symbol("no-such")]);
        }
        other => panic!("expected coding-system-error signal, got {other:?}"),
    }
}

#[test]
fn md5_unknown_coding_system_ignored_with_noerror() {
    crate::test_utils::init_test_tracing();
    let r = call_fns_builtin!(
        builtin_md5,
        vec![
            Value::string("abc"),
            Value::NIL,
            Value::NIL,
            Value::symbol("no-such"),
            Value::T,
        ]
    )
    .unwrap();
    assert_eq!(r.as_utf8_str(), Some("900150983cd24fb0d6963f7d28e17f72"));
}

#[test]
fn md5_string_honors_utf16le_coding_system() {
    crate::test_utils::init_test_tracing();
    let r = call_fns_builtin!(
        builtin_md5,
        vec![
            Value::string("é"),
            Value::NIL,
            Value::NIL,
            Value::symbol("utf-16le"),
        ]
    )
    .unwrap();
    assert_eq!(r.as_utf8_str(), Some("ed71e8ffd3d8c47c1a2e22c53cd384aa"));
}

#[test]
fn md5_accepts_iso_8859_15_alias() {
    crate::test_utils::init_test_tracing();
    let r = call_fns_builtin!(
        builtin_md5,
        vec![
            Value::string("abc"),
            Value::NIL,
            Value::NIL,
            Value::symbol("iso-8859-15"),
        ]
    )
    .unwrap();
    assert_eq!(r.as_utf8_str(), Some("900150983cd24fb0d6963f7d28e17f72"));
}

#[test]
fn md5_accepts_iso_8859_9_alias() {
    crate::test_utils::init_test_tracing();
    let r = call_fns_builtin!(
        builtin_md5,
        vec![
            Value::string("abc"),
            Value::NIL,
            Value::NIL,
            Value::symbol("iso-8859-9"),
        ]
    )
    .unwrap();
    assert_eq!(r.as_utf8_str(), Some("900150983cd24fb0d6963f7d28e17f72"));
}

#[test]
fn md5_non_symbol_coding_system_errors() {
    crate::test_utils::init_test_tracing();
    match call_fns_builtin!(
        builtin_md5,
        vec![
            Value::string("abc"),
            Value::NIL,
            Value::NIL,
            Value::fixnum(1),
        ]
    ) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "coding-system-error");
            assert_eq!(sig.data, vec![Value::fixnum(1)]);
        }
        other => panic!("expected coding-system-error signal, got {other:?}"),
    }
}

#[test]
fn md5_eval_buffer_core_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("abc");
    }
    let id = eval.buffers.current_buffer().expect("current buffer").id;

    let full = builtin_md5(&mut eval, vec![Value::make_buffer(id)]).unwrap();
    assert_eq!(full.as_utf8_str(), Some("900150983cd24fb0d6963f7d28e17f72"));

    let swapped = builtin_md5(
        &mut eval,
        vec![Value::make_buffer(id), Value::fixnum(4), Value::fixnum(3)],
    )
    .unwrap();
    assert_eq!(
        swapped.as_utf8_str(),
        Some("4a8a08f09d37b73795649038408b5f33")
    );
}

#[test]
fn md5_eval_buffer_range_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("abc");
    }
    let id = eval.buffers.current_buffer().expect("current buffer").id;

    match builtin_md5(&mut eval, vec![Value::make_buffer(id), Value::fixnum(5)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(5), Value::NIL]);
        }
        other => panic!("expected args-out-of-range signal, got {other:?}"),
    }
}

#[test]
fn md5_eval_buffer_index_type_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let id = eval.buffers.current_buffer().expect("current buffer").id;

    match builtin_md5(
        &mut eval,
        vec![Value::make_buffer(id), Value::T, Value::fixnum(3)],
    ) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data.first(),
                Some(&Value::symbol("integer-or-marker-p"))
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn md5_eval_deleted_buffer_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let id = eval.buffers.create_buffer("*md5-doomed*");
    assert!(eval.buffers.kill_buffer(id));

    match builtin_md5(&mut eval, vec![Value::make_buffer(id)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|v| v.as_utf8_str()),
                Some("Selecting deleted buffer")
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn md5_and_secure_hash_preserve_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.set_multibyte_value(false);
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert_lisp_string(&crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    }
    let id = eval.buffers.current_buffer().expect("current buffer").id;

    let string_md5 = builtin_md5(&mut eval, vec![raw]).unwrap();
    assert_eq!(
        string_md5.as_utf8_str(),
        Some("00594fd4f42ba43fc1ca0427a0576295")
    );

    let buffer_md5 = builtin_md5(&mut eval, vec![Value::make_buffer(id)]).unwrap();
    assert_eq!(
        buffer_md5.as_utf8_str(),
        Some("00594fd4f42ba43fc1ca0427a0576295")
    );

    let string_sha1 = builtin_secure_hash(&mut eval, vec![Value::symbol("sha1"), raw]).unwrap();
    assert_eq!(
        string_sha1.as_utf8_str(),
        Some("85e53271e14006f0265921d02d4d736cdc580b0b")
    );

    let buffer_sha1 = builtin_secure_hash(
        &mut eval,
        vec![Value::symbol("sha1"), Value::make_buffer(id)],
    )
    .unwrap();
    assert_eq!(
        buffer_sha1.as_utf8_str(),
        Some("85e53271e14006f0265921d02d4d736cdc580b0b")
    );

    let buffer_hash = builtin_buffer_hash(&mut eval, vec![Value::make_buffer(id)]).unwrap();
    assert_eq!(
        buffer_hash.as_utf8_str(),
        Some("85e53271e14006f0265921d02d4d736cdc580b0b")
    );
}

// ---- secure-hash ----

#[test]
fn secure_hash_sha256_known() {
    crate::test_utils::init_test_tracing();
    let r = call_fns_builtin!(
        builtin_secure_hash,
        vec![Value::symbol("sha256"), Value::string("abc")]
    )
    .unwrap();
    assert_eq!(
        r.as_utf8_str(),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
}

#[test]
fn secure_hash_sha1_known() {
    crate::test_utils::init_test_tracing();
    let r = call_fns_builtin!(
        builtin_secure_hash,
        vec![Value::symbol("sha1"), Value::string("abc")]
    )
    .unwrap();
    assert_eq!(
        r.as_utf8_str(),
        Some("a9993e364706816aba3e25717850c26c9cd0d89d")
    );
}

#[test]
fn secure_hash_md5_known() {
    crate::test_utils::init_test_tracing();
    let r = call_fns_builtin!(
        builtin_secure_hash,
        vec![Value::symbol("md5"), Value::string("abc")]
    )
    .unwrap();
    assert_eq!(r.as_utf8_str(), Some("900150983cd24fb0d6963f7d28e17f72"));
}

#[test]
fn secure_hash_binary_string_uses_unibyte_storage() {
    crate::test_utils::init_test_tracing();
    let r = call_fns_builtin!(
        builtin_secure_hash,
        vec![
            Value::symbol("sha1"),
            Value::string("abc"),
            Value::NIL,
            Value::NIL,
            Value::T,
        ]
    )
    .unwrap();

    let ls = r
        .as_lisp_string()
        .expect("binary secure-hash should return a string");
    assert_eq!(ls.sbytes(), 20);
    assert_eq!(ls.as_bytes().first(), Some(&169u8));

    let printed = print::print_value_bytes(&r);
    assert_eq!(printed.first(), Some(&b'"'));
    assert_eq!(printed.last(), Some(&b'"'));
}

#[test]
fn secure_hash_subrange_semantics() {
    crate::test_utils::init_test_tracing();
    let r = call_fns_builtin!(
        builtin_secure_hash,
        vec![
            Value::symbol("sha256"),
            Value::string("abcdef"),
            Value::fixnum(1),
            Value::fixnum(4),
        ]
    )
    .unwrap();
    assert_eq!(
        r.as_utf8_str(),
        Some("a6b0f90d2ac2b8d1f250c687301aef132049e9016df936680e81fa7bc7d81d70")
    );
}

#[test]
fn secure_hash_invalid_algorithm_errors() {
    crate::test_utils::init_test_tracing();
    match call_fns_builtin!(
        builtin_secure_hash,
        vec![Value::symbol("no-such"), Value::string("abc")]
    ) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|v| v.as_utf8_str()),
                Some("Invalid algorithm arg: no-such")
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn secure_hash_invalid_algorithm_type_errors() {
    crate::test_utils::init_test_tracing();
    match call_fns_builtin!(
        builtin_secure_hash,
        vec![Value::fixnum(1), Value::string("abc")]
    ) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("symbolp")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn secure_hash_invalid_object_errors() {
    crate::test_utils::init_test_tracing();
    match call_fns_builtin!(
        builtin_secure_hash,
        vec![Value::symbol("sha256"), Value::fixnum(123)]
    ) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|v| v.as_utf8_str()),
                Some("Invalid object argument")
            );
            assert_eq!(sig.data.get(1), Some(&Value::fixnum(123)));
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn secure_hash_eval_buffer_sha1() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("abc");
    }
    let id = eval.buffers.current_buffer().expect("current buffer").id;
    let r = builtin_secure_hash(
        &mut eval,
        vec![Value::symbol("sha1"), Value::make_buffer(id)],
    )
    .unwrap();
    assert_eq!(
        r.as_utf8_str(),
        Some("a9993e364706816aba3e25717850c26c9cd0d89d")
    );
}

#[test]
fn secure_hash_encodes_multibyte_buffer_raw_byte_characters_for_writing() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.insert_lisp_string(&crate::heap_types::LispString::from_unibyte(vec![
            65, 195, 169, 90,
        ]));
    }
    eval.set_variable("coding-system-for-write", Value::symbol("utf-8-unix"));
    let id = eval.buffers.current_buffer().expect("current buffer").id;

    let result = builtin_secure_hash(
        &mut eval,
        vec![Value::symbol("sha256"), Value::make_buffer(id)],
    )
    .expect("secure-hash should encode the buffer through its write coding");

    assert_eq!(
        result.as_utf8_str(),
        Some("6a1917777ebb7105da25b045353aeda24a7a9863e4d1ab0d72e1dc5f7d482257")
    );
}

#[test]
fn secure_hash_eval_buffer_range_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("abc");
    }
    let id = eval.buffers.current_buffer().expect("current buffer").id;

    match builtin_secure_hash(
        &mut eval,
        vec![
            Value::symbol("sha1"),
            Value::make_buffer(id),
            Value::fixnum(5),
        ],
    ) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(5), Value::NIL]);
        }
        other => panic!("expected args-out-of-range signal, got {other:?}"),
    }
}

#[test]
fn secure_hash_eval_buffer_index_type_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let id = eval.buffers.current_buffer().expect("current buffer").id;

    match builtin_secure_hash(
        &mut eval,
        vec![
            Value::symbol("sha1"),
            Value::make_buffer(id),
            Value::T,
            Value::fixnum(3),
        ],
    ) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data.first(),
                Some(&Value::symbol("integer-or-marker-p"))
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn secure_hash_eval_buffer_marker_range() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("abc");
    }
    let id = eval.buffers.current_buffer().expect("current buffer").id;
    let marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.buffers,
        id,
        LispCharPos1::new(2),
        false,
    );
    let r = builtin_secure_hash(
        &mut eval,
        vec![
            Value::symbol("sha1"),
            Value::make_buffer(id),
            marker,
            Value::fixnum(4),
        ],
    )
    .unwrap();
    assert_eq!(
        r.as_utf8_str(),
        Some("5b2505039ac5af9e197f5dad04113906a9cf9a2a")
    );
}

#[test]
fn secure_hash_eval_deleted_buffer_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let id = eval.buffers.create_buffer("*secure-doomed*");
    assert!(eval.buffers.kill_buffer(id));

    match builtin_secure_hash(
        &mut eval,
        vec![Value::symbol("sha1"), Value::make_buffer(id)],
    ) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|v| v.as_utf8_str()),
                Some("Selecting deleted buffer")
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn buffer_hash_eval_current_buffer_sha1() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buf = eval.buffers.current_buffer_mut().expect("current buffer");
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
        buf.point_min_emacs_byte_pos().get(),
        buf.point_max_emacs_byte_pos().get(),
    ));
    buf.insert("abc");
    let r = builtin_buffer_hash(&mut eval, vec![]).unwrap();
    assert_eq!(
        r.as_utf8_str(),
        Some("a9993e364706816aba3e25717850c26c9cd0d89d")
    );
}

#[test]
fn buffer_hash_eval_by_name_sha1() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let buf = eval.buffers.current_buffer_mut().expect("current buffer");
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
        buf.point_min_emacs_byte_pos().get(),
        buf.point_max_emacs_byte_pos().get(),
    ));
    buf.insert("abc");
    let name = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .name_value();
    let r = builtin_buffer_hash(&mut eval, vec![name]).unwrap();
    assert_eq!(
        r.as_utf8_str(),
        Some("a9993e364706816aba3e25717850c26c9cd0d89d")
    );
}

/// Bug 8: buffer-hash hashes the *whole* buffer (BUF_BEG..BUF_Z_BYTE),
/// ignoring narrowing — unlike md5/secure-hash, which use the accessible
/// region. A narrowed buffer must hash the same as the same un-narrowed
/// content.
///
/// GNU oracle:
///   (with-temp-buffer (insert "0123456789") (narrow-to-region 3 7)
///     (equal (buffer-hash)
///            (with-temp-buffer (insert "0123456789") (buffer-hash))))
///   => t
#[test]
fn buffer_hash_ignores_narrowing_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("0123456789");
    }

    // Hash of the whole, un-narrowed buffer.
    let full_hash = builtin_buffer_hash(&mut eval, vec![]).unwrap();

    // Narrow to lisp region 3..7, which is the accessible text "2345"
    // (bytes 2..6 for this ASCII content), matching GNU's
    // (narrow-to-region 3 7).
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(2, 6));
        assert_eq!(buf.buffer_string(), "2345", "narrowing is in effect");
    }
    let narrowed_hash = builtin_buffer_hash(&mut eval, vec![]).unwrap();

    assert_eq!(
        narrowed_hash.as_utf8_str(),
        full_hash.as_utf8_str(),
        "buffer-hash must ignore narrowing and hash the whole buffer"
    );
    // Sanity: it is the sha1 of the whole "0123456789", not of "3456".
    assert_eq!(
        full_hash.as_utf8_str(),
        Some("87acec17cd9dcd20a716cc2cf67417b71c8a7016")
    );
}

#[test]
fn buffer_hash_eval_missing_name_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    match builtin_buffer_hash(&mut eval, vec![Value::string("*missing*")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|v| v.as_utf8_str()),
                Some("No buffer named *missing*")
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

// ---- equal-including-properties ----

#[test]
fn equal_including_properties_strings() {
    crate::test_utils::init_test_tracing();
    let r =
        builtin_equal_including_properties(vec![Value::string("hello"), Value::string("hello")])
            .unwrap();
    assert!(r.is_truthy());
}

#[test]
fn equal_including_properties_distinguishes_string_text_properties() {
    crate::test_utils::init_test_tracing();
    let with_props = Value::string("abcd");
    crate::emacs_core::value::set_string_text_properties_for_value(
        with_props,
        vec![crate::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 3,
            plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
        }],
    );
    let plain = Value::string("abcd");

    assert!(crate::emacs_core::value::equal_value(
        &with_props,
        &plain,
        0
    ));

    let equal_props = builtin_equal_including_properties(vec![with_props, plain])
        .expect("equal-including-properties");
    assert!(equal_props.is_nil());
}

#[test]
fn equal_including_properties_recurses_into_cons_string_text_properties() {
    crate::test_utils::init_test_tracing();
    let with_props = Value::string("abcd");
    crate::emacs_core::value::set_string_text_properties_for_value(
        with_props,
        vec![crate::emacs_core::value::StringTextPropertyRun {
            start: 1,
            end: 3,
            plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
        }],
    );
    let left = Value::cons(with_props, Value::NIL);
    let right = Value::cons(Value::string("abcd"), Value::NIL);

    assert!(crate::emacs_core::value::equal_value(&left, &right, 0));

    let equal_props =
        builtin_equal_including_properties(vec![left, right]).expect("equal-including-properties");
    assert!(equal_props.is_nil());
}

/// `equal-including-properties` must honor `symbols-with-pos-enabled` exactly
/// like plain `equal`: GNU's `Fequal_including_properties` shares the same
/// `internal_equal` that unwraps position-symbols when the flag is set.  The
/// byte-compiler binds the flag to `t`, so a hardcoded `false` on the entry
/// path silently diverged (returning nil where GNU returns t).  Probed against
/// GNU Emacs 31: flag off -> (nil nil); flag on -> (t t).
#[test]
fn equal_including_properties_honors_symbols_with_pos_enabled() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    // Flag off (default): a position-symbol differs from another position of
    // the same symbol AND from its bare symbol.
    let off = ev.eval_str(
        "(let ((a (position-symbol 'foo 5)) (b (position-symbol 'foo 9)))
           (list (equal-including-properties a b)
                 (equal-including-properties a 'foo)))",
    );
    assert_eq!(
        crate::emacs_core::error::format_eval_result(&off),
        "OK (nil nil)",
        "flag off: position-symbols are distinct (GNU 31 parity)"
    );
    // Flag on: positions are ignored, so both compare eq-wise -> t.
    let on = ev.eval_str(
        "(let ((a (position-symbol 'foo 5)) (b (position-symbol 'foo 9)))
           (let ((symbols-with-pos-enabled t))
             (list (equal-including-properties a b)
                   (equal-including-properties a 'foo))))",
    );
    assert_eq!(
        crate::emacs_core::error::format_eval_result(&on),
        "OK (t t)",
        "flag on: position ignored, compared eq-wise (GNU 31 parity)"
    );
}

/// GNU Emacs 31 parity for the fast-path-relevant edges of
/// `equal-including-properties`: distinct NaN boxes (bitwise float equality),
/// `-0.0` vs `0.0` (distinct bits -> nil), int vs float, records, bool-vectors,
/// unibyte-vs-multibyte strings (ASCII equal; high bytes differ by SBYTES),
/// property-order-independence, and propertized-vs-plain.  The whole vector was
/// probed against GNU 31 and pins to `(t nil nil t nil t nil t nil t nil t)`.
#[test]
fn equal_including_properties_gnu_parity_edges() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let edges = ev.eval_str(
        "(list
           (equal-including-properties (/ 0.0 0.0) (/ 0.0 0.0))
           (equal-including-properties -0.0 0.0)
           (equal-including-properties 1 1.0)
           (equal-including-properties (record 'r 1 2) (record 'r 1 2))
           (equal-including-properties (record 'r 1 2) (record 'r 1 3))
           (equal-including-properties (bool-vector t nil t) (bool-vector t nil t))
           (equal-including-properties (bool-vector t nil t) (bool-vector t nil nil))
           (equal-including-properties (string-to-unibyte \"abc\") (string-to-multibyte \"abc\"))
           (let ((u (unibyte-string 200 201))
                 (m (string-to-multibyte (unibyte-string 200 201))))
             (equal-including-properties u m))
           (equal-including-properties (propertize \"ab\" 'a 1 'b 2)
                                       (propertize \"ab\" 'b 2 'a 1))
           (equal-including-properties (propertize \"abc\" 'face 'bold) \"abc\")
           (equal-including-properties (propertize \"abc\" 'face 'bold)
                                       (propertize \"abc\" 'face 'bold)))",
    );
    assert_eq!(
        crate::emacs_core::error::format_eval_result(&edges),
        "OK (t nil nil t nil t nil t nil t nil t)",
        "equal-including-properties edge behavior must match GNU Emacs 31"
    );
}

/// Per-op timing helper: warm once, then take the min wall-time over `rounds`
/// of an `inner`-iteration loop and report nanoseconds per call.
#[cfg(test)]
fn eip_bench_ns(rounds: u32, inner: u32, mut f: impl FnMut() -> bool) -> f64 {
    let mut warm = false;
    for _ in 0..inner {
        warm ^= f();
    }
    std::hint::black_box(warm);
    let mut best = std::time::Duration::MAX;
    for _ in 0..rounds {
        let t = std::time::Instant::now();
        let mut acc = false;
        for _ in 0..inner {
            acc ^= f();
        }
        best = best.min(t.elapsed());
        std::hint::black_box(acc);
    }
    best.as_nanos() as f64 / inner as f64
}

/// Micro-benchmark for `equal-including-properties` (Ladder Task 2). Four cases:
/// (a) eq-identical large list (eq-bits fast path), (b) propertyless equal 1 KiB
/// strings (length+memcmp+propertyless short-circuit), (c) propertied equal
/// strings (interval walk; the clone-free borrow must not regress it), (d) deep
/// structurally-equal list (full recursion).  Run:
///   cargo nextest run -p neovm-core --release --run-ignored ignored-only \
///     --no-capture equal_including_properties_bench
#[test]
#[ignore = "micro benchmark; run explicitly in release with --no-capture"]
fn equal_including_properties_bench() {
    use crate::buffer::text_props::TextPropertyTable;
    use crate::emacs_core::value::{
        StringTextPropertyRun, get_string_text_properties_table_for_value,
        set_string_text_properties_for_value, try_equal_value_including_properties,
    };
    crate::test_utils::init_test_tracing();

    // (a) large list compared to ITSELF -> bit-equal short-circuit.
    let big = {
        let mut v = Value::NIL;
        for i in 0..500 {
            v = Value::cons(Value::fixnum(i), v);
        }
        v
    };
    // (b) two distinct propertyless 1 KiB strings with identical content.
    let s1 = Value::string("x".repeat(1024));
    let s2 = Value::string("x".repeat(1024));
    // (c) two distinct strings with identical content AND identical properties.
    let mk_propertied = || {
        let s = Value::string("y".repeat(64));
        set_string_text_properties_for_value(
            s,
            vec![StringTextPropertyRun {
                start: 8,
                end: 40,
                plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
            }],
        );
        s
    };
    let p1 = mk_propertied();
    let p2 = mk_propertied();
    // (d) two distinct deep structurally-equal lists.
    let mk_deep = || {
        let mut v = Value::NIL;
        for i in 0..200 {
            v = Value::cons(Value::cons(Value::fixnum(i), Value::string("k")), v);
        }
        v
    };
    let d1 = mk_deep();
    let d2 = mk_deep();

    let eq = |a: &Value, b: &Value| try_equal_value_including_properties(a, b, 0).unwrap();

    let a_ns = eip_bench_ns(50, 20_000, || eq(&big, &big));
    let b_ns = eip_bench_ns(50, 5_000, || eq(&s1, &s2));
    let c_ns = eip_bench_ns(50, 5_000, || eq(&p1, &p2));
    let d_ns = eip_bench_ns(50, 2_000, || eq(&d1, &d2));

    // Before/after decomposition of the property-walk sub-path this change
    // touched, using only public APIs (no second build). "Removed" is the exact
    // per-compare work the string arm no longer does: for a propertied compare,
    // the two interval-tree CLONES it used to make; for a propertyless compare,
    // the None/None interval walk it now short-circuits.  So the pre-change cost
    // of case (c) was ~= c_ns + removed_clone, and case (b) ~= b_ns + removed_none.
    let removed_clone_ns = eip_bench_ns(50, 5_000, || {
        let l = get_string_text_properties_table_for_value(p1);
        let r = get_string_text_properties_table_for_value(p2);
        let used = l.is_some() && r.is_some();
        std::hint::black_box(&l);
        std::hint::black_box(&r);
        used
    });
    let removed_none_ns = eip_bench_ns(50, 5_000, || {
        TextPropertyTable::equal_including_property_values(None, None, 1024)
    });

    panic!(
        "BENCH equal-including-properties (ns/call, min of 50): \
         (a) eq-identical 500-list {a_ns:.1} | \
         (b) propertyless =1KiB strings {b_ns:.1} | \
         (c) propertied =64B strings {c_ns:.1} | \
         (d) deep =200-list {d_ns:.1} || \
         clone-elim removed/compare: propertied 2x-clone {removed_clone_ns:.1} (c before~={:.1}), \
         propertyless None-walk {removed_none_ns:.1} (b before~={:.1})",
        c_ns + removed_clone_ns,
        b_ns + removed_none_ns,
    );
}

/// Macro datapoint (Ladder Task 2): wall-time of the byte-compile workload that
/// records `equal-including-properties` at 38% of builtin calls (mirrors
/// `vm_subr_mix_byte_compile` but interpreted+timed, no vm-profile). Run:
///   cargo nextest run -p neovm-core --release --run-ignored ignored-only \
///     --no-capture equal_including_properties_byte_compile_macro_bench
#[test]
#[ignore = "macro benchmark; run explicitly in release with --no-capture"]
fn equal_including_properties_byte_compile_macro_bench() {
    crate::test_utils::init_test_tracing();
    // SAFETY: nextest runs each test in its own process; set before the VM
    // reads the JIT gate so the byte-compiler stays interpreted.
    unsafe { std::env::set_var("NEOVM_JIT", "0") };
    let mut ev = crate::test_utils::runtime_startup_context();
    let mut body = String::new();
    for i in 0..30 {
        body.push_str(&format!(
            "(setq acc (cons (list {i} (format \"s%d\" n) (assq 'k tbl)) acc)) \
             (when (> (length acc) 40) (setq acc (nthcdr 2 acc))) \
             (setq s (concat s (substring (symbol-name 'sym{i}) 0 2))) ",
        ));
    }
    let defun = format!(
        "(progn (defun sm-work (n) \
           (let ((acc nil) (s \"\") (tbl '((k . 1) (j . 2)))) {body} (list acc s))) t)"
    );
    ev.eval_str(&defun).expect("defun sm-work");
    // Warm the byte-compiler.
    ev.eval_str("(progn (byte-compile 'sm-work) t)")
        .expect("warm byte-compile");

    let rounds = 8u32;
    let mut best = std::time::Duration::MAX;
    for _ in 0..rounds {
        let t = std::time::Instant::now();
        ev.eval_str(&defun).expect("re-defun sm-work");
        ev.eval_str("(progn (byte-compile 'sm-work) t)")
            .expect("byte-compile sm-work");
        best = best.min(t.elapsed());
    }
    panic!("BENCH byte-compile sm-work (min of {rounds} defun+compile cycles): {best:?}");
}

#[test]
fn string_make_multibyte_passthrough_ascii() {
    crate::test_utils::init_test_tracing();
    let s = Value::string("abc");
    let r = builtin_string_make_multibyte(vec![s]).unwrap();
    assert!(crate::emacs_core::value::eq_value(&s, &r));
    let ls = r.as_lisp_string().unwrap();
    assert!(!ls.is_multibyte());
    assert_eq!(r.as_utf8_str(), Some("abc"));
}

#[test]
fn string_make_multibyte_promotes_unibyte_byte() {
    crate::test_utils::init_test_tracing();
    let v = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let r = builtin_string_make_multibyte(vec![v]).unwrap();
    let ls = r.as_lisp_string().unwrap();
    assert!(ls.is_multibyte());
    let codes: Vec<u32> = crate::emacs_core::builtins::lisp_string_char_codes(ls);
    assert_eq!(codes, vec![0x3FFFFF]);
}

#[test]
fn string_make_unibyte_passthrough_ascii() {
    crate::test_utils::init_test_tracing();
    let s = Value::string("abc");
    let r = builtin_string_make_unibyte(vec![s]).unwrap();
    assert!(crate::emacs_core::value::eq_value(&s, &r));
    let ls = r.as_lisp_string().unwrap();
    assert!(!ls.is_multibyte());
    assert_eq!(ls.as_bytes(), b"abc");
}

#[test]
fn string_make_unibyte_truncates_unicode_char_code() {
    crate::test_utils::init_test_tracing();
    let r = builtin_string_make_unibyte(vec![Value::string("😀")]).unwrap();
    let ls = r.as_lisp_string().unwrap();
    assert!(!ls.is_multibyte());
    // 😀 is U+1F600, truncated to byte: low byte is 0x00
    assert_eq!(ls.as_bytes(), &[0]);
}

// ---- compare-strings ----

#[test]
fn compare_strings_unifies_unibyte_and_multibyte_eight_bit() {
    crate::test_utils::init_test_tracing();
    // GNU compare-strings treats a unibyte raw byte and the corresponding
    // multibyte eight-bit char as the same character.
    let mut buf = [0u8; 8];
    let len = crate::emacs_core::emacs_char::char_string(
        crate::emacs_core::emacs_char::byte8_to_char(0xFF),
        &mut buf,
    );
    let multibyte_eight_bit = Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(
        buf[..len].to_vec(),
    ));
    let unibyte = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let r = builtin_compare_strings(vec![
        unibyte,
        Value::NIL,
        Value::NIL,
        multibyte_eight_bit,
        Value::NIL,
        Value::NIL,
    ])
    .unwrap();
    assert!(
        r.is_t(),
        "unibyte 0xFF should compare equal to the multibyte eight-bit char"
    );
}

#[test]
fn compare_strings_equal() {
    crate::test_utils::init_test_tracing();
    let r = builtin_compare_strings(vec![
        Value::string("hello"),
        Value::NIL,
        Value::NIL,
        Value::string("hello"),
        Value::NIL,
        Value::NIL,
    ])
    .unwrap();
    assert!(r.is_t());
}

#[test]
fn compare_strings_less() {
    crate::test_utils::init_test_tracing();
    let r = builtin_compare_strings(vec![
        Value::string("abc"),
        Value::NIL,
        Value::NIL,
        Value::string("abd"),
        Value::NIL,
        Value::NIL,
    ])
    .unwrap();
    // First diff at position 3, "c" < "d" so negative
    assert_eq!(r.as_int(), Some(-3));
}

#[test]
fn compare_strings_greater() {
    crate::test_utils::init_test_tracing();
    let r = builtin_compare_strings(vec![
        Value::string("abd"),
        Value::NIL,
        Value::NIL,
        Value::string("abc"),
        Value::NIL,
        Value::NIL,
    ])
    .unwrap();
    assert_eq!(r.as_int(), Some(3));
}

#[test]
fn compare_strings_ignore_case() {
    crate::test_utils::init_test_tracing();
    let r = builtin_compare_strings(vec![
        Value::string("Hello"),
        Value::NIL,
        Value::NIL,
        Value::string("hello"),
        Value::NIL,
        Value::NIL,
        Value::T, // IGNORE-CASE
    ])
    .unwrap();
    assert!(r.is_t());
}

#[test]
fn compare_strings_subrange() {
    crate::test_utils::init_test_tracing();
    // Compare "hel" from "hello" (chars 1-3) with "hel" from "help" (chars 1-3)
    let r = builtin_compare_strings(vec![
        Value::string("hello"),
        Value::fixnum(1),
        Value::fixnum(3),
        Value::string("help"),
        Value::fixnum(1),
        Value::fixnum(3),
    ])
    .unwrap();
    assert!(r.is_t());
}

#[test]
fn compare_strings_length_diff() {
    crate::test_utils::init_test_tracing();
    let r = builtin_compare_strings(vec![
        Value::string("ab"),
        Value::NIL,
        Value::NIL,
        Value::string("abc"),
        Value::NIL,
        Value::NIL,
    ])
    .unwrap();
    // "ab" shorter — negative
    assert!(r.as_int().unwrap() < 0);
}

#[test]
fn compare_strings_negative_bounds_and_too_large_end_match_gnu() {
    crate::test_utils::init_test_tracing();

    let negative_bounds = builtin_compare_strings(vec![
        Value::string("abcdef"),
        Value::fixnum(-3),
        Value::fixnum(-1),
        Value::string("cd"),
        Value::NIL,
        Value::NIL,
    ])
    .unwrap();
    assert_eq!(negative_bounds.as_int(), Some(1));

    let clamped_end = builtin_compare_strings(vec![
        Value::string("abc"),
        Value::fixnum(0),
        Value::fixnum(99),
        Value::string("abc"),
        Value::fixnum(0),
        Value::fixnum(99),
    ])
    .unwrap();
    assert_eq!(clamped_end, Value::T);
}

#[test]
fn compare_strings_reversed_and_out_of_range_bounds_signal_like_gnu() {
    crate::test_utils::init_test_tracing();

    for args in [
        vec![
            Value::string("abc"),
            Value::fixnum(3),
            Value::fixnum(2),
            Value::string("abc"),
            Value::NIL,
            Value::NIL,
        ],
        vec![
            Value::string("abc"),
            Value::fixnum(9),
            Value::NIL,
            Value::string(""),
            Value::NIL,
            Value::NIL,
        ],
        vec![
            Value::string("abc"),
            Value::NIL,
            Value::fixnum(-9),
            Value::string(""),
            Value::NIL,
            Value::NIL,
        ],
    ] {
        match builtin_compare_strings(args) {
            Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "args-out-of-range"),
            other => panic!("expected args-out-of-range signal, got {other:?}"),
        }
    }
}

#[test]
fn compare_strings_ignore_case_uses_upcase_like_gnu() {
    crate::test_utils::init_test_tracing();

    let result = builtin_compare_strings(vec![
        Value::string("İ"),
        Value::NIL,
        Value::NIL,
        Value::string("i"),
        Value::NIL,
        Value::NIL,
        Value::T,
    ])
    .unwrap();
    assert_eq!(result.as_int(), Some(1));
}

// ---- string-version-lessp ----

fn call_string_version_lessp(args: Vec<Value>) -> EvalResult {
    builtin_string_version_lessp(&mut Context::new(), args)
}

fn call_string_collate_lessp(args: Vec<Value>) -> EvalResult {
    builtin_string_collate_lessp(&mut Context::new(), args)
}

fn call_string_collate_equalp(args: Vec<Value>) -> EvalResult {
    builtin_string_collate_equalp(&mut Context::new(), args)
}

#[test]
fn version_lessp_basic() {
    crate::test_utils::init_test_tracing();
    let r = call_string_version_lessp(vec![Value::string("foo2"), Value::string("foo10")]).unwrap();
    assert!(r.is_truthy());
}

#[test]
fn version_lessp_equal() {
    crate::test_utils::init_test_tracing();
    let r =
        call_string_version_lessp(vec![Value::string("foo10"), Value::string("foo10")]).unwrap();
    assert!(r.is_nil());
}

#[test]
fn version_lessp_alpha() {
    crate::test_utils::init_test_tracing();
    let r = call_string_version_lessp(vec![Value::string("abc"), Value::string("abd")]).unwrap();
    assert!(r.is_truthy());
}

#[test]
fn version_lessp_numeric_segments() {
    crate::test_utils::init_test_tracing();
    let r = call_string_version_lessp(vec![
        Value::string("emacs-27.1"),
        Value::string("emacs-27.2"),
    ])
    .unwrap();
    assert!(r.is_truthy());
}

#[test]
fn version_lessp_leading_zero_runs_match_gnu() {
    crate::test_utils::init_test_tracing();
    let equal_numeric = call_string_version_lessp(vec![Value::string("1"), Value::string("001")])
        .expect("string-version-lessp should evaluate");
    assert!(equal_numeric.is_nil());

    let reverse_equal_numeric =
        call_string_version_lessp(vec![Value::string("001"), Value::string("1")])
            .expect("string-version-lessp should evaluate");
    assert!(reverse_equal_numeric.is_nil());
}

/// Regression: a string allocated AFTER an evaluator has been dropped must be
/// readable by the next evaluator.
///
/// Before the `Context` drop hook existed, `Context::new` installed its boxed
/// `TaggedHeap` in the thread-local allocation slot and nothing ever removed
/// it, so dropping the evaluator left that slot pointing at freed storage.
/// The next `Value::string` allocated INTO the freed heap and the following
/// `Context::new` reused the same storage, so reading the string's header
/// produced a garbage `sbytes()` and aborted the process
/// ("memory allocation of N bytes failed"). This is the two-call shape of
/// `version_lessp_leading_zero_runs_match_gnu` reduced to its cause.
#[test]
fn string_allocated_after_a_dropped_evaluator_stays_readable() {
    crate::test_utils::init_test_tracing();
    // Repeat the construct-then-evaluate cycle: each iteration drops the
    // previous evaluator, and under the bug the next allocation lands in that
    // freed heap while the following `Context::new` reuses the same storage.
    for _ in 0..8 {
        let args = vec![Value::string("001"), Value::string("1")];
        let mut ev = Context::new();
        let r =
            builtin_string_version_lessp(&mut ev, args).expect("string-version-lessp evaluates");
        assert!(r.is_nil());
    }
}

// ---- string-collate-lessp ----

#[test]
fn collate_lessp_basic() {
    crate::test_utils::init_test_tracing();
    let r = call_string_collate_lessp(vec![Value::string("abc"), Value::string("abd")]).unwrap();
    assert!(r.is_truthy());
}

#[test]
fn collate_lessp_ignore_case() {
    crate::test_utils::init_test_tracing();
    let r = call_string_collate_lessp(vec![
        Value::string("ABC"),
        Value::string("abd"),
        Value::NIL, // locale
        Value::T,   // ignore-case
    ])
    .unwrap();
    assert!(r.is_truthy());
}

#[test]
fn collate_lessp_rejects_non_string_locale() {
    crate::test_utils::init_test_tracing();
    let err = call_string_collate_lessp(vec![
        Value::string("a"),
        Value::string("b"),
        Value::fixnum(42),
    ])
    .expect_err("non-nil locale must be a string");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(
                sig.symbol,
                Value::symbol("wrong-type-argument").as_symbol_id().unwrap()
            );
            assert_eq!(sig.data[0], Value::symbol("stringp"));
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

#[test]
fn collate_lessp_invalid_locale_signals_error() {
    crate::test_utils::init_test_tracing();
    let err = call_string_collate_lessp(vec![
        Value::string("a"),
        Value::string("b"),
        Value::string("neomacs-invalid-locale"),
    ])
    .expect_err("invalid explicit locale should signal error");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol, Value::symbol("error").as_symbol_id().unwrap());
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

// ---- string-collate-equalp ----

#[test]
fn collate_equalp_basic() {
    crate::test_utils::init_test_tracing();
    let r = call_string_collate_equalp(vec![Value::string("abc"), Value::string("abc")]).unwrap();
    assert!(r.is_truthy());
}

#[test]
fn collate_equalp_ignore_case() {
    crate::test_utils::init_test_tracing();
    let r = call_string_collate_equalp(vec![
        Value::string("ABC"),
        Value::string("abc"),
        Value::NIL,
        Value::T,
    ])
    .unwrap();
    assert!(r.is_truthy());
}

#[test]
fn collate_equalp_different() {
    crate::test_utils::init_test_tracing();
    let r = call_string_collate_equalp(vec![Value::string("abc"), Value::string("abd")]).unwrap();
    assert!(r.is_nil());
}

#[test]
fn collate_equalp_rejects_non_string_locale() {
    crate::test_utils::init_test_tracing();
    let err = call_string_collate_equalp(vec![
        Value::string("a"),
        Value::string("a"),
        Value::symbol("not-a-locale"),
    ])
    .expect_err("non-nil locale must be a string");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(
                sig.symbol,
                Value::symbol("wrong-type-argument").as_symbol_id().unwrap()
            );
            assert_eq!(sig.data[0], Value::symbol("stringp"));
        }
        other => panic!("expected signal, got {other:?}"),
    }
}

// ---- widget-get / widget-put ----

#[test]
fn widget_get_found() {
    crate::test_utils::init_test_tracing();
    // Widget: (button :tag "OK" :value 42)
    let widget = Value::list(vec![
        Value::symbol("button"),
        Value::keyword("tag"),
        Value::string("OK"),
        Value::keyword("value"),
        Value::fixnum(42),
    ]);
    let r = builtin_widget_get(vec![widget, Value::keyword("value")]).unwrap();
    assert!(r.is_fixnum());
}

#[test]
fn widget_get_not_found() {
    crate::test_utils::init_test_tracing();
    let widget = Value::list(vec![
        Value::symbol("button"),
        Value::keyword("tag"),
        Value::string("OK"),
    ]);
    let r = builtin_widget_get(vec![widget, Value::keyword("missing")]).unwrap();
    assert!(r.is_nil());
}

#[test]
fn widget_put_existing() {
    crate::test_utils::init_test_tracing();
    let widget = Value::list(vec![
        Value::symbol("button"),
        Value::keyword("value"),
        Value::fixnum(1),
    ]);
    let r = builtin_widget_put(vec![widget, Value::keyword("value"), Value::fixnum(99)]).unwrap();
    assert!(r.is_fixnum());

    // Verify it was modified
    let got = builtin_widget_get(vec![widget, Value::keyword("value")]).unwrap();
    assert!(got.is_fixnum());
}

#[test]
fn widget_put_new_property() {
    crate::test_utils::init_test_tracing();
    let widget = Value::list(vec![Value::symbol("button")]);
    let r =
        builtin_widget_put(vec![widget, Value::keyword("tag"), Value::string("Hello")]).unwrap();
    assert_eq!(r.as_utf8_str(), Some("Hello"));

    let got = builtin_widget_get(vec![widget, Value::keyword("tag")]).unwrap();
    assert_eq!(got.as_utf8_str(), Some("Hello"));
}

#[test]
fn widget_apply_missing_property_signals_void_function_nil() {
    crate::test_utils::init_test_tracing();
    let widget = Value::list(vec![Value::symbol("button")]);
    let mut ctx = test_eval_ctx();
    let err = builtin_widget_apply(&mut ctx, vec![widget, Value::keyword("action")])
        .expect_err("widget-apply should signal void-function for missing property");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "void-function");
            assert_eq!(sig.data, vec![Value::NIL]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn widget_apply_calls_symbol_property_with_widget_as_first_arg() {
    crate::test_utils::init_test_tracing();
    let widget = Value::list(vec![
        Value::symbol("button"),
        Value::keyword("action"),
        Value::symbol("car"),
    ]);
    let mut ctx = test_eval_ctx();
    let r = builtin_widget_apply(&mut ctx, vec![widget, Value::keyword("action")]).unwrap();
    assert_eq!(r, Value::symbol("button"));
}

#[test]
fn widget_apply_passes_rest_arguments() {
    crate::test_utils::init_test_tracing();
    let widget = Value::list(vec![
        Value::symbol("button"),
        Value::keyword("action"),
        Value::symbol("list"),
    ]);
    let mut ctx = test_eval_ctx();
    let r = builtin_widget_apply(
        &mut ctx,
        vec![
            widget,
            Value::keyword("action"),
            Value::fixnum(1),
            Value::fixnum(2),
        ],
    )
    .unwrap();
    assert_eq!(
        r,
        Value::list(vec![widget, Value::fixnum(1), Value::fixnum(2)])
    );
}

#[test]
fn widget_apply_non_callable_property_signals_invalid_function() {
    crate::test_utils::init_test_tracing();
    let widget = Value::list(vec![
        Value::symbol("button"),
        Value::keyword("action"),
        Value::fixnum(7),
    ]);
    let mut ctx = test_eval_ctx();
    let err = builtin_widget_apply(&mut ctx, vec![widget, Value::keyword("action")])
        .expect_err("widget-apply should reject non-callable property values");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "invalid-function");
            assert_eq!(sig.data, vec![Value::fixnum(7)]);
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

// ---- Line break in base64 ----

#[test]
fn base64_encode_line_break() {
    crate::test_utils::init_test_tracing();
    // A string long enough to trigger line breaks at column 76
    let long = "a".repeat(100);
    let encoded = builtin_base64_encode_string(vec![Value::string(long.clone())]).unwrap();
    let s = encoded.as_utf8_str().unwrap();
    assert!(s.contains('\n'));

    // No line break variant
    let encoded_no_lb = builtin_base64_encode_string(vec![Value::string(long), Value::T]).unwrap();
    let s2 = encoded_no_lb.as_utf8_str().unwrap();
    assert!(!s2.contains('\n'));
}

#[test]
fn base64_decode_ignores_whitespace() {
    crate::test_utils::init_test_tracing();
    // Encoded "Hello" with embedded whitespace
    let r = builtin_base64_decode_string(vec![Value::string("SGVs\nbG8=")]).unwrap();
    assert_eq!(r.as_utf8_str(), Some("Hello"));
}
