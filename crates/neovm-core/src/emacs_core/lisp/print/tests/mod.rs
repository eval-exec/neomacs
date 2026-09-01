use super::super::intern::{intern, intern_uninterned, intern_uninterned_lisp_string};
use super::super::marker::make_marker_value;
use super::*;
use crate::buffer::LispCharPos1;
use crate::emacs_core::builtins::{builtin_puthash, builtin_remhash};
use crate::emacs_core::value::{
    HashTableTest, LambdaData, LambdaParams, StringTextPropertyRun,
    set_string_text_properties_for_value,
};

#[test]
fn print_basic_values() {
    crate::test_utils::init_test_tracing();
    assert_eq!(print_value(&Value::NIL), "nil");
    assert_eq!(print_value(&Value::T), "t");
    assert_eq!(print_value(&Value::fixnum(42)), "42");
    assert_eq!(print_value(&Value::make_float(3.125)), "3.125");
    assert_eq!(print_value(&Value::make_float(1.0)), "1.0");
    assert_eq!(print_value(&Value::symbol("foo")), "foo");
    assert_eq!(print_value(&Value::symbol(".foo")), ".foo");
    assert_eq!(print_value(&Value::symbol("")), "##");
    assert_eq!(print_value(&Value::keyword(":bar")), ":bar");
}

#[test]
fn print_symbol_escapes_reader_sensitive_chars() {
    crate::test_utils::init_test_tracing();
    assert_eq!(print_value(&Value::symbol("a b")), "a\\ b");
    assert_eq!(print_value(&Value::symbol("a,b")), "a\\,b");
    assert_eq!(print_value(&Value::symbol("a,@b")), "a\\,@b");
    assert_eq!(print_value(&Value::symbol("a#b")), "a\\#b");
    assert_eq!(print_value(&Value::symbol("a'b")), "a\\'b");
    assert_eq!(print_value(&Value::symbol("a`b")), "a\\`b");
    assert_eq!(print_value(&Value::symbol("a\\b")), "a\\\\b");
    assert_eq!(print_value(&Value::symbol("a\"b")), "a\\\"b");
    assert_eq!(print_value(&Value::symbol("a(b")), "a\\(b");
    assert_eq!(print_value(&Value::symbol("a)b")), "a\\)b");
    assert_eq!(print_value(&Value::symbol("a[b")), "a\\[b");
    assert_eq!(print_value(&Value::symbol("a]b")), "a\\]b");
    assert_eq!(print_value(&Value::symbol("##")), "\\#\\#");
    assert_eq!(print_value(&Value::symbol("?a")), "\\?a");
    assert_eq!(print_value(&Value::symbol("a?b")), "a?b");
}

#[test]
fn print_symbol_escapes_numeric_looking_names_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(print_value(&Value::symbol("2")), "\\2");
    assert_eq!(print_value(&Value::symbol("+1")), "\\+1");
    assert_eq!(print_value(&Value::symbol("-1")), "\\-1");
    assert_eq!(print_value(&Value::symbol("1e2")), "\\1e2");
    assert_eq!(print_value(&Value::symbol("1.")), "\\1.");
    assert_eq!(print_value(&Value::symbol(".5")), "\\.5");
    assert_eq!(print_value(&Value::symbol("+.5")), "\\+.5");
    assert_eq!(print_value(&Value::symbol("1e+INF")), "\\1e+INF");
    assert_eq!(print_value(&Value::symbol("0.0e+NaN")), "\\0.0e+NaN");

    assert_eq!(print_value(&Value::symbol("1+")), "1+");
    assert_eq!(print_value(&Value::symbol("0x10")), "0x10");
    assert_eq!(print_value(&Value::symbol("+.")), "+.");
    assert_eq!(print_value(&Value::symbol("-.")), "-.");
    assert_eq!(print_value(&Value::symbol("+")), "+");
    assert_eq!(print_value(&Value::symbol("-")), "-");
}

#[test]
fn print_uninterned_symbols_follow_gnu_default_print_gensym_nil() {
    crate::test_utils::init_test_tracing();
    assert_eq!(print_value(&Value::symbol(intern_uninterned("foo"))), "foo");
    assert_eq!(
        print_value(&Value::symbol(intern_uninterned(":foo"))),
        ":foo"
    );
    assert_eq!(print_value(&Value::symbol(intern_uninterned(""))), "##");
}

#[test]
fn print_raw_unibyte_uninterned_symbol_bytes_match_gnu_encoding() {
    crate::test_utils::init_test_tracing();
    let raw_name = crate::heap_types::LispString::from_unibyte(vec![0xFF, b'a']);
    let sym = Value::symbol(intern_uninterned_lisp_string(&raw_name));
    assert_eq!(print_value_bytes(&sym), vec![0xC1, 0xBF, b'a']);
}

