//! Real resolver outputs precede the allocation-site assertion.
use super::property_keys_test_support::{self as probes, Guard};
use super::*;

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
