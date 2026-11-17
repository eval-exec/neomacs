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

/// A variable view with nothing bound, as under `emacs -Q`:
/// `char-property-alias-alist` and `default-text-properties` read as void,
/// so the `syntax-table` resolver reads each interval's own plist.
#[derive(Clone, Copy)]
struct UnboundVariables<'a>(&'a neovm_value::symbol::Obarray);

impl<'a> EditVariableRead<'a> for UnboundVariables<'a> {
    fn obarray(self) -> &'a neovm_value::symbol::Obarray {
        self.0
    }

    fn value(self, _id: neovm_value::intern::SymId) -> Option<Value> {
        None
    }

    fn value_by_name(self, _name: &str) -> Option<Value> {
        None
    }

    fn value_in_buffer(
        self,
        _buf: Option<&Buffer>,
        _id: neovm_value::intern::SymId,
    ) -> Option<Value> {
        None
    }

    fn value_or_nil(self, _id: neovm_value::intern::SymId) -> Value {
        Value::NIL
    }
}

/// `nchars` of 70-char lines with a two-byte `é` every 10 characters, the
/// text of the `string-match "\\w+"` reproduction.
fn multibyte_lines(nchars: usize) -> String {
    let line: String = (0..70)
        .map(|i| match i {
            69 => '\n',
            i if i % 10 == 5 => 'é',
            i if i % 7 == 3 => ' ',
            i => char::from(b'a' + (i % 26) as u8),
        })
        .collect();
    line.chars().cycle().take(nchars).collect()
}

fn put_string_property(
    string: &mut LispString,
    start: usize,
    end: usize,
    name: &'static str,
    value: Value,
) {
    let len = string.schars();
    string.intervals_mut().put_property_for_object_char_len(
        neovm_edit::buffer::CharRange::new(CharPos0::new(start), CharPos0::new(end)),
        CharLen::new(len),
        Value::symbol(name),
        value,
    );
}

#[test]
fn string_syntax_run_refills_do_not_rescan_the_string() {
    crate::test_utils::init_test_tracing();
    // A fontified multibyte string: a `face` interval on every other 5-char
    // span, 20k intervals over 200k characters. Reading the `syntax-table`
    // property at every byte must cost about the bytes read, not one
    // whole-string position conversion per interval crossed.
    let obarray = neovm_value::symbol::Obarray::new();
    let buffers = BufferManager::new();
    let nchars = 200_000;
    let mut string = LispString::from_utf8(&multibyte_lines(nchars));
    assert_eq!(string.schars(), nchars);
    assert!(string.sbytes() > nchars, "the string is multibyte");
    let bold = Value::symbol("bold");
    for i in 0..nchars / 10 {
        put_string_property(&mut string, 10 * i, 10 * i + 5, "face", bold);
    }
    let props = SyntaxProperties::for_scan(true, UnboundVariables(&obarray), &buffers);
    let scans_before = neovm_value::emacs_char::position_conversion_scan_steps_for_test();

    let forward = StringSyntaxPropByteRun::new(props, &string, string.intervals().as_table());
    for byte in 0..string.sbytes() {
        assert!(forward.syntax_table_prop_at_string_byte(byte).is_none());
    }
    // A scan that starts mid-string and runs backward, as `\b` and the
    // backward syntax skips do.
    let backward = StringSyntaxPropByteRun::new(props, &string, string.intervals().as_table());
    for byte in (0..string.sbytes() / 2).rev() {
        assert!(backward.syntax_table_prop_at_string_byte(byte).is_none());
    }

    let scans = neovm_value::emacs_char::position_conversion_scan_steps_for_test() - scans_before;
    // Each scan may locate its first position from an end of the string;
    // every later run is reached by walking from the run before it.
    assert!(
        scans <= 2,
        "{scans} whole-string position conversions for {} property intervals",
        nchars / 10
    );
    // The walks themselves visit each byte a bounded number of times.
    let read = string.sbytes() + string.sbytes() / 2;
    let walked = forward.scanned_bytes.get() + backward.scanned_bytes.get();
    assert!(walked <= 4 * read, "{walked} bytes walked to read {read}");
}

#[test]
fn string_syntax_run_refills_cost_no_more_than_the_nearer_string_end() {
    crate::test_utils::init_test_tracing();
    // Long runs: one interval over the whole string, as a `fontified` or
    // `face` run over a region gives, and a property only at the start, which
    // leaves one property-free run to the end. A `string-match` loop with a
    // small START opens a new scan near the start each time. Locating that
    // scan's run must cost about the distance to the start, as converting
    // from the nearer end of the string did, not the run's length.
    let obarray = neovm_value::symbol::Obarray::new();
    let buffers = BufferManager::new();
    let nchars = 200_000;
    let text = "é".repeat(nchars);
    let mut whole = LispString::from_utf8(&text);
    put_string_property(&mut whole, 0, nchars, "fontified", Value::T);
    let mut head = LispString::from_utf8(&text);
    put_string_property(&mut head, 0, 5, "face", Value::symbol("bold"));
    let props = SyntaxProperties::for_scan(true, UnboundVariables(&obarray), &buffers);
    for string in [&whole, &head] {
        for start in (0..2000).map(|i| 2 * i) {
            let run = StringSyntaxPropByteRun::new(props, string, string.intervals().as_table());
            let read = start..start + 20;
            for byte in read.clone().step_by(2) {
                assert!(run.syntax_table_prop_at_string_byte(byte).is_none());
            }
            let walked = run.scanned_bytes.get();
            assert!(
                walked <= 4 * read.end,
                "{walked} bytes walked to read bytes {read:?} of {}",
                string.sbytes()
            );
        }
    }
}