#[test]
fn print_uninterned_symbols_support_print_gensym_round_trip_syntax() {
    crate::test_utils::init_test_tracing();
    let options = PrintOptions::with_print_gensym(true);
    assert_eq!(
        print_value_with_options(&Value::symbol(intern_uninterned("foo")), options),
        "#:foo"
    );
    assert_eq!(
        print_value_with_options(&Value::symbol(intern_uninterned(":foo")), options),
        "#::foo"
    );
    assert_eq!(
        print_value_with_options(&Value::symbol(intern_uninterned("")), options),
        "#:"
    );
}

#[test]
fn print_gensym_raw_unibyte_symbol_bytes_match_gnu_encoding() {
    crate::test_utils::init_test_tracing();
    let options = PrintOptions::with_print_gensym(true);
    let raw_name = crate::heap_types::LispString::from_unibyte(vec![0xFF, b'a']);
    let sym = Value::symbol(intern_uninterned_lisp_string(&raw_name));
    assert_eq!(
        print_value_bytes_with_options(&sym, options),
        vec![b'#', b':', 0xC1, 0xBF, b'a']
    );
}

#[test]
fn print_float_nan_preserves_sign() {
    crate::test_utils::init_test_tracing();
    assert_eq!(print_value(&Value::make_float(f64::NAN)), "0.0e+NaN");
    let neg_nan = f64::from_bits(f64::NAN.to_bits() | (1_u64 << 63));
    assert_eq!(print_value(&Value::make_float(neg_nan)), "-0.0e+NaN");
}

#[test]
fn print_float_nan_payload_tag_round_trip_shape() {
    crate::test_utils::init_test_tracing();
    let tagged = f64::from_bits((0x7ffu64 << 52) | (1u64 << 51) | 1u64);
    assert_eq!(print_value(&Value::make_float(tagged)), "1.0e+NaN");

    let neg_tagged = f64::from_bits((1u64 << 63) | (0x7ffu64 << 52) | (1u64 << 51) | 2u64);
    assert_eq!(print_value(&Value::make_float(neg_tagged)), "-2.0e+NaN");
}

#[test]
fn float_output_format_zero_precision_matches_gnu_print_c() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        format_float_with_output_format(1.25, Some(Value::string("%.0f"))),
        "1"
    );
    assert_eq!(
        format_float_with_output_format(1.25, Some(Value::string("%.f"))),
        "1.0"
    );
    assert_eq!(
        format_float_with_output_format(1.25, Some(Value::string("%.e"))),
        "1e+00"
    );
    assert_eq!(
        format_float_with_output_format(1.25, Some(Value::string("%.0g"))),
        "1.25"
    );
}

#[test]
fn print_string() {
    crate::test_utils::init_test_tracing();
    assert_eq!(print_value(&Value::string("hello")), "\"hello\"");
}

#[test]
fn print_empty_char_table_uses_gnu_vector_shape() {
    crate::test_utils::init_test_tracing();
    let table = crate::emacs_core::chartable::make_char_table_with_extra_slots(
        Value::symbol("syntax-table"),
        Value::NIL,
        0,
    );
    let rendered = print_value(&table);
    assert!(rendered.starts_with("#^[nil nil syntax-table"));
}

#[test]
fn print_propertized_string_literal_shape() {
    crate::test_utils::init_test_tracing();
    let value = Value::string_with_text_properties(
        " ",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![
                Value::symbol("display"),
                Value::list(vec![
                    Value::symbol("space"),
                    Value::keyword(":align-to"),
                    Value::list(vec![
                        Value::symbol("+"),
                        Value::symbol("header-line-indent-width"),
                        Value::fixnum(0),
                    ]),
                ]),
            ]),
        }],
    );
    assert_eq!(
        print_value(&value),
        r##"#(" " 0 1 (display (space :align-to (+ header-line-indent-width 0))))"##
    );
    assert_eq!(
        print_value_bytes(&value),
        br#"#(" " 0 1 (display (space :align-to (+ header-line-indent-width 0))))"#
    );
}

#[test]
fn print_propertized_string_properties_keep_buffer_context() {
    crate::test_utils::init_test_tracing();
    let mut buffers = crate::buffer::BufferManager::new();
    let buffer_id = buffers
        .find_buffer_by_name("*scratch*")
        .expect("scratch buffer");
    let value = Value::string_with_text_properties(
        "x",
        vec![StringTextPropertyRun {
            start: 0,
            end: 1,
            plist: Value::list(vec![Value::symbol("owner"), Value::make_buffer(buffer_id)]),
        }],
    );

    buffers.kill_buffer(buffer_id);

    assert_eq!(
        print_value_with_buffers(&value, &buffers),
        r#"#("x" 0 1 (owner #<killed buffer>))"#
    );
}

