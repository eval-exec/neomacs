use super::*;
use neovm_value::heap_types::{GnuDeclaredCharacterCount, LispString};

fn source(bytes: &[u8], count: i128) -> LispString {
    LispString::from_emacs_bytes_with_declared_chars(
        bytes.to_vec(),
        GnuDeclaredCharacterCount::try_from(count).expect("test count"),
    )
}

fn logical_reader<'a>(
    input: &'a LispString,
    chars: usize,
    ob: &'a neovm_value::symbol::Obarray,
) -> Reader<'a> {
    let range = CharacterReadRange::for_source(
        input,
        CharRange::from_start_len(CharPos0::ZERO, CharLen::new(chars)),
        EmacsBytePos::ZERO,
    )
    .expect("test logical range");
    Reader::new_lisp_string(
        input,
        LispReaderRange::Characters(range),
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            ob,
        ),
    )
}

#[test]
fn declared_count_caps_fast_ascii_token_and_whitespace() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let ob = neovm_value::symbol::Obarray::new();
    let text = source(b"abc", 1);
    let mut reader = logical_reader(&text, 1, &ob);
    assert!(
        reader
            .read_form()
            .expect("one symbol")
            .is_symbol_named("a", SymbolMode::Bare)
    );
    assert_eq!(reader.pos, 1);
    assert_eq!(reader.character_cursor.position(), Some(CharPos0::new(1)));
    assert!(reader.current_code().is_none());
    let text = source(b"   a", 3);
    let mut reader = logical_reader(&text, 3, &ob);
    assert!(!reader.skip_ws_and_comments());
    assert_eq!(reader.pos, 3);
}

#[test]
fn logical_index_does_not_reverse_map_consumed_unicode_bytes() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let ob = neovm_value::symbol::Obarray::new();
    let text = source("éa".as_bytes(), 1);
    let result = LispReadSource::new(&text)
        .read_one_character_range_with_locate_syms(
            CharacterReadRange::for_source(
                &text,
                CharRange::from_start_len(CharPos0::ZERO, CharLen::new(1)),
                EmacsBytePos::ZERO,
            )
            .expect("test range"),
            SymbolPositionMode::Bare,
            crate::emacs_core::eval::VariableView::canonical(
                neovm_value::symbol::CanonicalAccess::TEST,
                &ob,
            ),
            ReadSymbolShorthandBinding::Unbound,
        )
        .expect("read")
        .expect("symbol");
    assert!(result.0.is_symbol_named("é", SymbolMode::Bare));
    assert_eq!(result.1, CharPos0::new(1));
}

#[test]
fn logical_end_stops_unicode_comment_and_string_ascii_copy() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let ob = neovm_value::symbol::Obarray::new();
    let text = source(";é\nbX".as_bytes(), 4);
    let mut reader = logical_reader(&text, 4, &ob);
    assert!(reader.skip_ws_and_comments());
    assert!(
        reader
            .read_form()
            .expect("symbol")
            .is_symbol_named("b", SymbolMode::Bare)
    );
    assert_eq!(reader.pos, 5);
    let text = source(b"\"abc\"tail", 4);
    let mut reader = logical_reader(&text, 4, &ob);
    assert_eq!(
        reader.read_form().expect_err("missing logical close").kind,
        ReadErrorKind::EndOfFile
    );
    assert_eq!(reader.pos, 4);
}

#[test]
fn error_unread_restores_both_coordinate_domains() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let ob = neovm_value::symbol::Obarray::new();
    let text = source(b"#s[tail", 3);
    let mut reader = logical_reader(&text, 3, &ob);
    let error = reader.read_form().expect_err("dispatch error");
    assert_eq!(error.position, 2);
    assert_eq!(reader.pos, 2);
    assert_eq!(reader.character_cursor.position(), Some(CharPos0::new(2)));
    assert_eq!(reader.current_code(), Some(b'[' as u32));
}

#[test]
fn logical_read_all_does_not_leak_labels_across_forms() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let ob = neovm_value::symbol::Obarray::new();
    let text = LispString::from_utf8("#1=(a) #1#");
    assert!(
        read_all_lisp_source(
            &text,
            crate::emacs_core::eval::VariableView::canonical(
                neovm_value::symbol::CanonicalAccess::TEST,
                &ob
            )
        )
        .is_err()
    );
}

#[test]
fn logical_partial_end_zero_and_end_at_schars_preserve_original_units() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let ob = neovm_value::symbol::Obarray::new();
    let text = source("bé".as_bytes(), 1);
    let source = LispReadSource::new(&text);
    for (start, length) in [(0, 0), (1, 0)] {
        assert!(
            source
                .read_one_character_range_with_locate_syms(
                    CharacterReadRange::for_source(
                        &text,
                        CharRange::from_start_len(CharPos0::new(start), CharLen::new(length)),
                        EmacsBytePos::new(text.char_to_byte_pos(start))
                    )
                    .expect("test range"),
                    SymbolPositionMode::Bare,
                    crate::emacs_core::eval::VariableView::canonical(
                        neovm_value::symbol::CanonicalAccess::TEST,
                        &ob
                    ),
                    ReadSymbolShorthandBinding::Unbound,
                )
                .expect("empty range")
                .is_none()
        );
    }
    let pair = source
        .read_one_character_range_with_locate_syms(
            CharacterReadRange::for_source(
                &text,
                CharRange::from_start_len(CharPos0::ZERO, CharLen::new(1)),
                EmacsBytePos::ZERO,
            )
            .expect("test range"),
            SymbolPositionMode::Bare,
            crate::emacs_core::eval::VariableView::canonical(
                neovm_value::symbol::CanonicalAccess::TEST,
                &ob,
            ),
            ReadSymbolShorthandBinding::Unbound,
        )
        .expect("full logical range")
        .expect("symbol");
    assert!(pair.0.is_symbol_named("b", SymbolMode::Bare));
    assert_eq!(pair.1, CharPos0::new(1));
}

