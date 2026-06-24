use super::*;
use crate::buffer::buffer::{Buffer, BufferId};
use crate::buffer::{CharPos0, LispCharPos1};
use crate::emacs_core::value::eq_value;

/// Helper: create a buffer with given text, point at start, full accessible range.
fn buf_with_text(text: &str) -> Buffer {
    let mut buf = Buffer::new(BufferId(99), Value::string("test-syntax"));
    buf.insert(text);
    buf.widen();
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    buf
}

fn char_pos_to_byte(buf: &Buffer, char_pos: usize) -> usize {
    buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(char_pos))
        .get()
}

fn current_point_lisp_pos(eval: &crate::emacs_core::eval::Context) -> i64 {
    eval.buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1
}

fn replace_current_buffer_text(eval: &mut crate::emacs_core::eval::Context, text: &str) {
    let buf = eval.buffers.current_buffer_mut().expect("current buffer");
    buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
        buf.point_min_emacs_byte_pos().get(),
        buf.point_max_emacs_byte_pos().get(),
    ));
    buf.insert(text);
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
        buf.point_min_emacs_byte_pos().get(),
    ));
}

fn install_line_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(eval, vec![Value::fixnum(';' as i64), Value::string("<")])
        .expect("install semicolon comment start syntax");
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('\n' as i64), Value::string(">")])
        .expect("install newline comment end syntax");
}

fn install_c_block_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('/' as i64), Value::string(". 124b")],
    )
    .expect("install C slash comment syntax");
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('*' as i64), Value::string(". 23")])
        .expect("install C star comment syntax");
}

fn install_nestable_c_block_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('/' as i64), Value::string(". 124bn")],
    )
    .expect("install nestable C slash comment syntax");
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('*' as i64), Value::string(". 23n")],
    )
    .expect("install nestable C star comment syntax");
}

// -----------------------------------------------------------------------
// SyntaxClass parsing
// -----------------------------------------------------------------------

#[test]
fn syntax_class_roundtrip() {
    crate::test_utils::init_test_tracing();
    let classes = [
        (' ', SyntaxClass::Whitespace),
        ('w', SyntaxClass::Word),
        ('_', SyntaxClass::Symbol),
        ('.', SyntaxClass::Punctuation),
        ('(', SyntaxClass::Open),
        (')', SyntaxClass::Close),
        ('\'', SyntaxClass::Quote),
        ('"', SyntaxClass::StringDelim),
        ('$', SyntaxClass::Math),
        ('\\', SyntaxClass::Escape),
        ('/', SyntaxClass::CharQuote),
        ('<', SyntaxClass::Comment),
        ('>', SyntaxClass::EndComment),
        ('@', SyntaxClass::InheritStd),
        ('!', SyntaxClass::CommentFence),
        ('|', SyntaxClass::StringFence),
    ];
    for (ch, class) in &classes {
        assert_eq!(SyntaxClass::from_char(*ch), Some(*class));
        assert_eq!(class.to_char(), *ch);
    }
}

#[test]
fn syntax_class_codes_match_gnu_syntaxcode() {
    crate::test_utils::init_test_tracing();
    let classes = [
        (SyntaxClass::Whitespace, 0, ' '),
        (SyntaxClass::Punctuation, 1, '.'),
        (SyntaxClass::Word, 2, 'w'),
        (SyntaxClass::Symbol, 3, '_'),
        (SyntaxClass::Open, 4, '('),
        (SyntaxClass::Close, 5, ')'),
        (SyntaxClass::Quote, 6, '\''),
        (SyntaxClass::StringDelim, 7, '"'),
        (SyntaxClass::Math, 8, '$'),
        (SyntaxClass::Escape, 9, '\\'),
        (SyntaxClass::CharQuote, 10, '/'),
        (SyntaxClass::Comment, 11, '<'),
        (SyntaxClass::EndComment, 12, '>'),
        (SyntaxClass::InheritStd, 13, '@'),
        (SyntaxClass::CommentFence, 14, '!'),
        (SyntaxClass::StringFence, 15, '|'),
    ];

    for (class, code, syntax_char) in classes {
        assert_eq!(class.code(), code);
        assert_eq!(class.to_char(), syntax_char);
        assert_eq!(SyntaxClass::from_code(code), Some(class));
        assert_eq!(SyntaxClass::from_code(code | (0x5a << 16)), Some(class));
    }
    assert_eq!(SyntaxClass::from_code(16), None);
}

#[test]
fn syntax_class_dash_is_whitespace() {
    crate::test_utils::init_test_tracing();
    assert_eq!(SyntaxClass::from_char('-'), Some(SyntaxClass::Whitespace));
}

#[test]
fn syntax_spec_byte_table_matches_gnu_syntax_c() {
    crate::test_utils::init_test_tracing();
    let cases = [
        (b' ', SyntaxClass::Whitespace),
        (b'-', SyntaxClass::Whitespace),
        (b'.', SyntaxClass::Punctuation),
        (b'w', SyntaxClass::Word),
        (b'_', SyntaxClass::Symbol),
        (b'(', SyntaxClass::Open),
        (b')', SyntaxClass::Close),
        (b'\'', SyntaxClass::Quote),
        (b'"', SyntaxClass::StringDelim),
        (b'$', SyntaxClass::Math),
        (b'\\', SyntaxClass::Escape),
        (b'/', SyntaxClass::CharQuote),
        (b'<', SyntaxClass::Comment),
        (b'>', SyntaxClass::EndComment),
        (b'@', SyntaxClass::InheritStd),
        (b'!', SyntaxClass::CommentFence),
        (b'|', SyntaxClass::StringFence),
    ];

    for (byte, class) in cases {
        assert_eq!(SyntaxClass::from_syntax_spec_byte(byte), Some(class));
        assert_eq!(u8::from(class), class.code() as u8);
    }
    for invalid in [b'a', b'W', b'?', 0, 0x80, 0xff] {
        assert_eq!(SyntaxClass::from_syntax_spec_byte(invalid), None);
    }
}

// -----------------------------------------------------------------------
// string-to-syntax parser
// -----------------------------------------------------------------------

#[test]
fn string_to_syntax_whitespace() {
    crate::test_utils::init_test_tracing();
    let entry = string_to_syntax(" ").unwrap();
    assert_eq!(entry.class, SyntaxClass::Whitespace);
    assert_eq!(entry.matching_char, None);
    assert!(entry.flags.is_empty());
}

#[test]
fn string_to_syntax_word() {
    crate::test_utils::init_test_tracing();
    let entry = string_to_syntax("w").unwrap();
    assert_eq!(entry.class, SyntaxClass::Word);
}

#[test]
fn string_to_syntax_open_paren() {
    crate::test_utils::init_test_tracing();
    let entry = string_to_syntax("()").unwrap();
    assert_eq!(entry.class, SyntaxClass::Open);
    assert_eq!(entry.matching_char, Some(')'));
}

#[test]
fn string_to_syntax_close_paren() {
    crate::test_utils::init_test_tracing();
    let entry = string_to_syntax(")(").unwrap();
    assert_eq!(entry.class, SyntaxClass::Close);
    assert_eq!(entry.matching_char, Some('('));
}

