use super::*;
use crate::buffer::CharPos0;
use crate::emacs_core::value::{equal_value, get_string_text_properties_table_for_value};

#[test]
fn compose_region_internal_min_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("current buffer");
        buffer.insert("0123456789");
    }
    let result = compose_region_internal(&mut eval, vec![Value::fixnum(1), Value::fixnum(10)]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn compose_region_internal_max_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("current buffer");
        buffer.insert("0123456789");
    }
    let result = compose_region_internal(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(10), Value::NIL, Value::NIL],
    );
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn compose_region_internal_sets_composition_property() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("current buffer");
        buffer.insert("abcdef");
    }
    let components = Value::list(vec![Value::fixnum('X' as i64), Value::fixnum('Y' as i64)]);

    compose_region_internal(
        &mut eval,
        vec![Value::fixnum(2), Value::fixnum(5), components, Value::NIL],
    )
    .expect("compose region");

    let prop = crate::emacs_core::textprop::builtin_get_text_property(
        &mut eval,
        vec![Value::fixnum(2), Value::symbol("composition")],
    )
    .expect("get composition property");
    assert!(prop.is_cons());
    let header = prop.cons_car();
    assert_eq!(header.cons_car().as_fixnum(), Some(3));
    assert!(equal_value(&header.cons_cdr(), &components, 0));
    assert!(prop.cons_cdr().is_nil());
}

#[test]
fn compose_region_internal_too_few_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = compose_region_internal(&mut eval, vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn compose_region_internal_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = compose_region_internal(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(10),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn compose_region_internal_rejects_non_integer_positions() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = compose_region_internal(&mut eval, vec![Value::symbol("x"), Value::fixnum(10)]);
    assert!(result.is_err());
    let result = compose_region_internal(&mut eval, vec![Value::fixnum(1), Value::symbol("y")]);
    assert!(result.is_err());
}

#[test]
fn compose_region_internal_eval_range_checks() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("current buffer");
        buffer.insert("abc");
    }
    let ok = compose_region_internal(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]);
    assert!(ok.is_ok());

    let out_of_range = compose_region_internal(&mut eval, vec![Value::fixnum(0), Value::fixnum(0)]);
    assert!(out_of_range.is_err());
}

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
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
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
fn find_composition_internal_returns_nil_when_no_composition() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("current buffer");
        buffer.insert("abcde");
    }
    let result = find_composition_internal(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(6), Value::NIL, Value::NIL],
    );
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

/// GNU's default `composition-function-table' rule for a combining mark looks
/// back one character and composes the base plus following marks.  This is the
/// path used by `string-glyph-split' for decomposed graphemes.
#[test]
fn find_composition_internal_finds_automatic_combining_sequence_in_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let current_buffer = eval
        .buffers
        .current_buffer_id()
        .expect("current buffer for automatic composition");
    eval.frames
        .create_frame("automatic-composition", 80, 24, current_buffer);
    let table = eval.visible_variable_value_or_nil("composition-function-table");
    let combining_rule = Value::vector(vec![
        Value::string("\\c.\\c^+"),
        Value::fixnum(1),
        Value::symbol("compose-gstring-for-graphic"),
    ]);
    crate::emacs_core::chartable::builtin_set_char_table_range(
        vec![
            table,
            Value::fixnum(0x0301),
            Value::list(vec![combining_rule]),
        ],
        Some(&eval.obarray),
    )
    .expect("install combining composition rule");
    let string = Value::string("e\u{0301}");

    let found = find_composition_internal(
        &mut eval,
        vec![Value::fixnum(0), Value::fixnum(1), string, Value::NIL],
    )
    .expect("find automatic composition");
    let items = list_to_vec(&found).expect("composition result");

    assert_eq!(items[0].as_fixnum(), Some(0));
    assert_eq!(items[1].as_fixnum(), Some(2));
    assert!(
        items[2].is_vector(),
        "automatic composition returns a gstring"
    );

    eval.obarray
        .set_symbol_value("auto-composition-mode", Value::NIL);
    let inhibited = find_composition_internal(
        &mut eval,
        vec![Value::fixnum(0), Value::fixnum(1), string, Value::NIL],
    )
    .expect("inhibited automatic composition lookup");
    assert!(
        inhibited.is_nil(),
        "auto-composition-mode nil must expose the two characters separately"
    );
}