#[test]
fn read_from_string_start_uses_original_source_cached_boundary() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let mut context = _context;
    let input = Value::heap_string(source("béaz".as_bytes(), 3));
    neovm_value::string_pos_cache::reset_string_pos_cache();
    let payload = input.as_lisp_string().expect("test string");
    assert_eq!(
        neovm_value::string_pos_cache::string_char_to_byte(input, payload, 2),
        4
    );
    let pair = crate::emacs_core::reader::builtin_read_from_string(
        &mut context,
        vec![input, Value::fixnum(1), Value::fixnum(2)],
    )
    .expect("cached logical source range");
    assert!(pair.cons_car().is_symbol_named("a", context.symbol_mode()));
    assert_eq!(pair.cons_cdr().as_fixnum(), Some(2));
}

#[test]
fn logical_string_reads_one_initialized_terminal_nul() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let ob = neovm_value::symbol::Obarray::new();
    let text = source(b"?", 2);
    let mut reader = logical_reader(&text, 2, &ob);
    assert_eq!(
        reader
            .read_form()
            .expect("initialized terminal character")
            .as_fixnum(),
        Some(0)
    );
    assert_eq!(reader.pos, 2);
    assert_eq!(reader.character_cursor.position(), Some(CharPos0::new(2)));
    assert!(reader.current_code().is_none());
    assert!(!reader.skip_ws_and_comments());
    assert_eq!(reader.pos, 2);
    reader.bump();
    assert_eq!(reader.pos, 2);
    let longer = source(b"?", 3);
    let bounded = logical_reader(&longer, 3, &ob);
    assert!(
        bounded
            .initialized_terminator_next(EmacsBytePos::new(2), CharLen::ZERO)
            .is_none()
    );
    let mut bytes = Reader::new_lisp_string(
        &text,
        LispReaderRange::Bytes(EmacsByteRange::new(
            EmacsBytePos::ZERO,
            EmacsBytePos::new(1),
        )),
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &ob,
        ),
    );
    assert_eq!(
        bytes
            .read_form()
            .expect_err("byte source excludes terminator")
            .kind,
        ReadErrorKind::EndOfFile
    );
    let mut partial = logical_reader(&text, 1, &ob);
    assert_eq!(
        partial
            .read_form()
            .expect_err("logical END1 excludes terminator")
            .kind,
        ReadErrorKind::EndOfFile
    );
}

#[test]
fn terminal_logical_cursor_keeps_string_comment_and_error_slices_bounded() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let ob = neovm_value::symbol::Obarray::new();
    for bytes in [b";".as_slice(), b"\"".as_slice()] {
        let text = source(bytes, 2);
        let mut reader = logical_reader(&text, 2, &ob);
        if bytes == b";" {
            assert!(!reader.skip_ws_and_comments());
        } else {
            assert_eq!(
                reader.read_form().expect_err("missing close").kind,
                ReadErrorKind::EndOfFile
            );
        }
        assert_eq!(reader.pos, 2);
        assert!(reader.current_code().is_none());
        assert!(!reader.skip_ws_and_comments());
    }
    let text = source(b"a", 2);
    let mut reader = logical_reader(&text, 2, &ob);
    assert!(
        reader
            .read_form()
            .expect("symbol stops before terminal whitespace")
            .is_symbol_named("a", SymbolMode::Bare)
    );
    assert_eq!(reader.character_cursor.position(), Some(CharPos0::new(1)));
    assert_eq!(reader.pos, 1);
}

#[test]
fn terminal_invalid_dispatch_spans_restore_both_cursor_units() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let ob = neovm_value::symbol::Obarray::new();
    for prefix in [b"#^".as_slice(), b"#^^".as_slice(), b"#s".as_slice()] {
        let text = source(
            prefix,
            i128::try_from(prefix.len() + 1).expect("bounded test prefix"),
        );
        let mut reader = logical_reader(&text, prefix.len() + 1, &ob);
        let error = reader
            .read_form()
            .expect_err("terminal offending character");
        assert_eq!(error.kind, ReadErrorKind::InvalidReadSyntax);
        assert_eq!(error.message.as_bytes(), prefix);
        assert_eq!(reader.pos, prefix.len());
        assert_eq!(
            reader.character_cursor.position(),
            Some(CharPos0::new(prefix.len()))
        );
        assert_eq!(reader.current_code(), Some(0));
        assert_eq!(reader.peek_code_at(1), None);
        reader.bump();
        assert_eq!(reader.pos, prefix.len() + 1);
        assert!(reader.current_code().is_none());
    }
    let text = source(b"#1", 3);
    let mut reader = logical_reader(&text, 3, &ob);
    let error = reader.read_form().expect_err("invalid label suffix");
    assert_eq!(error.message, "#1");
    assert_eq!(reader.pos, 3);
    assert_eq!(reader.source_slice_string(0, 3).as_bytes(), b"#1\0");
}
