use super::*;

use neovm_edit::buffer::CharPos0;

use neovm_value::error::{FlowKind, FlowResultExt as _};

use neovm_value::value::{equal_value, get_string_text_properties_table_for_value};

#[test]
fn compose_string_internal_returns_string() {
    crate::test_utils::init_test_tracing();
    let s = Value::string("hello");
    let result = compose_string_internal(vec![s, Value::fixnum(0), Value::fixnum(5)]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_utf8_str(), Some("hello"));
}

#[test]
fn compose_string_internal_with_optional_args() {
    crate::test_utils::init_test_tracing();
    let s = Value::string("hello");
    let result = compose_string_internal(vec![
        s,
        Value::fixnum(0),
        Value::fixnum(5),
        Value::NIL,
        Value::NIL,
    ]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_utf8_str(), Some("hello"));
}

#[test]
fn compose_string_internal_sets_composition_property() {
    crate::test_utils::init_test_tracing();
    let s = Value::string("hello");
    let components = Value::string("XY");
    let result = compose_string_internal(vec![
        s,
        Value::fixnum(1),
        Value::fixnum(4),
        components,
        Value::NIL,
    ]);
    assert!(result.is_ok());

    let table = get_string_text_properties_table_for_value(s).expect("string text properties");
    let prop = table
        .get_property_at_char_pos(CharPos0::new(1), Value::symbol("composition"))
        .expect("composition property");
    let header = prop.cons_car();
    assert_eq!(header.cons_car().as_fixnum(), Some(3));
    assert_eq!(header.cons_cdr().as_utf8_str(), Some("XY"));
    assert!(prop.cons_cdr().is_nil());
}

#[test]
fn compose_string_internal_uses_gnu_subarray_bounds() {
    crate::test_utils::init_test_tracing();
    let s = Value::string("abcd");
    let result = compose_string_internal(vec![
        s,
        Value::fixnum(-2),
        Value::fixnum(-1),
        Value::NIL,
        Value::NIL,
    ]);
    assert!(result.is_ok());

    let table = get_string_text_properties_table_for_value(s).expect("string text properties");
    assert!(
        table
            .get_property_at_char_pos(CharPos0::new(2), Value::symbol("composition"))
            .is_some()
    );
    assert!(
        table
            .get_property_at_char_pos(CharPos0::new(1), Value::symbol("composition"))
            .is_none()
    );
}

#[test]
fn compose_string_internal_nil_bounds_default_like_gnu() {
    crate::test_utils::init_test_tracing();
    let s = Value::string("abcd");
    let result = compose_string_internal(vec![s, Value::NIL, Value::NIL, Value::NIL, Value::NIL]);
    assert!(result.is_ok());

    let table = get_string_text_properties_table_for_value(s).expect("string text properties");
    let prop = table
        .get_property_at_char_pos(CharPos0::new(0), Value::symbol("composition"))
        .expect("composition property at start");
    assert_eq!(prop.cons_car().cons_car().as_fixnum(), Some(4));
}

#[test]
fn compose_string_internal_does_not_validate_components_like_region() {
    crate::test_utils::init_test_tracing();
    let s = Value::string("abcd");
    let result = compose_string_internal(vec![
        s,
        Value::fixnum(0),
        Value::fixnum(2),
        Value::T,
        Value::NIL,
    ]);
    assert!(result.is_ok());

    let table = get_string_text_properties_table_for_value(s).expect("string text properties");
    let prop = table
        .get_property_at_char_pos(CharPos0::new(0), Value::symbol("composition"))
        .expect("composition property");
    assert_eq!(prop.cons_car().cons_cdr(), Value::T);
}

#[test]
fn compose_string_internal_too_few_args() {
    crate::test_utils::init_test_tracing();
    let result = compose_string_internal(vec![Value::string("hi"), Value::fixnum(0)]);
    assert!(result.is_err());
}

#[test]
fn compose_string_internal_type_checks() {
    crate::test_utils::init_test_tracing();
    let non_string =
        compose_string_internal(vec![Value::fixnum(1), Value::fixnum(0), Value::fixnum(1)]);
    assert!(non_string.is_err());
    let bad_start = compose_string_internal(vec![
        Value::string("abc"),
        Value::symbol("x"),
        Value::fixnum(1),
    ]);
    assert!(bad_start.is_err());
    let bad_end = compose_string_internal(vec![
        Value::string("abc"),
        Value::fixnum(0),
        Value::symbol("y"),
    ]);
    assert!(bad_end.is_err());
}

