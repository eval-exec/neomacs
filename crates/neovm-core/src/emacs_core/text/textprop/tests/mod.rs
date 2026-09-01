use super::super::eval::Context;
use super::*;
use crate::buffer::{CharLen, CharRange};
use crate::emacs_core::buffer::{
    builtin_delete_overlay, builtin_make_overlay, builtin_move_overlay,
    builtin_next_overlay_change, builtin_overlay_buffer, builtin_overlay_end, builtin_overlay_get,
    builtin_overlay_properties, builtin_overlay_put, builtin_overlay_start, builtin_overlayp,
    builtin_overlays_at, builtin_overlays_in, builtin_previous_overlay_change,
};
use crate::emacs_core::builtins::{
    builtin_current_buffer, builtin_get_pos_property, builtin_goto_char, builtin_insert,
    builtin_make_indirect_buffer, builtin_next_char_property_change,
    builtin_previous_char_property_change, builtin_previous_property_change,
};
use crate::emacs_core::error::Flow;
use malachite::Integer;

/// Helper: create an evaluator with a buffer containing the given text.
fn eval_with_text(text: &str) -> Context {
    let mut eval = Context::new();
    eval.buffers.current_buffer_mut().unwrap().insert(text);
    // Reset point to beginning.
    eval.buffers
        .current_buffer_mut()
        .unwrap()
        .goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(0));
    eval
}

// -----------------------------------------------------------------------
// put-text-property / get-text-property
// -----------------------------------------------------------------------

#[test]
fn put_and_get_text_property() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    // Put 'face -> bold on positions 1..6 (1-based, "hello")
    let result = builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(6),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    );
    assert!(result.is_ok());

    // Get at position 3 (1-based, 'l')
    let result =
        builtin_get_text_property(&mut eval, vec![Value::fixnum(3), Value::symbol("face")]);
    match result {
        Ok(v) if v.as_symbol_id().is_some() => {
            assert_eq!(
                crate::emacs_core::intern::resolve_sym(v.as_symbol_id().unwrap()),
                "bold"
            );
        }
        other => panic!("Expected Symbol(bold), got {:?}", other),
    }
}

#[test]
fn get_text_property_returns_nil_when_absent() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result =
        builtin_get_text_property(&mut eval, vec![Value::fixnum(1), Value::symbol("face")]);
    assert!(result.as_ref().map_or(false, |v| v.is_nil()));
}

#[test]
fn buffer_substring_runs_access_fontify_functions_before_copying_properties() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("B");
    let result = eval
        .eval_str(
            r#"(progn
                 (setq buffer-access-fontify-functions
                       (list (lambda (beg end)
                               (put-text-property beg end 'fontified nil))))
                 (let ((copied (buffer-substring 1 2)))
                   (text-properties-at 0 copied)))"#,
        )
        .expect("buffer-substring should run its access-fontification hook");

    assert_eq!(format!("{result}"), "(fontified nil)");
}

#[test]
fn get_text_property_out_of_range_signal_uses_gnu_point_range_payload() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result =
        builtin_get_text_property(&mut eval, vec![Value::fixnum(0), Value::symbol("face")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(sig.data, vec![Value::fixnum(0), Value::fixnum(0)]);
        }
        other => panic!("expected args-out-of-range signal, got {other:?}"),
    }
}

#[test]
fn previous_single_property_change_steps_back_over_multibyte_char() {
    // Regression: `previous-single-property-change' looked up the property of
    // the character *before* a boundary by subtracting one BYTE.  In a
    // multibyte buffer the previous character is several bytes back, so the
    // byte position landed mid-character and tripped the Emacs-char-boundary
    // assertion (crash on repeated `j' in a Doom dashboard).  It must step
    // back one whole character, matching GNU's char-based `position - 1`.
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("a汉b"); // 汉 = U+6C49, 3 internal bytes
    // face=bold on Lisp positions [1,3): covers 'a' and '汉', but not 'b'.
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(3),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .expect("put-text-property");
    // Scanning back from end (pos 4) must walk over the 3-byte 汉 by a whole
    // character and report the bold->nil boundary at position 3.
    let result = builtin_previous_single_property_change(
        &mut eval,
        vec![Value::fixnum(4), Value::symbol("face")],
    )
    .expect("previous-single-property-change must not panic");
    assert_eq!(result, Value::fixnum(3));
}

#[test]
fn previous_property_change_steps_back_over_multibyte_char() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("a汉b"); // 汉 = U+6C49, 3 internal bytes
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(3),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .expect("put-text-property");

    let result = eval
        .eval_str("(previous-property-change 4)")
        .expect("previous-property-change must not step into a multibyte char");
    assert_eq!(result, Value::fixnum(3));
}

#[test]
fn get_text_property_uses_category_symbol_identity() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(let ((cat (make-symbol "text-category")))
                 (put cat 'oracle-prop 'from-category)
                 (insert "abc")
                 (put-text-property 2 3 'category cat)
                 (get-text-property 2 'oracle-prop))"#,
        )
        .expect("evaluation succeeds");
    assert_eq!(result.as_symbol_name(), Some("from-category"));
}

#[test]
fn propertize_copies_interval_plist_spines_like_gnu_copy_sequence() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r##"(let* ((source (propertize "x" 'face 'old))
                       (copy (propertize source 'face 'new)))
                  (list (get-text-property 0 'face source)
                        (get-text-property 0 'face copy)))"##,
        )
        .expect("propertize should preserve the source string's interval plist");

    assert_eq!(result.cons_car(), Value::symbol("old"));
    assert_eq!(result.cons_cdr().cons_car(), Value::symbol("new"));
}

#[test]
fn put_text_property_outside_range() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(3),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();

    // Position 4 is outside the propertized range.
    let result =
        builtin_get_text_property(&mut eval, vec![Value::fixnum(4), Value::symbol("face")]);
    assert!(result.as_ref().map_or(false, |v| v.is_nil()));
}

#[test]
fn indirect_buffers_share_text_property_updates() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let base = builtin_current_buffer(&mut eval, vec![]).unwrap();
    let indirect =
        builtin_make_indirect_buffer(&mut eval, vec![base, Value::string("*tp-indirect*")])
            .unwrap();

    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(6),
            Value::symbol("face"),
            Value::symbol("bold"),
            base,
        ],
    )
    .unwrap();

    let via_indirect = builtin_get_text_property(
        &mut eval,
        vec![Value::fixnum(3), Value::symbol("face"), indirect],
    )
    .unwrap();
    assert!(via_indirect.is_symbol_named("bold"));

    builtin_remove_text_properties(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(6),
            Value::list(vec![Value::symbol("face"), Value::NIL]),
            indirect,
        ],
    )
    .unwrap();

    let via_base = builtin_get_text_property(
        &mut eval,
        vec![Value::fixnum(3), Value::symbol("face"), base],
    )
    .unwrap();
    assert!(via_base.is_nil());
}

#[test]
fn buffer_string_preserves_raw_default_interval_after_plain_replacement() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let key = Value::string("C-u");
    let mut props = crate::buffer::text_props::TextPropertyTable::new();
    props.put_property_in_char_range(
        CharRange::from_usize(0, 3),
        Value::symbol("face"),
        Value::symbol("help-key-binding"),
    );
    props.put_property_in_char_range(
        CharRange::from_usize(0, 3),
        Value::symbol("font-lock-face"),
        Value::symbol("help-key-binding"),
    );
    crate::emacs_core::value::set_string_text_properties_table_for_value(key, props);

    builtin_insert(&mut eval, vec![key]).expect("insert propertized string");
    builtin_insert(&mut eval, vec![Value::string("' rest")]).expect("insert plain string");
    builtin_goto_char(&mut eval, vec![Value::fixnum(4)]).expect("goto inserted quote");
    builtin_insert(&mut eval, vec![Value::string("X")]).expect("insert replacement");
    crate::emacs_core::editfns::builtin_delete_region(
        &mut eval,
        vec![Value::fixnum(5), Value::fixnum(6)],
    )
    .expect("delete original quote");

    let current_id = eval.buffers.current_buffer_id().expect("current buffer");
    let buffer = eval.buffers.get(current_id).expect("buffer exists");
    let text = buffer.buffer_substring_lisp_string_range(buffer.accessible_emacs_byte_range());
    let shape: Vec<_> = text
        .intervals()
        .object_interval_runs_for_char_len(CharLen::new(text.schars()))
        .into_iter()
        .map(|run| {
            (
                run.start().get(),
                run.end().get(),
                run.properties().is_empty(),
            )
        })
        .collect();

    assert_eq!(text.as_utf8_str(), Some("C-uX rest"));
    assert_eq!(shape, vec![(0, 3, false), (3, 4, true), (4, 9, true)]);
}

#[test]
fn plain_insert_splits_inserted_nil_interval_like_gnu_graft() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    builtin_insert(&mut eval, vec![Value::string("AAAAABBBBB")]).expect("insert base text");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(6),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .expect("put bold face");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(6),
            Value::fixnum(11),
            Value::symbol("face"),
            Value::symbol("italic"),
        ],
    )
    .expect("put italic face");

    builtin_goto_char(&mut eval, vec![Value::fixnum(3)]).expect("goto first insert");
    builtin_insert(&mut eval, vec![Value::string("SPLIT")]).expect("insert split");
    builtin_goto_char(&mut eval, vec![Value::fixnum(6)]).expect("goto second insert");
    builtin_insert(&mut eval, vec![Value::string("HERE")]).expect("insert here");

    let buffer = eval.buffers.current_buffer().expect("current buffer");
    let shape: Vec<_> = buffer
        .text_props_object_interval_runs()
        .into_iter()
        .map(|run| {
            (
                run.start().get(),
                run.end().get(),
                run.properties().is_empty(),
            )
        })
        .collect();

    assert_eq!(
        shape,
        vec![
            (0, 2, false),
            (2, 5, true),
            (5, 9, true),
            (9, 11, true),
            (11, 14, false),
            (14, 19, false),
        ]
    );
}

