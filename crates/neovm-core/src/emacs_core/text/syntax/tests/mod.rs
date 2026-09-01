use super::*;
use crate::buffer::buffer::{Buffer, BufferId};
use crate::buffer::{CharPos0, LispCharPos1};
use crate::emacs_core::value::eq_value;

/// Helper: create a buffer with given text, point at start, full accessible range.
fn buf_with_text(text: &str) -> Buffer {
    let mut buf = Buffer::new(
        BufferId(99),
        Value::string("test-syntax"),
        crate::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
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

fn install_c_line_and_block_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    install_c_block_comment_syntax(eval);
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('\n' as i64), Value::string("> b")])
        .expect("install C++ line-comment end syntax");
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

fn install_haskell_nested_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('{' as i64), Value::string("(}1nb")],
    )
    .expect("install Haskell opening brace syntax");
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('}' as i64), Value::string("){4nb")],
    )
    .expect("install Haskell closing brace syntax");
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('-' as i64), Value::string("< 123")],
    )
    .expect("install Haskell dash comment syntax");
}

fn install_single_character_style_c_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('@' as i64), Value::string("< c")])
        .expect("install style-c comment start");
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('!' as i64), Value::string("> c")])
        .expect("install style-c comment end");
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('\n' as i64), Value::string(">")])
        .expect("install style-a comment end");
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
        let _cell_cdr = value.cons_cdr();
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

// -----------------------------------------------------------------------
// Escape / char-quote joining the next char into a symbol sexp (GROUP=syntax-scan, bug A)
//
// GNU oracle (emacs-lisp-mode, `\` is escape, `?` is symbol):
//   "foo\)bar baz", (forward-sexp 1) from point 1 -> point 9
//   "?\(",          (forward-sexp 1) from point 1 -> point 4
//   "foo\)bar baz", scan-sexps from 1 -> 9
//   "foo\)bar baz", (backward-sexp) from point 9 -> point 1
//   "foo\",         scan-sexps trailing escape -> scan-error
// neomacs scan_sexps uses 0-based positions, so GNU N maps to neomacs N-1.
// -----------------------------------------------------------------------

/// Standard syntax table with `?` treated as a symbol constituent, matching
/// emacs-lisp-mode where the GNU oracle examples were captured.
fn elisp_like_table() -> SyntaxTable {
    let mut table = SyntaxTable::new_standard();
    table.modify_syntax_entry('?', SyntaxEntry::simple(SyntaxClass::Symbol));
    table
}

#[test]
fn scan_sexps_forward_absorbs_escaped_char_in_symbol() {
    crate::test_utils::init_test_tracing();
    // "foo\)bar baz" — the escaped `)` joins foo...bar into one symbol.
    let buf = buf_with_text("foo\\)bar baz");
    let table = SyntaxTable::new_standard();
    let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
    assert_eq!(pos, 8); // GNU point 9 -> neomacs 8 (end of "foo\)bar")
}

#[test]
fn scan_sexps_forward_symbol_then_escape() {
    crate::test_utils::init_test_tracing();
    // "?\(" — `?` is a symbol, `\` escapes `(` into the same symbol sexp.
    let buf = buf_with_text("?\\(");
    let table = elisp_like_table();
    let pos = scan_sexps(&buf, &table, 0, 1).unwrap();
    assert_eq!(pos, 3); // GNU point 4 -> neomacs 3
}

#[test]
fn scan_sexps_backward_absorbs_escaped_char_in_symbol() {
    crate::test_utils::init_test_tracing();
    // Backward from after "foo\)bar" lands at the start of the symbol.
    let buf = buf_with_text("foo\\)bar baz");
    let table = SyntaxTable::new_standard();
    let pos = scan_sexps(&buf, &table, 8, -1).unwrap();
    assert_eq!(pos, 0); // GNU point 1 -> neomacs 0
}

#[test]
fn scan_sexps_backward_symbol_then_escape() {
    crate::test_utils::init_test_tracing();
    let buf = buf_with_text("?\\(");
    let table = elisp_like_table();
    let pos = scan_sexps(&buf, &table, 3, -1).unwrap();
    assert_eq!(pos, 0);
}

#[test]
fn scan_sexps_trailing_escape_signals_error() {
    crate::test_utils::init_test_tracing();
    // "foo\" — symbol foo then a trailing escape with nothing to quote.
    // GNU signals scan-error ("Unbalanced parentheses" 1 5).
    let buf = buf_with_text("foo\\");
    let table = SyntaxTable::new_standard();
    let err = scan_sexps_with_options(
        &buf,
        &table,
        0,
        1,
        SyntaxProperties::Ignore,
        SexpScanPolicy::default(),
    )
    .unwrap_err();
    assert_eq!(err.message, "Unbalanced parentheses");
    // last_good = GNU 1 -> char index 0; at = GNU 5 -> char index 4 (EOB).
    assert_eq!(err.last_good, 0);
    assert_eq!(err.at, 4);
}

#[test]
fn scan_sexps_lone_escape_forward_signals_error() {
    crate::test_utils::init_test_tracing();
    // A lone escape at the very start (nothing absorbed yet) also errors.
    let buf = buf_with_text("\\");
    let table = SyntaxTable::new_standard();
    assert!(
        scan_sexps_with_options(
            &buf,
            &table,
            0,
            1,
            SyntaxProperties::Ignore,
            SexpScanPolicy::default(),
        )
        .is_err()
    );
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
        maybe_skip_comment_backward(
            buf,
            2,
            SyntaxProperties::Ignore,
            entry.class,
            entry.flags,
            CommentEndEscapePolicy::default(),
        ),
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
        let _cell_cdr = val.cons_cdr();
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
fn standard_quote_and_escape_entries_are_not_canonical_bare_objects() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    for character in [34, 92] {
        let result = eval
            .eval_str(&format!(
                "(eq (char-table-range (standard-syntax-table) {character}) \
                     (string-to-syntax (string {character})))"
            ))
            .unwrap();
        assert!(
            result.is_nil(),
            "standard syntax entry for character {character} must be fresh"
        );
    }
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
fn forward_comment_closes_haskell_nested_comment_when_ender_starts_with_comment_class() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "{-# pragma #-}module");
    install_haskell_nested_comment_syntax(&mut eval);

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 15);
}