#[test]
fn string_to_syntax_string_delim() {
    crate::test_utils::init_test_tracing();
    let entry = string_to_syntax("\"").unwrap();
    assert_eq!(entry.class, SyntaxClass::StringDelim);
}

#[test]
fn string_to_syntax_prefix_class() {
    crate::test_utils::init_test_tracing();
    let entry = string_to_syntax("'").unwrap();
    assert_eq!(entry.class, SyntaxClass::Quote);
    let value = syntax_entry_to_value(&entry);
    if value.is_cons() {
        let cell_car = value.cons_car();
        let cell_cdr = value.cons_cdr();
        assert!(cell_car.is_fixnum());
    } else {
        panic!("Expected cons cell");
    }
}

#[test]
fn builtin_string_to_syntax_at_returns_nil() {
    crate::test_utils::init_test_tracing();
    let out = builtin_string_to_syntax(vec![Value::string("@")]).unwrap();
    assert_eq!(out, Value::NIL);
}

#[test]
fn string_to_syntax_reuses_gnu_shared_bare_syntax_objects() {
    crate::test_utils::init_test_tracing();
    let word_one = builtin_string_to_syntax(vec![Value::string("w")]).unwrap();
    let word_two = builtin_string_to_syntax(vec![Value::string("w")]).unwrap();
    assert!(eq_value(&word_one, &word_two));

    let flagged_one = builtin_string_to_syntax(vec![Value::string("w1")]).unwrap();
    let flagged_two = builtin_string_to_syntax(vec![Value::string("w1")]).unwrap();
    assert!(!eq_value(&flagged_one, &flagged_two));
}

#[test]
fn string_to_syntax_with_flags() {
    crate::test_utils::init_test_tracing();
    let entry = string_to_syntax(". 12").unwrap();
    assert_eq!(entry.class, SyntaxClass::Punctuation);
    assert_eq!(entry.matching_char, None);
    assert!(entry.flags.contains(SyntaxFlags::COMMENT_START_FIRST));
    assert!(entry.flags.contains(SyntaxFlags::COMMENT_START_SECOND));
}

#[test]
fn string_to_syntax_comment_style_b() {
    crate::test_utils::init_test_tracing();
    let entry = string_to_syntax(". 12b").unwrap();
    assert!(entry.flags.contains(SyntaxFlags::COMMENT_STYLE_B));
}

#[test]
fn string_to_syntax_comment_style_c() {
    crate::test_utils::init_test_tracing();
    let entry = string_to_syntax(". c").unwrap();
    assert!(entry.flags.contains(SyntaxFlags::COMMENT_STYLE_C));
}

#[test]
fn string_to_syntax_prefix_flag() {
    crate::test_utils::init_test_tracing();
    let entry = string_to_syntax(". p").unwrap();
    assert_eq!(entry.class, SyntaxClass::Punctuation);
    assert!(entry.flags.contains(SyntaxFlags::PREFIX));
}

#[test]
fn string_to_syntax_empty_errors() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        string_to_syntax("").unwrap_err(),
        "Invalid syntax description letter: \0"
    );
}

#[test]
fn string_to_syntax_invalid_class() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        string_to_syntax("Z").unwrap_err(),
        "Invalid syntax description letter: Z"
    );
}

// -----------------------------------------------------------------------
// SyntaxTable
// -----------------------------------------------------------------------

#[test]
fn standard_table_word_chars() {
    crate::test_utils::init_test_tracing();
    let table = SyntaxTable::new_standard();
    assert_eq!(table.char_syntax('a'), SyntaxClass::Word);
    assert_eq!(table.char_syntax('Z'), SyntaxClass::Word);
    assert_eq!(table.char_syntax('5'), SyntaxClass::Word);
    assert_eq!(table.char_syntax('$'), SyntaxClass::Word);
    assert_eq!(table.char_syntax('%'), SyntaxClass::Word);
}

#[test]
fn standard_table_whitespace() {
    crate::test_utils::init_test_tracing();
    let table = SyntaxTable::new_standard();
    assert_eq!(table.char_syntax(' '), SyntaxClass::Whitespace);
    assert_eq!(table.char_syntax('\t'), SyntaxClass::Whitespace);
    assert_eq!(table.char_syntax('\n'), SyntaxClass::Whitespace);
}

#[test]
fn standard_table_parens() {
    crate::test_utils::init_test_tracing();
    let table = SyntaxTable::new_standard();
    assert_eq!(table.char_syntax('('), SyntaxClass::Open);
    assert_eq!(table.char_syntax(')'), SyntaxClass::Close);
    assert_eq!(table.char_syntax('['), SyntaxClass::Open);
    assert_eq!(table.char_syntax(']'), SyntaxClass::Close);
}

#[test]
fn standard_table_string_delim() {
    crate::test_utils::init_test_tracing();
    let table = SyntaxTable::new_standard();
    assert_eq!(table.char_syntax('"'), SyntaxClass::StringDelim);
}

#[test]
fn standard_table_escape() {
    crate::test_utils::init_test_tracing();
    let table = SyntaxTable::new_standard();
    assert_eq!(table.char_syntax('\\'), SyntaxClass::Escape);
}

#[test]
fn standard_table_punctuation() {
    crate::test_utils::init_test_tracing();
    let table = SyntaxTable::new_standard();
    assert_eq!(table.char_syntax('\u{0001}'), SyntaxClass::Punctuation);
    assert_eq!(table.char_syntax('\u{007f}'), SyntaxClass::Punctuation);
    assert_eq!(table.char_syntax(';'), SyntaxClass::Punctuation);
    assert_eq!(table.char_syntax('?'), SyntaxClass::Punctuation);
    assert_eq!(table.char_syntax('.'), SyntaxClass::Punctuation);
}

#[test]
fn standard_table_symbol_constituents() {
    crate::test_utils::init_test_tracing();
    let table = SyntaxTable::new_standard();
    assert_eq!(table.char_syntax('_'), SyntaxClass::Symbol);
    assert_eq!(table.char_syntax('-'), SyntaxClass::Symbol);
    assert_eq!(table.char_syntax('+'), SyntaxClass::Symbol);
    assert_eq!(table.char_syntax('/'), SyntaxClass::Symbol);
    assert_eq!(table.char_syntax('='), SyntaxClass::Symbol);
}

#[test]
fn modify_syntax_entry_overrides() {
    crate::test_utils::init_test_tracing();
    let mut table = SyntaxTable::new_standard();
    assert_eq!(table.char_syntax('+'), SyntaxClass::Symbol);
    table.modify_syntax_entry('+', SyntaxEntry::simple(SyntaxClass::Word));
    assert_eq!(table.char_syntax('+'), SyntaxClass::Word);
}

#[test]
fn inherited_table_falls_back() {
    crate::test_utils::init_test_tracing();
    let table = SyntaxTable::make_syntax_table();
    // Should inherit from standard.
    assert_eq!(table.char_syntax('a'), SyntaxClass::Word);
    assert_eq!(table.char_syntax(' '), SyntaxClass::Whitespace);
}

#[test]
fn inherited_table_override() {
    crate::test_utils::init_test_tracing();
    let mut table = SyntaxTable::make_syntax_table();
    table.modify_syntax_entry('a', SyntaxEntry::simple(SyntaxClass::Punctuation));
    assert_eq!(table.char_syntax('a'), SyntaxClass::Punctuation);
    // Other inherited entries still work.
    assert_eq!(table.char_syntax('b'), SyntaxClass::Word);
}