/// GNU `adjust_intervals_for_insertion` reads `Qfront_sticky`,
/// `Qrear_nonsticky`, and `Vtext_property_default_nonsticky` through
/// predeclared identities.  Completion construction inserts thousands of
/// candidate strings, so the steady-state insertion path must not translate
/// those fixed identities from Rust strings for every candidate.
#[test]
fn inheriting_insert_uses_predeclared_text_property_stickiness_identities() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    {
        let buf = eval.buffers.current_buffer_mut().expect("current buffer");
        buf.insert("ab");
        buf.text_props_put_property_in_emacs_byte_range(
            crate::buffer::EmacsByteRange::from_usize(0, 1),
            Value::symbol("face"),
            Value::symbol("bold"),
        );
        buf.text_props_put_property_in_emacs_byte_range(
            crate::buffer::EmacsByteRange::from_usize(1, 2),
            Value::symbol("face"),
            Value::symbol("italic"),
        );
        buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(1));
    }

    crate::emacs_core::buffer::builtin_insert_and_inherit(&mut eval, vec![Value::string("X")])
        .expect("warm insertion path");
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(1));

    crate::emacs_core::intern::reset_intern_calls();
    crate::emacs_core::buffer::builtin_insert_and_inherit(&mut eval, vec![Value::string("Y")])
        .expect("repeat insertion path");

    const GNU_PREDECLARED_STICKINESS_IDENTITIES: &[&str] = &[
        "text-property-default-nonsticky",
        "front-sticky",
        "rear-nonsticky",
    ];
    let interned = crate::emacs_core::intern::intern_call_names();
    let repeated_fixed_names = interned
        .iter()
        .filter(|name| GNU_PREDECLARED_STICKINESS_IDENTITIES.contains(&name.as_str()))
        .collect::<Vec<_>>();
    assert!(
        repeated_fixed_names.is_empty(),
        "steady-state insertion must use GNU-shaped predeclared identities; interned \
         {repeated_fixed_names:?}"
    );
}

// -----------------------------------------------------------------------
// get-char-property
// -----------------------------------------------------------------------

#[test]
fn get_char_property_delegates() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcdef");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(5),
            Value::symbol("help-echo"),
            Value::string("tooltip"),
        ],
    )
    .unwrap();

    let result = builtin_get_char_property(
        &mut eval,
        vec![Value::fixnum(3), Value::symbol("help-echo")],
    );
    assert!(result.unwrap().is_string());
}

#[test]
fn overlay_lifecycle_bumps_overlay_modified_tick() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcdef");

    // Creating an overlay must bump the tick. This was an asymmetry:
    // move/put/delete bumped `overlay_modified_tick` but `make-overlay` did
    // not, so incremental redisplay could miss a freshly created overlay.
    let before_make = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .overlay_modified_tick();
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();
    let after_make = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .overlay_modified_tick();
    assert!(
        after_make > before_make,
        "make-overlay must bump overlay_modified_tick (before={before_make} after={after_make})"
    );

    // Putting a display-affecting property bumps it too (already worked).
    builtin_overlay_put(
        &mut eval,
        vec![ov, Value::symbol("face"), Value::symbol("highlight")],
    )
    .unwrap();
    let after_put = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .overlay_modified_tick();
    assert!(
        after_put > after_make,
        "overlay-put must bump overlay_modified_tick"
    );

    // Deleting bumps it too (already worked).
    builtin_delete_overlay(&mut eval, vec![ov]).unwrap();
    let after_delete = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .overlay_modified_tick();
    assert!(
        after_delete > after_put,
        "delete-overlay must bump overlay_modified_tick"
    );
}

#[test]
fn get_char_property_and_overlay_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcd");
    let result = builtin_get_char_property_and_overlay(
        &mut eval,
        vec![Value::fixnum(2), Value::symbol("missing")],
    )
    .unwrap();
    let pair = list_to_vec(&result).unwrap();
    assert_eq!(pair, vec![Value::NIL]);

    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(2), Value::fixnum(4)]).unwrap();
    builtin_overlay_put(
        &mut eval,
        vec![ov, Value::symbol("foo"), Value::symbol("bar")],
    )
    .unwrap();
    let result = builtin_get_char_property_and_overlay(
        &mut eval,
        vec![Value::fixnum(3), Value::symbol("foo")],
    )
    .unwrap();
    if !result.is_cons() {
        panic!("expected cons");
    };
    let (value, overlay) = {
        let pair_car = result.cons_car();
        let pair_cdr = result.cons_cdr();
        (pair_car, pair_cdr)
    };
    assert!(value.is_symbol_named("bar"));
    let overlayp = builtin_overlayp(&mut eval, vec![overlay]).unwrap();
    assert!(overlayp.is_t());
}

#[test]
fn get_char_property_prefers_highest_priority_overlay() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcd");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(3),
            Value::symbol("face"),
            Value::symbol("text"),
        ],
    )
    .unwrap();

    let low = builtin_make_overlay(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::NIL,
            Value::T,
            Value::NIL,
        ],
    )
    .unwrap();
    let high = builtin_make_overlay(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::NIL,
            Value::T,
            Value::NIL,
        ],
    )
    .unwrap();

    builtin_overlay_put(
        &mut eval,
        vec![low, Value::symbol("face"), Value::symbol("low")],
    )
    .unwrap();
    builtin_overlay_put(
        &mut eval,
        vec![low, Value::symbol("priority"), Value::fixnum(1)],
    )
    .unwrap();
    builtin_overlay_put(
        &mut eval,
        vec![high, Value::symbol("face"), Value::symbol("high")],
    )
    .unwrap();
    builtin_overlay_put(
        &mut eval,
        vec![
            high,
            Value::symbol("priority"),
            Value::cons(Value::fixnum(10), Value::fixnum(0)),
        ],
    )
    .unwrap();

    let char_prop =
        builtin_get_char_property(&mut eval, vec![Value::fixnum(2), Value::symbol("face")])
            .unwrap();
    assert_eq!(char_prop.as_symbol_name(), Some("high"));

    let pair = builtin_get_char_property_and_overlay(
        &mut eval,
        vec![Value::fixnum(2), Value::symbol("face")],
    )
    .unwrap();
    if !pair.is_cons() {
        panic!("expected cons");
    };
    let pair_car = pair.cons_car();
    let pair_cdr = pair.cons_cdr();
    assert_eq!(pair_car.as_symbol_name(), Some("high"));
    assert_eq!(pair_cdr, high);
}

#[test]
fn get_char_property_same_range_overlays_use_gnu_identity_tiebreaker() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcd");
    let first = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();
    let second = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(5)]).unwrap();

    builtin_overlay_put(
        &mut eval,
        vec![first, Value::symbol("face"), Value::symbol("first")],
    )
    .unwrap();
    builtin_overlay_put(
        &mut eval,
        vec![second, Value::symbol("face"), Value::symbol("second")],
    )
    .unwrap();
    assert_eq!(
        lookup_overlay_property(&eval.obarray, &eval.buffers, first, Value::symbol("face"))
            .as_symbol_name(),
        Some("first")
    );
    assert_eq!(
        lookup_overlay_property(&eval.obarray, &eval.buffers, second, Value::symbol("face"))
            .as_symbol_name(),
        Some("second")
    );
    let raw = builtin_overlays_at(&mut eval, vec![Value::fixnum(2)]).unwrap();
    assert_eq!(list_to_vec(&raw).unwrap(), vec![second, first]);
    let sorted = builtin_overlays_at(&mut eval, vec![Value::fixnum(2), Value::T]).unwrap();
    assert_eq!(list_to_vec(&sorted).unwrap(), vec![second, first]);

    let result =
        builtin_get_char_property(&mut eval, vec![Value::fixnum(2), Value::symbol("face")])
            .unwrap();
    assert_eq!(result.as_symbol_name(), Some("second"));
}

#[test]
fn get_pos_property_respects_overlay_advance_and_text_stickiness() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcd");

    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("carry"),
            Value::symbol("before"),
        ],
    )
    .unwrap();
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("rear-nonsticky"),
            Value::list(vec![Value::symbol("carry")]),
        ],
    )
    .unwrap();
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(3),
            Value::symbol("carry"),
            Value::symbol("after"),
        ],
    )
    .unwrap();
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(3),
            Value::symbol("front-sticky"),
            Value::list(vec![Value::symbol("carry")]),
        ],
    )
    .unwrap();
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(3),
            Value::symbol("face"),
            Value::symbol("text"),
        ],
    )
    .unwrap();

    let low = builtin_make_overlay(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::NIL,
            Value::T,
            Value::NIL,
        ],
    )
    .unwrap();
    let high = builtin_make_overlay(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::NIL,
            Value::T,
            Value::NIL,
        ],
    )
    .unwrap();
    builtin_overlay_put(
        &mut eval,
        vec![low, Value::symbol("face"), Value::symbol("low")],
    )
    .unwrap();
    builtin_overlay_put(
        &mut eval,
        vec![low, Value::symbol("priority"), Value::fixnum(1)],
    )
    .unwrap();
    builtin_overlay_put(
        &mut eval,
        vec![high, Value::symbol("face"), Value::symbol("high")],
    )
    .unwrap();
    builtin_overlay_put(
        &mut eval,
        vec![high, Value::symbol("priority"), Value::fixnum(10)],
    )
    .unwrap();

    let start_face =
        builtin_get_pos_property(&mut eval, vec![Value::fixnum(2), Value::symbol("face")]).unwrap();
    assert!(start_face.is_nil());

    let carry = builtin_get_pos_property(&mut eval, vec![Value::fixnum(2), Value::symbol("carry")])
        .unwrap();
    assert_eq!(carry.as_symbol_name(), Some("after"));

    let inside_face =
        builtin_get_pos_property(&mut eval, vec![Value::fixnum(3), Value::symbol("face")]).unwrap();
    assert_eq!(inside_face.as_symbol_name(), Some("high"));
}

