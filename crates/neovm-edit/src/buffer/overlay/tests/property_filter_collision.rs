use super::*;

#[test]
fn colliding_property_signatures_keep_filtered_sweeps_and_extents_exact() {
    crate::test_utils::init_test_tracing();
    let (property, collision) = colliding_properties();
    let mut list = OverlayList::new();
    let carrier = alloc_overlay(3, 9);
    let unrelated = alloc_overlay(1, 11);
    list.insert_overlay(carrier);
    list.insert_overlay(unrelated);
    list.overlay_put(carrier, Value::symbol("priority"), Value::fixnum(20))
        .unwrap();
    list.overlay_put(carrier, property, Value::fixnum(11))
        .unwrap();
    list.overlay_put(unrelated, Value::symbol("priority"), Value::fixnum(10))
        .unwrap();
    list.overlay_put(unrelated, collision, Value::fixnum(99))
        .unwrap();

    // These value runs come from GNU next-single-char-property-change and
    // get-char-property; GNU's 1-based positions are shifted down one byte.
    // GNU textprop.c:647-668 resolves the exact plist key; 836-845 skips
    // unrelated overlay boundaries until the queried value changes.
    assert_eq!(
        filtered_runs(&list, property),
        [(0, 3, None), (3, 9, Some(11)), (9, 12, None)]
    );
    assert_eq!(
        filtered_runs(&list, collision),
        [(0, 1, None), (1, 11, Some(99)), (11, 12, None)]
    );
    assert_filtered_extent(&list, property, carrier, emacs_byte_range(3, 9), 11);
    assert_filtered_extent(&list, collision, unrelated, emacs_byte_range(1, 11), 99);

    // Both sweeps above published the endpoint index. Adding the colliding
    // name leaves the signature unchanged but must change exact resolution.
    list.overlay_put(carrier, collision, Value::fixnum(22))
        .unwrap();
    let overlapping_collision = [
        (0, 1, None),
        (1, 3, Some(99)),
        (3, 9, Some(22)),
        (9, 11, Some(99)),
        (11, 12, None),
    ];
    assert_eq!(filtered_runs(&list, collision), overlapping_collision);
    assert_filtered_extent(&list, collision, carrier, emacs_byte_range(3, 9), 22);

    // Removing the original key and replacing the sole remaining key also
    // preserve the signature. The filter still admits false positives, so
    // exact absence and every winning value must come from the resolver.
    replace_carrier_property(&mut list, carrier, collision, 22);
    assert!(list.index.may_contain_property(property));
    assert!(!list.may_contain_property(property));
    assert_eq!(filtered_runs(&list, property), [(0, 12, None)]);
    assert_eq!(filtered_runs(&list, collision), overlapping_collision);
    assert_filtered_extent(&list, collision, carrier, emacs_byte_range(3, 9), 22);

    replace_carrier_property(&mut list, carrier, property, 33);
    assert!(list.may_contain_property(property));
    assert_eq!(
        filtered_runs(&list, property),
        [(0, 3, None), (3, 9, Some(33)), (9, 12, None)]
    );
    assert_eq!(
        filtered_runs(&list, collision),
        [(0, 1, None), (1, 11, Some(99)), (11, 12, None)]
    );
    assert_filtered_extent(&list, property, carrier, emacs_byte_range(3, 9), 33);
    assert_filtered_extent(&list, collision, unrelated, emacs_byte_range(1, 11), 99);
}

fn colliding_properties() -> (Value, Value) {
    let mut signatures: Vec<(OverlayPropertyFilter, Value)> = Vec::new();
    // Each single-name signature selects one of 256 bits. Comparing the
    // public filters finds a pair even if symbol allocation order changes.
    for index in 0..=256 {
        let property = Value::symbol(format!("overlay-filter-collision-{index}"));
        let filter = OverlayPropertyFilter::for_properties([property]);
        if let Some((_, previous)) = signatures
            .iter()
            .find(|(signature, _)| *signature == filter)
        {
            assert_ne!(property.bits(), previous.bits());
            return (*previous, property);
        }
        signatures.push((filter, property));
    }
    panic!("257 single-property signatures must contain a collision");
}

fn filtered_runs(list: &OverlayList, property: Value) -> Vec<(usize, usize, Option<i64>)> {
    list.overlay_property_sweep(
        emacs_byte_range(0, 12),
        None,
        FilteredPropertyResolver {
            overlays: list,
            lookup_order: &[property],
        },
    )
    .map(|run| {
        (
            run.range().start().get(),
            run.range().end().get(),
            run.winner().map(|winner| {
                winner
                    .value()
                    .as_fixnum()
                    .expect("test properties are fixnums")
            }),
        )
    })
    .collect()
}

fn assert_filtered_extent(
    list: &OverlayList,
    property: Value,
    winner: Value,
    range: EmacsByteRange,
    value: i64,
) {
    let lookup_order = [property];
    let OverlayPropertyAtPoint::Present(resolution) = list
        .resolve_overlay_property_at_emacs_byte_pos(
            emacs_byte_pos(5),
            None,
            FilteredPropertyResolver {
                overlays: list,
                lookup_order: &lookup_order,
            },
        )
    else {
        panic!("the exact queried property should have a carrier");
    };
    let extent = resolution
        .extent(emacs_byte_range(0, 12))
        .expect("bounded extent");
    assert_eq!(extent.overlay().bits(), winner.bits());
    assert_eq!(extent.value(), Value::fixnum(value));
    assert_eq!(extent.range(), range);
}

fn replace_carrier_property(list: &mut OverlayList, carrier: Value, property: Value, value: i64) {
    carrier
        .with_overlay_data_mut(|data| {
            data.plist = Value::cons(
                Value::symbol("priority"),
                Value::cons(
                    Value::fixnum(20),
                    Value::cons(property, Value::cons(Value::fixnum(value), Value::NIL)),
                ),
            );
        })
        .expect("test carrier is live");
    list.index_mut().overlay_properties_changed(carrier);
    list.index.assert_invariants();
}