#[test]
fn find_composition_internal_reports_composed_region() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("current buffer");
        buffer.insert("abcde");
    }
    // (compose-region 2 4 "") => find-composition detail (2 4 [] t nil 0)
    compose_region_internal(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::string(""),
            Value::NIL,
        ],
    )
    .expect("compose-region-internal");

    // Without DETAIL-P: (FROM TO VALID-P).
    let plain_val = find_composition_internal(
        &mut eval,
        vec![Value::fixnum(2), Value::NIL, Value::NIL, Value::NIL],
    )
    .expect("find-composition plain");
    let plain = crate::emacs_core::value::list_to_vec(&plain_val).expect("list result");
    assert_eq!(plain.len(), 3);
    assert_eq!(plain[0].as_fixnum(), Some(2));
    assert_eq!(plain[1].as_fixnum(), Some(4));
    assert_eq!(plain[2], Value::T);

    // With DETAIL-P: (FROM TO COMPONENTS RELATIVE-P MOD-FUNC WIDTH).
    let detail_val = find_composition_internal(
        &mut eval,
        vec![Value::fixnum(2), Value::NIL, Value::NIL, Value::T],
    )
    .expect("find-composition detail");
    let detail = crate::emacs_core::value::list_to_vec(&detail_val).expect("list result");
    assert_eq!(detail.len(), 6);
    assert_eq!(detail[0].as_fixnum(), Some(2));
    assert_eq!(detail[1].as_fixnum(), Some(4));
    assert_eq!(detail[2].as_vector_data().map(|v| v.len()), Some(0)); // [] empty components
    assert_eq!(detail[3], Value::T); // relative-p
    assert!(detail[4].is_nil()); // mod-func
    assert_eq!(detail[5].as_fixnum(), Some(0)); // width

    // A position outside the composed region returns nil.
    let outside = find_composition_internal(
        &mut eval,
        vec![Value::fixnum(1), Value::NIL, Value::NIL, Value::T],
    )
    .expect("find-composition outside");
    assert!(outside.is_nil());
}

#[test]
fn find_composition_internal_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = find_composition_internal(&mut eval, vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn find_composition_internal_type_checks() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let bad_pos = find_composition_internal(
        &mut eval,
        vec![
            Value::symbol("x"),
            Value::fixnum(10),
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(bad_pos.is_err());

    let bad_limit = find_composition_internal(
        &mut eval,
        vec![Value::fixnum(1), Value::symbol("y"), Value::NIL, Value::NIL],
    );
    assert!(bad_limit.is_err());

    let bad_string = find_composition_internal(
        &mut eval,
        vec![Value::fixnum(1), Value::NIL, Value::fixnum(1), Value::NIL],
    );
    assert!(bad_string.is_err());
}

#[test]
fn find_composition_internal_position_range_checks() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let zero = find_composition_internal(
        &mut eval,
        vec![Value::fixnum(0), Value::NIL, Value::NIL, Value::NIL],
    );
    assert!(zero.is_err());

    let negative = find_composition_internal(
        &mut eval,
        vec![Value::fixnum(-1), Value::NIL, Value::NIL, Value::NIL],
    );
    assert!(negative.is_err());
}

#[test]
fn composition_get_gstring_returns_vector_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = composition_get_gstring(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::fixnum(1),
            Value::NIL,
            Value::string("ab"),
        ],
    );
    let result = result.unwrap();
    if !result.is_vector() {
        panic!("expected vector gstring");
    };
    let gs = result.as_vector_data().unwrap().clone();
    assert!(!gs.is_empty());
    assert!(gs[0].is_vector());
}

#[test]
fn composition_get_gstring_uses_gnu_subarray_bounds() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = composition_get_gstring(
        &mut eval,
        vec![
            Value::fixnum(-2),
            Value::fixnum(-1),
            Value::NIL,
            Value::string("abcd"),
        ],
    )
    .unwrap();
    let gs = result.as_vector_data().expect("gstring vector").clone();
    let header = gs[0].as_vector_data().expect("gstring header").clone();
    assert_eq!(
        header,
        vec![Value::symbol("utf-8-unix"), Value::fixnum('c' as i64)]
    );
}

#[test]
fn composition_get_gstring_nil_bounds_default_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = composition_get_gstring(
        &mut eval,
        vec![
            Value::NIL,
            Value::fixnum(2),
            Value::NIL,
            Value::string("abcd"),
        ],
    )
    .unwrap();
    let gs = result.as_vector_data().expect("gstring vector").clone();
    let header = gs[0].as_vector_data().expect("gstring header").clone();
    assert_eq!(
        header,
        vec![
            Value::symbol("utf-8-unix"),
            Value::fixnum('a' as i64),
            Value::fixnum('b' as i64),
        ]
    );
}

