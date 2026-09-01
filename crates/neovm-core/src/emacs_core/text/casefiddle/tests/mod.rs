use super::*;

// =======================================================================
// capitalize
// =======================================================================

#[test]
fn capitalize_string_basic() {
    crate::test_utils::init_test_tracing();
    let result = builtin_capitalize(vec![Value::string("hello world")]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("Hello World"));
}

#[test]
fn capitalize_honors_case_symbols_as_words() {
    // GNU `casefiddle.c` `case_ch_is_word`: with `case-symbols-as-words` set,
    // symbol-constituent characters (`_`, `-`) count as word constituents, so
    // `foo_bar` is a single word.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();

    // Default (nil): `_` is a word boundary.
    let r = ev.eval_str(r#"(capitalize "foo_bar baz")"#).unwrap();
    assert_eq!(r.as_utf8_str(), Some("Foo_Bar Baz"));

    // With the flag: `_` (Ssymbol) is part of the word.
    let r = ev
        .eval_str(r#"(let ((case-symbols-as-words t)) (capitalize "foo_bar baz"))"#)
        .unwrap();
    assert_eq!(r.as_utf8_str(), Some("Foo_bar Baz"));

    // upcase-initials honors it too.
    let r = ev
        .eval_str(r#"(let ((case-symbols-as-words t)) (upcase-initials "foo_bar baz"))"#)
        .unwrap();
    assert_eq!(r.as_utf8_str(), Some("Foo_bar Baz"));

    // capitalize-region in a buffer.
    let r = ev
        .eval_str(
            r#"(let ((case-symbols-as-words t)) (with-temp-buffer (insert "foo_bar baz") (capitalize-region (point-min) (point-max)) (buffer-string)))"#,
        )
        .unwrap();
    assert_eq!(r.as_utf8_str(), Some("Foo_bar Baz"));
}

#[test]
fn capitalize_string_mixed() {
    crate::test_utils::init_test_tracing();
    let result = builtin_capitalize(vec![Value::string("hELLO wORLD")]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("Hello World"));
}

#[test]
fn capitalize_char() {
    crate::test_utils::init_test_tracing();
    let result = builtin_capitalize(vec![Value::char('a')]).unwrap();
    assert_eq!(result.as_int(), Some('A' as i64));
}

#[test]
fn capitalize_empty_string() {
    crate::test_utils::init_test_tracing();
    let result = builtin_capitalize(vec![Value::string("")]).unwrap();
    assert_eq!(result.as_utf8_str(), Some(""));
}

#[test]
fn capitalize_raw_unibyte_string_preserves_bytes() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'a', 0xFF, b'b',
    ]));
    let result = builtin_capitalize(vec![raw]).unwrap();
    let result = result.as_lisp_string().expect("string");
    assert!(!result.is_multibyte());
    assert_eq!(result.as_bytes(), &[b'A', 0xFF, b'b']);
}

// =======================================================================
// upcase-initials
// =======================================================================

#[test]
fn upcase_initials_basic() {
    crate::test_utils::init_test_tracing();
    let result = builtin_upcase_initials(vec![Value::string("hello world")]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("Hello World"));
}

#[test]
fn upcase_initials_preserves_rest() {
    crate::test_utils::init_test_tracing();
    let result = builtin_upcase_initials(vec![Value::string("hELLO wORLD")]).unwrap();
    // Only first letter of each word is uppercased; rest is left alone.
    assert_eq!(result.as_utf8_str(), Some("HELLO WORLD"));
}

#[test]
fn upcase_initials_char() {
    crate::test_utils::init_test_tracing();
    let result = builtin_upcase_initials(vec![Value::char('a')]).unwrap();
    assert_eq!(result.as_int(), Some('A' as i64));
}

#[test]
fn upcase_initials_raw_unibyte_string_preserves_bytes() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        b'a', 0xFF, b'b',
    ]));
    let result = builtin_upcase_initials(vec![raw]).unwrap();
    let result = result.as_lisp_string().expect("string");
    assert!(!result.is_multibyte());
    assert_eq!(result.as_bytes(), &[b'A', 0xFF, b'b']);
}

// =======================================================================
// char-resolve-modifiers
// =======================================================================

#[test]
fn char_resolve_modifiers_resolves_shift_lowercase() {
    crate::test_utils::init_test_tracing();
    let result =
        builtin_char_resolve_modifiers(vec![Value::fixnum(0x2000000 | ('a' as i64))]).unwrap();
    assert_eq!(result.as_int(), Some('A' as i64));
}

#[test]
fn char_resolve_modifiers_clears_shift_on_uppercase() {
    crate::test_utils::init_test_tracing();
    let result =
        builtin_char_resolve_modifiers(vec![Value::fixnum(0x2000000 | ('A' as i64))]).unwrap();
    assert_eq!(result.as_int(), Some('A' as i64));
}

#[test]
fn char_resolve_modifiers_wrong_type_predicate() {
    crate::test_utils::init_test_tracing();
    let result = builtin_char_resolve_modifiers(vec![Value::string("a")]).unwrap_err();
    match result {
        super::super::error::Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("fixnump"), Value::string("a")]);
        }
        other => panic!("expected signal flow, got {other:?}"),
    }
}