#[test]
fn get_pos_property_on_string_delegates_to_text_property() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let string = Value::string("abcd");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::symbol("face"),
            Value::symbol("bold"),
            string,
        ],
    )
    .unwrap();

    let result = builtin_get_pos_property(
        &mut eval,
        vec![Value::fixnum(3), Value::symbol("face"), string],
    )
    .unwrap();
    assert_eq!(result.as_symbol_name(), Some("bold"));
}

#[test]
fn string_multibyte_text_property_intervals_are_char_indexed() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let string = Value::string("éx");

    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::fixnum(1),
            Value::symbol("face"),
            Value::symbol("bold"),
            string,
        ],
    )
    .unwrap();

    let intervals = crate::emacs_core::value::get_string_text_properties_table_for_value(string)
        .unwrap()
        .intervals_snapshot();
    assert_eq!(intervals.len(), 1);
    assert_eq!((intervals[0].start, intervals[0].end), (0, 1));
}

#[test]
fn buffer_multibyte_text_property_intervals_are_char_indexed() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("éx");

    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();

    let intervals = eval
        .buffers
        .current_buffer()
        .unwrap()
        .text_props_intervals_snapshot_for_test();
    assert_eq!(intervals.len(), 1);
    assert_eq!((intervals[0].start, intervals[0].end), (0, 1));
}

#[test]
fn string_text_properties_handle_raw_unibyte_storage() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let string = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'A', 0x80, b'Z',
    ]));

    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::symbol("face"),
            Value::symbol("bold"),
            string,
        ],
    )
    .unwrap();

    let result = builtin_get_text_property(
        &mut eval,
        vec![Value::fixnum(3), Value::symbol("face"), string],
    )
    .unwrap();
    assert_eq!(result.as_symbol_name(), Some("bold"));
}

#[test]
fn string_property_change_navigation_handles_raw_unibyte_storage() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let string = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'A', 0x80, b'Z',
    ]));

    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::symbol("face"),
            Value::symbol("bold"),
            string,
        ],
    )
    .unwrap();

    let next = builtin_next_single_property_change(
        &mut eval,
        vec![Value::fixnum(1), Value::symbol("face"), string],
    )
    .unwrap();
    assert_eq!(next, Value::fixnum(2));

    let prev = builtin_previous_single_property_change(
        &mut eval,
        vec![Value::fixnum(4), Value::symbol("face"), string],
    )
    .unwrap();
    assert_eq!(prev, Value::fixnum(2));
}

#[test]
fn next_single_char_property_change_on_raw_unibyte_string_uses_lisp_length() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let string = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        0xFF, b'A', 0x80, b'Z',
    ]));

    let result = crate::emacs_core::builtins::builtin_next_single_char_property_change(
        &mut eval,
        vec![Value::fixnum(1), Value::symbol("face"), string],
    )
    .unwrap();
    assert_eq!(result, Value::fixnum(4));
}

#[test]
fn get_display_property_queries_display_only() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcd");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::symbol("p"),
            Value::symbol("v"),
        ],
    )
    .unwrap();
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::symbol("display"),
            Value::symbol("dv"),
        ],
    )
    .unwrap();
    let non_display = builtin_get_display_property(
        &mut eval,
        vec![Value::fixnum(2), Value::symbol("p"), Value::NIL, Value::NIL],
    )
    .unwrap();
    assert!(non_display.is_nil());

    let display = builtin_get_display_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::symbol("display"),
            Value::NIL,
            Value::NIL,
        ],
    )
    .unwrap();
    assert!(display.is_symbol_named("dv"));
}

// -----------------------------------------------------------------------
// add-text-properties
// -----------------------------------------------------------------------

#[test]
fn add_text_properties_multiple() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let props = Value::list(vec![
        Value::symbol("face"),
        Value::symbol("bold"),
        Value::symbol("mouse-face"),
        Value::symbol("highlight"),
    ]);
    let result =
        builtin_add_text_properties(&mut eval, vec![Value::fixnum(1), Value::fixnum(6), props]);
    assert!(result.is_ok());

    let face = builtin_get_text_property(&mut eval, vec![Value::fixnum(2), Value::symbol("face")])
        .unwrap();
    assert!(face.is_symbol_named("bold"));

    let mouse = builtin_get_text_property(
        &mut eval,
        vec![Value::fixnum(2), Value::symbol("mouse-face")],
    )
    .unwrap();
    assert!(mouse.is_symbol_named("highlight"));
}

#[test]
fn add_text_properties_odd_plist_signals_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let props = Value::list(vec![Value::symbol("face")]);
    let result =
        builtin_add_text_properties(&mut eval, vec![Value::fixnum(1), Value::fixnum(3), props]);
    assert!(result.is_err());
}

#[test]
fn add_face_text_property_basic_and_merge_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abc");
    builtin_add_face_text_property(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(3), Value::symbol("bold")],
    )
    .unwrap();
    let face = builtin_get_text_property(&mut eval, vec![Value::fixnum(2), Value::symbol("face")])
        .unwrap();
    assert_eq!(face, Value::symbol("bold"));

    let mut eval = eval_with_text("abc");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("face"),
            Value::symbol("italic"),
        ],
    )
    .unwrap();
    builtin_add_face_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("bold"),
            Value::T,
        ],
    )
    .unwrap();
    let appended =
        builtin_get_text_property(&mut eval, vec![Value::fixnum(1), Value::symbol("face")])
            .unwrap();
    assert_eq!(
        appended,
        Value::list(vec![Value::symbol("italic"), Value::symbol("bold")])
    );

    let mut eval = eval_with_text("abc");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("face"),
            Value::symbol("italic"),
        ],
    )
    .unwrap();
    builtin_add_face_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("bold"),
            Value::NIL,
        ],
    )
    .unwrap();
    let prepended =
        builtin_get_text_property(&mut eval, vec![Value::fixnum(1), Value::symbol("face")])
            .unwrap();
    assert_eq!(
        prepended,
        Value::list(vec![Value::symbol("bold"), Value::symbol("italic")])
    );

    let mut eval = eval_with_text("abc");
    builtin_add_face_text_property(
        &mut eval,
        vec![Value::fixnum(1), Value::fixnum(3), Value::symbol("bold")],
    )
    .unwrap();
    builtin_add_face_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(3),
            Value::symbol("bold"),
            Value::T,
        ],
    )
    .unwrap();
    let duplicate =
        builtin_get_text_property(&mut eval, vec![Value::fixnum(1), Value::symbol("face")])
            .unwrap();
    assert_eq!(duplicate, Value::symbol("bold"));
}

#[test]
fn add_face_text_property_argument_contracts() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abc");

    let begin_err = builtin_add_face_text_property(
        &mut eval,
        vec![Value::string("1"), Value::fixnum(2), Value::symbol("bold")],
    )
    .unwrap_err();
    match begin_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("integer-or-marker-p"), Value::string("1")]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let object_err = builtin_add_face_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("bold"),
            Value::NIL,
            Value::T,
        ],
    )
    .unwrap_err();
    match object_err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("buffer-or-string-p"), Value::T]
            );
        }
        other => panic!("unexpected flow: {other:?}"),
    }

    let string_obj = builtin_add_face_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("bold"),
            Value::NIL,
            Value::string("abc"),
        ],
    )
    .unwrap();
    assert!(string_obj.is_nil());
}

// -----------------------------------------------------------------------
// remove-text-properties
// -----------------------------------------------------------------------

#[test]
fn remove_text_properties_basic() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(6),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();

    let props = Value::list(vec![Value::symbol("face"), Value::NIL]);
    builtin_remove_text_properties(&mut eval, vec![Value::fixnum(1), Value::fixnum(6), props])
        .unwrap();

    let result =
        builtin_get_text_property(&mut eval, vec![Value::fixnum(3), Value::symbol("face")]);
    assert!(result.as_ref().map_or(false, |v| v.is_nil()));
}

#[test]
fn set_text_properties_replaces_existing_values() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcd");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::symbol("p"),
            Value::symbol("v"),
        ],
    )
    .unwrap();

    let result = builtin_set_text_properties(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::list(vec![Value::symbol("q"), Value::symbol("z")]),
        ],
    )
    .unwrap();
    assert!(result.is_t());

    let q =
        builtin_get_text_property(&mut eval, vec![Value::fixnum(2), Value::symbol("q")]).unwrap();
    let p =
        builtin_get_text_property(&mut eval, vec![Value::fixnum(2), Value::symbol("p")]).unwrap();
    assert!(q.is_symbol_named("z"));
    assert!(p.is_nil());
}

#[test]
fn set_text_properties_preserves_replacement_plist_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abc");

    let props = Value::list(vec![
        Value::symbol("x"),
        Value::fixnum(1),
        Value::symbol("y"),
        Value::fixnum(2),
        Value::symbol("z"),
        Value::fixnum(3),
    ]);
    builtin_set_text_properties(&mut eval, vec![Value::fixnum(1), Value::fixnum(4), props])
        .unwrap();

    let observed =
        builtin_text_properties_at(&mut eval, vec![Value::fixnum(1)]).expect("plist lookup");
    assert_eq!(
        crate::emacs_core::print::print_value(&observed),
        "(x 1 y 2 z 3)"
    );
}