#[test]
fn composition_get_gstring_rejects_non_ascii_unibyte_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let result = composition_get_gstring(
        &mut eval,
        vec![Value::fixnum(0), Value::fixnum(1), Value::NIL, raw],
    );
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|value| value.as_utf8_str()),
                Some("Attempt to shape unibyte text")
            );
        }
        other => panic!("expected unibyte shaping error, got {other:?}"),
    }
}

#[test]
fn composition_get_gstring_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let result = composition_get_gstring(&mut eval, vec![Value::fixnum(0)]);
    assert!(result.is_err());
}

#[test]
fn composition_get_gstring_type_checks() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let bad_from = composition_get_gstring(
        &mut eval,
        vec![
            Value::symbol("x"),
            Value::fixnum(5),
            Value::NIL,
            Value::string("hello"),
        ],
    );
    assert!(bad_from.is_err());

    let bad_to = composition_get_gstring(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::symbol("y"),
            Value::NIL,
            Value::string("hello"),
        ],
    );
    assert!(bad_to.is_err());

    let bad_string = composition_get_gstring(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::fixnum(5),
            Value::NIL,
            Value::fixnum(1),
        ],
    );
    assert!(bad_string.is_err());
}

#[test]
fn composition_get_gstring_range_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let from_gt_to = composition_get_gstring(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(1),
            Value::NIL,
            Value::string("ab"),
        ],
    );
    assert!(from_gt_to.is_err());

    let zero_length = composition_get_gstring(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::NIL,
            Value::string("ab"),
        ],
    );
    assert!(zero_length.is_err());
}

#[test]
fn composition_get_gstring_nil_string_uses_current_buffer_region() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("abcd");

    let result = composition_get_gstring(
        &mut eval,
        vec![Value::fixnum(2), Value::fixnum(4), Value::NIL, Value::NIL],
    )
    .unwrap();

    let gs = result.as_vector_data().expect("gstring vector").clone();
    let header = gs[0].as_vector_data().expect("gstring header").clone();
    assert_eq!(
        header,
        vec![
            Value::symbol("utf-8-unix"),
            Value::fixnum('b' as i64),
            Value::fixnum('c' as i64),
        ]
    );
}

#[test]
fn composition_get_gstring_nil_string_accepts_reversed_buffer_region() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("abcd");

    let result = composition_get_gstring(
        &mut eval,
        vec![Value::fixnum(4), Value::fixnum(2), Value::NIL, Value::NIL],
    )
    .unwrap();

    let gs = result.as_vector_data().expect("gstring vector").clone();
    let header = gs[0].as_vector_data().expect("gstring header").clone();
    assert_eq!(
        header,
        vec![
            Value::symbol("utf-8-unix"),
            Value::fixnum('b' as i64),
            Value::fixnum('c' as i64),
        ]
    );
}

#[test]
fn composition_get_gstring_nil_string_rejects_unibyte_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("current buffer");
        buffer.set_multibyte_value(false);
        buffer.insert("abcd");
    }

    let result = composition_get_gstring(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(2), Value::NIL, Value::NIL],
    );
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data.first().and_then(|value| value.as_utf8_str()),
                Some("Attempt to shape unibyte text")
            );
        }
        other => panic!("expected unibyte shaping error, got {other:?}"),
    }
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

// --- GNU-parity regression tests for compose-region/find-composition ---
//
// These drive the `*-internal` C builtins directly with the same arguments the
// Lisp wrappers (`compose-region`, `compose-string`, `find-composition`) pass
// after `encode-composition-components`. The expected values were taken from
// `emacs --batch --eval "(prin1 ...)"` on `find-composition-internal`.

