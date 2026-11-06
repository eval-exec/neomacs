use super::*;
use neovm_value::heap_types::{GnuDeclaredCharacterCount, LispString};

fn original(bytes: &[u8], chars: i128) -> Value {
    Value::heap_string(LispString::from_emacs_bytes_with_declared_chars(
        bytes.to_vec(),
        GnuDeclaredCharacterCount::try_from(chars).expect("nonnegative test count"),
    ))
}

fn reader<'a>(
    identity: &'a Value,
    snapshot: &'a LispString,
    end: usize,
    vars: crate::emacs_core::eval::VariableView<'a>,
) -> Reader<'a> {
    let range = CharacterReadRange::for_original(
        identity,
        CharRange::from_start_len(CharPos0::ZERO, CharLen::new(end)),
        EmacsBytePos::ZERO,
    )
    .expect("original source range");
    Reader::new_lisp_string(snapshot, LispReaderRange::Characters(range), vars)
}

#[test]
fn error_unread_caches_original_identity_and_payload_then_remaps_byte() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let obarray = neovm_value::symbol::Obarray::new();
    for prefix in [b"#^".as_slice(), b"#s".as_slice()] {
        let identity = original(prefix, 3);
        let payload = identity.as_lisp_string().expect("source");
        let snapshot = payload.clone();
        neovm_value::string_pos_cache::reset_string_pos_cache();
        let mut parser = reader(
            &identity,
            &snapshot,
            3,
            crate::emacs_core::eval::VariableView::canonical(
                neovm_value::symbol::CanonicalAccess::TEST,
                &obarray,
            ),
        );
        assert_eq!(
            parser
                .read_form()
                .expect_err("dispatch error")
                .message
                .as_bytes(),
            prefix
        );
        assert_eq!(parser.pos, 1);
        assert_eq!(parser.character_cursor.position(), Some(CharPos0::new(2)));
        assert_eq!(parser.current_code(), Some(u32::from(prefix[1])));
        // A cached snapshot pointer would be rejected and map char1 to byte1.
        assert_eq!(
            neovm_value::string_pos_cache::string_char_to_byte(identity, payload, 1),
            0
        );
    }
}

#[test]
fn native_same_source_read_observes_error_cache_and_fresh_control() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    for (prefix, name) in [(b"#^".as_slice(), "^"), (b"#s".as_slice(), "s")] {
        let identity = original(prefix, 3);
        neovm_value::string_pos_cache::reset_string_pos_cache();
        crate::emacs_core::reader::builtin_read_from_string(
            &mut context,
            vec![identity, Value::fixnum(0), Value::fixnum(3)],
        )
        .expect_err("initial dispatch error");
        crate::emacs_core::reader::builtin_read_from_string(
            &mut context,
            vec![identity, Value::fixnum(1), Value::fixnum(2)],
        )
        .expect_err("same source maps to cached byte zero");
        let fresh = original(prefix, 3);
        let pair = crate::emacs_core::reader::builtin_read_from_string(
            &mut context,
            vec![fresh, Value::fixnum(1), Value::fixnum(2)],
        )
        .expect("fresh source maps forward");
        assert!(pair.cons_car().is_symbol_named(name, context.symbol_mode()));
        assert_eq!(pair.cons_cdr().as_fixnum(), Some(2));
    }
}

#[test]
fn consumed_label_suffix_does_not_run_string_unget() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let obarray = neovm_value::symbol::Obarray::new();
    let identity = original(b"#1", 3);
    let payload = identity.as_lisp_string().expect("source");
    let snapshot = payload.clone();
    neovm_value::string_pos_cache::reset_string_pos_cache();
    let mut parser = reader(
        &identity,
        &snapshot,
        3,
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &obarray,
        ),
    );
    assert_eq!(parser.read_form().expect_err("label error").message, "#1");
    assert_eq!(parser.pos, 3);
    assert_eq!(parser.character_cursor.position(), Some(CharPos0::new(3)));
    assert_eq!(
        neovm_value::string_pos_cache::string_char_to_byte(identity, payload, 1),
        1
    );
}