// =======================================================================
// Edge cases
// =======================================================================

#[test]
fn capitalize_with_punctuation() {
    crate::test_utils::init_test_tracing();
    let result = builtin_capitalize(vec![Value::string("it's a test")]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("It'S A Test"));
}

#[test]
fn capitalize_unicode_edge_semantics() {
    crate::test_utils::init_test_tracing();
    let int_sharp_s = builtin_capitalize(vec![Value::fixnum(223)]).unwrap();
    assert_eq!(int_sharp_s.as_int(), Some(7838));

    let int_mod_i = builtin_capitalize(vec![Value::fixnum(7306)]).unwrap();
    assert_eq!(int_mod_i.as_int(), Some(7306));

    let int_dz_small = builtin_capitalize(vec![Value::fixnum(452)]).unwrap();
    assert_eq!(int_dz_small.as_int(), Some(453));

    let int_georgian_an = builtin_capitalize(vec![Value::fixnum(4304)]).unwrap();
    assert_eq!(int_georgian_an.as_int(), Some(4304));

    let string_sharp_s = builtin_capitalize(vec![Value::string("ß")]).unwrap();
    assert_eq!(string_sharp_s.as_utf8_str(), Some("Ss"));

    let string_n_preceded = builtin_capitalize(vec![Value::string("\u{0149}")]).unwrap();
    assert_eq!(string_n_preceded.as_utf8_str(), Some("\u{02BC}N"));

    let string_j_caron = builtin_capitalize(vec![Value::string("\u{01F0}")]).unwrap();
    assert_eq!(string_j_caron.as_utf8_str(), Some("J\u{030C}"));

    let string_greek_dialytika_tonos = builtin_capitalize(vec![Value::string("\u{0390}")]).unwrap();
    assert_eq!(
        string_greek_dialytika_tonos.as_utf8_str(),
        Some("\u{0399}\u{0308}\u{0301}")
    );

    let string_armenian_small_ligature =
        builtin_capitalize(vec![Value::string("\u{0587}")]).unwrap();
    assert_eq!(
        string_armenian_small_ligature.as_utf8_str(),
        Some("\u{0535}\u{0582}")
    );

    let string_latin_ligature_ff = builtin_capitalize(vec![Value::string("\u{FB00}")]).unwrap();
    assert_eq!(string_latin_ligature_ff.as_utf8_str(), Some("Ff"));

    let string_armenian_presentation_ligature =
        builtin_capitalize(vec![Value::string("\u{FB13}")]).unwrap();
    assert_eq!(
        string_armenian_presentation_ligature.as_utf8_str(),
        Some("\u{0544}\u{0576}")
    );

    let string_greek_precomposed_prosgegrammeni =
        builtin_capitalize(vec![Value::string("\u{1F88}")]).unwrap();
    assert_eq!(
        string_greek_precomposed_prosgegrammeni.as_utf8_str(),
        Some("\u{1F88}")
    );

    let string_greek_small_alpha_ypogegrammeni =
        builtin_capitalize(vec![Value::string("\u{1F80}")]).unwrap();
    assert_eq!(
        string_greek_small_alpha_ypogegrammeni.as_utf8_str(),
        Some("\u{1F88}")
    );

    let string_greek_combining_prosgegrammeni =
        builtin_capitalize(vec![Value::string("\u{1FB2}")]).unwrap();
    assert_eq!(
        string_greek_combining_prosgegrammeni.as_utf8_str(),
        Some("\u{1FBA}\u{0345}")
    );
}

