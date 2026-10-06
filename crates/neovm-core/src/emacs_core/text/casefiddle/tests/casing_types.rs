use super::*;

#[test]
fn casing_extents_pair_character_and_byte_units() {
    crate::test_utils::init_test_tracing();
    let input = LispString::from_utf8("aßﬃİ");
    let mut extents = Vec::new();
    let upper = casify_lisp_string_with_extents(
        &input,
        CaseAction::Up,
        standard_word_predicate,
        &CaseTableOverride::none(),
        |extent| extents.push(extent),
    );
    assert_eq!(upper.as_bytes(), "ASSFFIİ".as_bytes());
    assert_eq!(
        extents
            .iter()
            .map(|extent| (
                extent.source_pos().get(),
                extent.output_extent().chars().get(),
                extent.output_extent().emacs_bytes().get()
            ))
            .collect::<Vec<_>>(),
        vec![(0, 1, 1), (1, 2, 2), (2, 3, 3), (3, 1, 2)]
    );
}

#[test]
fn missing_installed_case_entries_are_identity() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let table = Value::make_char_table(Value::symbol("case-table"), Value::NIL, 3);
    super::super::casetab::builtin_set_case_table(&mut ev, vec![table]).unwrap();
    let cases = CaseTableOverride::for_current_buffer(&mut ev).unwrap();
    for which in [CaseMap::Up, CaseMap::Down, CaseMap::Canon] {
        assert_eq!(cases.map(which, 'a' as i64), Some('a' as i64));
        assert_eq!(cases.map(which, 'É' as i64), Some('É' as i64));
    }
}

#[test]
fn target_policy_distinguishes_unibyte_strings_and_buffers() {
    crate::test_utils::init_test_tracing();
    let mut ev = super::super::eval::Context::new();
    let table = super::super::casetab::make_case_table_with_pair(304, 105);
    super::super::casetab::builtin_set_case_table(&mut ev, vec![table]).unwrap();
    let cases = CaseTableOverride::for_current_buffer(&mut ev).unwrap();
    let input = LispString::from_unibyte(vec![b'i']);
    let string = casify_text_with_extents(
        &input,
        CaseAction::Up,
        CaseTarget::String,
        standard_word_predicate,
        &cases,
        |_| {},
    );
    let buffer = casify_text_with_extents(
        &input,
        CaseAction::Up,
        CaseTarget::Buffer,
        standard_word_predicate,
        &cases,
        |_| {},
    );
    assert_eq!(string.as_bytes(), b"I");
    assert_eq!(buffer.as_bytes(), b"0");
}