#[test]
fn set_text_properties_preserves_string_replacement_plist_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let string = Value::string("abc");

    let props = Value::list(vec![
        Value::symbol("x"),
        Value::fixnum(1),
        Value::symbol("y"),
        Value::fixnum(2),
        Value::symbol("z"),
        Value::fixnum(3),
    ]);
    builtin_set_text_properties(
        &mut eval,
        vec![Value::fixnum(0), Value::fixnum(3), props, string],
    )
    .unwrap();

    let observed = builtin_text_properties_at(&mut eval, vec![Value::fixnum(0), string])
        .expect("string plist lookup");
    assert_eq!(
        crate::emacs_core::print::print_value(&observed),
        "(x 1 y 2 z 3)"
    );
}

#[test]
fn set_text_properties_replaces_covered_string_intervals_with_one_run() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let string = Value::string("abcd");

    builtin_set_text_properties(
        &mut eval,
        vec![
            Value::fixnum(0),
            Value::fixnum(1),
            Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
            string,
        ],
    )
    .unwrap();
    builtin_set_text_properties(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(3),
            Value::list(vec![Value::symbol("help-echo"), Value::string("mid")]),
            string,
        ],
    )
    .unwrap();
    builtin_set_text_properties(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(3),
            Value::list(vec![Value::symbol("help-echo"), Value::string("mid")]),
            string,
        ],
    )
    .unwrap();

    let intervals = crate::emacs_core::value::get_string_text_properties_table_for_value(string)
        .unwrap()
        .intervals_snapshot();
    assert_eq!(intervals.len(), 2);
    assert_eq!((intervals[0].start, intervals[0].end), (0, 1));
    assert_eq!((intervals[1].start, intervals[1].end), (1, 3));
}

#[test]
fn set_text_properties_replaces_covered_buffer_intervals_with_one_run() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcd");

    builtin_set_text_properties(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
        ],
    )
    .unwrap();
    builtin_set_text_properties(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(4),
            Value::list(vec![Value::symbol("help-echo"), Value::string("mid")]),
        ],
    )
    .unwrap();
    builtin_set_text_properties(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::list(vec![Value::symbol("help-echo"), Value::string("mid")]),
        ],
    )
    .unwrap();

    let intervals = eval
        .buffers
        .current_buffer()
        .unwrap()
        .text_props_intervals_snapshot_for_test();
    assert_eq!(intervals.len(), 2);
    assert_eq!((intervals[0].start, intervals[0].end), (0, 1));
    assert_eq!((intervals[1].start, intervals[1].end), (1, 3));
}

#[test]
fn remove_list_of_text_properties_returns_t_only_when_changed() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcd");
    builtin_set_text_properties(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::list(vec![Value::symbol("q"), Value::symbol("z")]),
        ],
    )
    .unwrap();

    let first = builtin_remove_list_of_text_properties(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::list(vec![Value::symbol("q")]),
        ],
    )
    .unwrap();
    let second = builtin_remove_list_of_text_properties(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::list(vec![Value::symbol("q")]),
        ],
    )
    .unwrap();
    assert!(first.is_t());
    assert!(second.is_nil());
}

// -----------------------------------------------------------------------
// text-properties-at
// -----------------------------------------------------------------------

#[test]
fn text_properties_at_returns_plist() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(6),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();

    let result = builtin_text_properties_at(&mut eval, vec![Value::fixnum(2)]).unwrap();
    // Should be a plist with at least 'face 'bold.
    let items = list_to_vec(&result).unwrap();
    assert!(items.len() >= 2);
}

#[test]
fn text_properties_at_empty_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_text_properties_at(&mut eval, vec![Value::fixnum(1)]).unwrap();
    // Empty plist is nil.
    assert!(result.is_nil());
}

// -----------------------------------------------------------------------
// next-property-change
// -----------------------------------------------------------------------

#[test]
fn next_property_change_basic() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(6),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();

    // From position 1, next change should be at position 6.
    let result = builtin_next_property_change(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert!(result.is_fixnum());
}

#[test]
fn next_property_change_with_limit() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(6),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();

    // Limit at 4 — the actual change is at 6, so should return 4.
    let result = builtin_next_property_change(
        &mut eval,
        vec![Value::fixnum(1), Value::NIL, Value::fixnum(4)],
    )
    .unwrap();
    assert!(result.is_fixnum());
}

#[test]
fn next_property_change_no_change() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_next_property_change(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn property_change_limits_coerce_bignums_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abc");
    let positive_big = Value::bignum(Integer::from(1u64) << 100u32);
    let negative_big = Value::bignum(-(Integer::from(1u64) << 100u32));

    assert_eq!(
        builtin_next_char_property_change(&mut eval, vec![Value::fixnum(1), positive_big])
            .expect("next-char-property-change should clamp positive bignum limit")
            .as_fixnum(),
        Some(4)
    );
    assert_eq!(
        builtin_previous_char_property_change(&mut eval, vec![Value::fixnum(4), negative_big])
            .expect("previous-char-property-change should clamp negative bignum limit")
            .as_fixnum(),
        Some(1)
    );

    let s = Value::string("abc");
    let previous =
        builtin_previous_property_change(&mut eval, vec![Value::fixnum(3), s, negative_big])
            .expect("previous-property-change string limit should coerce negative bignum");
    assert!(previous.is_fixnum());
}

// -----------------------------------------------------------------------
// next-single-property-change
// -----------------------------------------------------------------------

#[test]
fn next_single_property_change_basic() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(6),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();

    let result = builtin_next_single_property_change(
        &mut eval,
        vec![Value::fixnum(1), Value::symbol("face")],
    )
    .unwrap();
    assert!(result.is_fixnum());
}

/// Regression guard for the interval-cursor rewrite of
/// next_single_property_change (locate-once + O(1) sibling stepping instead of a
/// find_id re-descent per boundary). Exercises the two edges the cursor walk must
/// get right: (1) COALESCING -- a boundary that changes a DIFFERENT property must
/// not be reported (the walk compares only the named property off each node's
/// plist); (2) the TRAILING implicit-nil region past the last interval.
#[test]
fn next_single_property_change_coalesces_and_reports_trailing_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("0123456789");
    let put = |eval: &mut _, s, e, k, v| {
        builtin_put_text_property(
            eval,
            vec![Value::fixnum(s), Value::fixnum(e), Value::symbol(k), v],
        )
        .unwrap();
    };
    // face=bold on [1,7) but split by an invisible-only interval at [4,7); then
    // face=italic on [7,11) to the buffer end.
    put(&mut eval, 1, 7, "face", Value::symbol("bold"));
    put(&mut eval, 4, 7, "invisible", Value::T);
    put(&mut eval, 7, 11, "face", Value::symbol("italic"));

    // From 1, the next `face` change must skip the [4,7) invisible boundary
    // (face is still bold there) and land at 7 where face becomes italic.
    let at7 = builtin_next_single_property_change(
        &mut eval,
        vec![Value::fixnum(1), Value::symbol("face")],
    )
    .unwrap();
    assert_eq!(
        at7.as_fixnum(),
        Some(7),
        "must coalesce past the invisible-only boundary"
    );

    // From 7, face=italic is constant to the buffer end, so next-single-property-change
    // returns nil (GNU: nil when the property never changes before point-max). This
    // exercises the cursor walk running to the last interval with no reported change.
    let at_end = builtin_next_single_property_change(
        &mut eval,
        vec![Value::fixnum(7), Value::symbol("face")],
    )
    .unwrap();
    assert!(
        at_end.is_nil(),
        "property constant to the buffer end must yield nil, got {at_end:?}"
    );
}

/// Backward mirror of the coalescing guard: previous-single-property-change now
/// walks intervals via a reverse cursor (prev_id) instead of a find_id re-descent
/// per boundary. Covers coalescing past an unrelated-property boundary and the
/// point-min (position 0) return.
#[test]
fn previous_single_property_change_coalesces_backward_and_reports_point_min() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("0123456789");
    let put = |eval: &mut _, s, e, k, v| {
        builtin_put_text_property(
            eval,
            vec![Value::fixnum(s), Value::fixnum(e), Value::symbol(k), v],
        )
        .unwrap();
    };
    put(&mut eval, 1, 7, "face", Value::symbol("bold"));
    put(&mut eval, 4, 7, "invisible", Value::T);
    put(&mut eval, 7, 11, "face", Value::symbol("italic"));

    // From 8 (face=italic), scanning back the previous face change is at 7.
    let at7 = builtin_previous_single_property_change(
        &mut eval,
        vec![Value::fixnum(8), Value::symbol("face")],
    )
    .unwrap();
    assert_eq!(
        at7.as_fixnum(),
        Some(7),
        "previous face change from 8 is at 7"
    );

    // From 6 (face=bold), the bold run coalesces past the invisible-only boundary
    // at 4 all the way to point-min, so previous-single-property-change is nil
    // (GNU: nil when the property is constant to the start). A walk that wrongly
    // stopped at the invisible boundary would instead return 4.
    let at_min = builtin_previous_single_property_change(
        &mut eval,
        vec![Value::fixnum(6), Value::symbol("face")],
    )
    .unwrap();
    assert!(
        at_min.is_nil(),
        "bold run constant to point-min must yield nil (coalesced past invisible), got {at_min:?}"
    );
}

#[test]
fn next_single_property_change_nil_when_none() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_next_single_property_change(
        &mut eval,
        vec![Value::fixnum(1), Value::symbol("face")],
    )
    .unwrap();
    assert!(result.is_nil());
}

// -----------------------------------------------------------------------
// previous-single-property-change
// -----------------------------------------------------------------------

#[test]
fn previous_single_property_change_basic() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(6),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();

    // From position 8 (past the propertized region), looking backward for 'face change.
    let result = builtin_previous_single_property_change(
        &mut eval,
        vec![Value::fixnum(8), Value::symbol("face")],
    )
    .unwrap();
    assert!(result.is_fixnum());
}

#[test]
fn previous_single_property_change_from_interval_end_boundary() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcd");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::symbol("p"),
            Value::symbol("v"),
        ],
    )
    .unwrap();

    let result = builtin_previous_single_property_change(
        &mut eval,
        vec![Value::fixnum(4), Value::symbol("p")],
    )
    .unwrap();
    assert!(result.is_fixnum());
}

