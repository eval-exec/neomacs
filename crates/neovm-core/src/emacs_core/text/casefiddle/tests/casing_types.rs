use super::*;

#[test]
fn natnum_validation_and_modifier_preservation() {
    crate::test_utils::init_test_tracing();
    assert!(CaseNatnum::try_from(-1).is_err());
    assert!(CaseNatnum::try_from(Value::MOST_POSITIVE_FIXNUM + 1).is_err());
    let event = CaseNatnum::try_from(CHAR_META | 'a' as i64).unwrap();
    assert_eq!(
        event.casify(
            CaseAction::Up,
            CaseEncoding::Multibyte,
            &CaseTableOverride::none()
        ),
        CHAR_META | 'A' as i64
    );
    let large = CaseNatnum::try_from(1 << 30).unwrap();
    assert_eq!(
        large.casify(
            CaseAction::Up,
            CaseEncoding::Multibyte,
            &CaseTableOverride::none()
        ),
        1 << 30
    );
}

#[test]
fn integer_byte_domain_depends_on_buffer_encoding() {
    crate::test_utils::init_test_tracing();
    let code = CaseNatnum::try_from(233).unwrap();
    assert_eq!(
        code.casify(
            CaseAction::Up,
            CaseEncoding::Unibyte,
            &CaseTableOverride::none()
        ),
        233
    );
    assert_eq!(
        code.casify(
            CaseAction::Up,
            CaseEncoding::Multibyte,
            &CaseTableOverride::none()
        ),
        201
    );
}

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

#[test]
fn fixnum_projection_matches_gnu_signed_c_int_event_domain() {
    crate::test_utils::init_test_tracing();
    let cases = CaseTableOverride::none();
    let wrapped = CaseNatnum::try_from((1 << 40) + 97).unwrap();
    assert_eq!(
        wrapped.casify(CaseAction::Up, CaseEncoding::Multibyte, &cases),
        65
    );
    let unchanged = CaseNatnum::try_from((1 << 40) + 65).unwrap();
    assert_eq!(
        unchanged.casify(CaseAction::Up, CaseEncoding::Multibyte, &cases),
        (1 << 40) + 65
    );
    let negative_projection = CaseNatnum::try_from((1 << 40) + (1 << 31)).unwrap();
    assert_eq!(
        negative_projection.casify(CaseAction::Up, CaseEncoding::Multibyte, &cases),
        (1 << 40) + (1 << 31)
    );
}