#[test]
fn copy_syntax_table_is_independent() {
    crate::test_utils::init_test_tracing();
    let original = SyntaxTable::new_standard();
    let mut copy = original.copy_syntax_table();
    copy.modify_syntax_entry('a', SyntaxEntry::simple(SyntaxClass::Punctuation));
    assert_eq!(original.char_syntax('a'), SyntaxClass::Word);
    assert_eq!(copy.char_syntax('a'), SyntaxClass::Punctuation);
}

#[test]
fn non_ascii_defaults_to_word() {
    crate::test_utils::init_test_tracing();
    let table = SyntaxTable::new_standard();
    // A random Unicode character not in the table.
    assert_eq!(table.char_syntax('\u{1F600}'), SyntaxClass::Word);
}

#[test]
fn char_syntax_accepts_full_emacs_character_codes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    // GNU Emacs `CHAR_VALID_P` accepts integer character codes through
    // MAX_CHAR (0x3FFFFF), including values that are not Unicode scalar
    // values.  `char-syntax` indexes the syntax table by that integer
    // code instead of converting through a host character type.
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(0x20_0220)]).unwrap(),
        Value::char('w')
    );

    let err = builtin_char_syntax(&mut eval, vec![Value::fixnum(0x40_0000)])
        .expect_err("out-of-range character code should signal");
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("characterp"), Value::fixnum(0x40_0000)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn char_syntax_unibyte_buffer_maps_through_make_char_multibyte() {
    // GNU `Fchar_syntax`: in a unibyte buffer the character is mapped through
    // `make_char_multibyte` before the syntax lookup. Bytes 0x80-0xFF become
    // their eight-bit (word) character; codes >= 0x100 map past MAX_CHAR to an
    // invalid character whose syntax is the default whitespace.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    // Multibyte (default): a BMP CJK character is a word constituent.
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(0x4e16)]).unwrap(),
        Value::char('w')
    );

    let id = eval.buffers.current_buffer_id().unwrap();
    eval.buffers.set_buffer_multibyte_flag(id, false).unwrap();
    // CJK and other >= 0x100 codes collapse to whitespace.
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(0x4e16)]).unwrap(),
        Value::char(' ')
    );
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(0x100)]).unwrap(),
        Value::char(' ')
    );
    // ASCII keeps its real syntax; high bytes become eight-bit word chars.
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(b'a' as i64)]).unwrap(),
        Value::char('w')
    );
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(b'.' as i64)]).unwrap(),
        Value::char('.')
    );
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(200)]).unwrap(),
        Value::char('w')
    );
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(255)]).unwrap(),
        Value::char('w')
    );
}

// -----------------------------------------------------------------------
// forward_word / backward_word
// -----------------------------------------------------------------------

#[test]
fn forward_word_basic() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    let table = SyntaxTable::new_standard();
    let pos = forward_word(&buf, &table, 1);
    // "hello" ends at byte 5.
    assert_eq!(pos, crate::buffer::EmacsBytePos::new(5));
}

#[test]
fn forward_word_two() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    let table = SyntaxTable::new_standard();
    let pos = forward_word(&buf, &table, 2);
    // Past "hello world" = byte 11.
    assert_eq!(pos, crate::buffer::EmacsBytePos::new(11));
}

#[test]
fn forward_word_from_middle() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(3)); // inside "hello"
    let table = SyntaxTable::new_standard();
    let pos = forward_word(&buf, &table, 1);
    assert_eq!(pos, crate::buffer::EmacsBytePos::new(5)); // end of "hello"
}

#[test]
fn backward_word_basic() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(11)); // end of text
    let table = SyntaxTable::new_standard();
    let pos = backward_word(&buf, &table, 1);
    assert_eq!(pos, crate::buffer::EmacsBytePos::new(6)); // start of "world"
}

#[test]
fn backward_word_two() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(11));
    let table = SyntaxTable::new_standard();
    let pos = backward_word(&buf, &table, 2);
    assert_eq!(pos, crate::buffer::EmacsBytePos::new(0)); // start of "hello"
}

#[test]
fn forward_word_negative_goes_backward() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(11));
    let table = SyntaxTable::new_standard();
    let pos = forward_word(&buf, &table, -1);
    assert_eq!(pos, crate::buffer::EmacsBytePos::new(6));
}

// -----------------------------------------------------------------------
// skip_syntax_forward / skip_syntax_backward
// -----------------------------------------------------------------------

#[test]
fn skip_syntax_forward_word() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    let table = SyntaxTable::new_standard();
    let pos = skip_syntax_forward(&buf, &table, "w", None);
    assert_eq!(pos, 5); // end of "hello"
}

#[test]
fn skip_syntax_forward_whitespace_and_word() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("  hello");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    let table = SyntaxTable::new_standard();
    let pos = skip_syntax_forward(&buf, &table, " w", None);
    assert_eq!(pos, 7); // end of "  hello"
}

#[test]
fn skip_syntax_backward_word() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(11));
    let table = SyntaxTable::new_standard();
    let pos = skip_syntax_backward(&buf, &table, "w", None);
    assert_eq!(pos, 6); // start of "world"
}

#[test]
fn skip_syntax_forward_with_limit() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("helloworld");
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    let table = SyntaxTable::new_standard();
    let pos = skip_syntax_forward(&buf, &table, "w", Some(3));
    assert_eq!(pos, 3);
}

#[test]
fn builtin_skip_syntax_forward_limit_uses_char_positions_for_multibyte_text() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("éézz");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    let moved =
        builtin_skip_syntax_forward(&mut eval, vec![Value::string("w"), Value::fixnum(3)]).unwrap();
    assert_eq!(moved, Value::fixnum(2));
    assert_eq!(
        eval.buffers
            .current_buffer()
            .expect("current buffer")
            .point_char_pos()
            .get() as i64
            + 1,
        3
    );
}

#[test]
fn builtin_skip_syntax_forward_limit_stays_absolute_under_narrowing() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("aéézz");
        buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            1,
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    let moved =
        builtin_skip_syntax_forward(&mut eval, vec![Value::string("w"), Value::fixnum(4)]).unwrap();
    assert_eq!(moved, Value::fixnum(2));
    assert_eq!(
        eval.buffers
            .current_buffer()
            .expect("current buffer")
            .point_char_pos()
            .get() as i64
            + 1,
        4
    );
}

// -----------------------------------------------------------------------
// scan_sexps (balanced expressions)
// -----------------------------------------------------------------------

#[test]
fn scan_sexps_forward_parens() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("(hello world)");
    let table = SyntaxTable::new_standard();
    let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
    assert_eq!(pos, 13); // past closing paren
}

#[test]
fn scan_sexps_forward_nested() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("(a (b c) d)");
    let table = SyntaxTable::new_standard();
    let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
    assert_eq!(pos, 11);
}

#[test]
fn scan_sexps_forward_word() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("hello world");
    let table = SyntaxTable::new_standard();
    let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
    assert_eq!(pos, 5); // end of "hello"
}