#[test]
fn compose_string_internal_range_checks() {
    crate::test_utils::init_test_tracing();
    let ok = compose_string_internal(vec![
        Value::string("abc"),
        Value::fixnum(0),
        Value::fixnum(0),
    ]);
    assert!(ok.is_ok());

    let start_gt_end = compose_string_internal(vec![
        Value::string("abc"),
        Value::fixnum(2),
        Value::fixnum(1),
    ]);
    assert!(start_gt_end.is_err());

    let end_oob = compose_string_internal(vec![
        Value::string("abc"),
        Value::fixnum(0),
        Value::fixnum(4),
    ]);
    assert!(end_oob.is_err());

    let start_too_negative = compose_string_internal(vec![
        Value::string("abc"),
        Value::fixnum(-4),
        Value::fixnum(1),
    ]);
    assert!(start_too_negative.is_err());
}

#[test]
fn compose_string_internal_accepts_raw_unibyte_ranges() {
    crate::test_utils::init_test_tracing();
    let raw = Value::heap_string(neovm_value::heap_types::LispString::from_unibyte(vec![
        0xFF,
    ]));
    let result = compose_string_internal(vec![raw, Value::fixnum(0), Value::fixnum(1)]);
    assert!(result.is_ok());
    let value = result.unwrap();
    let string = value
        .as_lisp_string()
        .expect("compose-string-internal string");
    assert!(!string.is_multibyte());
    assert_eq!(string.as_bytes(), &[0xFF]);
}

#[test]
fn composition_display_text_treats_tabs_as_padding_markers() {
    crate::test_utils::init_test_tracing();
    // `descr-text.el` composes "\tA\t" to center the described character.
    // GNU's terminal encoder skips those structural TAB components; exposing
    // them as text would expand both tabs across the Help row.
    let components = Value::vector(vec![
        Value::fixnum('\t' as i64),
        Value::fixnum('A' as i64),
        Value::fixnum('\t' as i64),
    ]);
    let id = composition_assign_id(&components, true);
    let property = Value::list(vec![Value::fixnum(id), Value::fixnum(1), components]);

    let display = composition_display_text_for_property(property).expect("valid composition");
    assert_eq!(display.text(), "A");
    assert_eq!(display.char_len(), 1);
}

#[test]
fn composition_display_text_treats_string_tabs_as_padding_markers() {
    crate::test_utils::init_test_tracing();
    // `describe-char-padded-string` passes the fourth `compose-string`
    // argument as the string "\tA\t".  GNU registers its characters as the
    // same static component vector as the explicit vector form, so both input
    // representations must share one display-component decoder.
    let components = Value::string("\tA\t");
    let property = Value::cons(Value::cons(Value::fixnum(1), components), Value::NIL);

    let display = composition_display_text_for_property(property).expect("valid composition");
    assert_eq!(display.text(), "A");
    assert_eq!(display.char_len(), 1);
}

#[test]
fn terminal_automatic_composition_matches_gnu_gstring_cell_rules() {
    let javanese = automatic_composition_for_terminal("ꦮꦶ");
    assert_eq!(javanese.width_cols, 2);
    assert_eq!(javanese.cells[0].base, ' ');
    assert_eq!(javanese.cells[0].extenders.as_ref(), "ꦮ");
    assert_eq!(javanese.cells[1].base, ' ');
    assert_eq!(javanese.cells[1].extenders.as_ref(), "ꦶ");

    let rejang = automatic_composition_for_terminal("ꤸꥉ");
    assert_eq!(rejang.width_cols, 1);
    assert_eq!(rejang.cells[0].base, 'ꤸ');
    assert_eq!(rejang.cells[0].extenders.as_ref(), "ꥉ");

    let mongolian = automatic_composition_for_terminal("ᠨ\u{180E}");
    assert_eq!(mongolian.width_cols, 1);
    assert_eq!(mongolian.cells[0].base, 'ᠨ');
    assert_eq!(mongolian.cells[0].extenders.as_ref(), "\u{180E}");
}

#[test]
fn clear_composition_cache_no_args() {
    crate::test_utils::init_test_tracing();
    let result = clear_composition_cache(vec![]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn clear_composition_cache_too_many_args() {
    crate::test_utils::init_test_tracing();
    let result = clear_composition_cache(vec![Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn composition_sort_rules_nil_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = composition_sort_rules(vec![Value::NIL]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn composition_sort_rules_rejects_non_lists() {
    crate::test_utils::init_test_tracing();
    let result = composition_sort_rules(vec![Value::vector(vec![Value::fixnum(1)])]);
    assert!(result.is_err());
}

#[test]
fn composition_sort_rules_rejects_invalid_rules() {
    crate::test_utils::init_test_tracing();
    let rules = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    let result = composition_sort_rules(vec![rules]);
    assert!(result.is_err());
}

#[test]
fn composition_sort_rules_accepts_cons_rules() {
    crate::test_utils::init_test_tracing();
    let rules = Value::list(vec![Value::cons(Value::fixnum(1), Value::fixnum(2))]);
    let result = composition_sort_rules(vec![rules]).unwrap();
    assert_eq!(result, rules);
}

#[test]
fn composition_sort_rules_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let result = composition_sort_rules(vec![]);
    assert!(result.is_err());
}
