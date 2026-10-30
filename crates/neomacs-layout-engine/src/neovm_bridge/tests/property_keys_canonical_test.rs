//! Real resolver outputs precede the allocation-site assertion.
use super::property_keys_test_support::{self as probes, Guard};
use super::*;

#[test]
fn direct_mouse_face_extent_does_not_scan_distant_categories() {
    use neovm_core::tagged::collection_reads::capture;
    let mut eval = Context::new();
    let id = eval.buffer_manager().current_buffer().unwrap().id();
    let buffer = eval.buffer_manager_mut().get_mut(id).unwrap();
    buffer.insert(&"x".repeat(9_000));
    for index in 0..9_000 {
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(index), EmacsBytePos::new(index + 1)),
            Value::symbol("history-event"),
            Value::fixnum(index as i64),
        );
    }
    let button = EmacsByteRange::new(EmacsBytePos::new(4_500), EmacsBytePos::new(4_510));
    buffer.text_props_put_property_in_emacs_byte_range(
        button,
        Value::symbol("mouse-face"),
        Value::symbol("highlight"),
    );
    buffer.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::new(EmacsBytePos::ZERO, EmacsBytePos::new(1)),
        Value::symbol("category"),
        Value::symbol("history-category"),
    );
    let bounds = EmacsByteRange::new(EmacsBytePos::ZERO, buffer.point_max_emacs_byte_pos());
    let lookup = LayoutCharPropertyLookup::new(buffer, Value::symbol("mouse-face"));
    let (extent, reads) =
        capture(|| lookup.effective_text_extent_at(buffer, EmacsBytePos::new(4_505), bounds));
    assert_eq!(extent, Some(button));
    assert!(
        reads.is_some(),
        "one visible button must not exhaust the layout dependency budget"
    );
}

#[test]
fn effective_extent_crosses_equal_direct_alias_and_default_runs() {
    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (insert "α好cdefgh")
             (put-text-property 2 4 'mouse-face 'highlight)
             (put-text-property 4 6 'alternate-face 'highlight)
             (put-text-property 7 8 'mouse-face nil))"#,
    )
    .unwrap();
    let id = eval.buffer_manager().current_buffer().unwrap().id();
    let buffer = eval.buffer_manager_mut().get_mut(id).unwrap();
    buffer.set_buffer_local(
        "char-property-alias-alist",
        Value::list(vec![Value::list(vec![
            Value::symbol("mouse-face"),
            Value::symbol("alternate-face"),
        ])]),
    );
    buffer.set_buffer_local(
        "default-text-properties",
        Value::list(vec![
            Value::symbol("mouse-face"),
            Value::symbol("highlight"),
        ]),
    );
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let bounds = EmacsByteRange::new(EmacsBytePos::ZERO, buffer.point_max_emacs_byte_pos());
    let lookup = LayoutCharPropertyLookup::new(buffer, Value::symbol("mouse-face"));
    for position in 0..8 {
        let pos = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(position));
        let expected = match position {
            0..=5 => Some(EmacsByteRange::new(
                EmacsBytePos::ZERO,
                buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(6)),
            )),
            6 => None,
            _ => Some(EmacsByteRange::new(pos, bounds.end())),
        };
        assert_eq!(
            lookup.effective_text_extent_at(buffer, pos, bounds),
            expected
        );
    }
}

#[test]
fn canonical_property_key_inline_skips_heap_after_effective_lookup_and_extent() {
    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
    buffer.insert("abcdefgh\n");
    buffer.set_buffer_local("char-property-alias-alist", Value::NIL);
    buffer.set_buffer_local(
        "default-text-properties",
        Value::list(vec![Value::symbol("face"), Value::symbol("default")]),
    );
    assert!(buffer.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::new(EmacsBytePos::new(1), EmacsBytePos::new(5)),
        Value::symbol("face"),
        Value::symbol("highlight")
    ));
    let bounds = EmacsByteRange::new(EmacsBytePos::ZERO, buffer.point_max_emacs_byte_pos());
    let observe = |enabled| {
        let _scope = Guard::set(enabled);
        let lookup = LayoutCharPropertyLookup::new(buffer, Value::symbol("face"));
        let keys = lookup.lookup_order.ordered().collect::<Vec<_>>();
        let values: Vec<_> = (0..8)
            .map(|pos| {
                lookup
                    .text_value_at(buffer, EmacsBytePos::new(pos))
                    .map(Value::bits)
            })
            .collect();
        let extents: Vec<_> = (0..8)
            .map(|pos| lookup.effective_text_extent_at(buffer, EmacsBytePos::new(pos), bounds))
            .collect();
        (keys, values, extents, probes::counts())
    };
    let off = observe(false);
    let on = observe(true);
    assert_eq!(off.0, vec![Value::symbol("face")]);
    assert_eq!(on.0, off.0, "ordered canonical keys");
    assert_eq!(on.1, off.1, "direct and default values");
    assert_eq!(
        on.2, off.2,
        "complete effective extents including category watches"
    );
    assert_eq!(off.3.heap_materializations, 1);
    // Proposed constructor-counter RED on the actual future G29-based test-only predecessor,
    // after all resolver/extent comparisons above; no execution is claimed.
    assert_eq!(on.3.heap_materializations, 0);
    assert_eq!(on.3.inline_constructions, 1);
}