#[test]
fn previous_single_property_change_does_not_escape_narrowing() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(progn
                 (insert "abcdefghijklmnopqrst")
                 (put-text-property 5 7 'p t)
                 (narrow-to-region 10 21)
                 (goto-char (point-min))
                 (previous-single-property-change (point) 'p))"#,
        )
        .expect("previous-single-property-change in a narrowed buffer");

    assert_eq!(result, Value::NIL);
}

// -----------------------------------------------------------------------
// text-property-any
// -----------------------------------------------------------------------

#[test]
fn text_property_any_found() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(6),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();

    let result = builtin_text_property_any(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(10),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();
    // Should find it at position 3.
    assert!(result.is_fixnum());
}

#[test]
fn text_property_any_not_found() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_text_property_any(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(6),
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn text_property_any_uses_live_marker_end_after_insertions() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(insert "abc")
           (let ((end (copy-marker (point-max) t)))
             (goto-char (point-max))
             (insert "Z")
             (put-text-property 4 5 'hard t)
             (text-property-any 1 end 'hard t))"#,
        )
        .expect("evaluation succeeds");
    assert_eq!(result, Value::fixnum(4));
}

#[test]
fn text_property_not_all_reports_first_mismatch() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcd");
    builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::symbol("p"),
            Value::symbol("v"),
        ],
    )
    .unwrap();

    let mismatch = builtin_text_property_not_all(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(5),
            Value::symbol("p"),
            Value::symbol("v"),
        ],
    )
    .unwrap();
    let no_mismatch = builtin_text_property_not_all(
        &mut eval,
        vec![
            Value::fixnum(2),
            Value::fixnum(4),
            Value::symbol("p"),
            Value::symbol("v"),
        ],
    )
    .unwrap();
    assert!(mismatch.is_fixnum());
    assert!(no_mismatch.is_nil());
}

// -----------------------------------------------------------------------
// make-overlay / delete-overlay
// -----------------------------------------------------------------------

#[test]
fn make_and_delete_overlay() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();

    assert!(ov.is_overlay());

    // Delete it.
    let result = builtin_delete_overlay(&mut eval, vec![ov]);
    assert!(result.is_ok());
}

// -----------------------------------------------------------------------
// overlay-put / overlay-get
// -----------------------------------------------------------------------

#[test]
fn overlay_put_and_get() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();

    builtin_overlay_put(
        &mut eval,
        vec![ov, Value::symbol("face"), Value::symbol("bold")],
    )
    .unwrap();

    let result = builtin_overlay_get(&mut eval, vec![ov, Value::symbol("face")]).unwrap();
    assert!(result.is_symbol_named("bold"));
}

#[test]
fn deleted_overlay_preserves_plist_and_identity() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();

    builtin_overlay_put(
        &mut eval,
        vec![ov, Value::symbol("face"), Value::symbol("bold")],
    )
    .unwrap();
    builtin_delete_overlay(&mut eval, vec![ov]).unwrap();

    let overlayp = builtin_overlayp(&mut eval, vec![ov]).unwrap();
    assert!(overlayp.is_t());

    let face = builtin_overlay_get(&mut eval, vec![ov, Value::symbol("face")]).unwrap();
    assert_eq!(face.as_symbol_name(), Some("bold"));

    let properties = builtin_overlay_properties(&mut eval, vec![ov]).unwrap();
    assert_eq!(
        crate::emacs_core::print::print_value(&properties),
        "(face bold)"
    );

    let start = builtin_overlay_start(&mut eval, vec![ov]).unwrap();
    let end = builtin_overlay_end(&mut eval, vec![ov]).unwrap();
    let buffer = builtin_overlay_buffer(&mut eval, vec![ov]).unwrap();
    assert!(start.is_nil());
    assert!(end.is_nil());
    assert!(buffer.is_nil());
}

#[test]
fn overlay_get_absent_property() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();

    let result = builtin_overlay_get(&mut eval, vec![ov, Value::symbol("missing")]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn overlay_get_uses_category_symbol_identity() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(let ((cat (make-symbol "overlay-category")))
                 (put cat 'oracle-prop 'from-category)
                 (insert "abc")
                 (let ((overlay (make-overlay 1 2)))
                   (overlay-put overlay 'category cat)
                   (overlay-get overlay 'oracle-prop)))"#,
        )
        .expect("evaluation succeeds");
    assert_eq!(result.as_symbol_name(), Some("from-category"));
}

// -----------------------------------------------------------------------
// overlayp
// -----------------------------------------------------------------------

#[test]
fn overlayp_true() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();

    let result = builtin_overlayp(&mut eval, vec![ov]).unwrap();
    assert!(result.is_t());
}

#[test]
fn overlayp_false() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_overlayp(&mut eval, vec![Value::fixnum(42)]).unwrap();
    assert!(result.is_nil());
}

// -----------------------------------------------------------------------
// overlays-at / overlays-in
// -----------------------------------------------------------------------

#[test]
fn overlays_at_finds_overlay() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let _ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();

    let result = builtin_overlays_at(&mut eval, vec![Value::fixnum(3)]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 1);
}

#[test]
fn overlays_at_outside() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let _ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();

    let result = builtin_overlays_at(&mut eval, vec![Value::fixnum(5)]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 0);
}

#[test]
fn overlays_at_unsorted_matches_gnu_same_start_itree_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let first = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();
    let second = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();
    let third = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();

    let result = builtin_overlays_at(&mut eval, vec![Value::fixnum(3)]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items, vec![third, second, first]);
}

#[test]
fn move_overlay_end_only_preserves_gnu_equal_start_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(progn
                 (insert "abcdef")
                 (let ((a (make-overlay 1 3))
                       (b (make-overlay 1 4)))
                   (move-overlay a 1 5)
                   (mapcar (lambda (overlay)
                             (if (eq overlay a) 'a 'b))
                           (overlays-at 2))))"#,
        )
        .expect("end-only move-overlay succeeds");

    let order: Vec<_> = list_to_vec(&result)
        .expect("overlays-at returns a proper list")
        .into_iter()
        .map(|value| value.as_symbol_name().expect("label is a symbol"))
        .collect();
    assert_eq!(order, vec!["b", "a"]);
}

#[test]
fn insert_reorders_equal_start_front_advancing_overlays_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(progn
                 (insert "abcdef")
                 (let ((a (make-overlay 1 4 nil t nil))
                       (b (make-overlay 1 4 nil t nil))
                       (c (make-overlay 1 4 nil t nil)))
                   (let ((label (lambda (overlay)
                                  (cond ((eq overlay a) 'a)
                                        ((eq overlay b) 'b)
                                        (t 'c)))))
                     (list (mapcar label (overlays-at 2))
                           (progn
                             (goto-char 1)
                             (insert "X")
                             (mapcar label (overlays-at 2)))))))"#,
        )
        .expect("insertion at front-advancing overlays succeeds");

    let observations = list_to_vec(&result).expect("result is a proper list");
    assert_eq!(observations.len(), 2);
    let labels = |value: &Value| {
        list_to_vec(value)
            .expect("overlay order is a proper list")
            .into_iter()
            .map(|item| item.as_symbol_name().expect("label is a symbol"))
            .collect::<Vec<_>>()
    };
    assert_eq!(labels(&observations[0]), vec!["c", "b", "a"]);
    assert_eq!(labels(&observations[1]), vec!["b", "c", "a"]);
}

#[test]
fn overlays_at_sorted_matches_gnu_same_range_identity_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let first = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();
    let second = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();

    let result = builtin_overlays_at(&mut eval, vec![Value::fixnum(3), Value::T]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items, vec![second, first]);
}

#[test]
fn overlays_at_sorted_returns_highest_priority_first() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let low = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();
    let high = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();
    let nil_priority =
        builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();

    builtin_overlay_put(
        &mut eval,
        vec![low, Value::symbol("priority"), Value::fixnum(1)],
    )
    .unwrap();
    builtin_overlay_put(
        &mut eval,
        vec![high, Value::symbol("priority"), Value::fixnum(10)],
    )
    .unwrap();

    let result = builtin_overlays_at(&mut eval, vec![Value::fixnum(3), Value::T]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items, vec![high, low, nil_priority]);
}

#[test]
fn overlays_in_basic() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();
    builtin_make_overlay(&mut eval, vec![Value::fixnum(4), Value::fixnum(10)]).unwrap();

    let result = builtin_overlays_in(&mut eval, vec![Value::fixnum(1), Value::fixnum(12)]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 2);
}

#[test]
fn overlays_in_empty_region_matches_gnu_boundary_rules() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcdef");
    builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();
    builtin_make_overlay(&mut eval, vec![Value::fixnum(3), Value::fixnum(3)]).unwrap();

    let at_start = builtin_overlays_in(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)])
        .expect("overlays-in at start");
    let inside = builtin_overlays_in(&mut eval, vec![Value::fixnum(3), Value::fixnum(3)])
        .expect("overlays-in inside");
    let at_end = builtin_overlays_in(&mut eval, vec![Value::fixnum(6), Value::fixnum(6)])
        .expect("overlays-in at end");

    assert!(at_start.is_nil());
    assert_eq!(list_to_vec(&inside).expect("inside list").len(), 2);
    assert!(at_end.is_nil());
}