#[test]
fn string_syntax_runs_walked_from_a_neighbour_match_a_fresh_lookup() {
    crate::test_utils::init_test_tracing();
    // Characters of one to four bytes, `syntax-table` intervals of varying
    // length with gaps between them, and a long interval at each end, read at
    // every character start in forward, backward and jumping order. Every
    // cached run must equal the interval's bounds converted from an end of
    // the string, and every answer what a fresh lookup computes. A Lisp
    // string, which converts through its object's position cache, must agree
    // the same way.
    let obarray = neovm_value::symbol::Obarray::new();
    let buffers = BufferManager::new();
    let alphabet = ['a', 'é', '€', '😀', ' ', 'z'];
    let text: String = (0..1500)
        .map(|i| alphabet[(i * 7 + i / 5) % alphabet.len()])
        .collect();
    let string = varied_syntax_intervals(&text);
    let props = SyntaxProperties::for_scan(true, UnboundVariables(&obarray), &buffers);
    let intervals = string.intervals().as_table();
    let lisp = Value::heap_string(varied_syntax_intervals(&text));
    let lisp_string = lisp.as_lisp_string().expect("a Lisp string");
    let lisp_intervals = lisp_string.intervals().as_table();
    let table = intervals.expect("the string has intervals");
    // The matcher reads at character starts (and the end); a byte inside a
    // character is not a GNU syntax position.
    let forward: Vec<usize> = text
        .char_indices()
        .map(|(byte, _)| byte)
        .chain([text.len()])
        .collect();
    let backward: Vec<usize> = forward.iter().rev().copied().collect();
    let jumping: Vec<usize> = (0..4000)
        .map(|i| forward[(i * 7919 + i * i * 31) % forward.len()])
        .collect();
    // Let the Lisp string's conversions fill the position cache, as they do
    // under the evaluator.
    neovm_value::string_pos_cache::install_notification_hook_for_test();
    neovm_value::string_pos_cache::reset_string_pos_cache();
    for order in [forward, backward, jumping] {
        let walked = StringSyntaxPropByteRun::new(props, &string, intervals);
        let walked_lisp = StringSyntaxPropByteRun::for_source(
            props,
            StringSyntaxPositionSource::try_from(&lisp).expect("a Lisp string"),
            lisp_intervals,
        );
        for &byte in &order {
            let fresh = StringSyntaxPropByteRun::new(props, &string, intervals);
            let expected = fresh
                .syntax_table_prop_at_string_byte(byte)
                .map(Value::bits);
            let char_pos = usize::try_from(string.byte_to_gnu_char_pos(byte).get())
                .expect("a character start");
            let (_, start, end) =
                table.interval_plist_run_at_char_pos(CharPos0::new(char_pos), string.schars());
            let run = (
                string.char_to_byte_pos(start.get()),
                string.char_to_byte_pos(end.get()),
            );
            for (name, walked) in [("owned", &walked), ("lisp", &walked_lisp)] {
                assert_eq!(
                    walked
                        .syntax_table_prop_at_string_byte(byte)
                        .map(Value::bits),
                    expected,
                    "{name}: property at byte {byte}"
                );
                assert_eq!(
                    (walked.run.start.get(), walked.run.end.get()),
                    run,
                    "{name}: run at byte {byte}"
                );
                assert_eq!(walked.position_error(), None);
            }
            // A refill's conversion leaves a pair the cache may hand to the
            // next conversion of the same string; it must be exact.
            if let Some((char_pos, byte_pos)) =
                neovm_value::string_pos_cache::cached_pair(lisp, lisp_string)
            {
                assert_eq!(
                    char_pos,
                    string.byte_to_gnu_char_pos(byte_pos),
                    "cached pair after byte {byte}"
                );
            }
        }
    }
    assert!(
        neovm_value::string_pos_cache::cached_pair(lisp, lisp_string).is_some(),
        "the Lisp string's refills convert through its position cache"
    );
    neovm_value::string_pos_cache::reset_string_pos_cache();
}

/// `text` with `syntax-table` intervals of 1 to 11 characters and gaps of 0
/// to 3 between them, behind a 200-character interval at the start and in
/// front of one at the end.
fn varied_syntax_intervals(text: &str) -> LispString {
    let mut string = LispString::from_utf8(text);
    let nchars = string.schars();
    let mut start = 200;
    let mut k = 0_i64;
    put_string_property(&mut string, 0, start, "syntax-table", Value::fixnum(-1));
    while start < nchars - 200 {
        let len = 1 + (k as usize * 3) % 11;
        let end = (start + len).min(nchars - 200);
        if k % 3 != 2 {
            put_string_property(&mut string, start, end, "syntax-table", Value::fixnum(k));
        }
        start = end + (k as usize) % 4;
        k += 1;
    }
    put_string_property(&mut string, nchars - 200, nchars, "syntax-table", Value::T);
    string
}