#[test]
fn upcase_initials_unicode_edge_semantics() {
    crate::test_utils::init_test_tracing();
    let int_sharp_s = builtin_upcase_initials(vec![Value::fixnum(223)]).unwrap();
    assert_eq!(int_sharp_s.as_int(), Some(7838));

    let int_mod_i = builtin_upcase_initials(vec![Value::fixnum(7306)]).unwrap();
    assert_eq!(int_mod_i.as_int(), Some(7306));

    let int_dz_small = builtin_upcase_initials(vec![Value::fixnum(454)]).unwrap();
    assert_eq!(int_dz_small.as_int(), Some(453));

    let int_georgian_an = builtin_upcase_initials(vec![Value::fixnum(4304)]).unwrap();
    assert_eq!(int_georgian_an.as_int(), Some(4304));

    let string_sharp_s = builtin_upcase_initials(vec![Value::string("ß")]).unwrap();
    assert_eq!(string_sharp_s.as_utf8_str(), Some("Ss"));

    let string_n_preceded = builtin_upcase_initials(vec![Value::string("\u{0149}")]).unwrap();
    assert_eq!(string_n_preceded.as_utf8_str(), Some("\u{02BC}N"));

    let string_j_caron = builtin_upcase_initials(vec![Value::string("\u{01F0}")]).unwrap();
    assert_eq!(string_j_caron.as_utf8_str(), Some("J\u{030C}"));

    let string_greek_dialytika_tonos =
        builtin_upcase_initials(vec![Value::string("\u{0390}")]).unwrap();
    assert_eq!(
        string_greek_dialytika_tonos.as_utf8_str(),
        Some("\u{0399}\u{0308}\u{0301}")
    );

    let string_armenian_small_ligature =
        builtin_upcase_initials(vec![Value::string("\u{0587}")]).unwrap();
    assert_eq!(
        string_armenian_small_ligature.as_utf8_str(),
        Some("\u{0535}\u{0582}")
    );

    let string_latin_ligature_ff =
        builtin_upcase_initials(vec![Value::string("\u{FB00}")]).unwrap();
    assert_eq!(string_latin_ligature_ff.as_utf8_str(), Some("Ff"));

    let string_armenian_presentation_ligature =
        builtin_upcase_initials(vec![Value::string("\u{FB13}")]).unwrap();
    assert_eq!(
        string_armenian_presentation_ligature.as_utf8_str(),
        Some("\u{0544}\u{0576}")
    );

    let string_greek_precomposed_prosgegrammeni =
        builtin_upcase_initials(vec![Value::string("\u{1F88}")]).unwrap();
    assert_eq!(
        string_greek_precomposed_prosgegrammeni.as_utf8_str(),
        Some("\u{1F88}")
    );

    let string_greek_small_alpha_ypogegrammeni =
        builtin_upcase_initials(vec![Value::string("\u{1F80}")]).unwrap();
    assert_eq!(
        string_greek_small_alpha_ypogegrammeni.as_utf8_str(),
        Some("\u{1F88}")
    );

    let string_greek_combining_prosgegrammeni =
        builtin_upcase_initials(vec![Value::string("\u{1FB2}")]).unwrap();
    assert_eq!(
        string_greek_combining_prosgegrammeni.as_utf8_str(),
        Some("\u{1FBA}\u{0345}")
    );
}

#[test]
fn eval_upcase_region_noncontiguous_uses_region_extract_bounds() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let buffer_id = ev.buffers.current_buffer_id().expect("current buffer");
    ev.eval_str("(setq region-extract-function (lambda (_op) (list (cons 2 4))))")
        .expect("region-extract-function setup");
    ev.buffers.insert_into_buffer(buffer_id, "abcde");

    super::builtin_upcase_region(&mut ev, vec![Value::fixnum(1), Value::fixnum(5), Value::T])
        .expect("upcase-region");

    let buffer = ev.buffers.get(buffer_id).expect("buffer");
    assert_eq!(buffer.buffer_string(), "aBCde");
}

#[test]
fn eval_upcase_region_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let mut ev = super::super::eval::Context::new();
    let buffer_id = ev.buffers.current_buffer_id().expect("current buffer");
    ev.buffers
        .set_buffer_multibyte_flag(buffer_id, false)
        .expect("set-buffer-multibyte should accept current buffer");
    ev.buffers.insert_lisp_string_into_buffer(
        buffer_id,
        &crate::heap_types::LispString::from_unibyte(vec![0xFF, b'a', b'b']),
    );

    super::builtin_upcase_region(&mut ev, vec![Value::fixnum(1), Value::fixnum(4)])
        .expect("upcase-region");

    let buffer = ev.buffers.get(buffer_id).expect("buffer");
    let text = buffer.buffer_substring_lisp_string_range(buffer.accessible_emacs_byte_range());
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF, b'A', b'B']);
}

#[test]
fn eval_capitalize_word_preserves_raw_unibyte_bytes() {
    crate::test_utils::init_test_tracing();
    let mut ev = super::super::eval::Context::new();
    let buffer_id = ev.buffers.current_buffer_id().expect("current buffer");
    ev.buffers
        .set_buffer_multibyte_flag(buffer_id, false)
        .expect("set-buffer-multibyte should accept current buffer");
    ev.buffers.insert_lisp_string_into_buffer(
        buffer_id,
        &crate::heap_types::LispString::from_unibyte(vec![0xFF, b'a', b'b']),
    );
    ev.buffers
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(0));

    super::builtin_capitalize_word(&mut ev, vec![Value::fixnum(1)]).expect("capitalize-word");

    let buffer = ev.buffers.get(buffer_id).expect("buffer");
    let text = buffer.buffer_substring_lisp_string_range(buffer.accessible_emacs_byte_range());
    assert!(!text.is_multibyte());
    assert_eq!(text.as_bytes(), &[0xFF, b'a', b'b']);
}

#[test]
fn eval_capitalize_word_updates_buffer_text() {
    crate::test_utils::init_test_tracing();
    let mut ev = super::super::eval::Context::new();
    let buffer_id = ev.buffers.current_buffer_id().expect("current buffer");
    ev.buffers.insert_into_buffer(buffer_id, "hELLO world");
    ev.buffers
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(0));

    super::builtin_capitalize_word(&mut ev, vec![Value::fixnum(1)]).expect("capitalize-word");

    let buffer = ev.buffers.get(buffer_id).expect("buffer");
    assert_eq!(buffer.buffer_string(), "Hello world");
}