fn install_dual_role_nested_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(
        eval,
        vec![Value::fixnum('|' as i64), Value::string(". 1234n")],
    )
    .expect("install nested dual-role delimiter syntax");
}

/// GNU `forw_comment` gives an ender precedence over an opener while already
/// inside a comment.  A nested token with both roles must therefore close the
/// current level rather than opening levels until end-of-buffer.
#[test]
fn forward_comment_gives_dual_role_nested_marker_ender_precedence() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "|| body ||");
    install_dual_role_nested_comment_syntax(&mut eval);

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 11);
}

/// GNU processes a nested single-character opener before asking whether the
/// same character also starts a two-character ender.  With both roles on the
/// first character, the depth increment and decrement cancel instead of
/// closing the outer comment.
#[test]
fn forward_comment_preserves_single_opener_before_pair_ender_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "|| body ||");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('|' as i64), Value::string("< 1234n")],
    )
    .expect("install nested single-opener and dual-role pair syntax");

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 11);
}

/// Two-character comment entry is one token in GNU: its pair flags override
/// the first character's base `<` syntax and carry the pair's nestedness.
#[test]
fn forward_comment_classifies_two_char_opener_before_base_syntax() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "~^ body !?");
    for (ch, syntax) in [('~', "< 1"), ('^', ". 2n"), ('!', ". 3"), ('?', ". 4n")] {
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum(ch as i64), Value::string(syntax)],
        )
        .expect("install two-character comment syntax");
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 11);
}

fn install_comment_fence_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('!' as i64), Value::string("!")])
        .expect("install generic comment fence syntax");
}

fn set_comment_end_can_be_escaped(eval: &mut crate::emacs_core::eval::Context, enabled: Value) {
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    eval.set_buffer_local_binding_by_id(
        buffer_id,
        crate::emacs_core::intern::intern("comment-end-can-be-escaped"),
        enabled,
    )
    .expect("set comment-end-can-be-escaped");
}

#[test]
fn forward_comment_fence_honors_comment_end_escape_policy() {
    crate::test_utils::init_test_tracing();

    for (policy, expected_point) in [(Value::NIL, 5), (Value::T, 7)] {
        let mut eval = crate::emacs_core::eval::Context::new();
        replace_current_buffer_text(&mut eval, "!a\\!b!");
        install_comment_fence_syntax(&mut eval);
        set_comment_end_can_be_escaped(&mut eval, policy);

        let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();

        assert_eq!(moved, Value::T);
        assert_eq!(current_point_lisp_pos(&eval), expected_point);
    }
}

#[test]
fn backward_comment_fence_ignores_quoted_fence_candidate() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "!a\\!b!");
    install_comment_fence_syntax(&mut eval);
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 1);
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

/// GNU recognizes the complete `/*` marker before asking `char_quoted`
/// whether its first character is escaped.  The backslash therefore prevents
/// this apparent opener from matching the later `*/`.
#[test]
fn backward_comment_rejects_escaped_two_char_opener() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "\\/* x */");
    install_c_block_comment_syntax(&mut eval);
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 9);
}

/// Once GNU recognizes a two-character opener it replaces the first
/// character's raw class with `Scomment` before checking quoting.  A raw `>`
/// class therefore cannot make an escaped opener unescapable.
#[test]
fn backward_comment_escaped_pair_opener_overrides_raw_ender_class() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "\\~^ x !?");
    for (ch, syntax) in [('~', "> 1"), ('^', ". 2"), ('!', ". 3"), ('?', ". 4")] {
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum(ch as i64), Value::string(syntax)],
        )
        .expect("install two-character comment syntax");
    }
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 9);
}

/// GNU forms the complete two-character marker before interpreting either
/// character's base class.  Escape syntax on the first character therefore
/// does not quote the second, and String syntax on the second does not hide
/// the opener from the backward walk.
#[test]
fn backward_comment_classifies_two_char_opener_before_base_syntax() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "~^ x !?");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('~' as i64), Value::string("\\ 1")],
    )
    .expect("install escape-class opener first syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('^' as i64), Value::string("\" 2")],
    )
    .expect("install string-class opener second syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('!' as i64), Value::string(". 3")],
    )
    .expect("install ender first syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('?' as i64), Value::string(". 4")],
    )
    .expect("install ender second syntax");
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 1);
}

#[test]
fn backward_comment_treats_first_ambiguous_two_char_delimiter_as_opener() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "AB\n-- C\nDE\n");
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        let mut table = super::SyntaxTable::isolate_for_buffer(buf);
        table.modify_syntax_entry('-', string_to_syntax(". 1234").unwrap());
        table.modify_syntax_entry('\n', string_to_syntax(">").unwrap());
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(8));
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-100)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 3);
}

/// GNU's overlap guard includes a base `<` character even when the
/// `(current,right)` pair is not itself a marker.  Here `b` looks like the
/// opener for the final `>`, but it is also the second half of `ab`, which
/// already closed the earlier comment.  Forward reparse must reject it.
#[test]
fn backward_comment_reparses_base_opener_overlapping_left_ender() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "< x ab y >");
    for (ch, syntax) in [('<', "<"), ('>', ">"), ('a', ". 3"), ('b', "< 4")] {
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum(ch as i64), Value::string(syntax)],
        )
        .expect("install overlapping delimiter syntax");
    }
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 11);
}