#[test]
fn overlays_in_beg_greater_than_end_returns_nil_like_gnu() {
    // GNU `overlays_in` (buffer.c) treats BEG > END as an empty region and
    // returns no overlays (the interval-tree walk is empty and the first node
    // breaks on `node->begin > end`). Unlike `make-overlay`/`move-overlay`,
    // `overlays-in` must NOT swap the endpoints.
    //   (with-temp-buffer (insert "abcdef") (make-overlay 2 5)
    //                     (length (overlays-in 5 2)))  => 0   (GNU)
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcdef");
    builtin_make_overlay(&mut eval, vec![Value::fixnum(2), Value::fixnum(5)]).unwrap();

    // BEG > END: every reversed range is empty.
    for (beg, end) in [(5, 2), (6, 1), (4, 3)] {
        let result =
            builtin_overlays_in(&mut eval, vec![Value::fixnum(beg), Value::fixnum(end)]).unwrap();
        assert!(
            result.is_nil(),
            "(overlays-in {beg} {end}) must be nil like GNU, got {result:?}"
        );
    }

    // BEG == END and the normal BEG < END case still work.
    let equal = builtin_overlays_in(&mut eval, vec![Value::fixnum(3), Value::fixnum(3)]).unwrap();
    assert_eq!(list_to_vec(&equal).unwrap().len(), 1);
    let forward = builtin_overlays_in(&mut eval, vec![Value::fixnum(2), Value::fixnum(5)]).unwrap();
    assert_eq!(list_to_vec(&forward).unwrap().len(), 1);
}

#[test]
fn get_char_property_coerces_non_fixnum_overlay_priority_to_zero_like_gnu() {
    // GNU `make_sortvec_item` (buffer.c) only uses a priority value when it is a
    // FIXNUM (or a cons of fixnums); any other priority (float, bignum, string,
    // symbol) is treated as 0. So an overlay with a NON-fixnum priority loses to
    // an overlay carrying a positive fixnum priority.
    //   o1 priority 0 (fixnum) , o2 priority 1 (fixnum) -> face b   (GNU & neo)
    //   o1 priority 1.5 (float->0), o2 priority 1       -> face b   (defined)
    crate::test_utils::init_test_tracing();

    // `p1_make`/`p2_make` build the priority values inside the same `Context`
    // whose heap owns the resulting overlays, and we assert the winning face
    // before that `Context` (and its heap) is dropped — returning a heap Value
    // across `Context` boundaries would leave a dangling reference.
    fn winning_face_is<F1, F2>(p1_make: F1, p2_make: F2, expected: &str)
    where
        F1: FnOnce() -> Value,
        F2: FnOnce() -> Value,
    {
        let mut eval = eval_with_text("abcdef");
        let o1 = builtin_make_overlay(&mut eval, vec![Value::fixnum(2), Value::fixnum(5)]).unwrap();
        let o2 = builtin_make_overlay(&mut eval, vec![Value::fixnum(2), Value::fixnum(5)]).unwrap();
        builtin_overlay_put(
            &mut eval,
            vec![o1, Value::symbol("face"), Value::symbol("a")],
        )
        .unwrap();
        builtin_overlay_put(&mut eval, vec![o1, Value::symbol("priority"), p1_make()]).unwrap();
        builtin_overlay_put(
            &mut eval,
            vec![o2, Value::symbol("face"), Value::symbol("b")],
        )
        .unwrap();
        builtin_overlay_put(&mut eval, vec![o2, Value::symbol("priority"), p2_make()]).unwrap();
        let face =
            builtin_get_char_property(&mut eval, vec![Value::fixnum(3), Value::symbol("face")])
                .unwrap();
        assert!(
            face.is_symbol_named(expected),
            "expected face {expected}, got {face:?}"
        );
    }

    // Control: both fixnum, 0 vs 1 -> higher (b) wins. Matches GNU.
    winning_face_is(|| Value::fixnum(0), || Value::fixnum(1), "b");
    // Float priority is coerced to 0 (the defined GNU contract), so it ties
    // with priority 0 and loses to the fixnum-1 overlay -> face b.
    winning_face_is(|| Value::make_float(1.5), || Value::fixnum(1), "b");
    // A cons-of-fixnums priority is a defined high priority and wins.
    winning_face_is(
        || Value::cons(Value::fixnum(10), Value::fixnum(0)),
        || Value::fixnum(5),
        "a",
    );
}

#[test]
fn next_previous_overlay_change_boundaries() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcd");
    let no_overlay_next = builtin_next_overlay_change(&mut eval, vec![Value::fixnum(1)]).unwrap();
    let no_overlay_prev =
        builtin_previous_overlay_change(&mut eval, vec![Value::fixnum(4)]).unwrap();
    assert!(no_overlay_next.is_fixnum());
    assert!(no_overlay_prev.is_fixnum());

    builtin_make_overlay(&mut eval, vec![Value::fixnum(2), Value::fixnum(4)]).unwrap();
    let next_from_1 = builtin_next_overlay_change(&mut eval, vec![Value::fixnum(1)]).unwrap();
    let next_from_2 = builtin_next_overlay_change(&mut eval, vec![Value::fixnum(2)]).unwrap();
    let prev_from_4 = builtin_previous_overlay_change(&mut eval, vec![Value::fixnum(4)]).unwrap();
    let prev_from_2 = builtin_previous_overlay_change(&mut eval, vec![Value::fixnum(2)]).unwrap();
    assert!(next_from_1.is_fixnum());
    assert!(next_from_2.is_fixnum());
    assert!(prev_from_4.is_fixnum());
    assert!(prev_from_2.is_fixnum());
}

#[test]
fn overlay_change_boundaries_respect_narrowing_limits() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("abcdef");
    builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(1)]).unwrap();
    builtin_make_overlay(&mut eval, vec![Value::fixnum(2), Value::fixnum(2)]).unwrap();
    builtin_make_overlay(&mut eval, vec![Value::fixnum(4), Value::fixnum(4)]).unwrap();
    builtin_make_overlay(&mut eval, vec![Value::fixnum(7), Value::fixnum(7)]).unwrap();

    let buffer_id = eval.buffers.current_buffer_id().unwrap();
    eval.buffers
        .narrow_buffer_to_emacs_byte_range(buffer_id, EmacsByteRange::from_usize(1, 5))
        .unwrap();

    let next_at_zv = builtin_next_overlay_change(&mut eval, vec![Value::fixnum(6)]).unwrap();
    let prev_at_begv = builtin_previous_overlay_change(&mut eval, vec![Value::fixnum(2)]).unwrap();

    assert_eq!(next_at_zv.as_fixnum(), Some(6));
    assert_eq!(prev_at_begv.as_fixnum(), Some(2));
}

// -----------------------------------------------------------------------
// move-overlay
// -----------------------------------------------------------------------

#[test]
fn move_overlay_changes_range() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();

    builtin_move_overlay(&mut eval, vec![ov, Value::fixnum(3), Value::fixnum(8)]).unwrap();

    let start = builtin_overlay_start(&mut eval, vec![ov]).unwrap();
    let end = builtin_overlay_end(&mut eval, vec![ov]).unwrap();
    assert!(start.is_fixnum());
    assert!(end.is_fixnum());
}

#[test]
fn move_overlay_evaporates_zero_width_overlay() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();
    builtin_overlay_put(&mut eval, vec![ov, Value::symbol("evaporate"), Value::T]).unwrap();

    builtin_move_overlay(&mut eval, vec![ov, Value::fixnum(4), Value::fixnum(4)]).unwrap();

    let start = builtin_overlay_start(&mut eval, vec![ov]).unwrap();
    let end = builtin_overlay_end(&mut eval, vec![ov]).unwrap();
    let buffer = builtin_overlay_buffer(&mut eval, vec![ov]).unwrap();
    assert!(start.is_nil());
    assert!(end.is_nil());
    assert!(buffer.is_nil());
}

#[test]
fn move_deleted_evaporating_overlay_into_empty_range_keeps_it_deleted() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();
    builtin_overlay_put(&mut eval, vec![ov, Value::symbol("evaporate"), Value::T]).unwrap();
    builtin_delete_overlay(&mut eval, vec![ov]).unwrap();

    builtin_move_overlay(&mut eval, vec![ov, Value::fixnum(4), Value::fixnum(4)]).unwrap();

    let start = builtin_overlay_start(&mut eval, vec![ov]).unwrap();
    let end = builtin_overlay_end(&mut eval, vec![ov]).unwrap();
    let buffer = builtin_overlay_buffer(&mut eval, vec![ov]).unwrap();
    assert!(start.is_nil());
    assert!(end.is_nil());
    assert!(buffer.is_nil());
}

// -----------------------------------------------------------------------
// overlay-start / overlay-end
// -----------------------------------------------------------------------

#[test]
fn overlay_start_and_end() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(2), Value::fixnum(8)]).unwrap();

    let start = builtin_overlay_start(&mut eval, vec![ov]).unwrap();
    let end = builtin_overlay_end(&mut eval, vec![ov]).unwrap();
    assert!(start.is_fixnum());
    assert!(end.is_fixnum());
}

// -----------------------------------------------------------------------
// overlay-buffer
// -----------------------------------------------------------------------

#[test]
fn overlay_buffer_returns_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();

    let result = builtin_overlay_buffer(&mut eval, vec![ov]).unwrap();
    assert!(result.is_buffer());
}

// -----------------------------------------------------------------------
// overlay-properties
// -----------------------------------------------------------------------

#[test]
fn overlay_properties_returns_plist() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();

    builtin_overlay_put(
        &mut eval,
        vec![ov, Value::symbol("face"), Value::symbol("bold")],
    )
    .unwrap();
    builtin_overlay_put(
        &mut eval,
        vec![ov, Value::symbol("priority"), Value::fixnum(10)],
    )
    .unwrap();

    let result = builtin_overlay_properties(&mut eval, vec![ov]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 4); // 2 properties * 2 (key+value)
}

#[test]
fn overlay_properties_empty() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();

    let result = builtin_overlay_properties(&mut eval, vec![ov]).unwrap();
    // Empty plist is nil.
    assert!(result.is_nil());
}

// -----------------------------------------------------------------------
// remove-overlays
// -----------------------------------------------------------------------

