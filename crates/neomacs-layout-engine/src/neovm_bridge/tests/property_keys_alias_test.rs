//! Native property-order, effective-value and extent preservation controls.
//! Every observation is made with the same exclusively active Context alive;
//! returned handles never outlive that Context or switch to another heap.
use super::property_keys_test_support::{self as probes, Guard};
use super::*;

#[test]
fn property_key_aliases_keep_first_matching_order_duplicates_and_heap_fallback() {
    let mut eval = Context::new();
    let face = Value::symbol("face");
    let alias = Value::symbol("font-lock-face");
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
    buffer.insert("abcd\n");
    buffer.set_buffer_local(
        "char-property-alias-alist",
        Value::list(vec![
            Value::make_int(3),
            Value::list(vec![
                Value::symbol("other-property"),
                Value::symbol("ignored"),
            ]),
            Value::list(vec![face, face, alias, alias, Value::NIL, Value::T]),
            Value::list(vec![face, Value::symbol("ignored-second-entry")]),
        ]),
    );
    assert!(buffer.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::new(EmacsBytePos::new(0), EmacsBytePos::new(4)),
        alias,
        Value::symbol("highlight")
    ));
    let bounds = EmacsByteRange::new(EmacsBytePos::ZERO, buffer.point_max_emacs_byte_pos());
    let observe = |enabled| {
        let _scope = Guard::set(enabled);
        let lookup = LayoutCharPropertyLookup::new(buffer, face);
        let keys = lookup.lookup_order.ordered().collect::<Vec<_>>();
        let values: Vec<_> = (0..5)
            .map(|pos| {
                lookup
                    .text_value_at(buffer, EmacsBytePos::new(pos))
                    .map(Value::bits)
            })
            .collect();
        let extents: Vec<_> = (0..5)
            .map(|pos| lookup.effective_text_extent_at(buffer, EmacsBytePos::new(pos), bounds))
            .collect();
        (keys, values, extents, probes::counts())
    };
    let off = observe(false);
    let on = observe(true);
    assert_eq!(off.0, vec![face, alias, Value::NIL, Value::T]);
    assert_eq!(off.1[2], Some(Value::symbol("highlight").bits()));
    assert_eq!(
        on.0, off.0,
        "complete canonical-first order and first-match entry"
    );
    assert_eq!(
        on.1, off.1,
        "all effective values including the final newline"
    );
    assert_eq!(
        on.2, off.2,
        "all effective extents before allocation witnesses"
    );
    assert_eq!(off.3.heap_materializations, 1);
    assert_eq!(off.3.inline_constructions, 0);
    assert_eq!(off.3.alias_upgrades, 0);
    assert_eq!(
        on.3.heap_materializations, 1,
        "real distinct-alias heap fallback"
    );
    // On a test-only predecessor this fails after the semantic comparisons:
    // the old Vec path does not record an actual inline-to-heap alias upgrade.
    assert_eq!(
        on.3.alias_upgrades, 1,
        "actual first distinct alias upgrade"
    );
    assert_eq!(on.3.inline_constructions, 1);
}

#[test]
fn duplicate_canonical_aliases_stay_inline_after_complete_effective_extent_lookup() {
    let mut eval = Context::new();
    let face = Value::symbol("face");
    let alias = Value::symbol("font-lock-face");
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
    buffer.insert("abcdef\n");
    buffer.set_buffer_local(
        "char-property-alias-alist",
        Value::list(vec![
            Value::make_int(3),
            Value::cons(
                face,
                Value::cons(face, Value::cons(face, Value::make_int(7))),
            ),
            Value::list(vec![face, alias]),
        ]),
    );
    buffer.set_buffer_local(
        "default-text-properties",
        Value::list(vec![face, Value::symbol("d5-default-face")]),
    );
    assert!(buffer.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::new(EmacsBytePos::new(0), EmacsBytePos::new(6)),
        alias,
        Value::symbol("d5-ignored-alias-face")
    ));
    assert!(buffer.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::new(EmacsBytePos::new(0), EmacsBytePos::new(1)),
        face,
        Value::NIL
    ));
    assert!(buffer.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::new(EmacsBytePos::new(1), EmacsBytePos::new(4)),
        face,
        Value::symbol("highlight")
    ));
    let bounds = EmacsByteRange::new(EmacsBytePos::ZERO, buffer.point_max_emacs_byte_pos());
    let observe = |enabled| {
        let _scope = Guard::set(enabled);
        let lookup = LayoutCharPropertyLookup::new(buffer, face);
        let keys = lookup.lookup_order.ordered().collect::<Vec<_>>();
        let values: Vec<_> = (0..7)
            .map(|pos| {
                lookup
                    .text_value_at(buffer, EmacsBytePos::new(pos))
                    .map(Value::bits)
            })
            .collect();
        let extents: Vec<_> = (0..7)
            .map(|pos| lookup.effective_text_extent_at(buffer, EmacsBytePos::new(pos), bounds))
            .collect();
        (keys, values, extents, probes::counts())
    };
    let off = observe(false);
    let on = observe(true);
    assert_eq!(off.0, vec![face]);
    assert_eq!(off.1[0], Some(Value::NIL.bits()));
    assert_eq!(off.1[2], Some(Value::symbol("highlight").bits()));
    assert_eq!(off.1[5], Some(Value::symbol("d5-default-face").bits()));
    assert_eq!(
        on.0, off.0,
        "duplicate canonical keys and improper first-match tail"
    );
    assert_eq!(on.1, off.1, "all direct-nil, direct and default results");
    assert_eq!(
        on.2, off.2,
        "all effective extents before constructor witnesses"
    );
    assert_eq!(off.3.heap_materializations, 1);
    assert_eq!(off.3.inline_constructions, 0);
    assert_eq!(off.3.alias_upgrades, 0);
    // On a test-only predecessor the constructor still materializes the Vec.
    assert_eq!(
        on.3.heap_materializations, 0,
        "duplicates require no alias vector"
    );
    assert_eq!(on.3.inline_constructions, 1);
    assert_eq!(on.3.alias_upgrades, 0);
}