/// GNU `Fcompose_region_internal` runs `validate_region`, which swaps START/END
/// so START <= END. An out-of-order region must compose, not signal
/// `args-out-of-range`.
/// GNU: (with-temp-buffer (insert "hello") (compose-region-internal 4 2)
///       (find-composition-internal 2 nil nil nil)) => (2 4 t)
#[test]
fn compose_region_internal_swaps_out_of_order_bounds() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("current buffer");
        buffer.insert("hello");
    }
    // compose-region-internal 4 2 -- must swap to compose [2,4), not signal.
    compose_region_internal(&mut eval, vec![Value::fixnum(4), Value::fixnum(2)])
        .expect("compose-region-internal must swap, not signal args-out-of-range");

    let detail = find_composition_internal(
        &mut eval,
        vec![Value::fixnum(2), Value::NIL, Value::NIL, Value::NIL],
    )
    .expect("find-composition");
    // Expect (2 4 t).
    let items = list_to_vec(&detail).expect("list");
    assert_eq!(
        items.len(),
        3,
        "got {:?}",
        crate::emacs_core::print::print_value(&detail)
    );
    assert_eq!(items[0].as_fixnum(), Some(2));
    assert_eq!(items[1].as_fixnum(), Some(4));
    assert_eq!(items[2], Value::T);
}

/// GNU `get_composition_id` rejects an even-length rule/components vector
/// (id = -1), so the detail call reports only `(FROM TO)`. The components for
/// `(compose-region 1 3 (list ?X ?Y))` reach the builtin as the list `(88 89)`.
/// GNU: (find-composition-internal 1 nil nil t) => (1 3)
#[test]
fn find_composition_internal_rejects_even_length_components() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    {
        let buffer = eval.buffers.current_buffer_mut().expect("current buffer");
        buffer.insert("abc");
    }
    let components = Value::list(vec![Value::fixnum('X' as i64), Value::fixnum('Y' as i64)]);
    compose_region_internal(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(3), components, Value::NIL],
    )
    .expect("compose-region-internal");

    let detail = find_composition_internal(
        &mut eval,
        vec![Value::fixnum(1), Value::NIL, Value::NIL, Value::T],
    )
    .expect("find-composition detail");
    // Even-length rule vector is invalid -> only (FROM TO) = (1 3).
    let items = list_to_vec(&detail).expect("list");
    assert_eq!(
        items.len(),
        2,
        "even-length composition must report only (FROM TO); got {}",
        crate::emacs_core::print::print_value(&detail)
    );
    assert_eq!(items[0].as_fixnum(), Some(1));
    assert_eq!(items[1].as_fixnum(), Some(3));
}

/// GNU computes the WIDTH of a rule-based composition from the leftmost/rightmost
/// overlap geometry, not the max component glyph width. Stacked `(tc . bc)`
/// rules (encoded to 19) give width 1, even though the rule code 19 displays as
/// a 2-column control glyph.
/// GNU: (compose-string-internal "hello" 0 3 [65 19 66 19 67]) then
///      (find-composition-internal 0 nil s t) => (0 3 [65 19 66 19 67] nil nil 1)
#[test]
fn find_composition_internal_rule_based_width_is_geometry() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    // Components as produced by `encode-composition-components` on
    // [?A (tc . bc) ?B (tc . bc) ?C]: (tc . bc) -> 1*12 + 7 = 19.
    let components = Value::vector(vec![
        Value::fixnum('A' as i64),
        Value::fixnum(19),
        Value::fixnum('B' as i64),
        Value::fixnum(19),
        Value::fixnum('C' as i64),
    ]);
    let s = Value::string("hello");
    let composed = compose_string_internal(vec![
        s,
        Value::fixnum(0),
        Value::fixnum(3),
        components,
        Value::NIL,
    ])
    .expect("compose-string-internal");

    let detail = find_composition_internal(
        &mut eval,
        vec![Value::fixnum(0), Value::NIL, composed, Value::T],
    )
    .expect("find-composition detail");
    // (0 3 [65 19 66 19 67] nil nil 1)
    let items = list_to_vec(&detail).expect("list");
    assert_eq!(
        items.len(),
        6,
        "got {}",
        crate::emacs_core::print::print_value(&detail)
    );
    assert_eq!(items[0].as_fixnum(), Some(0));
    assert_eq!(items[1].as_fixnum(), Some(3));
    let expected_comps = Value::vector(vec![
        Value::fixnum(65),
        Value::fixnum(19),
        Value::fixnum(66),
        Value::fixnum(19),
        Value::fixnum(67),
    ]);
    assert!(
        equal_value(&items[2], &expected_comps, 0),
        "components mismatch: got {}",
        crate::emacs_core::print::print_value(&items[2])
    );
    assert_eq!(items[3], Value::NIL, "relative-p must be nil (rule-based)");
    assert_eq!(items[4], Value::NIL, "mod-func");
    assert_eq!(
        items[5].as_fixnum(),
        Some(1),
        "rule-based width must be geometry-computed (1), not max glyph width (2)"
    );
}