#[test]
fn remove_overlays_all() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();
    builtin_make_overlay(&mut eval, vec![Value::fixnum(3), Value::fixnum(10)]).unwrap();

    builtin_remove_overlays(&mut eval, vec![]).unwrap();

    let result = builtin_overlays_in(&mut eval, vec![Value::fixnum(1), Value::fixnum(12)]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 0);
}

#[test]
fn remove_overlays_by_property() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let ov1 = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(6)]).unwrap();
    let ov2 = builtin_make_overlay(&mut eval, vec![Value::fixnum(3), Value::fixnum(10)]).unwrap();

    builtin_overlay_put(
        &mut eval,
        vec![ov1, Value::symbol("face"), Value::symbol("bold")],
    )
    .unwrap();
    builtin_overlay_put(
        &mut eval,
        vec![ov2, Value::symbol("face"), Value::symbol("italic")],
    )
    .unwrap();

    // Remove only overlays with face = bold.
    builtin_remove_overlays(
        &mut eval,
        vec![
            Value::NIL,
            Value::NIL,
            Value::symbol("face"),
            Value::symbol("bold"),
        ],
    )
    .unwrap();

    let result = builtin_overlays_in(&mut eval, vec![Value::fixnum(1), Value::fixnum(12)]).unwrap();
    let items = list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 1); // only the italic one remains
}

// -----------------------------------------------------------------------
// Wrong argument count tests
// -----------------------------------------------------------------------

#[test]
fn put_text_property_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_put_text_property(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]);
    assert!(result.is_err());
}

#[test]
fn put_text_property_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("face"),
            Value::symbol("bold"),
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn get_text_property_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_get_text_property(&mut eval, vec![]);
    assert!(result.is_err());
}