/// `#` starts a comment, `\n` ends one; `"` and `\` keep their standard
/// string-quote and escape syntax.  This is the three-entry table from the
/// upstream reproducer for backward sexp motion across a comment character
/// that only *appears* inside a string.
fn install_hash_line_comment_syntax(eval: &mut crate::emacs_core::eval::Context) {
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('#' as i64), Value::string("<")])
        .expect("install hash comment start syntax");
    builtin_modify_syntax_entry(eval, vec![Value::fixnum('\n' as i64), Value::string(">")])
        .expect("install newline comment end syntax");
}

/// With `comment-end-can-be-escaped` non-nil, GNU does not enter
/// `back_comment` for an escaped newline ender.  It treats that newline as
/// whitespace, then stops at the quoting backslash.
#[test]
fn backward_comment_honors_escaped_comment_ender_policy() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "# x \\\n");
    install_hash_line_comment_syntax(&mut eval);
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    eval.set_buffer_local_binding_by_id(
        buffer_id,
        crate::emacs_core::intern::intern("comment-end-can-be-escaped"),
        Value::T,
    )
    .expect("enable escaped comment enders");
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 6);
}

/// When backward motion first encounters the second character of a
/// two-character ender, GNU applies `comment-end-can-be-escaped` to that
/// position.  The pair's first character can itself supply Escape syntax,
/// making the second character quoted even though the first is not.
#[test]
fn backward_comment_applies_escape_policy_to_two_char_ender_second_character() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "~^ x !?");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('~' as i64), Value::string(". 1")],
    )
    .expect("install opener first syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('^' as i64), Value::string(". 2")],
    )
    .expect("install opener second syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('!' as i64), Value::string("\\ 3")],
    )
    .expect("install escape-class ender first syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('?' as i64), Value::string(". 4")],
    )
    .expect("install ender second syntax");
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    eval.set_buffer_local_binding_by_id(
        buffer_id,
        crate::emacs_core::intern::intern("comment-end-can-be-escaped"),
        Value::T,
    )
    .expect("enable escaped comment enders");
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 8);
}

/// GNU `Fforward_comment` stops on quoted whitespace rather than silently
/// skipping it as ordinary inter-comment space.
#[test]
fn backward_comment_stops_at_quoted_whitespace() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "\\ ");
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 3);
}

/// A failed two-character end marker ending in newline is consumed as one
/// whitespace-like unit.  GNU continues from the marker's first character,
/// never from the byte between the pair.
#[test]
fn backward_comment_failed_two_char_newline_ender_resumes_before_pair() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "x\n");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('x' as i64), Value::string(". 3")],
    )
    .expect("install ender-first syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('\n' as i64), Value::string("> 4")],
    )
    .expect("install newline ender-second syntax");
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 1);
}

/// `scan_lists` always enters GNU `back_comment` after it has classified an
/// ender; unlike `Fforward_comment`, its entry is not suppressed by
/// `comment-end-can-be-escaped`.
#[test]
fn backward_sexp_comment_entry_ignores_forward_comment_escape_policy() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "a # (\\\n");
    install_hash_line_comment_syntax(&mut eval);
    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    set_comment_end_can_be_escaped(&mut eval, Value::T);

    let end = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .accessible_char_region()
        .end_lisp()
        .as_i64();
    let start = builtin_scan_sexps(&mut eval, vec![Value::fixnum(end), Value::fixnum(-1)])
        .expect("scan backward over escaped comment ender");

    assert_eq!(start, Value::fixnum(1));
}

#[test]
fn backward_sexp_recognizes_two_char_ender_with_quoted_first_character() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "a # (\\!?");
    install_hash_line_comment_syntax(&mut eval);
    for (ch, syntax) in [('!', ". 3"), ('?', ". 4")] {
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum(ch as i64), Value::string(syntax)],
        )
        .expect("install two-character ender syntax");
    }
    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);
    set_comment_end_can_be_escaped(&mut eval, Value::T);

    let end = eval
        .buffers
        .current_buffer()
        .expect("current buffer")
        .accessible_char_region()
        .end_lisp()
        .as_i64();
    let start = builtin_scan_sexps(&mut eval, vec![Value::fixnum(end), Value::fixnum(-1)])
        .expect("scan backward over quoted two-character comment ender");

    assert_eq!(start, Value::fixnum(1));

    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }
    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();
    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 9);
}

fn backward_single_char_comment_with_syntax(
    opener_syntax: &str,
    ender_syntax: &str,
) -> (Value, i64) {
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "< x >");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('<' as i64), Value::string(opener_syntax)],
    )
    .expect("install comment opener syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('>' as i64), Value::string(ender_syntax)],
    )
    .expect("install comment ender syntax");
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();
    (moved, current_point_lisp_pos(&eval))
}

/// GNU matches a comment delimiter's style *and* `n` flag.  The style alone
/// does not let a flat ender close a nested opener, or vice versa.
#[test]
fn backward_comment_requires_matching_nestability() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        backward_single_char_comment_with_syntax("< n", ">"),
        (Value::NIL, 6)
    );
    assert_eq!(
        backward_single_char_comment_with_syntax("<", "> n"),
        (Value::NIL, 6)
    );
    assert_eq!(
        backward_single_char_comment_with_syntax("< n", "> n"),
        (Value::T, 1)
    );
}