#[test]
fn scan_sexps_forward_string() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("\"hello\" world");
    let table = SyntaxTable::new_standard();
    let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
    assert_eq!(pos, 7); // past closing quote
}

#[test]
fn scan_sexps_backward_parens() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("(hello world)");
    let table = SyntaxTable::new_standard();
    // Start after closing paren.
    let pos = scan_sexps(&buf, &table, 13, -1).unwrap();
    assert_eq!(pos, 0); // back to opening paren
}

#[test]
fn scan_sexps_forward_unbalanced() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("(hello");
    let table = SyntaxTable::new_standard();
    assert!(scan_sexps(&buf, &table, 0, 1).is_err());
}

#[test]
fn scan_sexps_backward_unbalanced() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("hello)");
    let table = SyntaxTable::new_standard();
    assert!(scan_sexps(&buf, &table, 6, -1).is_err());
}

#[test]
fn scan_sexps_zero_count() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("(hello)");
    let table = SyntaxTable::new_standard();
    let pos = scan_sexps(&buf, &table, 3, 0).unwrap();
    assert_eq!(pos, 3); // unchanged
}

#[test]
fn scan_sexps_forward_brackets() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("[a b c]");
    let table = SyntaxTable::new_standard();
    let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
    assert_eq!(pos, 7);
}

#[test]
fn scan_sexps_string_with_escape() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("\"he\\\"llo\" world");
    let table = SyntaxTable::new_standard();
    let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
    assert_eq!(pos, 9); // past the closing quote
}

#[test]
fn sexp_motion_respects_parse_sexp_ignore_comments_for_line_comments() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "(a ; comment with )\n b)");
    install_line_comment_syntax(&mut eval);

    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::NIL);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(20)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(20)
    );
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }
    builtin_forward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 20);

    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(24)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(24)
    );
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }
    builtin_forward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 24);
}

#[test]
fn sexp_motion_ignores_unmatched_open_paren_inside_line_comment() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "(a ; unmatched (\n b)");
    install_line_comment_syntax(&mut eval);

    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(21)
    );

    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::NIL);
    match builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(sig.data[0], Value::string("Unbalanced parentheses"));
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }
}

#[test]
fn sexp_scanners_treat_eof_comments_as_one_forward_sexp() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, ";foo");
    install_line_comment_syntax(&mut eval);
    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(5)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(5)
    );
    builtin_forward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 5);

    replace_current_buffer_text(&mut eval, "/*x");
    install_c_block_comment_syntax(&mut eval);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(4)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(4)
    );
    builtin_forward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 4);
}

#[test]
fn backward_sexp_motion_respects_parse_sexp_ignore_comments() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "(a ; comment with )\n b)");
    install_line_comment_syntax(&mut eval);

    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(24), Value::fixnum(-1)]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(24), Value::fixnum(-1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(1)
    );
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_max_emacs_byte_pos().get(),
        ));
    }
    builtin_backward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 1);

    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::NIL);
    match builtin_scan_sexps(&mut eval, vec![Value::fixnum(24), Value::fixnum(-1)]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(sig.data[0], Value::string("Unbalanced parentheses"));
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }
    match builtin_scan_lists(
        &mut eval,
        vec![Value::fixnum(24), Value::fixnum(-1), Value::fixnum(0)],
    ) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(sig.data[0], Value::string("Unbalanced parentheses"));
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }

    replace_current_buffer_text(&mut eval, "(a /* comment with ( */ b)");
    install_c_block_comment_syntax(&mut eval);
    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(27), Value::fixnum(-1)]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(27), Value::fixnum(-1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(1)
    );
    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::NIL);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(27), Value::fixnum(-1)]).unwrap(),
        Value::fixnum(20)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(27), Value::fixnum(-1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(20)
    );
}

#[test]
fn backward_sexp_motion_skips_nested_block_comments() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let text = "(a /* outer ( /* inner */ after */ b)";
    let end = text.len() as i64 + 1;
    replace_current_buffer_text(&mut eval, text);
    install_nestable_c_block_comment_syntax(&mut eval);
    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(end), Value::fixnum(-1)]).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(end), Value::fixnum(-1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(1)
    );
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_max_emacs_byte_pos().get(),
        ));
    }
    builtin_backward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 1);
}

#[test]
fn backward_comment_skip_requires_matching_comment_start() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "ab");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('a' as i64), Value::string("w 3")],
    )
    .expect("install first two-char comment-end syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('b' as i64), Value::string("w 4")],
    )
    .expect("install second two-char comment-end syntax");

    let buf = eval.buffers.current_buffer().expect("current buffer");
    let entry = SyntaxTable::for_buffer(buf)
        .get_entry('b')
        .expect("syntax entry for b");
    assert_eq!(entry.class, SyntaxClass::Word);
    assert!(entry.flags.contains(SyntaxFlags::COMMENT_END_SECOND));
    assert_eq!(
        maybe_skip_comment_backward(buf, 2, false, entry.class, entry.flags),
        None
    );
}

#[test]
fn sexp_motion_respects_parse_sexp_ignore_comments_for_block_comments() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "(a /* comment with ) */ b)");
    install_c_block_comment_syntax(&mut eval);

    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::NIL);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(21)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(21)
    );

    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    assert_eq!(
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap(),
        Value::fixnum(27)
    );
    assert_eq!(
        builtin_scan_lists(
            &mut eval,
            vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)]
        )
        .unwrap(),
        Value::fixnum(27)
    );

    replace_current_buffer_text(&mut eval, "(a /* comment with ) b)");
    match builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(sig.data[0], Value::string("Unbalanced parentheses"));
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }
}

#[test]
fn doom_cli_style_struct_comment_does_not_break_forward_sexp() {
    crate::test_utils::init_test_tracing();
    let text = "(cl-defstruct doom-cli-context\n  ;; A session-specific ID of the current context (defaults to number\n  (pid (if-let* ((pid (getenv \"__DOOMPID\")))\n           (string-to-number pid)\n         (emacs-pid))))\n(setq after t)";
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, text);
    install_line_comment_syntax(&mut eval);
    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    builtin_forward_sexp(&mut eval, vec![Value::fixnum(1)]).unwrap();
    let expected_point = text.find("\n(setq after t)").expect("marker") as i64 + 1;
    assert_eq!(current_point_lisp_pos(&eval), expected_point);
}

// -----------------------------------------------------------------------
// syntax_entry_to_value
// -----------------------------------------------------------------------

#[test]
fn syntax_entry_to_value_simple() {
    crate::test_utils::init_test_tracing();
    let entry = SyntaxEntry::simple(SyntaxClass::Word);
    let val = syntax_entry_to_value(&entry);
    // Should be (2 . nil) since Word code = 2
    if val.is_cons() {
        let cell_car = val.cons_car();
        let cell_cdr = val.cons_cdr();
        assert!(cell_car.is_fixnum());
        assert!(cell_cdr.is_nil());
    } else {
        panic!("Expected cons cell");
    }
}

#[test]
fn syntax_entry_to_value_with_match() {
    crate::test_utils::init_test_tracing();
    let entry = SyntaxEntry::with_match(SyntaxClass::Open, ')');
    let val = syntax_entry_to_value(&entry);
    if val.is_cons() {
        let cell_car = val.cons_car();
        let cell_cdr = val.cons_cdr();
        assert!(cell_car.is_fixnum()); // Open code = 4
        assert!(cell_cdr.is_fixnum()); // ')' = 41
    } else {
        panic!("Expected cons cell");
    }
}