#[test]
fn print_propertized_string_parent_cycle_matches_gnu_default_cycle_path() {
    crate::test_utils::init_test_tracing();
    let text = Value::string("body");
    let parent = Value::list(vec![Value::symbol("section"), Value::NIL, text]);
    set_string_text_properties_for_value(
        text,
        vec![StringTextPropertyRun {
            start: 0,
            end: 4,
            plist: Value::list(vec![Value::keyword(":parent"), parent]),
        }],
    );

    assert_eq!(
        print_value(&Value::list(vec![text])),
        r#"(#("body" 0 4 (:parent (section nil #1))))"#
    );
    assert_eq!(
        print_value_bytes(&Value::list(vec![text])),
        br#"(#("body" 0 4 (:parent (section nil #1))))"#
    );
}

#[test]
fn print_circle_preprocess_traverses_string_text_property_plists_like_gnu() {
    crate::test_utils::init_test_tracing();
    let text = Value::string("body");
    let parent = Value::list(vec![Value::symbol("section"), Value::NIL, text]);
    set_string_text_properties_for_value(
        text,
        vec![StringTextPropertyRun {
            start: 0,
            end: 4,
            plist: Value::list(vec![Value::keyword(":parent"), parent]),
        }],
    );

    let options = PrintOptions::new(false, true, None, None);
    assert_eq!(
        print_value_with_options(&Value::list(vec![text]), options),
        r#"(#1=#("body" 0 4 (:parent (section nil #1#))))"#
    );
}

#[test]
fn print_string_keeps_non_bmp_visible() {
    crate::test_utils::init_test_tracing();
    assert_eq!(print_value(&Value::string("\u{10ffff}")), "\"\u{10ffff}\"");
}

#[test]
fn print_string_bytes_preserve_non_utf8_payloads() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        print_value_bytes(&Value::heap_string(
            crate::heap_types::LispString::from_emacs_bytes(vec![0xC1, 0xBF],)
        )),
        b"\"\\377\""
    );
}

#[test]
fn print_literal_private_use_unicode_does_not_masquerade_as_raw_byte() {
    crate::test_utils::init_test_tracing();
    let private_use = char::from_u32(0xE0FF).expect("private use scalar");
    assert_eq!(
        print_value_bytes(&Value::string(private_use.to_string())),
        format!("\"{}\"", private_use).into_bytes()
    );
}

/// Issue #131: Private-Use-Area characters (nerd-font icons live across
/// U+E000..U+F8FF) must extract as their real code points. neomacs reused
/// U+E300..U+E3FF as a unibyte "sentinel", so char access masked e.g.
/// U+E322 → 0x22 (`"`), corrupting glyphs and breaking byte-compiled `.elc`
/// syntax. This guards that whole range (the nerd-font weather/material icons).
///
/// (U+E080..U+E0FF is still used as the in-`String` storage for eight-bit raw
/// bytes, so genuine glyphs there remain ambiguous until the storage rework —
/// issue #131 Step B — and are intentionally not covered here.)
#[test]
fn private_use_chars_survive_char_extraction_issue_131() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::builtins::{lisp_string_char_at, lisp_string_char_codes};
    for cp in [0xE300u32, 0xE322, 0xE325, 0xE379, 0xE39A, 0xE3FF] {
        let ch = char::from_u32(cp).expect("private use scalar");
        let value = Value::string(ch.to_string());
        let ls = value.as_lisp_string().expect("string value");
        assert_eq!(ls.schars(), 1, "U+{cp:04X} must be one character");
        assert_eq!(
            lisp_string_char_at(ls, 0),
            Some(cp),
            "aref of U+{cp:04X} must keep the real code point (issue #131)"
        );
        assert_eq!(lisp_string_char_codes(ls), vec![cp]);
        // The printer must emit the real glyph, not a masked low byte.
        assert_eq!(print_value(&value), format!("\"{ch}\""));
    }
}

#[test]
fn print_list() {
    crate::test_utils::init_test_tracing();
    let lst = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    assert_eq!(print_value(&lst), "(1 2 3)");
}

#[test]
fn print_stateful_record_preprocess_uses_record_storage() {
    crate::test_utils::init_test_tracing();
    let record = Value::make_record(vec![Value::symbol("foo"), Value::fixnum(1)]);
    let options = PrintOptions::new(false, false, Some(10), None);

    assert_eq!(print_value_stateful(&record, options), "#s(foo 1)");
}