fn scan_lists_backward_from(eval: &mut crate::emacs_core::eval::Context, from: i64) -> Value {
    builtin_scan_lists(
        eval,
        vec![Value::fixnum(from), Value::fixnum(-1), Value::fixnum(0)],
    )
    .expect("scan-lists backward")
}

/// GNU `back_comment` tracks string-quote parity across its backward walk and
/// refuses a comment starter found while inside a string (`src/syntax.c`,
/// `case Scomment`).  Without that, the `#` inside `"#"` is taken for a real
/// comment start, the closing quote is swallowed, and the scan runs off the
/// beginning of the buffer.
#[test]
fn scan_lists_backward_ignores_comment_char_inside_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    //           1234 5 6  789012345 6 78 9
    //           x = { \n   a = "#"; \n }  \n
    replace_current_buffer_text(&mut eval, "x = {\n  a = \"#\";\n}\n");
    install_hash_line_comment_syntax(&mut eval);
    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(scan_lists_backward_from(&mut eval, 19), Value::fixnum(5));
}

/// An escaped string quote must not flip the parity: GNU skips characters that
/// `char_quoted` reports as escaped before classifying them.
#[test]
fn scan_lists_backward_ignores_escaped_string_quote_before_comment_char() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "x = {\n  a = \"y\\\"#\\\"z\";\n}\n");
    install_hash_line_comment_syntax(&mut eval);
    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(scan_lists_backward_from(&mut eval, 25), Value::fixnum(5));
}

/// The parity guard must not cost us genuine comments: the same motion over a
/// real `# hi` line still has to reach the opening brace.
#[test]
fn scan_lists_backward_still_crosses_a_genuine_comment() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "x = {\n  # hi\n  a = 1;\n}\n");
    install_hash_line_comment_syntax(&mut eval);
    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(scan_lists_backward_from(&mut eval, 24), Value::fixnum(5));
}

/// A lone string quote *inside* a real comment makes the backward walk
/// untrustworthy, so GNU falls back to re-parsing forward from a known-safe
/// point and takes the comment start from that parse.  Refusing the comment
/// outright would leave the unbalanced `"` to be scanned as a string.
#[test]
fn scan_lists_backward_reparses_forward_when_the_comment_body_holds_a_string_quote() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "x = {\n  # a \" b\n}\n");
    install_hash_line_comment_syntax(&mut eval);
    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(scan_lists_backward_from(&mut eval, 18), Value::fixnum(5));
}

/// Two string quotes inside the comment body leave the parity even again, so
/// the backward walk reaches the `#` outside any string and never needs the
/// forward re-parse.  Pins that the parity guard is a parity guard and not a
/// blanket "a quote appeared" refusal.
#[test]
fn scan_lists_backward_accepts_a_comment_whose_body_holds_balanced_string_quotes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "x = {\n  # a \" b \" c\n}\n");
    install_hash_line_comment_syntax(&mut eval);
    eval.obarray
        .set_symbol_value("parse-sexp-ignore-comments", Value::T);

    assert_eq!(scan_lists_backward_from(&mut eval, 22), Value::fixnum(5));
}

/// `(forward-comment -1)` over a newline whose line holds a comment character
/// inside a string finds no comment: it stops just before the newline.
#[test]
fn backward_comment_rejects_comment_start_inside_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "a = \"#\";\nb\n");
    install_hash_line_comment_syntax(&mut eval);
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(char_pos_to_byte(buf, 9)));
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 9);
}

/// The escaped-quote variant of the same rejection: the `#` is still inside the
/// string, because `\"` does not close it.
#[test]
fn backward_comment_rejects_comment_start_inside_string_with_escaped_quotes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "a = \"y\\\"#\\\"z\";\nb\n");
    install_hash_line_comment_syntax(&mut eval);
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(char_pos_to_byte(buf, 15)));
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 15);
}

/// A string quote inside the comment body sends the backward walk to the
/// forward re-parse, which still reports the comment: point lands on `#`.
#[test]
fn backward_comment_finds_comment_start_through_forward_reparse() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "a\n# x \" y\nb\n");
    install_hash_line_comment_syntax(&mut eval);
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(char_pos_to_byte(buf, 10)));
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(current_point_lisp_pos(&eval), 3);
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

#[test]
fn forward_comment_keeps_style_c_distinct_from_style_a() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "@first\nsecond!code");
    install_single_character_style_c_comment_syntax(&mut eval);

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(
        current_point_lisp_pos(&eval),
        15,
        "the style-a newline must not terminate a style-c comment"
    );
}

#[test]
fn backward_comment_keeps_style_c_distinct_from_style_a() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "code@first\nsecond!");
    install_single_character_style_c_comment_syntax(&mut eval);
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.goto_emacs_byte_pos(buf.point_max_emacs_byte_pos());
    }

    let moved = builtin_forward_comment(&mut eval, vec![Value::fixnum(-1)]).unwrap();

    assert_eq!(moved, Value::T);
    assert_eq!(
        current_point_lisp_pos(&eval),
        5,
        "the style-a newline must not hide the matching style-c opener"
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
fn parse_partial_sexp_closes_haskell_nested_comment_before_following_code() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "{-# pragma #-}module");
    install_haskell_nested_comment_syntax(&mut eval);

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(21)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::NIL);
    assert_eq!(nth_value(&state, 8), Value::NIL);
}

#[test]
fn parse_partial_sexp_gives_dual_role_nested_marker_ender_precedence() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "|| body ||");
    install_dual_role_nested_comment_syntax(&mut eval);

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(11)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::NIL);
}

#[test]
fn parse_partial_sexp_preserves_single_opener_before_pair_ender_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "|| body ||");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('|' as i64), Value::string("< 1234n")],
    )
    .expect("install nested single-opener and dual-role pair syntax");

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(11)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::fixnum(1));
}