#[test]
fn property_keys_preserve_category_alias_default_nil_overlay_and_extent_order() {
    let mut eval = Context::new();
    eval.eval_str("(put 'd5-property-category 'face 'd5-category-face)")
        .unwrap();
    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    let face = Value::symbol("face");
    let alias = Value::symbol("font-lock-face");
    let category = Value::symbol("d5-property-category");
    let overlay;
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.insert("abcdefgh\n");
        buffer.set_buffer_local(
            "char-property-alias-alist",
            Value::list(vec![Value::list(vec![face, alias])]),
        );
        buffer.set_buffer_local(
            "default-text-properties",
            Value::list(vec![face, Value::symbol("d5-default-face")]),
        );
        assert!(buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(0), EmacsBytePos::new(3)),
            Value::symbol("category"),
            category
        ));
        assert!(buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(0), EmacsBytePos::new(6)),
            alias,
            Value::symbol("d5-alias-face")
        ));
        assert!(buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(0), EmacsBytePos::new(1)),
            face,
            Value::NIL
        ));
        overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buffer_id),
            start: 4,
            end: 6,
            front_advance: false,
            rear_advance: false,
        });
        buffer.overlays_mut().insert_overlay(overlay);
        let _ = buffer
            .overlays_mut()
            .overlay_put(overlay, alias, Value::symbol("d5-overlay-face"));
    }
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    let snapshot = LayoutBufferSnapshot::from_buffer_with_obarray(buffer, eval.obarray());
    // A snapshot owns cloned overlays and preserves their precedence serials.
    // Property-source identity must name that snapshot's object.
    let snapshot_overlays = snapshot
        .layout_overlays()
        .overlays_at_emacs_byte_pos(EmacsBytePos::new(4));
    assert_eq!(snapshot_overlays.len(), 1);
    let snapshot_overlay = snapshot_overlays[0];
    assert_ne!(snapshot_overlay.bits(), overlay.bits());
    let original = overlay.as_overlay_data().unwrap();
    let captured = snapshot_overlay.as_overlay_data().unwrap();
    assert_eq!(
        (
            captured.serial,
            captured.buffer,
            captured.front_advance,
            captured.rear_advance
        ),
        (
            original.serial,
            original.buffer,
            original.front_advance,
            original.rear_advance
        )
    );
    assert_eq!(captured.plist, original.plist);
    assert_eq!(
        snapshot
            .layout_overlays()
            .overlay_start_emacs_byte_pos(snapshot_overlay),
        buffer.overlays().overlay_start_emacs_byte_pos(overlay)
    );
    assert_eq!(
        snapshot
            .layout_overlays()
            .overlay_end_emacs_byte_pos(snapshot_overlay),
        buffer.overlays().overlay_end_emacs_byte_pos(overlay)
    );
    let bounds = EmacsByteRange::new(
        EmacsBytePos::ZERO,
        snapshot.layout_point_max_emacs_byte_pos(),
    );
    let observe = |enabled| {
        let _scope = Guard::set(enabled);
        let lookup = LayoutCharPropertyLookup::new(&snapshot, face);
        let text: Vec<_> = (0..8)
            .map(|pos| {
                lookup
                    .text_value_at(&snapshot, EmacsBytePos::new(pos))
                    .map(Value::bits)
            })
            .collect();
        let extents: Vec<_> = (0..8)
            .map(|pos| lookup.effective_text_extent_at(&snapshot, EmacsBytePos::new(pos), bounds))
            .collect();
        let sources: Vec<_> = (0..8)
            .map(|pos| {
                lookup
                    .overlay_or_text_source_at(&snapshot, EmacsBytePos::new(pos), None)
                    .map(|source| (source.value.bits(), source.overlay.map(Value::bits)))
            })
            .collect();
        (
            text,
            extents,
            sources,
            lookup.effective_overlay_value(&snapshot, snapshot_overlay),
        )
    };
    let off = observe(false);
    let on = observe(true);
    assert_eq!(
        on, off,
        "all effective values, source ownership and exact extents"
    );
    // GNU lookup_char_property (intervals.c:1712): direct nil wins, then
    // category, then first nonnil alias, then text-only default.
    assert_eq!(off.0[0], Some(Value::NIL.bits()));
    assert_eq!(off.0[1], Some(Value::symbol("d5-category-face").bits()));
    assert_eq!(off.0[3], Some(Value::symbol("d5-alias-face").bits()));
    assert_eq!(off.0[7], Some(Value::symbol("d5-default-face").bits()));
    assert_eq!(off.3, Some(Value::symbol("d5-overlay-face")));
    assert_eq!(
        off.2[4],
        Some((
            Value::symbol("d5-overlay-face").bits(),
            Some(snapshot_overlay.bits())
        ))
    );
}