#[test]
fn print_hash_s_literal_shorthand() {
    crate::test_utils::init_test_tracing();
    let literal = Value::list(vec![
        Value::symbol("make-hash-table-from-literal"),
        Value::list(vec![
            Value::symbol("quote"),
            Value::list(vec![Value::symbol("x")]),
        ]),
    ]);
    assert_eq!(print_value(&literal), "#s(x)");
    assert_eq!(print_value_bytes(&literal), b"#s(x)");
}

#[test]
fn print_hash_table_object_uses_readable_hash_s_shape() {
    crate::test_utils::init_test_tracing();
    let table = Value::hash_table(HashTableTest::Equal);
    // GNU Emacs prints "test equal" for non-default test (default is eql).
    assert_eq!(print_value(&table), "#s(hash-table test equal)");
    assert_eq!(print_value_bytes(&table), b"#s(hash-table test equal)");
}

#[test]
fn print_hash_table_uses_live_slots_after_remhash_reinsert() {
    crate::test_utils::init_test_tracing();
    let table = Value::hash_table(HashTableTest::Eq);

    builtin_puthash(vec![Value::fixnum(1), Value::fixnum(10), table]).unwrap();
    builtin_puthash(vec![Value::fixnum(2), Value::fixnum(20), table]).unwrap();
    builtin_remhash(vec![Value::fixnum(1), table]).unwrap();
    builtin_puthash(vec![Value::fixnum(1), Value::fixnum(10), table]).unwrap();

    let rendered = print_value(&table);
    assert_eq!(rendered.matches("1 10").count(), 1);
    assert_eq!(rendered.matches("2 20").count(), 1);
    assert_eq!(print_value_bytes(&table), rendered.as_bytes());
}

#[test]
fn print_quote_shorthand_lists() {
    crate::test_utils::init_test_tracing();
    let quoted = Value::list(vec![Value::symbol("quote"), Value::symbol("foo")]);
    let function = Value::list(vec![Value::symbol("function"), Value::symbol("car")]);
    let quasiquoted = Value::list(vec![
        Value::symbol("`"),
        Value::list(vec![Value::symbol("a"), Value::symbol("b")]),
    ]);
    let unquoted = Value::list(vec![Value::symbol(","), Value::symbol("x")]);
    let unquote_splice = Value::list(vec![Value::symbol(",@"), Value::symbol("xs")]);

    assert_eq!(print_value(&quoted), "'foo");
    assert_eq!(print_value(&function), "#'car");
    assert_eq!(print_value(&quasiquoted), "`(a b)");
    assert_eq!(print_value(&unquoted), "(\\, x)");
    assert_eq!(print_value(&unquote_splice), "(\\,@ xs)");
}

#[test]
fn print_backquote_preserves_nested_unquote_shorthand_only_in_context() {
    crate::test_utils::init_test_tracing();
    let nested = Value::list(vec![
        Value::symbol("`"),
        Value::list(vec![
            Value::symbol("a"),
            Value::list(vec![Value::symbol(","), Value::symbol("x")]),
        ]),
    ]);

    assert_eq!(print_value(&nested), "`(a ,x)");
}

#[test]
fn print_dotted_pair() {
    crate::test_utils::init_test_tracing();
    let pair = Value::cons(Value::fixnum(1), Value::fixnum(2));
    assert_eq!(print_value(&pair), "(1 . 2)");
}

#[test]
fn print_vector() {
    crate::test_utils::init_test_tracing();
    let v = Value::vector(vec![Value::fixnum(1), Value::fixnum(2)]);
    assert_eq!(print_value(&v), "[1 2]");
}

#[test]
fn print_default_handles_circular_vector_like_gnu() {
    crate::test_utils::init_test_tracing();
    let vector = Value::vector(vec![Value::NIL]);
    assert!(vector.set_vector_slot(0, vector));

    assert_eq!(print_value(&vector), "[#0]");
    assert_eq!(print_value_bytes(&vector), b"[#0]");
}

#[test]
fn print_default_handles_circular_cons_like_gnu() {
    crate::test_utils::init_test_tracing();
    let cell = Value::cons(Value::NIL, Value::NIL);
    cell.set_cdr(cell);

    assert_eq!(print_value(&cell), "(nil . #0)");
    assert_eq!(print_value_bytes(&cell), b"(nil . #0)");
}