/// GNU recognizes an atomic two-character opener before applying STOPBEFORE
/// to the first character's raw base syntax.
#[test]
fn parse_partial_sexp_stopbefore_does_not_split_two_char_opener() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "~^ body !?");
    for (ch, syntax) in [('~', "( 1"), ('^', ". 2"), ('!', ". 3"), ('?', ". 4")] {
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum(ch as i64), Value::string(syntax)],
        )
        .expect("install two-character comment syntax");
    }

    let state = builtin_parse_partial_sexp(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(11), Value::NIL, Value::T],
    )
    .unwrap();

    assert_eq!(nth_value(&state, 4), Value::NIL);
    assert_eq!(current_point_lisp_pos(&eval), 11);
}

/// GNU recognizes a pair inside `symstarted` before promoting the pending
/// atom.  A word-like raw class on the pair's first character therefore keeps
/// element 2 (the last complete sexp start) nil.
#[test]
fn parse_partial_sexp_word_class_pair_does_not_finish_pending_atom() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "a~^x!?");
    for (ch, syntax) in [('~', "w 1"), ('^', ". 2"), ('!', ". 3"), ('?', ". 4")] {
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum(ch as i64), Value::string(syntax)],
        )
        .expect("install two-character comment syntax");
    }

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(7)]).unwrap();

    assert_eq!(nth_value(&state, 2), Value::NIL);
}

#[test]
fn parse_partial_sexp_incomplete_comment_publishes_trailing_escape() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "/* x \\");
    install_c_block_comment_syntax(&mut eval);

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(7)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::T);
    assert_eq!(nth_value(&state, 5), Value::T);
    assert_eq!(
        nth_value(&state, 10),
        Value::fixnum(parse_prev_syntax_int(
            SyntaxClass::Escape,
            SyntaxFlags::empty()
        ))
    );
}

#[test]
fn parse_partial_sexp_flat_incomplete_comment_hides_trailing_start_first() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "/* x /");
    install_c_block_comment_syntax(&mut eval);

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(7)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::T);
    assert_eq!(nth_value(&state, 10), Value::NIL);
}

#[test]
fn parse_partial_sexp_nested_single_ender_retains_end_first_syntax() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "<<x>");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('<' as i64), Value::string("< n")],
    )
    .expect("install nested single-character opener");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('>' as i64), Value::string("> 3n")],
    )
    .expect("install nested single-character ender with end-first flag");

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::fixnum(1));
    assert_eq!(
        nth_value(&state, 10),
        Value::fixnum(parse_prev_syntax_int(
            SyntaxClass::EndComment,
            SyntaxFlags::COMMENT_END_FIRST | SyntaxFlags::COMMENT_NESTABLE,
        ))
    );
}

#[test]
fn parse_partial_sexp_consumed_comment_escape_is_not_incomplete() {
    crate::test_utils::init_test_tracing();

    for generic_fence in [false, true] {
        let mut eval = crate::emacs_core::eval::Context::new();
        if generic_fence {
            replace_current_buffer_text(&mut eval, "!x\\a");
            install_comment_fence_syntax(&mut eval);
        } else {
            replace_current_buffer_text(&mut eval, "/* x \\a");
            install_c_block_comment_syntax(&mut eval);
        }
        set_comment_end_can_be_escaped(&mut eval, Value::T);
        let point_max = eval
            .buffers
            .current_buffer()
            .expect("current buffer")
            .accessible_char_region()
            .end_lisp()
            .as_i64();

        let state =
            builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(point_max)])
                .unwrap();

        assert!(nth_value(&state, 4).is_truthy());
        assert_eq!(nth_value(&state, 5), Value::NIL);
        assert_eq!(nth_value(&state, 10), Value::NIL);
    }
}

#[test]
fn parse_partial_sexp_commentstop_preserves_entry_syntax() {
    crate::test_utils::init_test_tracing();

    for (commentstop, expected_prev_syntax) in [
        (
            Value::T,
            Value::fixnum(parse_prev_syntax_int(
                SyntaxClass::Comment,
                SyntaxFlags::COMMENT_START_FIRST,
            )),
        ),
        (Value::NIL, Value::NIL),
    ] {
        let mut eval = crate::emacs_core::eval::Context::new();
        replace_current_buffer_text(&mut eval, "<x");
        builtin_modify_syntax_entry(
            &mut eval,
            vec![Value::fixnum('<' as i64), Value::string("< 1")],
        )
        .expect("install start-first single-character comment opener");

        let state = builtin_parse_partial_sexp(
            &mut eval,
            vec![
                Value::fixnum(1),
                Value::fixnum(if commentstop.is_truthy() { 2 } else { 3 }),
                Value::NIL,
                Value::NIL,
                Value::NIL,
                commentstop,
            ],
        )
        .unwrap();

        assert_eq!(nth_value(&state, 10), expected_prev_syntax);
    }
}

#[test]
fn parse_partial_sexp_comment_resume_does_not_apply_old_quoted_as_skip() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "/* \\*/");
    install_c_block_comment_syntax(&mut eval);
    set_comment_end_can_be_escaped(&mut eval, Value::T);

    let first =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();
    assert_eq!(nth_value(&first, 4), Value::T);
    assert_eq!(nth_value(&first, 5), Value::T);

    let resumed = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(5),
            Value::fixnum(7),
            Value::NIL,
            Value::NIL,
            first,
        ],
    )
    .unwrap();

    assert_eq!(nth_value(&resumed, 4), Value::NIL);
    assert_eq!(nth_value(&resumed, 5), Value::NIL);
}