#[test]
fn get_text_property_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_get_text_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::symbol("face"),
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn get_char_property_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_get_char_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::symbol("face"),
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn get_char_property_and_overlay_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_get_char_property_and_overlay(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::symbol("face"),
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn get_display_property_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_get_display_property(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::symbol("face"),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn overlay_put_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_overlay_put(&mut eval, vec![Value::fixnum(42), Value::symbol("face")]);
    assert!(result.is_err());
}

#[test]
fn text_properties_at_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result =
        builtin_text_properties_at(&mut eval, vec![Value::fixnum(1), Value::NIL, Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn text_property_any_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_text_property_any(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("face"),
            Value::symbol("bold"),
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn text_property_not_all_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_text_property_not_all(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("face"),
            Value::symbol("bold"),
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn set_text_properties_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_set_text_properties(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn remove_list_of_text_properties_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_remove_list_of_text_properties(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn remove_overlays_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_remove_overlays(
        &mut eval,
        vec![Value::NIL, Value::NIL, Value::NIL, Value::NIL, Value::NIL],
    );
    assert!(result.is_err());
}

#[test]
fn make_overlay_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_make_overlay(&mut eval, vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn make_overlay_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_make_overlay(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn overlays_at_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_overlays_at(&mut eval, vec![Value::fixnum(1), Value::NIL, Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn next_overlay_change_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_next_overlay_change(&mut eval, vec![]);
    assert!(result.is_err());
}

#[test]
fn previous_overlay_change_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_previous_overlay_change(&mut eval, vec![Value::fixnum(1), Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn next_property_change_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_next_property_change(
        &mut eval,
        vec![Value::fixnum(1), Value::NIL, Value::NIL, Value::NIL],
    );
    assert!(result.is_err());
}

#[test]
fn next_single_property_change_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_next_single_property_change(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::symbol("face"),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn previous_single_property_change_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_previous_single_property_change(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::symbol("face"),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

#[test]
fn move_overlay_rejects_too_many_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let result = builtin_move_overlay(
        &mut eval,
        vec![
            Value::NIL,
            Value::fixnum(1),
            Value::fixnum(2),
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(result.is_err());
}

// -----------------------------------------------------------------------
// Integration: overlays with advance flags
// -----------------------------------------------------------------------

#[test]
fn overlay_front_advance() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    // Create overlay with front-advance = t
    let ov = builtin_make_overlay(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(8),
            Value::NIL, // buffer
            Value::T,   // front-advance
            Value::NIL, // rear-advance
        ],
    )
    .unwrap();

    // Verify overlay was created.
    let start = builtin_overlay_start(&mut eval, vec![ov]).unwrap();
    assert!(start.is_fixnum());
}

#[test]
fn overlay_rear_advance() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let ov = builtin_make_overlay(
        &mut eval,
        vec![
            Value::fixnum(3),
            Value::fixnum(8),
            Value::NIL,
            Value::NIL,
            Value::T, // rear-advance
        ],
    )
    .unwrap();

    let end = builtin_overlay_end(&mut eval, vec![ov]).unwrap();
    assert!(end.is_fixnum());
}

// -----------------------------------------------------------------------
// Edge cases
// -----------------------------------------------------------------------

#[test]
fn text_property_on_empty_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    // Scratch buffer is empty.
    let result =
        builtin_get_text_property(&mut eval, vec![Value::fixnum(1), Value::symbol("face")]);
    assert!(result.as_ref().map_or(false, |v| v.is_nil()));
}

#[test]
fn overlays_at_empty_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_overlays_at(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn delete_overlay_twice_is_ok() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let ov = builtin_make_overlay(&mut eval, vec![Value::fixnum(1), Value::fixnum(3)]).unwrap();

    builtin_delete_overlay(&mut eval, vec![ov]).unwrap();
    // Second delete should not crash.
    let result = builtin_delete_overlay(&mut eval, vec![ov]);
    assert!(result.is_ok());
}

// -----------------------------------------------------------------------
// A WINDOW object as the OBJECT arg resolves to the window's buffer
// (GNU textprop.c / editfns.c window handling), instead of signalling
// (wrong-type-argument buffer-or-string-p #<window>).
// -----------------------------------------------------------------------

/// Build an evaluator with a frame+window over a `*scratch*` buffer so that
/// `(selected-window)` yields a live window Value backed by the current buffer.
fn eval_with_frame_window() -> Context {
    let mut eval = Context::new();
    let buf = eval.buffers.create_buffer("*scratch*");
    eval.buffers.set_current(buf);
    eval.frames.create_frame("F1", 800, 600, buf);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&eval);
    eval
}

#[test]
fn get_pos_property_resolves_window_object_to_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_frame_window();
    // Insert text and put a `face` property covering positions 1..6.
    eval.eval_str_each(
        r#"(progn
             (insert "hello world")
             (put-text-property 1 6 'face 'bold)
             (put-text-property 1 6 'front-sticky t))"#,
    );

    // With a WINDOW object, get-pos-property must resolve to the window's
    // buffer and return the (front-sticky) property, NOT signal.
    let results = eval
        .eval_str_each("(get-pos-property 2 'face (selected-window))")
        .into_iter()
        .map(|r| crate::emacs_core::error::format_eval_result(&r))
        .collect::<Vec<_>>();
    assert_eq!(results, vec!["OK bold".to_string()]);
}

#[test]
fn next_single_char_property_change_resolves_window_object_to_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_frame_window();
    eval.eval_str_each(
        r#"(progn
             (insert "hello world")
             (put-text-property 1 6 'face 'bold))"#,
    );

    // With a WINDOW object, next-single-char-property-change must resolve to
    // the window's buffer and find the boundary at position 6, NOT signal.
    let results = eval
        .eval_str_each("(next-single-char-property-change 1 'face (selected-window))")
        .into_iter()
        .map(|r| crate::emacs_core::error::format_eval_result(&r))
        .collect::<Vec<_>>();
    assert_eq!(results, vec!["OK 6".to_string()]);
}

#[test]
fn previous_single_char_property_change_resolves_window_object_to_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_frame_window();
    eval.eval_str_each(
        r#"(progn
             (insert "hello world")
             (put-text-property 1 6 'face 'bold))"#,
    );

    let results = eval
        .eval_str_each("(previous-single-char-property-change 11 'face (selected-window))")
        .into_iter()
        .map(|r| crate::emacs_core::error::format_eval_result(&r))
        .collect::<Vec<_>>();
    assert_eq!(results, vec!["OK 6".to_string()]);
}

// -----------------------------------------------------------------------
// GNU position validation in the text-property family
//
// Expectations below were MEASURED by running tmp/coord-textprop-range-probe*.el
// under GNU Emacs 31.0.90, never derived.  GNU has two out-of-range shapes for
// a single text-property position and they are not interchangeable:
//
//   validate_interval_range          src/textprop.c:141,158 -- a point call
//     passes `&position' as BOTH `begin' and `end', so `args_out_of_range
//     (begin0, end0)' carries the position TWICE.
//   get_char_property_and_overlay    src/textprop.c:642-644 -- its own bounds
//     check uses `xsignal1', carrying the position ONCE.
//
// Which one a builtin gets is not a style choice; it is which GNU function the
// builtin goes through.
//
// GNU prints the whole error object, `(args-out-of-range 500 500)';
// `format_eval_result' prints the signal symbol and then its DATA as a list,
// `ERR (args-out-of-range (500 500))'.  Same signal, different rendering -- the
// payload to compare against GNU is what is inside the inner parens.
// -----------------------------------------------------------------------

#[test]
fn previous_property_change_out_of_range_signal_uses_gnu_point_range_payload() {
    // GNU `Fprevious_property_change' validates through
    // `validate_interval_range (object, &position, &position, soft)'
    // (src/textprop.c:1090), so the payload is the position twice.  The buffer
    // branch used to validate through the `get-char-property' shape instead and
    // signalled it once.
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let results = eval
        .eval_str_each(
            r#"(previous-property-change 500)
               (previous-char-property-change 500)
               (previous-char-property-change -7)
               (previous-property-change 500 "abc")"#,
        )
        .into_iter()
        .map(|r| crate::emacs_core::error::format_eval_result(&r))
        .collect::<Vec<_>>();
    assert_eq!(
        results,
        vec![
            "ERR (args-out-of-range (500 500))".to_string(),
            "ERR (args-out-of-range (500 500))".to_string(),
            "ERR (args-out-of-range (-7 -7))".to_string(),
            "ERR (args-out-of-range (500 500))".to_string(),
        ]
    );
}

#[test]
fn get_pos_property_validates_position_through_its_stickiness_reads() {
    // GNU `Fget_pos_property' has no bounds check of its own (src/editfns.c:285).
    // Its out-of-range signal is EMERGENT: `text_property_stickiness' reads text
    // properties twice through `Fget_text_property' -- at POS-1
    // (src/textprop.c:1919) and at POS (src/textprop.c:1931, whose comment says
    // "This signals an arg-out-of-range error if pos is outside the buffer's
    // accessible range") -- and each of those validates.  So WHICH position
    // appears in the error follows GNU's branch structure: the POS-1 read is
    // skipped when POS <= BEGV or when PROP is default-nonsticky
    // (src/textprop.c:1912-1914), and then POS names itself.
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello");
    let results = eval
        .eval_str_each(
            r#"(setq text-property-default-nonsticky
                     '((composition . t) (syntax-table . t) (display . t)))
               (get-pos-property 500 'face)
               (get-pos-property 500 'syntax-table)
               (get-pos-property 500 'display)
               (get-pos-property 0 'face)
               (get-pos-property -7 'face)
               (get-pos-property 7 'face)
               (progn (put-text-property 2 4 'face 'bold) nil)
               (get-pos-property 1 'face)
               (get-pos-property 2 'face)
               (get-pos-property 3 'face)
               (get-pos-property 4 'face)
               (get-pos-property 6 'face)"#,
        )
        .into_iter()
        .map(|r| crate::emacs_core::error::format_eval_result(&r))
        .collect::<Vec<_>>();
    assert_eq!(
        results,
        vec![
            "OK ((composition . t) (syntax-table . t) (display . t))".to_string(),
            // PROP is not default-nonsticky and POS > BEGV, so the POS-1 read
            // runs first and names 499.
            "ERR (args-out-of-range (499 499))".to_string(),
            // Default-nonsticky PROPs skip the POS-1 read entirely.
            "ERR (args-out-of-range (500 500))".to_string(),
            "ERR (args-out-of-range (500 500))".to_string(),
            // POS <= BEGV also skips the POS-1 read.
            "ERR (args-out-of-range (0 0))".to_string(),
            "ERR (args-out-of-range (-7 -7))".to_string(),
            // POS-1 == 6 == ZV is in range, so the POS read is the one to fail.
            "ERR (args-out-of-range (7 7))".to_string(),
            "OK nil".to_string(),
            // In-range answers are unchanged by validation.
            "OK nil".to_string(),
            "OK nil".to_string(),
            "OK bold".to_string(),
            "OK bold".to_string(),
            "OK nil".to_string(),
        ]
    );
}

#[test]
fn get_pos_property_validates_against_the_accessible_portion() {
    // `validate_interval_range' bounds against BEGV/ZV, not BEG/Z
    // (src/textprop.c:156-157), and BEGV is also what decides whether the POS-1
    // read happens at all (src/textprop.c:1912), so narrowing moves both.
    crate::test_utils::init_test_tracing();
    let mut eval = eval_with_text("hello world");
    let results = eval
        .eval_str_each(
            r#"(progn (narrow-to-region 3 6) nil)
               (get-pos-property 3 'face)
               (get-pos-property 2 'face)
               (get-pos-property 1 'face)
               (get-pos-property 9 'face)"#,
        )
        .into_iter()
        .map(|r| crate::emacs_core::error::format_eval_result(&r))
        .collect::<Vec<_>>();
    assert_eq!(
        results,
        vec![
            "OK nil".to_string(),
            "OK nil".to_string(),
            // POS <= BEGV: the POS-1 read is skipped and POS names itself.
            "ERR (args-out-of-range (2 2))".to_string(),
            "ERR (args-out-of-range (1 1))".to_string(),
            // POS-1 == 8 is outside the restriction even though it exists in
            // the buffer, so 8 is what fails.
            "ERR (args-out-of-range (8 8))".to_string(),
        ]
    );
}

#[test]
fn text_property_default_nonsticky_carries_the_composition_entry() {
    // GNU builds this default in two places: `syms_of_textprop' seeds it with
    // syntax-table and display (src/textprop.c:2428-2429), and
    // `syms_of_composite' CONSES composition onto the front
    // (src/composite.c:2212-2213).  Porting only the first leaves composition
    // sticky, so text inserted next to a composed sequence inherits a
    // composition that GNU would not give it.
    crate::test_utils::init_test_tracing();
    assert_eq!(
        crate::test_utils::runtime_startup_eval_one("text-property-default-nonsticky"),
        "OK ((composition . t) (syntax-table . t) (display . t))"
    );
}

/// One `textget` snapshot + tree-order interval walk (GNU
/// `Fnext_single_property_change`): expectations are GNU 31's answers for the
/// same forms (tmp/rr/nspc-probe.el), covering the trailing implicit-nil
/// region, LIMIT before/at/after the change, `category` indirection (equal and
/// different), `default-text-properties`, `char-property-alias-alist`, and the
/// buffer path with narrowing and two runs.
#[test]
fn next_single_property_change_matches_gnu_across_fallbacks_and_limits() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str("(put 'my-cat 'face 'italic)").unwrap();
    let cases: &[(&str, Value)] = &[
        (
            "(let ((s (concat (propertize \"abc\" 'face 'bold) \"defg\"))) (next-single-property-change 0 'face s))",
            Value::fixnum(3),
        ),
        (
            "(let ((s (concat (propertize \"abc\" 'face 'bold) \"defg\"))) (next-single-property-change 3 'face s))",
            Value::NIL,
        ),
        (
            "(let ((s (concat (propertize \"abc\" 'face 'bold) \"defg\"))) (next-single-property-change 0 'face s 2))",
            Value::fixnum(2),
        ),
        (
            "(let ((s (concat (propertize \"abc\" 'face 'bold) \"defg\"))) (next-single-property-change 0 'face s 3))",
            Value::fixnum(3),
        ),
        (
            "(let ((s (concat (propertize \"abc\" 'face 'bold) \"defg\"))) (next-single-property-change 0 'face s 5))",
            Value::fixnum(3),
        ),
        (
            "(next-single-property-change 0 'face \"plain\")",
            Value::NIL,
        ),
        (
            "(next-single-property-change 0 'face \"plain\" 4)",
            Value::fixnum(4),
        ),
        (
            "(let ((s (concat (propertize \"ab\" 'category 'my-cat) (propertize \"cd\" 'face 'italic) \"ef\"))) (next-single-property-change 0 'face s))",
            Value::fixnum(4),
        ),
        (
            "(let ((s (concat (propertize \"ab\" 'category 'my-cat) (propertize \"cd\" 'face 'bold) \"ef\"))) (next-single-property-change 0 'face s))",
            Value::fixnum(2),
        ),
        (
            "(let ((default-text-properties '(face bold))) (let ((s (concat (propertize \"ab\" 'face 'bold) \"cd\" (propertize \"ef\" 'face 'italic)))) (next-single-property-change 0 'face s)))",
            Value::fixnum(4),
        ),
        (
            "(let ((char-property-alias-alist '((face my-alias)))) (let ((s (concat (propertize \"ab\" 'face 'bold) (propertize \"cd\" 'my-alias 'bold) \"ef\"))) (next-single-property-change 0 'face s)))",
            Value::fixnum(4),
        ),
        (
            "(with-temp-buffer (insert (propertize \"abc\" 'face 'bold) \"defgh\") (next-single-property-change 1 'face))",
            Value::fixnum(4),
        ),
        (
            "(with-temp-buffer (insert (propertize \"abc\" 'face 'bold) \"defgh\") (next-single-property-change 4 'face))",
            Value::NIL,
        ),
        (
            "(with-temp-buffer (insert (propertize \"abc\" 'face 'bold) \"defgh\") (next-single-property-change 1 'face nil 3))",
            Value::fixnum(3),
        ),
        (
            "(with-temp-buffer (insert (propertize \"abc\" 'face 'bold) \"defgh\") (narrow-to-region 1 3) (next-single-property-change 1 'face))",
            Value::NIL,
        ),
        (
            "(equal (with-temp-buffer (insert (propertize \"abc\" 'face 'bold) \"defgh\") (put-text-property 6 8 'face 'bold) (list (next-single-property-change 1 'face) (next-single-property-change 4 'face))) '(4 6))",
            Value::T,
        ),
    ];
    for (form, expected) in cases {
        let got = eval.eval_str(form).unwrap();
        assert_eq!(&got, expected, "{form}");
    }
}

/// Oracle div_cx19 (and 40/43/46): a plist cons already returned by
/// `text-properties-at' must not be mutated by the property-change undo
/// entry that follows an undo re-insert at the interval's end.  GNU re-homes
/// the preceding interval's plist in `graft_intervals_into_buffer' (its
/// insert-adjust stretched that interval, so the graft splits it); our
/// boundary-shaped graft must re-home the predecessor the same way.
#[test]
fn graft_at_boundary_rehomes_predecessor_so_held_plists_survive_undo() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let got = eval
        .eval_str(
            "(equal
  (with-temp-buffer
    (buffer-enable-undo)
    (insert \"0123456789\")
    (put-text-property 1 5 'face 'bold)
    (let ((ov (make-overlay 3 7))) (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t))
    (undo-boundary)
    (delete-region 2 8)
    (let ((captured (text-properties-at 1)))
      (undo)
      (list captured (buffer-string))))
  '((face bold) \"\"))",
        )
        .unwrap();
    assert_eq!(got, Value::T);
}