#[test]
fn unknown_property_key_keeps_default_and_explicit_nil_semantics() {
    let mut eval = Context::new();
    let property = Value::symbol("d5-unknown-property");
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
    buffer.insert("abcd\n");
    buffer.set_buffer_local(
        "default-text-properties",
        Value::list(vec![property, Value::symbol("d5-default-value")]),
    );
    assert!(buffer.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::new(EmacsBytePos::new(0), EmacsBytePos::new(1)),
        property,
        Value::NIL
    ));
    for enabled in [false, true] {
        let _scope = Guard::set(enabled);
        let lookup = LayoutCharPropertyLookup::new(buffer, property);
        assert_eq!(
            lookup.lookup_order.ordered().collect::<Vec<_>>(),
            vec![property]
        );
        assert_eq!(
            lookup.text_value_at(buffer, EmacsBytePos::ZERO),
            Some(Value::NIL)
        );
        assert_eq!(
            lookup.text_value_at(buffer, EmacsBytePos::new(2)),
            Some(Value::symbol("d5-default-value"))
        );
    }
}

#[test]
fn fallback_extent_over_long_history_preserves_capture_budget() {
    use neovm_core::tagged::collection_reads::capture;
    let mut eval = Context::new();
    let id = eval.buffer_manager().current_buffer().unwrap().id();
    let buffer = eval.buffer_manager_mut().get_mut(id).unwrap();
    let count = 9_000;
    buffer.insert(&"x".repeat(count));
    for index in 0..count {
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(index), EmacsBytePos::new(index + 1)),
            Value::symbol("history-event"),
            Value::fixnum(index as i64),
        );
    }
    // Sparse category: it defeats the whole-table "no watched key present"
    // shortcut, but the syntax-run path must still avoid every history plist.
    buffer.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::new(EmacsBytePos::ZERO, EmacsBytePos::new(1)),
        Value::symbol("category"),
        Value::symbol("history-category"),
    );
    // No direct mouse-face anywhere: the value comes from the buffer default,
    // so the extent walk cannot use the direct-canonical fast path.
    buffer.set_buffer_local(
        "default-text-properties",
        Value::list(vec![
            Value::symbol("mouse-face"),
            Value::symbol("highlight"),
        ]),
    );
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let bounds = EmacsByteRange::new(EmacsBytePos::ZERO, buffer.point_max_emacs_byte_pos());
    let lookup = LayoutCharPropertyLookup::new(buffer, Value::symbol("mouse-face"));
    let pos = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(4_500));
    let (extent, reads) = capture(|| lookup.effective_text_extent_at(buffer, pos, bounds));
    assert_eq!(extent, Some(bounds));
    assert!(
        reads.is_some(),
        "a default-supplied value must not scan every unrelated plist boundary"
    );
}

#[test]
fn alternating_category_query_near_end_preserves_capture_budget() {
    use neovm_core::tagged::collection_reads::capture;
    let mut eval = Context::new();
    let id = eval.buffer_manager().current_buffer().unwrap().id();
    let buffer = eval.buffer_manager_mut().get_mut(id).unwrap();
    let count = 9_000;
    buffer.insert(&"x".repeat(count));
    for index in 0..count {
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(index), EmacsBytePos::new(index + 1)),
            Value::symbol("category"),
            Value::symbol(if index % 2 == 0 {
                "category-even"
            } else {
                "category-odd"
            }),
        );
    }
    eval.eval_str(
        "(progn (put 'category-even 'mouse-face 'highlight)
                (put 'category-odd 'mouse-face 'other))",
    )
    .unwrap();
    let snapshot = {
        let buffer = eval.buffer_manager().get(id).unwrap();
        LayoutBufferSnapshot::from_buffer_with_obarray(buffer, eval.variables())
    };
    let bounds = EmacsByteRange::new(EmacsBytePos::ZERO, EmacsBytePos::new(count));
    let lookup = LayoutCharPropertyLookup::new(&snapshot, Value::symbol("mouse-face"));
    // The last character is category-odd (mouse-face `other`); its neighbours
    // are category-even (`highlight`), so the extent is exactly one character.
    let pos = EmacsBytePos::new(count - 1);
    let (extent, reads) = capture(|| lookup.effective_text_extent_at(&snapshot, pos, bounds));
    assert_eq!(
        extent,
        Some(EmacsByteRange::new(
            EmacsBytePos::new(count - 1),
            EmacsBytePos::new(count)
        ))
    );
    assert!(
        reads.is_some(),
        "alternating categories near the end must not force a scan from buffer start"
    );
}

#[test]
fn explicit_nil_category_presence_splits_runs_without_losing_the_extent() {
    use neovm_core::tagged::collection_reads::capture;
    let mut eval = Context::new();
    let id = eval.buffer_manager().current_buffer().unwrap().id();
    let buffer = eval.buffer_manager_mut().get_mut(id).unwrap();
    buffer.insert("abcdef");
    // One constant syntax-table over the whole text: a value-only syntax run
    // would coalesce the explicit-nil category interval with its neighbours.
    buffer.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::new(EmacsBytePos::ZERO, EmacsBytePos::new(6)),
        Value::symbol("syntax-table"),
        Value::T,
    );
    buffer.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::new(EmacsBytePos::new(1), EmacsBytePos::new(3)),
        Value::symbol("category"),
        Value::NIL,
    );
    buffer.set_buffer_local(
        "default-text-properties",
        Value::list(vec![
            Value::symbol("mouse-face"),
            Value::symbol("highlight"),
        ]),
    );
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let bounds = EmacsByteRange::new(EmacsBytePos::ZERO, buffer.point_max_emacs_byte_pos());
    let lookup = LayoutCharPropertyLookup::new(buffer, Value::symbol("mouse-face"));
    let pos = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(1));
    let (extent, reads) = capture(|| lookup.effective_text_extent_at(buffer, pos, bounds));
    // Category is nil (not a symbol with a plist), so the effective value is
    // still the default highlight across the presence transition.
    assert_eq!(extent, Some(bounds));
    assert!(
        reads.is_some(),
        "an explicit-nil category boundary must not exhaust the layout budget"
    );
}
