use super::*;

use neovm_edit::buffer::buffer::{Buffer, BufferId};

use neovm_edit::buffer::{CharPos0, LispCharPos1};

use neovm_value::error::{FlowKind, FlowResultExt as _};

use neovm_value::value::eq_value;

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
fn forward_word_basic() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let table = SyntaxTable::new_standard();
    let pos = forward_word(&buf, &table, 1);
    // "hello" ends at byte 5.
    assert_eq!(pos, neovm_edit::buffer::EmacsBytePos::new(5));
}

#[test]
fn forward_word_two() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let table = SyntaxTable::new_standard();
    let pos = forward_word(&buf, &table, 2);
    // Past "hello world" = byte 11.
    assert_eq!(pos, neovm_edit::buffer::EmacsBytePos::new(11));
}

#[test]
fn forward_word_from_middle() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(3)); // inside "hello"
    let table = SyntaxTable::new_standard();
    let pos = forward_word(&buf, &table, 1);
    assert_eq!(pos, neovm_edit::buffer::EmacsBytePos::new(5)); // end of "hello"
}

#[test]
fn backward_word_basic() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(11)); // end of text
    let table = SyntaxTable::new_standard();
    let pos = backward_word(&buf, &table, 1);
    assert_eq!(pos, neovm_edit::buffer::EmacsBytePos::new(6)); // start of "world"
}

#[test]
fn backward_word_two() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(11));
    let table = SyntaxTable::new_standard();
    let pos = backward_word(&buf, &table, 2);
    assert_eq!(pos, neovm_edit::buffer::EmacsBytePos::new(0)); // start of "hello"
}

#[test]
fn forward_word_negative_goes_backward() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(11));
    let table = SyntaxTable::new_standard();
    let pos = forward_word(&buf, &table, -1);
    assert_eq!(pos, neovm_edit::buffer::EmacsBytePos::new(6));
}

#[test]
fn skip_syntax_forward_word() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let table = SyntaxTable::new_standard();
    let pos = skip_syntax_forward(&buf, &table, "w", None);
    assert_eq!(pos, 5); // end of "hello"
}

#[test]
fn skip_syntax_forward_whitespace_and_word() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("  hello");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let table = SyntaxTable::new_standard();
    let pos = skip_syntax_forward(&buf, &table, " w", None);
    assert_eq!(pos, 7); // end of "  hello"
}

#[test]
fn skip_syntax_backward_word() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(11));
    let table = SyntaxTable::new_standard();
    let pos = skip_syntax_backward(&buf, &table, "w", None);
    assert_eq!(pos, 6); // start of "world"
}

#[test]
fn skip_syntax_forward_with_limit() {
    crate::test_utils::init_test_tracing();
    let mut buf = buf_with_text("helloworld");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let table = SyntaxTable::new_standard();
    let pos = skip_syntax_forward(&buf, &table, "w", Some(3));
    assert_eq!(pos, 3);
}

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
    match builtin_make_syntax_table(vec![Value::fixnum(1)]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
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
    match builtin_copy_syntax_table(vec![Value::fixnum(1)]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("syntax-table-p")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }

    match builtin_copy_syntax_table(vec![Value::NIL, Value::NIL]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
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

    match builtin_syntax_class_to_char(vec![Value::fixnum(-1)]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(15), Value::fixnum(-1)]);
        }
        other => panic!("expected args-out-of-range signal, got {other:?}"),
    }

    match builtin_syntax_class_to_char(vec![Value::fixnum(0x1_0002)]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(15), Value::fixnum(0x1_0002)]);
        }
        other => panic!("expected args-out-of-range signal, got {other:?}"),
    }

    match builtin_syntax_class_to_char(vec![Value::string("x")]).kinded() {
        Err(neovm_value::error::FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("fixnump")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
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

/// Helper: create a buffer with given text, point at start, full accessible range.
fn buf_with_text(text: &str) -> Buffer {
    let mut buf = Buffer::new(
        BufferId(99),
        Value::string("test-syntax"),
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.insert(text);
    buf.widen();
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    buf
}

/// Standard syntax table with `?` treated as a symbol constituent, matching
/// emacs-lisp-mode where the GNU oracle examples were captured.
fn elisp_like_table() -> SyntaxTable {
    let mut table = SyntaxTable::new_standard();
    table.modify_syntax_entry('?', SyntaxEntry::simple(SyntaxClass::Symbol));
    table
}