#[test]
fn print_default_bounded_circular_list_uses_gnu_tail_index() {
    crate::test_utils::init_test_tracing();
    let first = Value::cons(Value::fixnum(1), Value::NIL);
    let second = Value::cons(Value::fixnum(2), Value::NIL);
    first.set_cdr(second);
    second.set_cdr(first);

    let options = PrintOptions::new(false, false, None, Some(6));

    assert_eq!(print_value_stateful(&first, options), "(1 2 1 2 . #2)");
}

#[test]
fn print_default_tail_cycle_uses_gnu_tail_index() {
    crate::test_utils::init_test_tracing();
    let first = Value::cons(Value::symbol("a"), Value::NIL);
    let second = Value::cons(Value::symbol("b"), Value::NIL);
    let third = Value::cons(Value::symbol("c"), Value::NIL);
    first.set_cdr(second);
    second.set_cdr(third);
    third.set_cdr(second);

    let options = PrintOptions::new(false, false, None, Some(7));

    assert_eq!(print_value_stateful(&first, options), "(a b c b . #2)");
}

#[test]
fn print_level_applies_to_conses_not_vectorlike_objects_like_gnu() {
    crate::test_utils::init_test_tracing();
    let options = PrintOptions::new(false, false, Some(1), None);
    let nested_vector = Value::vector(vec![
        Value::vector(vec![Value::symbol("a"), Value::symbol("b")]),
        Value::vector(vec![Value::symbol("c"), Value::symbol("d")]),
    ]);
    let record = Value::make_record(vec![
        Value::symbol("foo"),
        Value::list(vec![Value::symbol("a"), Value::symbol("b")]),
        Value::vector(vec![Value::symbol("c"), Value::symbol("d")]),
    ]);

    assert_eq!(
        print_value_stateful(&nested_vector, options),
        "[[a b] [c d]]"
    );
    assert_eq!(print_value_stateful(&record, options), "#s(foo ... [c d])");
}

#[test]
fn print_circle_handles_self_referential_records() {
    crate::test_utils::init_test_tracing();
    let record = Value::make_record(vec![Value::symbol("foo"), Value::NIL]);
    record.with_record_data_mut(|slots| slots[1] = record);

    let options = PrintOptions::new(false, true, None, None);
    assert_eq!(
        print_value_stateful_with_buffers(&record, None, options),
        "#1=#s(foo #1#)"
    );
}

#[test]
fn print_number_table_cleanup_preserves_only_labeled_entries() {
    crate::test_utils::init_test_tracing();
    let table = Value::hash_table(HashTableTest::Eq);
    let retained = Value::cons(Value::symbol("shared"), Value::NIL);
    let retained_key = print_number_table_key(table, &retained).unwrap();
    let alias = Value::string("#$");
    let alias_key_value = Value::string("alias-key");
    let alias_key = print_number_table_key(table, &alias_key_value).unwrap();

    for i in 0..64 {
        let seen_once = Value::cons(Value::fixnum(i), Value::NIL);
        let key = print_number_table_key(table, &seen_once).unwrap();
        put_print_number_table_entry(table, key, seen_once, Value::T);
    }
    put_print_number_table_entry(table, retained_key.clone(), retained, Value::fixnum(7));
    put_print_number_table_entry(table, alias_key.clone(), alias_key_value, alias);

    remove_print_number_table_t_entries(table);

    let hash_table = table.as_hash_table().unwrap();
    assert_eq!(hash_table.data.len(), 2);
    assert_eq!(hash_table.key_snapshots().count(), 2);
    assert_eq!(
        hash_table.live_hash_keys_in_slot_order(),
        vec![&retained_key, &alias_key]
    );
    assert_eq!(hash_table.data.get(&retained_key), Some(&Value::fixnum(7)));
    assert_eq!(hash_table.data.get(&alias_key), Some(&alias));
    assert!(hash_table.data.values().all(|value| *value != Value::T));
}

#[test]
fn print_default_handles_self_referential_records_like_gnu() {
    crate::test_utils::init_test_tracing();
    let record = Value::make_record(vec![Value::symbol("foo"), Value::NIL]);
    record.with_record_data_mut(|slots| slots[1] = record);

    assert_eq!(print_value(&record), "#s(foo #0)");
    assert_eq!(print_value_bytes(&record), b"#s(foo #0)");
}

#[test]
fn print_default_handles_self_referential_bytecode_constants() {
    crate::test_utils::init_test_tracing();
    let mut function =
        crate::emacs_core::bytecode::ByteCodeFunction::new(LambdaParams::simple(vec![]));
    function.constants.ensure_owned().push(Value::NIL);
    let bytecode = Value::make_bytecode(function);
    bytecode.with_bytecode_data_mut_for_test(|data| data.constants[0] = bytecode);

    assert_eq!(print_value(&bytecode), "#[nil nil [#0] 0]");
    assert_eq!(print_value_bytes(&bytecode), b"#[nil nil [#0] 0]");

    let options = PrintOptions::new(false, true, None, None);
    assert_eq!(
        print_value_with_options(&bytecode, options),
        "#1=#[nil nil [#1#] 0]"
    );
}