#[test]
fn syntax_entry_to_value_with_flags() {
    crate::test_utils::init_test_tracing();
    let entry = SyntaxEntry {
        class: SyntaxClass::Punctuation,
        matching_char: None,
        flags: SyntaxFlags::COMMENT_START_FIRST | SyntaxFlags::COMMENT_START_SECOND,
    };
    let val = syntax_entry_to_value(&entry);
    if val.is_cons() {
        let cell_car = val.cons_car();
        let cell_cdr = val.cons_cdr();
        // code = 1 (punctuation) | (0x03 << 16) = 1 | 196608 = 196609
        assert!(cell_car.is_fixnum());
    } else {
        panic!("Expected cons cell");
    }
}

#[test]
fn make_syntax_table_returns_syntax_char_table() {
    crate::test_utils::init_test_tracing();
    let table = builtin_make_syntax_table(vec![]).unwrap();
    let is_ct = crate::emacs_core::chartable::builtin_char_table_p(vec![table]).unwrap();
    assert_eq!(is_ct, Value::T);
    let subtype = crate::emacs_core::chartable::builtin_char_table_subtype(vec![table]).unwrap();
    assert_eq!(subtype, Value::symbol("syntax-table"));
}

#[test]
fn make_syntax_table_parent_must_be_char_table() {
    crate::test_utils::init_test_tracing();
    match builtin_make_syntax_table(vec![Value::fixnum(1)]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("char-table-p")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn standard_syntax_table_returns_char_table() {
    crate::test_utils::init_test_tracing();
    let table = builtin_standard_syntax_table(vec![]).unwrap();
    let is_ct = crate::emacs_core::chartable::builtin_char_table_p(vec![table]).unwrap();
    assert_eq!(is_ct, Value::T);
    let subtype = crate::emacs_core::chartable::builtin_char_table_subtype(vec![table]).unwrap();
    assert_eq!(subtype, Value::symbol("syntax-table"));
}

#[test]
fn copy_syntax_table_returns_fresh_syntax_table() {
    crate::test_utils::init_test_tracing();
    let source = builtin_make_syntax_table(vec![]).unwrap();
    let copied = builtin_copy_syntax_table(vec![source]).unwrap();

    let is_ct = crate::emacs_core::chartable::builtin_char_table_p(vec![copied]).unwrap();
    assert_eq!(is_ct, Value::T);
    let subtype = crate::emacs_core::chartable::builtin_char_table_subtype(vec![copied]).unwrap();
    assert_eq!(subtype, Value::symbol("syntax-table"));

    assert!(
        !eq_value(&source, &copied),
        "expected fresh allocation, got identity-equal"
    );

    let parent = crate::emacs_core::chartable::builtin_char_table_parent(vec![copied]).unwrap();
    assert!(
        crate::emacs_core::chartable::builtin_char_table_p(vec![parent])
            .unwrap()
            .is_truthy()
    );
}

#[test]
fn copy_syntax_table_validates_arity_and_type() {
    crate::test_utils::init_test_tracing();
    match builtin_copy_syntax_table(vec![Value::fixnum(1)]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("syntax-table-p")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }

    match builtin_copy_syntax_table(vec![Value::NIL, Value::NIL]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(sig.data.first(), Some(&Value::symbol("copy-syntax-table")));
        }
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn syntax_class_to_char_basics_and_errors() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        builtin_syntax_class_to_char(vec![Value::fixnum(0)]).unwrap(),
        Value::char(' ')
    );
    assert_eq!(
        builtin_syntax_class_to_char(vec![Value::fixnum(15)]).unwrap(),
        Value::char('|')
    );

    match builtin_syntax_class_to_char(vec![Value::fixnum(-1)]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(15), Value::fixnum(-1)]);
        }
        other => panic!("expected args-out-of-range signal, got {other:?}"),
    }

    match builtin_syntax_class_to_char(vec![Value::fixnum(0x1_0002)]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(15), Value::fixnum(0x1_0002)]);
        }
        other => panic!("expected args-out-of-range signal, got {other:?}"),
    }

    match builtin_syntax_class_to_char(vec![Value::string("x")]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("fixnump")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn matching_paren_basics_and_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    assert_eq!(
        builtin_matching_paren(&mut eval, vec![Value::fixnum('(' as i64)]).unwrap(),
        Value::char(')')
    );
    assert_eq!(
        builtin_matching_paren(&mut eval, vec![Value::fixnum(']' as i64)]).unwrap(),
        Value::char('[')
    );
    assert_eq!(
        builtin_matching_paren(&mut eval, vec![Value::fixnum('{' as i64)]).unwrap(),
        Value::char('}')
    );
    assert_eq!(
        builtin_matching_paren(&mut eval, vec![Value::fixnum('a' as i64)]).unwrap(),
        Value::NIL
    );

    let custom = builtin_copy_syntax_table(vec![builtin_standard_syntax_table(vec![]).unwrap()])
        .expect("copy standard syntax table");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('{' as i64), Value::string("_"), custom],
    )
    .expect("make brace a symbol constituent");
    builtin_set_syntax_table(&mut eval, vec![custom]).expect("set current syntax table");
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum('{' as i64)]).unwrap(),
        Value::char('_')
    );
    assert_eq!(
        builtin_matching_paren(&mut eval, vec![Value::fixnum('{' as i64)]).unwrap(),
        Value::NIL
    );

    match builtin_matching_paren(&mut eval, vec![Value::string("(")]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("characterp")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }

    match builtin_matching_paren(&mut eval, vec![]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(sig.data.first(), Some(&Value::symbol("matching-paren")));
        }
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn syntax_table_eval_returns_char_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let table = builtin_syntax_table(&mut eval, vec![]).unwrap();
    let is_ct = crate::emacs_core::chartable::builtin_char_table_p(vec![table]).unwrap();
    assert_eq!(is_ct, Value::T);
    let subtype = crate::emacs_core::chartable::builtin_char_table_subtype(vec![table]).unwrap();
    assert_eq!(subtype, Value::symbol("syntax-table"));
}

#[test]
fn syntax_table_p_recognizes_syntax_tables() {
    crate::test_utils::init_test_tracing();
    let syntax_table = builtin_make_syntax_table(vec![]).unwrap();
    let is_syntax = builtin_syntax_table_p(vec![syntax_table]).unwrap();
    assert_eq!(is_syntax, Value::T);

    let char_table =
        crate::emacs_core::chartable::make_char_table_value(Value::symbol("foo"), Value::NIL);
    let not_syntax = builtin_syntax_table_p(vec![char_table]).unwrap();
    assert_eq!(not_syntax, Value::NIL);

    let atom = builtin_syntax_table_p(vec![Value::fixnum(1)]).unwrap();
    assert_eq!(atom, Value::NIL);
}

#[test]
fn syntax_purpose_symbol_domain_matches_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        SyntaxPurposeSymbol::from_lisp_value(&Value::symbol("syntax-table")),
        Some(SyntaxPurposeSymbol::SyntaxTable)
    );
    assert_eq!(SyntaxPurposeSymbol::SyntaxTable.name(), "syntax-table");
    assert_eq!(
        SyntaxPurposeSymbol::from_lisp_value(&Value::symbol("other")),
        None
    );
}