#[test]
fn parse_partial_sexp_comment_resume_closes_a_pair_split_at_oldstate() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "/* x */");
    install_c_block_comment_syntax(&mut eval);

    let first =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(7)]).unwrap();
    assert_eq!(nth_value(&first, 4), Value::T);
    assert!(nth_value(&first, 10).as_fixnum().is_some());

    let resumed = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(7),
            Value::fixnum(8),
            Value::NIL,
            Value::NIL,
            first,
        ],
    )
    .unwrap();

    assert_eq!(nth_value(&resumed, 4), Value::NIL);
    assert_eq!(nth_value(&resumed, 10), Value::NIL);
}

#[test]
fn parse_partial_sexp_comment_resume_opens_a_nested_pair_split_at_oldstate() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "/* /* */ */");
    install_nestable_c_block_comment_syntax(&mut eval);

    let first =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();
    assert_eq!(nth_value(&first, 4), Value::fixnum(1));
    assert!(nth_value(&first, 10).as_fixnum().is_some());

    let resumed = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(5),
            Value::fixnum(9),
            Value::NIL,
            Value::NIL,
            first,
        ],
    )
    .unwrap();

    assert_eq!(
        nth_value(&resumed, 4),
        Value::fixnum(1),
        "the split nested opener must keep the outer comment open after the inner closer"
    );
}

#[test]
fn parse_partial_sexp_comment_resume_does_not_pair_oldstate_at_accessible_begv() {
    crate::test_utils::init_test_tracing();

    for narrowed in [false, true] {
        let mut eval = crate::emacs_core::eval::Context::new();
        replace_current_buffer_text(&mut eval, if narrowed { "x/" } else { "/" });
        install_c_block_comment_syntax(&mut eval);
        if narrowed {
            eval.buffers
                .current_buffer_mut()
                .expect("current buffer")
                .narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(1, 2));
        }

        let begv = if narrowed { 2 } else { 1 };
        let oldstate = Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::NIL,
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::fixnum(parse_prev_syntax_int(
                SyntaxClass::Punctuation,
                SyntaxFlags::COMMENT_END_FIRST,
            )),
        ]);

        let resumed = builtin_parse_partial_sexp(
            &mut eval,
            vec![
                Value::fixnum(begv),
                Value::fixnum(begv + 1),
                Value::NIL,
                Value::NIL,
                oldstate,
            ],
        )
        .unwrap();

        assert_eq!(
            nth_value(&resumed, 4),
            Value::T,
            "OLDSTATE must not synthesize a delimiter before accessible BEGV"
        );
        assert_eq!(nth_value(&resumed, 10), Value::NIL);
    }
}

#[test]
fn parse_partial_sexp_consumed_two_char_closer_has_no_prev_syntax() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "/* x */");
    install_c_block_comment_syntax(&mut eval);

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(8)]).unwrap();

    assert_eq!(nth_value(&state, 4), Value::NIL);
    assert_eq!(nth_value(&state, 10), Value::NIL);
}

#[test]
fn parse_partial_sexp_comment_fence_honors_comment_end_escape_policy() {
    crate::test_utils::init_test_tracing();

    for (policy, expected_comment_state) in [(Value::NIL, Value::T), (Value::T, Value::NIL)] {
        let mut eval = crate::emacs_core::eval::Context::new();
        replace_current_buffer_text(&mut eval, "!a\\!b!");
        install_comment_fence_syntax(&mut eval);
        set_comment_end_can_be_escaped(&mut eval, policy);

        let state = builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(7)])
            .unwrap();

        assert_eq!(nth_value(&state, 4), expected_comment_state);
    }
}

fn parse_partial_sexp_comment_state_for_delimiters(
    opener_syntax: &str,
    ender_syntax: &str,
) -> Value {
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "< x >");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('<' as i64), Value::string(opener_syntax)],
    )
    .expect("install parse comment opener syntax");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('>' as i64), Value::string(ender_syntax)],
    )
    .expect("install parse comment ender syntax");

    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();
    nth_value(&state, 4)
}

/// GNU's forward parser uses the same complete delimiter identity as
/// `back_comment`: a flat ender cannot close a nested comment, and a nested
/// ender cannot close a flat one.
#[test]
fn parse_partial_sexp_requires_matching_comment_nestability() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        parse_partial_sexp_comment_state_for_delimiters("< n", ">"),
        Value::fixnum(1)
    );
    assert_eq!(
        parse_partial_sexp_comment_state_for_delimiters("<", "> n"),
        Value::T
    );
    assert_eq!(
        parse_partial_sexp_comment_state_for_delimiters("< n", "> n"),
        Value::NIL
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

#[test]
fn parse_partial_sexp_preserves_generic_comment_fence_across_stops() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "!abc!");
    builtin_modify_syntax_entry(
        &mut eval,
        vec![Value::fixnum('!' as i64), Value::string("!")],
    )
    .expect("install generic comment fence");

    let first = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(6),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 2);
    assert_eq!(
        nth_value(&first, 4),
        Value::T,
        "generic comment fences are non-nestable"
    );
    assert_eq!(nth_value(&first, 7), Value::symbol("syntax-table"));

    let second = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(6),
            Value::NIL,
            Value::NIL,
            first,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 6);
    assert_eq!(nth_value(&second, 4), Value::NIL);
    assert_eq!(nth_value(&second, 7), Value::NIL);
}