#[test]
fn radix_digit_span_is_preserved_before_cursor_unread_remaps() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let obarray = neovm_value::symbol::Obarray::new();
    for (prefix, number) in [(b"#x1".as_slice(), 1), (b"#o7".as_slice(), 7)] {
        let identity = original(prefix, 4);
        let snapshot = identity.as_lisp_string().expect("source").clone();
        neovm_value::string_pos_cache::reset_string_pos_cache();
        let mut parser = reader(
            &identity,
            &snapshot,
            4,
            crate::emacs_core::eval::VariableView::canonical(
                neovm_value::symbol::CanonicalAccess::TEST,
                &obarray,
            ),
        );
        assert_eq!(
            parser.read_form().expect("radix number").as_fixnum(),
            Some(number)
        );
        assert_eq!(parser.pos, 2);
        assert_eq!(parser.character_cursor.position(), Some(CharPos0::new(3)));
    }
}

#[test]
fn logical_eof_does_not_update_cache_for_unread_minus_one() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let obarray = neovm_value::symbol::Obarray::new();
    let identity = original(b"ab", 1);
    let snapshot = identity.as_lisp_string().expect("source").clone();
    let mut parser = reader(
        &identity,
        &snapshot,
        1,
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &obarray,
        ),
    );
    let sentinel = original("béaz".as_bytes(), 3);
    let payload = sentinel.as_lisp_string().expect("sentinel");
    neovm_value::string_pos_cache::reset_string_pos_cache();
    assert_eq!(
        neovm_value::string_pos_cache::string_char_to_byte(sentinel, payload, 2),
        4
    );
    assert!(
        parser
            .read_form()
            .expect("limited symbol")
            .is_symbol_named("a", SymbolMode::Bare)
    );
    assert_eq!(
        neovm_value::string_pos_cache::string_char_to_byte(sentinel, payload, 1),
        3
    );
}

#[test]
fn dot_probe_defers_remapped_cursor_until_period_consumption() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let obarray = neovm_value::symbol::Obarray::new();
    let identity = original(b".", 2);
    let snapshot = identity.as_lisp_string().expect("source").clone();
    neovm_value::string_pos_cache::reset_string_pos_cache();
    let mut parser = reader(
        &identity,
        &snapshot,
        2,
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &obarray,
        ),
    );
    assert!(parser.is_dot_separator());
    assert_eq!(parser.pos, 0);
    assert_eq!(parser.character_cursor.position(), Some(CharPos0::ZERO));
    parser.bump();
    assert_eq!(parser.pos, 0);
    assert_eq!(parser.character_cursor.position(), Some(CharPos0::new(1)));
}

#[test]
fn byte_sources_do_not_bind_original_string_cache() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let obarray = neovm_value::symbol::Obarray::new();
    let input = LispString::from_utf8("name ");
    let mut parser = Reader::new_lisp_string(
        &input,
        LispReaderRange::Bytes(EmacsByteRange::new(
            EmacsBytePos::ZERO,
            EmacsBytePos::new(input.sbytes()),
        )),
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &obarray,
        ),
    );
    assert!(matches!(
        parser.original_string,
        OriginalStringCacheBinding::Uncached
    ));
    assert!(
        parser
            .read_form()
            .expect("name")
            .is_symbol_named("name", SymbolMode::Bare)
    );
}

#[test]
fn declared_count_zero_preview_uses_validated_bytes_without_character_slice() {
    let source = LispString::from_emacs_bytes_with_declared_chars(
        b"abcde".to_vec(),
        GnuDeclaredCharacterCount::try_from(0).expect("zero declared characters"),
    );
    assert_eq!(LispReadSource::new(&source).storage_slice_range(3, 4), "d");
}