#[test]
fn print_default_uses_gnu_depth_for_nested_bytecode_backrefs() {
    crate::test_utils::init_test_tracing();
    let record = Value::make_record(vec![Value::symbol("foo"), Value::NIL]);
    let mut function =
        crate::emacs_core::bytecode::ByteCodeFunction::new(LambdaParams::simple(vec![]));
    function.constants.ensure_owned().push(record);
    let bytecode = Value::make_bytecode(function);
    record.with_record_data_mut(|slots| slots[1] = bytecode);

    let wrapped = Value::list(vec![Value::T, record, Value::NIL]);
    assert_eq!(print_value(&wrapped), "(t #s(foo #[nil nil [#1] 0]) nil)");
}

#[test]
fn print_lambda() {
    crate::test_utils::init_test_tracing();
    let lam = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![intern("x"), intern("y")]),
        body: vec![Value::list(vec![
            Value::symbol("+"),
            Value::symbol("x"),
            Value::symbol("y"),
        ])],
        env: None,
        docstring: None,
        doc_form: None,
        interactive: None,
    });
    assert_eq!(print_value(&lam), "#[(x y) ((+ x y)) nil]");
}

#[test]
fn print_lexical_closure_uses_gnu_vector_syntax() {
    crate::test_utils::init_test_tracing();
    let closure = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![intern("a"), intern("b")]),
        body: vec![Value::list(vec![
            Value::symbol("+"),
            Value::symbol("a"),
            Value::symbol("b"),
            Value::symbol("x"),
        ])],
        env: Some(Value::list(vec![Value::cons(
            Value::symbol("x"),
            Value::fixnum(42),
        )])),
        docstring: None,
        doc_form: None,
        interactive: None,
    });

    assert_eq!(print_value(&closure), "#[(a b) ((+ a b x)) ((x . 42))]");
    assert_eq!(
        String::from_utf8(print_value_bytes(&closure)).expect("utf8"),
        "#[(a b) ((+ a b x)) ((x . 42))]"
    );
}

#[test]
fn print_recursive_closure_uses_backreference() {
    crate::test_utils::init_test_tracing();
    let binding = Value::cons(Value::symbol("f"), Value::NIL);
    let env = Value::list(vec![binding]);
    let closure = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![]),
        body: vec![Value::symbol("f")],
        env: Some(env),
        docstring: None,
        doc_form: None,
        interactive: None,
    });
    binding.set_cdr(closure);

    assert_eq!(print_value(&closure), "#[nil (f) ((f . #0))]");
    assert_eq!(
        String::from_utf8(print_value_bytes(&closure)).expect("utf8"),
        "#[nil (f) ((f . #0))]"
    );
}

#[test]
fn print_terminal_handle_special_form() {
    crate::test_utils::init_test_tracing();
    let list = super::super::terminal::pure::builtin_terminal_list(vec![]).unwrap();
    let items = list_to_vec(&list).expect("terminal-list should return a list");
    let handle = items
        .first()
        .expect("terminal-list should contain one handle");

    let printed = print_value(handle);
    assert!(printed.starts_with("#<terminal "));
    assert!(printed.contains("on initial_terminal>"));
}

#[test]
fn print_frame_handles_use_oracle_style_f_prefix() {
    crate::test_utils::init_test_tracing();
    let f1 = Value::make_frame(crate::window::FRAME_ID_BASE);
    let f2 = Value::make_frame(crate::window::FRAME_ID_BASE + 1);
    let legacy = Value::make_frame(7);

    assert_eq!(print_value(&f1), "#<frame F1 0x100000000>");
    assert_eq!(print_value_bytes(&f1), b"#<frame F1 0x100000000>");
    assert_eq!(print_value(&f2), "#<frame F2 0x100000001>");
    assert_eq!(print_value_bytes(&f2), b"#<frame F2 0x100000001>");
    assert_eq!(print_value(&legacy), "#<frame 7>");
}

#[test]
fn print_markers_use_gnu_style_handles() {
    crate::test_utils::init_test_tracing();
    let marker = make_marker_value(None, None, false);
    assert_eq!(print_value(&marker), "#<marker in no buffer>");

    let mut buffers = crate::buffer::BufferManager::new();
    let buffer_id = buffers
        .find_buffer_by_name("*scratch*")
        .expect("scratch buffer");
    let marker = make_marker_value(Some(buffer_id), Some(LispCharPos1::new(3)), false);
    assert_eq!(
        print_value_with_buffers(&marker, &buffers),
        "#<marker at 3 in *scratch*>"
    );

    buffers.kill_buffer(buffer_id);
    assert_eq!(
        print_value_with_buffers(&marker, &buffers),
        "#<marker in no buffer>"
    );
}