#[test]
fn parse_partial_sexp_commentstop_matches_gnu_symbol_domain() {
    crate::test_utils::init_test_tracing();
    assert_eq!(parse_commentstop_mode(None), CommentStopMode::None);
    assert_eq!(
        parse_commentstop_mode(Some(&Value::NIL)),
        CommentStopMode::None
    );
    assert_eq!(
        parse_commentstop_mode(Some(&Value::symbol("syntax-table"))),
        CommentStopMode::SyntaxTable
    );
    assert_eq!(
        parse_commentstop_mode(Some(&Value::symbol("other"))),
        CommentStopMode::Comment
    );
    assert_eq!(
        parse_commentstop_mode(Some(&Value::T)),
        CommentStopMode::Comment
    );
}

#[test]
fn set_syntax_table_validates_and_returns_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let table = builtin_make_syntax_table(vec![]).unwrap();
    let out = builtin_set_syntax_table(&mut eval, vec![table]).unwrap();
    assert_eq!(out, table);

    match builtin_set_syntax_table(&mut eval, vec![Value::fixnum(1)]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("syntax-table-p")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn syntax_table_and_standard_default_to_same_object() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let current = builtin_syntax_table(&mut eval, vec![]).unwrap();
    let standard = builtin_standard_syntax_table(vec![]).unwrap();
    assert!(eq_value(&current, &standard));
}

#[test]
fn set_syntax_table_updates_current_buffer_only() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let custom = builtin_make_syntax_table(vec![]).unwrap();
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum(';' as i64), Value::string("<"), custom],
    )
    .unwrap();
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('\n' as i64), Value::string(">"), custom],
    )
    .unwrap();
    let current_id = eval.buffers.current_buffer().expect("current buffer").id;
    let other_id = eval.buffers.create_buffer("*syntax-other*");

    let out = builtin_set_syntax_table(&mut eval, vec![custom]).unwrap();
    assert_eq!(out, custom);
    let current = builtin_syntax_table(&mut eval, vec![]).unwrap();
    assert_eq!(current, custom);

    eval.buffers.set_current(other_id);
    let other = builtin_syntax_table(&mut eval, vec![]).unwrap();
    assert!(!eq_value(&other, &custom));

    eval.buffers.set_current(current_id);
    let restored = builtin_syntax_table(&mut eval, vec![]).unwrap();
    assert_eq!(restored, custom);
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum(';' as i64)]).unwrap(),
        Value::char('<')
    );
    assert_eq!(
        builtin_char_syntax(&mut eval, vec![Value::fixnum('\n' as i64)]).unwrap(),
        Value::char('>')
    );
}

#[test]
fn forward_comment_skips_whitespace_and_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("  foo");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    let out = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(out, Value::NIL);
    let point_1 = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(point_1, 3);
}

#[test]
fn skip_syntax_forward_honors_complement_marker() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("  hello");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    let out = builtin_skip_syntax_forward(&mut eval, vec![Value::string("^ w")]).unwrap();
    assert_eq!(out, Value::fixnum(0));
    let point_1 = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(point_1, 1);
}

#[test]
fn forward_comment_two_char_end_style_uses_first_ender_char() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        let mut tbl = super::SyntaxTable::isolate_for_buffer(buf);
        tbl.modify_syntax_entry('/', string_to_syntax(". 124b").unwrap());
        tbl.modify_syntax_entry('*', string_to_syntax(". 23").unwrap());
        tbl.modify_syntax_entry('\n', string_to_syntax("> b").unwrap());
        buf.insert("code /* block comment */ more // line comment\nrest");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get() + 5,
        ));
    }

    let out = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(out, Value::T);
    let point_1 = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(point_1, 25);
}

#[test]
fn backward_comment_two_char_end_style_uses_first_ender_char() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "code /* block comment */ more");
    install_c_block_comment_syntax(&mut eval);
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        // Position immediately after the two-character comment end.
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(char_pos_to_byte(buf, 24)));
    }

    let out = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();
    assert_eq!(out, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 6);
}

#[test]
fn forward_comment_validates_arity_and_type() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    match builtin_forward_comment(&mut eval, vec![]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(sig.data.first(), Some(&Value::symbol("forward-comment")));
        }
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    match builtin_forward_comment(&mut eval, vec![Value::symbol("x")]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("integerp")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

/// Backward comment traversal with single-line (`;` ... `\n`) comments.
///
/// Buffer: "code\n;; c1\n;; c2\n;; c3\n"
/// Emacs 1-based positions:
///   1..4   "code"
///   5      \n
///   6..10  ";; c1"
///   11     \n
///   12..16 ";; c2"
///   17     \n
///   18..22 ";; c3"
///   23     \n
///
/// From point-max (24):
///   (forward-comment -1) => t, point=18  (before ";; c3")
///   (forward-comment -3) => t, point=6   (before ";; c1")
#[test]
fn forward_comment_backward_single_line_comments() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        // Set up ; as comment start, \n as comment end
        let mut tbl = super::SyntaxTable::isolate_for_buffer(buf);
        tbl.modify_syntax_entry(
            ';',
            SyntaxEntry {
                class: SyntaxClass::Comment,
                matching_char: None,
                flags: SyntaxFlags::empty(),
            },
        );
        tbl.modify_syntax_entry(
            '\n',
            SyntaxEntry {
                class: SyntaxClass::EndComment,
                matching_char: None,
                flags: SyntaxFlags::empty(),
            },
        );
        buf.insert("code\n;; c1\n;; c2\n;; c3\n");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_max_emacs_byte_pos().get(),
        ));
    }

    // forward-comment -1 from point-max: skip back one comment
    let out = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();
    assert_eq!(out, Value::T, "forward-comment -1 should return t");
    let point_1based = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(
        point_1based, 18,
        "after -1 skip, point should be at 18 (;; c3)"
    );

    // Reset to point-max, forward-comment -3: skip back three comments
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_max_emacs_byte_pos().get(),
        ));
    }
    let out = builtin_forward_comment(&mut eval, vec![Value::fixnum(-3)]).unwrap();
    assert_eq!(out, Value::T, "forward-comment -3 should return t");
    let point_1based = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(
        point_1based, 6,
        "after -3 skip, point should be at 6 (;; c1)"
    );
}