#[test]
fn parse_partial_sexp_preserves_c_line_comment_style_across_stops() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "// header\n#define VALUE 1\n");
    install_c_line_and_block_comment_syntax(&mut eval);

    let first = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(27),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 3);
    assert_eq!(nth_value(&first, 4), Value::T);
    assert_eq!(
        nth_value(&first, 7),
        Value::fixnum(1),
        "GNU serializes C line-comment style b in parse-state element 7"
    );

    let second = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(27),
            Value::NIL,
            Value::NIL,
            first,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(
        current_point_lisp_pos(&eval),
        11,
        "continued parsing must stop immediately after the style-b newline terminator"
    );
    assert_eq!(nth_value(&second, 4), Value::NIL);
    assert_eq!(nth_value(&second, 7), Value::NIL);
}

#[test]
fn parse_partial_sexp_uses_the_main_character_style_for_c_block_comment_end() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "/* header */\n#define VALUE 1\n");
    install_c_line_and_block_comment_syntax(&mut eval);

    let first = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(30),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(current_point_lisp_pos(&eval), 3);
    assert_eq!(nth_value(&first, 4), Value::T);
    assert_eq!(
        nth_value(&first, 7),
        Value::NIL,
        "GNU serializes C block comments as style a"
    );

    let second = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(30),
            Value::NIL,
            Value::NIL,
            first,
            Value::symbol("syntax-table"),
        ],
    )
    .unwrap();
    assert_eq!(
        current_point_lisp_pos(&eval),
        13,
        "continued parsing must stop immediately after the style-a */ terminator"
    );
    assert_eq!(nth_value(&second, 4), Value::NIL);
    assert_eq!(nth_value(&second, 7), Value::NIL);
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

/// Extract the nth element of a `parse-partial-sexp` result list.
fn nth_value(list: &Value, n: usize) -> Value {
    list_to_vec(list)
        .expect("parse-partial-sexp returns a list")
        .get(n)
        .cloned()
        .unwrap_or(Value::NIL)
}

/// Install the `emacs-lisp-mode` expression-prefix syntax on the current
/// buffer, making `'`, backquote, `,` and `#` Squote (class `'`) — the same
/// reassignment `lisp-mode-syntax-table` performs over the fundamental-mode
/// standard table (where these are punctuation).
fn install_elisp_prefix_syntax(eval: &mut crate::emacs_core::eval::Context) {
    let buf = eval.buffers.current_buffer_mut().expect("current buffer");
    let entry = string_to_syntax("'").unwrap();
    let mut table = super::SyntaxTable::isolate_for_buffer(buf);
    for ch in ['\'', '`', ',', '#'] {
        table.modify_syntax_entry(ch, entry.clone());
    }
}

// === parse-partial-sexp prefix-quote / last-complete-sexp (bug 7) ===
//
// GNU `scan_sexps_forward`: a top-level Squote (expression prefix) falls into
// the `default' switch arm — "Ignore whitespace, punctuation, quote,
// endcomment." — so it neither begins an atom nor becomes element 2
// (last-complete-sexp start).  Within a symbol run Squote is a constituent.

#[test]
fn parse_partial_sexp_prefix_quote_does_not_become_last_complete_sexp() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "'abc");
    install_elisp_prefix_syntax(&mut eval);
    // (with-temp-buffer (emacs-lisp-mode) (insert "'abc")
    //   (nth 2 (parse-partial-sexp (point-min) (point-max)))) => 2
    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();
    assert_eq!(nth_value(&state, 2), Value::fixnum(2));
}

#[test]
fn parse_partial_sexp_bare_prefix_quote_has_no_last_complete_sexp() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "'");
    install_elisp_prefix_syntax(&mut eval);
    // (insert "'") (nth 2 (parse-partial-sexp (point-min) (point-max))) => nil
    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(2)]).unwrap();
    assert_eq!(nth_value(&state, 2), Value::NIL);
}

#[test]
fn parse_partial_sexp_other_prefix_quotes_match_gnu() {
    crate::test_utils::init_test_tracing();
    // backquote, comma and `#'` prefixes are all Squote in elisp; each yields
    // the following sexp's start as element 2, never the prefix's own position.
    // GNU: ,foo => 2, `bar => 2, #'foo => 3.
    for (text, to, expected) in [(",foo", 5, 2i64), ("`bar", 5, 2), ("#'foo", 6, 3)] {
        let mut eval = crate::emacs_core::eval::Context::new();
        replace_current_buffer_text(&mut eval, text);
        install_elisp_prefix_syntax(&mut eval);
        let state =
            builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(to)])
                .unwrap();
        assert_eq!(
            nth_value(&state, 2),
            Value::fixnum(expected),
            "element 2 for {text:?}"
        );
    }
}

#[test]
fn parse_partial_sexp_double_prefix_quote_has_no_last_complete_sexp() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "''");
    install_elisp_prefix_syntax(&mut eval);
    // (insert "''") (nth 2 (parse-partial-sexp (point-min) (point-max))) => nil
    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();
    assert_eq!(nth_value(&state, 2), Value::NIL);
}

#[test]
fn parse_partial_sexp_quote_continues_in_progress_symbol_run() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "abc'def");
    install_elisp_prefix_syntax(&mut eval);
    // A quote inside a symbol run is a constituent: `abc'def` is one symbol
    // starting at 1, so element 2 is 1 (GNU => 1).
    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(8)]).unwrap();
    assert_eq!(nth_value(&state, 2), Value::fixnum(1));
}