// ---------------------------------------------------------------------------
// Eval-driven printer regression tests (printer dynamic variables).
// ---------------------------------------------------------------------------

fn print_eval_one(src: &str) -> String {
    let mut ev = crate::emacs_core::Context::new();
    let result = ev.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

#[test]
fn bool_vector_printing_honors_gnu_byte_escape_and_length_options() {
    crate::test_utils::init_test_tracing();

    assert_eq!(
        print_eval_one(
            "(let ((print-escape-newlines t)) \
               (prin1-to-string \
                 (bool-vector nil t nil t nil nil nil nil nil nil t t)))"
        ),
        r##"OK "#&12\"\\n\\f\"""##,
    );
    assert_eq!(
        print_eval_one(
            "(let ((print-escape-control-characters t)) \
               (prin1-to-string \
                 (bool-vector t nil nil nil nil nil nil nil \
                              t t t nil t t nil nil)))"
        ),
        r##"OK "#&16\"\\0017\"""##,
    );
    assert_eq!(
        print_eval_one(
            "(let ((print-length 1)) \
               (prin1-to-string \
                 (bool-vector nil t nil t nil nil nil nil nil nil t t)))"
        ),
        r##"OK "#&12\"
 ...\"""##,
    );
}

#[test]
fn print_integers_as_characters_uses_char_syntax_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU: (?A ?\t). Letters print via graphic_base_p; tab via named_escape.
    assert_eq!(
        print_eval_one("(let ((print-integers-as-characters t)) (prin1-to-string (list 65 9)))"),
        "OK \"(?A ?\\\\t)\"",
    );
    // A broader spread, matching GNU exactly:
    //  - named escapes: 8 -> ?\b, 10 -> ?\n, 32 -> ?\s, 13 -> ?\r
    //  - graphic bases: 256 -> ?Ā, 955 -> ?λ, 59 -> ?\; (escaped by prin1)
    //  - left as integers: 0, 7, 11, 27, 127 (control), 8203 (Cf format)
    assert_eq!(
        print_eval_one(
            "(let ((print-integers-as-characters t)) \
             (prin1-to-string (list 65 9 10 32 0 1 127 ?\\( 7 11 27 ?\\; 256 955 8203)))"
        ),
        "OK \"(?A ?\\\\t ?\\\\n ?\\\\s 0 1 127 ?\\\\( 7 11 27 ?\\\\; ?Ā ?λ 8203)\"",
    );
    // princ-style output (no escapeflag): the self-delimiting `;` is NOT
    // backslash-escaped, but named escapes and `?` still apply. Exercise the
    // printer directly with `print_noescape` (the C `escapeflag = false` path).
    let mut princ_opts = PrintOptions::default();
    princ_opts.print_integers_as_characters = true;
    princ_opts.print_noescape = true;
    let list = Value::list(vec![
        Value::fixnum(65),
        Value::fixnum(9),
        Value::fixnum(';' as i64),
    ]);
    assert_eq!(print_value_with_options(&list, princ_opts), "(?A ?\\t ?;)",);
    // When the variable is nil, integers print as integers.
    assert_eq!(
        print_eval_one("(prin1-to-string (list 65 9))"),
        "OK \"(65 9)\"",
    );
}

#[test]
fn print_preprocess_fills_number_table_for_circular_structures_like_gnu() {
    crate::test_utils::init_test_tracing();
    // Root cause of the cl-print circular-list hang: `print--preprocess` was a
    // no-op stub, so cl-print never built `print-number-table` and recursed
    // forever.  GNU's `print--preprocess` fills the table when `print-circle'
    // is non-nil; a shared/circular object gets a negative-fixnum label.
    //
    // Circular list: l = (1 2 . l).  GNU: (gethash l print-number-table) = -1.
    assert_eq!(
        print_eval_one(
            "(let ((print-circle t) \
                   (print-number-table (make-hash-table :test 'eq)) \
                   (l (list 1 2))) \
               (setcdr (cdr l) l) \
               (print--preprocess l) \
               (and (< (gethash l print-number-table 0) 0) t))"
        ),
        "OK t",
    );
    // Circular vector: v = [v nil].  GNU labels the shared vector negatively.
    assert_eq!(
        print_eval_one(
            "(let ((print-circle t) \
                   (print-number-table (make-hash-table :test 'eq)) \
                   (v (make-vector 2 nil))) \
               (aset v 0 v) \
               (print--preprocess v) \
               (and (< (gethash v print-number-table 0) 0) t))"
        ),
        "OK t",
    );
    // With `print-circle' nil, GNU does nothing (the table stays empty).
    assert_eq!(
        print_eval_one(
            "(let ((print-circle nil) \
                   (print-number-table (make-hash-table :test 'eq)) \
                   (l (list 1 2))) \
               (setcdr (cdr l) l) \
               (print--preprocess l) \
               (gethash l print-number-table 'absent))"
        ),
        "OK absent",
    );
    // Acyclic, non-shared structure: no shared label is assigned (the head
    // gets the transient `t` status, which is not a number), but GNU records
    // every traversed candidate, so the three cons cells of (1 2 3) leave a
    // table count of 3.  cl-print only treats *numberp* entries as labels, so
    // an acyclic list prints without any `#N=` prefix.
    assert_eq!(
        print_eval_one(
            "(let* ((print-circle t) \
                    (print-number-table (make-hash-table :test 'eq)) \
                    (l (list 1 2 3))) \
               (print--preprocess l) \
               (list (hash-table-count print-number-table) \
                     (numberp (gethash l print-number-table))))"
        ),
        "OK (3 nil)",
    );
}

#[test]
fn print_circle_candidate_set_matches_gnu_for_bool_vectors_and_char_tables() {
    crate::test_utils::init_test_tracing();
    // GNU's `print_circle_candidate_p` matches CLOSUREP || CHAR_TABLE_P ||
    // SUB_CHAR_TABLE_P || HASH_TABLE_P || FONTP || RECORDP for non-vector
    // vectorlikes; `VECTORP` excludes bool-vectors (a distinct pseudovector).
    //
    // Bug (a), cosmetic over-labeling: a bool-vector printed twice under
    // `print-circle' must NOT be labeled, because GNU does not treat
    // bool-vectors as circle candidates.  GNU prints each bool-vector in
    // full (`#&3"\7"`, where \7 is the 0x07 bit-pack byte for 3 set bits)
    // with no `#N=' / `#N#' label: bytes `# & 3 " \x07 "` per element.
    assert_eq!(
        print_eval_one(
            "(let ((v (make-bool-vector 3 t))) \
               (let ((print-circle t)) (prin1-to-string (list v v))))"
        ),
        "OK \"(#&3\\\"\u{7}\\\" #&3\\\"\u{7}\\\")\"",
    );
    // Bug (b), functional round-trip break: a genuinely shared char-table
    // printed twice under `print-circle' MUST get a `#N=' / `#N#' label so
    // that reading the output back preserves shared identity.  GNU returns t.
    assert_eq!(
        print_eval_one(
            "(let* ((v (make-char-table 'test)) \
                    (r (read (let ((print-circle t)) \
                               (prin1-to-string (list v v)))))) \
               (eq (nth 0 r) (nth 1 r)))"
        ),
        "OK t",
    );
    // A non-shared char-table (printed once) must NOT be labeled: GNU only
    // labels objects that appear more than once.
    assert_eq!(
        print_eval_one(
            "(let ((v (make-char-table 'test))) \
               (let ((print-circle t)) \
                 (string-match \"#[0-9]+=\" (prin1-to-string v) nil t)))"
        ),
        "OK nil",
    );
}

#[test]
fn hash_table_printer_omits_default_eql_test_like_gnu() {
    crate::test_utils::init_test_tracing();
    // Default test (no :test arg) -> omitted.
    assert_eq!(
        print_eval_one("(prin1-to-string (make-hash-table))"),
        "OK \"#s(hash-table)\"",
    );
    // Explicit :test 'eql is still the default -> omitted (GNU compares the
    // test *name* symbol against `eql`).
    assert_eq!(
        print_eval_one("(prin1-to-string (make-hash-table :test 'eql))"),
        "OK \"#s(hash-table)\"",
    );
    // Non-default tests are still printed.
    assert_eq!(
        print_eval_one("(prin1-to-string (make-hash-table :test 'eq))"),
        "OK \"#s(hash-table test eq)\"",
    );
    assert_eq!(
        print_eval_one("(prin1-to-string (make-hash-table :test 'equal))"),
        "OK \"#s(hash-table test equal)\"",
    );
    // Data is still printed; the default test stays omitted.
    assert_eq!(
        print_eval_one(
            "(let ((h (make-hash-table :test 'eql))) (puthash 1 2 h) (prin1-to-string h))"
        ),
        "OK \"#s(hash-table data (1 2))\"",
    );
}