/// Backward comment traversal stops on non-comment text.
///
/// Buffer: "code\n;; c1\n;; c2\n;; c3\n"
/// From point-max, (forward-comment -100) should stop at "code" boundary,
/// returning nil with point at 6 (the start of ";; c1").
/// Actually GNU returns nil at position 5 (after "code\n") since it can't
/// skip past "code".  Let me reconsider...
///
/// GNU's logic: from point-max(24), going backward:
///   Skips \n (EndComment/whitespace), then comment 3, then comment 2,
///   then comment 1. After skipping 3 comments, point is at 6 (before
///   ";; c1"). The \n at position 5 is EndComment — back_comment is
///   called, it tries to find a matching comment start before pos 5.
///   It finds no comment start (only "code"), so back_comment fails.
///   Since ch=='\n', treat as whitespace. Now at pos 4 (after "code"),
///   char_before is 'e' — class=Word, not whitespace/comment.
///   Return false → nil, point stays at 5.
///
/// Wait, that means -100 returns nil and point=5 (after skipping
/// the 3 comments but failing on the 4th).
/// Actually let me re-examine: in GNU when back_comment fails on the
/// \n and treats it as whitespace, it continues the inner loop.
/// Next char before pos 4 is 'e', class=Word → returns nil.
/// But GNU does `inc_both` at the leave label, so point = 5.
#[test]
fn forward_comment_backward_stops_at_non_comment() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        let mut tbl = super::SyntaxTable::isolate_for_buffer(buf);
        tbl.modify_syntax_entry(
            ';',
            SyntaxEntry {
                class: SyntaxClass::Comment,
                matching_char: None,
                flags: SyntaxFlags::empty(),
            },
        );
        tbl.modify_syntax_entry(
            '\n',
            SyntaxEntry {
                class: SyntaxClass::EndComment,
                matching_char: None,
                flags: SyntaxFlags::empty(),
            },
        );
        buf.insert("code\n;; c1\n;; c2\n;; c3\n");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_max_emacs_byte_pos().get(),
        ));
    }

    // forward-comment -100 from point-max: try to skip more comments than exist
    let out = builtin_forward_comment(&mut eval, vec![Value::fixnum(-100)]).unwrap();
    assert_eq!(
        out,
        Value::NIL,
        "forward-comment -100 should return nil (not enough comments)"
    );
    // Point should be after "code" — at position 5 in 1-based Emacs terms
    let point_1based = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(
        point_1based, 5,
        "after failed -100 skip, point should be at 5"
    );
}

#[test]
fn backward_prefix_chars_default_is_noop() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("''foo");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(char_pos_to_byte(buf, 2)));
    }

    let out = builtin_backward_prefix_chars(&mut eval, vec![]).unwrap();
    assert_eq!(out, Value::NIL);
    let point_1 = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(point_1, 3);
}

#[test]
fn backward_prefix_chars_moves_over_prefix_flag_chars() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("''foo");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
        let entry = string_to_syntax(". p").unwrap();
        super::SyntaxTable::isolate_for_buffer(buf).modify_syntax_entry('\'', entry);
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(char_pos_to_byte(buf, 2)));
    }

    builtin_backward_prefix_chars(&mut eval, vec![]).unwrap();
    let point_1 = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(point_1, 1);
}

#[test]
fn backward_prefix_chars_validates_arity() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    match builtin_backward_prefix_chars(&mut eval, vec![Value::fixnum(1)]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data.first(),
                Some(&Value::symbol("backward-prefix-chars"))
            );
        }
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn modify_syntax_entry_at_descriptor_inherits_parent_or_default() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('x' as i64), Value::string("@")],
    )
    .unwrap();

    let out = builtin_char_syntax(&mut eval, vec![Value::fixnum('x' as i64)]).unwrap();
    assert_eq!(out, Value::char(' '));
}

#[test]
fn syntax_ppss_flush_cache_contract() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    assert_eq!(
        builtin_syntax_ppss_flush_cache(&mut eval, vec![Value::fixnum(1)]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_syntax_ppss_flush_cache(
            &mut eval,
            vec![Value::fixnum(1), Value::symbol("ignored"), Value::fixnum(3)],
        )
        .unwrap(),
        Value::NIL
    );

    match builtin_syntax_ppss_flush_cache(&mut eval, vec![]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data.first(),
                Some(&Value::symbol("syntax-ppss-flush-cache"))
            );
        }
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    match builtin_syntax_ppss_flush_cache(&mut eval, vec![Value::NIL]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("number-or-marker-p")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn scan_lists_basic_and_backward_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(a b)");
    }

    let forward = builtin_scan_lists(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(0)],
    )
    .unwrap();
    assert_eq!(forward, Value::fixnum(6));

    let backward = builtin_scan_lists(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(-1), Value::fixnum(0)],
    )
    .unwrap();
    assert_eq!(backward, Value::NIL);

    let second_list = builtin_scan_lists(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(0)],
    )
    .unwrap();
    assert_eq!(second_list, Value::NIL);
}

#[test]
fn scan_lists_depth_exits_containing_list() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(progn\n  (message \"x\")\n  )tail\n");
    }

    let forward = builtin_scan_lists(
        &mut eval,
        vec![Value::fixnum(24), Value::fixnum(1), Value::fixnum(1)],
    )
    .unwrap();
    assert_eq!(forward, Value::fixnum(27));
}

#[test]
fn scan_lists_unbalanced_signal_carries_gnu_positions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(foo (bar baz) quux)");
    }

    match builtin_scan_lists(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(1), Value::fixnum(1)],
    ) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(
                sig.data,
                vec![
                    Value::string("Unbalanced parentheses"),
                    Value::fixnum(1),
                    Value::fixnum(21),
                ]
            );
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }
}

#[test]
fn syntax_after_returns_descriptor_and_nil_out_of_range() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("a(");
    }

    let word = builtin_syntax_after(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(
        word,
        syntax_entry_to_value(&SyntaxEntry::simple(SyntaxClass::Word))
    );

    let open = builtin_syntax_after(&mut eval, vec![Value::fixnum(2)]).unwrap();
    assert_eq!(
        open,
        syntax_entry_to_value(&SyntaxEntry::with_match(SyntaxClass::Open, ')'))
    );

    let oob = builtin_syntax_after(&mut eval, vec![Value::fixnum(3)]).unwrap();
    assert_eq!(oob, Value::NIL);
}

#[test]
fn scan_sexps_basic_and_backward_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(a b)");
    }

    let forward = builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap();
    assert_eq!(forward, Value::fixnum(6));

    let backward =
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(-1)]).unwrap();
    assert_eq!(backward, Value::NIL);
}

#[test]
fn scan_sexps_returns_nil_when_count_not_exhausted_at_boundary() {
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

    let past_end = builtin_scan_sexps(&mut eval, vec![Value::fixnum(4), Value::fixnum(1)]).unwrap();
    assert_eq!(past_end, Value::NIL);

    let not_enough =
        builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(2)]).unwrap();
    assert_eq!(not_enough, Value::NIL);
}

#[test]
fn scan_sexps_unbalanced_signal_carries_gnu_positions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(foo (bar baz)");
    }

    match builtin_scan_sexps(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(
                sig.data,
                vec![
                    Value::string("Unbalanced parentheses"),
                    Value::fixnum(1),
                    Value::fixnum(15),
                ]
            );
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }
}

#[test]
fn forward_sexp_unexpected_close_signal_carries_gnu_positions() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(")");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    match builtin_forward_sexp(&mut eval, vec![Value::fixnum(1)]) {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "scan-error");
            assert_eq!(
                sig.data,
                vec![
                    Value::string("Containing expression ends prematurely"),
                    Value::fixnum(1),
                    Value::fixnum(2),
                ]
            );
        }
        other => panic!("expected scan-error signal, got {other:?}"),
    }
}

#[test]
fn forward_sexp_stops_at_narrowing_boundary_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(aaa (bbb (ccc) ddd) eee)");
        buf.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(5, 20));
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    builtin_forward_sexp(&mut eval, vec![Value::fixnum(3)]).unwrap();

    let point = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .point_char_pos()
        .get() as i64
        + 1;
    assert_eq!(point, 21);
}