#[test]
fn parse_partial_sexp_prefix_quote_before_paren_full_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "'abc");
    install_elisp_prefix_syntax(&mut eval);
    // (insert "'abc") (parse-partial-sexp (point-min) (point-max))
    //   => (0 nil 2 nil nil nil 0 nil nil nil nil)
    let state =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();
    assert_eq!(
        state,
        Value::list(vec![
            Value::fixnum(0),
            Value::NIL,
            Value::fixnum(2),
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

// === parse-partial-sexp from-state element 8 normalization (bug 8) ===
//
// GNU `internalize_parse_state` sets `state->comstr_start` to -1 when element
// 8 of OLDSTATE is nil; while resuming inside a string (element 3) or comment
// (element 4) this -1 is reported as element 8 of the result.

#[test]
fn parse_partial_sexp_from_state_in_string_normalizes_unknown_start_to_minus_one() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "abc\" def");
    // (parse-partial-sexp (point-min) 4 nil nil
    //   (list 0 nil nil ?\" nil nil 0 nil nil nil nil)) => element 8 is -1
    let oldstate = Value::list(vec![
        Value::fixnum(0),
        Value::NIL,
        Value::NIL,
        Value::fixnum('"' as i64),
        Value::NIL,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    let state = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(4),
            Value::NIL,
            Value::NIL,
            oldstate,
        ],
    )
    .unwrap();
    assert_eq!(nth_value(&state, 8), Value::fixnum(-1));
    // Still inside the string at TO.
    assert_eq!(nth_value(&state, 3), Value::fixnum('"' as i64));
}

#[test]
fn parse_partial_sexp_from_state_in_comment_normalizes_unknown_start_to_minus_one() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "abc def");
    // Resume inside a comment (element 4 = t) with element 8 nil => -1.
    let oldstate = Value::list(vec![
        Value::fixnum(0),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::T,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]);
    let state = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(4),
            Value::NIL,
            Value::NIL,
            oldstate,
        ],
    )
    .unwrap();
    assert_eq!(nth_value(&state, 8), Value::fixnum(-1));
}

#[test]
fn parse_partial_sexp_from_state_not_in_string_or_comment_keeps_start_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    replace_current_buffer_text(&mut eval, "abc def");
    // No string/comment in the from-state: element 8 must stay nil (no
    // spurious -1), matching GNU's gated output of `comstr_start`.
    let oldstate = Value::list(vec![
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
    ]);
    let state = builtin_parse_partial_sexp(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(4),
            Value::NIL,
            Value::NIL,
            oldstate,
        ],
    )
    .unwrap();
    assert_eq!(nth_value(&state, 8), Value::NIL);
}

#[test]
fn forward_word_with_boundary_table_reaches_buffer_end() {
    crate::test_utils::init_test_tracing();
    // GNU Fforward_word: when scan_words finds no further word, point still
    // moves to the accessible limit. With a find-word-boundary-function-table
    // installed (the subword-mode configuration), the boundary path used to
    // leave point stalled before trailing non-word text, turning the
    // ubiquitous (while (< (point) (point-max)) (forward-word)) idiom into an
    // infinite loop.
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = eval
        .eval_str(
            r#"(progn
                 (insert "word ")
                 (let ((find-word-boundary-function-table
                        (let ((tab (make-char-table nil)))
                          (set-char-table-range tab t #'ignore)
                          tab)))
                   (goto-char (point-min))
                   (forward-word)
                   (forward-word)
                   (list (point) (point-max))))"#,
        )
        .expect("forward-word with boundary table");
    let rendered = crate::emacs_core::error::format_eval_result(&Ok(result));
    assert_eq!(rendered, "OK (6 6)");
}

/// The per-scan ASCII syntax memo must decode each distinct ASCII character at
/// most once, no matter how long the scan is.
///
/// Guards the shape that motivated it: font-lock drives `parse-partial-sexp`
/// over many SHORT ranges, so the memo is filled ON MISS rather than
/// precomputed. A regression to eager precompute (or to no memo at all) shows
/// up here as a fill count tied to the scan length instead of to the alphabet.
#[test]
fn ascii_syntax_memo_decodes_each_char_at_most_once_per_scan() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();

    // ~1.6k chars drawn from a small alphabet: many characters stepped over,
    // few DISTINCT ones to decode.
    let unit = "(defun f (x) (+ x 1)) ; c\n";
    let text = unit.repeat(64);
    let chars = text.chars().count();
    let distinct = {
        let mut seen = text.chars().collect::<Vec<_>>();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    };
    replace_current_buffer_text(&mut eval, &text);

    reset_syntax_table_decodes_for_test();
    let _ = builtin_parse_partial_sexp(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(chars as i64 + 1)],
    )
    .unwrap();
    let decodes = syntax_table_decodes_for_test();

    // Long scans (>= 256 chars) may eagerly build the flat 128-entry ASCII
    // table — a bounded, span-amortized cost that replaced the per-char memo
    // decode on the batch path (measured -88M Ir on a fontify pass). The
    // invariant is that decode cost is bounded by the ALPHABET (128 + the
    // distinct non-batched stragglers), never by the scan length.
    assert!(
        decodes <= 128 + distinct,
        "decoded {decodes} entries over a {chars}-char scan -- cost must be \
         bounded by the ASCII alphabet, not the scan length"
    );
    assert!(
        decodes * 4 < chars,
        "decoded {decodes} entries over a {chars}-char scan -- neither the \
         flat table nor the memo is holding"
    );

    // SHORT scans must keep the lazy on-miss memo: font-lock drives
    // parse-partial-sexp over many short ranges, and an eager 128-entry
    // fill measured strictly worse there (see the memo's variant table).
    reset_syntax_table_decodes_for_test();
    let _ =
        builtin_parse_partial_sexp(&mut eval, vec![Value::fixnum(1), Value::fixnum(40)]).unwrap();
    let short_decodes = syntax_table_decodes_for_test();
    let short_distinct = {
        let mut seen = text.chars().take(39).collect::<Vec<_>>();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    };
    assert!(
        short_decodes <= short_distinct,
        "short scan decoded {short_decodes} entries for {short_distinct} \
         distinct chars -- the lazy memo must survive on short spans"
    );
}