#[test]
fn parse_partial_sexp_baseline_shapes() {
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
    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(4)]).unwrap();
    assert_eq!(
        state,
        Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::fixnum(1),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ])
    );

    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(a)");
    }
    let nested =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();
    assert_eq!(
        nested,
        Value::list(vec![
            Value::fixnum(1),
            Value::fixnum(1),
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::list(vec![Value::fixnum(1)]),
            Value::NIL,
        ])
    );
}

#[test]
fn parse_partial_sexp_accepts_marker_positions_like_gnu() {
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

    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    let from = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.buffers,
        buffer_id,
        LispCharPos1::new(1),
        false,
    );
    let to = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.buffers,
        buffer_id,
        LispCharPos1::new(4),
        false,
    );

    let state = builtin_parse_partial_sexp(&mut eval, vec![from, to]).unwrap();
    assert_eq!(
        state,
        Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::fixnum(1),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ])
    );
}

#[test]
fn parse_partial_sexp_rejects_non_integer_positions_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let err = builtin_parse_partial_sexp(&mut eval, vec![Value::make_float(1.2), Value::fixnum(1)])
        .unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(
                sig.symbol,
                crate::emacs_core::intern::intern("wrong-type-argument")
            );
            assert_eq!(
                sig.data.first(),
                Some(&Value::symbol("integer-or-marker-p"))
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }
}

#[test]
fn parse_partial_sexp_tracks_completed_sexp_per_nesting_level_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(a (");
    }

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();
    assert_eq!(
        state,
        Value::list(vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::list(vec![Value::fixnum(1), Value::fixnum(4)]),
            Value::NIL,
        ])
    );
}

#[test]
fn parse_partial_sexp_preserves_gnu_negative_depth_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(")");
    }

    let close =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(2)]).unwrap();
    assert_eq!(
        close,
        Value::list(vec![
            Value::fixnum(-1),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(-1),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ])
    );

    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(")(");
    }

    let close_then_open =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();
    assert_eq!(
        close_then_open,
        Value::list(vec![
            Value::fixnum(0),
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(-1),
            Value::NIL,
            Value::NIL,
            Value::list(vec![Value::fixnum(2)]),
            Value::NIL,
        ])
    );
}

#[test]
fn syntax_ppss_baseline_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("(a)");
    }

    let state = builtin_syntax_ppss(&mut eval, vec![Value::fixnum(3)]).unwrap();
    assert_eq!(
        state,
        Value::list(vec![
            Value::fixnum(1),
            Value::fixnum(1),
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::list(vec![Value::fixnum(1)]),
            Value::NIL,
        ])
    );
}

#[test]
fn parse_partial_sexp_enters_single_char_line_comment_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        let mut tbl = super::SyntaxTable::isolate_for_buffer(buf);
        tbl.modify_syntax_entry(';', SyntaxEntry::simple(SyntaxClass::Comment));
        tbl.modify_syntax_entry('\n', SyntaxEntry::simple(SyntaxClass::EndComment));
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(";; x\n");
    }

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(2)]).unwrap();
    assert_eq!(
        state,
        Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::fixnum(1),
            Value::NIL,
            Value::NIL,
        ])
    );
}

#[test]
fn syntax_ppss_reports_string_state_and_start_position() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert("\"ab");
    }

    let state = builtin_syntax_ppss(&mut eval, vec![Value::fixnum(2)]).unwrap();
    assert_eq!(
        state,
        Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::fixnum('"' as i64),
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::fixnum(1),
            Value::NIL,
            Value::NIL,
        ])
    );
}

#[test]
fn parse_partial_sexp_commentstop_syntax_table_moves_point_across_comment() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        let mut tbl = super::SyntaxTable::isolate_for_buffer(buf);
        tbl.modify_syntax_entry(';', SyntaxEntry::simple(SyntaxClass::Comment));
        tbl.modify_syntax_entry('\n', SyntaxEntry::simple(SyntaxClass::EndComment));
        buf.delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            buf.point_min_emacs_byte_pos().get(),
            buf.point_max_emacs_byte_pos().get(),
        ));
        buf.insert(";; x\nfoo");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(
            buf.point_min_emacs_byte_pos().get(),
        ));
    }

    let first = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(9),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(
        first,
        Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::fixnum(1),
            Value::NIL,
            Value::NIL,
        ])
    );
    assert_eq!(
        eval.buffers
            .current_buffer()
            .expect("current buffer")
            .point_char_pos()
            .get() as i64
            + 1,
        2
    );

    let second = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(9),
            Value::NIL,
            Value::NIL,
            first,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(
        second,
        Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ])
    );
    assert_eq!(
        eval.buffers
            .current_buffer()
            .expect("current buffer")
            .point_char_pos()
            .get() as i64
            + 1,
        6
    );
}

// -----------------------------------------------------------------------
// Direct-read helpers (syntax_class_at_char / syntax_entry_at_char)
// GNU-parity: mirrors SYNTAX_ENTRY(c) = CHAR_TABLE_REF(syntax_table, c).
// -----------------------------------------------------------------------

#[test]
fn syntax_class_at_char_matches_gnu_defaults() {
    crate::test_utils::init_test_tracing();
    // Pin the GNU-standard syntax-class for representative characters,
    // read from the live chartable the default buffer holds.
    let mut eval = crate::emacs_core::eval::Context::new();
    let table = super::current_buffer_syntax_table_object_in_buffers(&mut eval.buffers)
        .expect("buffer syntax-table");

    let cases = [
        ('a', SyntaxClass::Word),
        ('Z', SyntaxClass::Word),
        ('0', SyntaxClass::Word),
        (' ', SyntaxClass::Whitespace),
        ('\t', SyntaxClass::Whitespace),
        ('\n', SyntaxClass::Whitespace),
        ('_', SyntaxClass::Symbol),
        ('(', SyntaxClass::Open),
        (')', SyntaxClass::Close),
        ('"', SyntaxClass::StringDelim),
        ('\\', SyntaxClass::Escape),
        ('.', SyntaxClass::Punctuation),
        (';', SyntaxClass::Punctuation),
        ('\u{4e2d}', SyntaxClass::Word), // CJK: range 0x80..=0x3FFFFF is Word
        ('\u{1F600}', SyntaxClass::Word),
    ];
    for (c, expected) in cases {
        let got = super::syntax_class_at_char(&table, c);
        assert_eq!(
            got, expected,
            "class for {:?} (U+{:04X}) expected {:?}, got {:?}",
            c, c as u32, expected, got
        );
    }
}

#[test]
fn syntax_class_at_char_defaults_for_missing_entries() {
    crate::test_utils::init_test_tracing();
    // An empty char-table with no explicit entries and no default:
    // ASCII → Whitespace, high codepoints → Word.
    let table = crate::emacs_core::chartable::make_char_table_value(
        Value::symbol("syntax-table"),
        Value::NIL,
    );
    assert_eq!(
        super::syntax_class_at_char(&table, 'a'),
        SyntaxClass::Whitespace
    );
    assert_eq!(
        super::syntax_class_at_char(&table, '\u{4e2d}'),
        SyntaxClass::Word
    );
}
