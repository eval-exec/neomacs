use super::*;
use neovm_value::intern::intern;

/// Give `overlay` exactly the property names in `names`, as successive
/// `overlay-put` calls (or a Lisp rewrite of its plist) would.
fn set_property_names(overlay: Value, names: &[Value]) {
    let plist = names.iter().rev().fold(Value::NIL, |tail, name| {
        Value::cons(*name, Value::cons(Value::T, tail))
    });
    overlay
        .with_overlay_data_mut(|data| data.plist = plist)
        .expect("test overlay is live");
}

fn put_property_names(index: &mut OverlayIndex, overlay: Value, names: &[Value]) {
    let previous_keys = index.property_keys_before_write(overlay);
    set_property_names(overlay, names);
    index.overlay_properties_changed(overlay, previous_keys);
    index.assert_invariants();
}

/// Every overlay carrying `property` must survive the conservative endpoint
/// filter at both endpoints, at the positions the interval index reports.
fn assert_carriers_pass_filter(index: &OverlayIndex, property: Value, carriers: &[Value]) {
    let endpoints: Vec<_> = index
        .endpoint_records_strictly_within(
            range(0, 100_000),
            OverlayPropertyFilter::for_properties([property]),
        )
        .collect();
    for carrier in carriers {
        let indexed = index.range(*carrier).expect("carrier is indexed");
        for (kind, position) in [
            (EndpointKind::Start, indexed.start()),
            (EndpointKind::End, indexed.end()),
        ] {
            assert!(
                endpoints
                    .iter()
                    .any(|endpoint| endpoint.overlay.bits() == carrier.bits()
                        && endpoint.kind == kind
                        && endpoint.position == position),
                "{kind:?} endpoint of a carrier was filtered out or misplaced"
            );
        }
    }
}

fn indexed_overlays(count: usize) -> (OverlayIndex, Vec<Value>) {
    let mut index = OverlayIndex::new();
    let overlays: Vec<_> = (0..count).map(|i| overlay(i * 4 + 1, i * 4 + 3)).collect();
    for (i, overlay) in overlays.iter().enumerate() {
        assert!(index.attach(*overlay, range(i * 4 + 1, i * 4 + 3)));
    }
    // Property sweeps publish endpoints; basic geometric queries do not.
    index.publish_endpoints();
    assert_eq!(
        index.next_boundary_after(EmacsBytePos::new(0), EmacsBytePos::new(100_000)),
        Some(EmacsBytePos::new(1))
    );
    (index, overlays)
}

#[test]
fn property_mask_changes_keep_multilevel_filters_exact_under_pending_shifts() {
    crate::test_utils::init_test_tracing();
    // 600 overlays publish 1,200 endpoints across leaves and two branch levels.
    let (mut index, overlays) = indexed_overlays(600);
    // Leave a lazy shift pending over the suffix; a mask-only update must
    // neither need nor disturb the records' current positions.
    index.adjust_for_text_edit(OverlayTextEdit::Insert {
        position: EmacsBytePos::new(600),
        length: EmacsByteLen::new(5),
        before_markers: false,
    });
    index.assert_invariants();

    let face = Value::symbol("face");
    let help_echo = Value::symbol("help-echo");
    let carriers: Vec<_> = overlays.iter().copied().step_by(37).collect();

    // Masks grow one name at a time, as successive `overlay-put`s do.
    for names in [&[face][..], &[face, help_echo][..]] {
        for carrier in &carriers {
            put_property_names(&mut index, *carrier, names);
        }
    }
    assert_carriers_pass_filter(&index, face, &carriers);
    assert_carriers_pass_filter(&index, help_echo, &carriers);

    // A mask can also shrink when Lisp rewrites a plist in place.
    let (cleared, kept) = carriers.split_at(carriers.len() / 2);
    for carrier in cleared {
        put_property_names(&mut index, *carrier, &[help_echo]);
    }
    assert_carriers_pass_filter(&index, face, kept);
    assert_carriers_pass_filter(&index, help_echo, &carriers);
    for carrier in &carriers {
        put_property_names(&mut index, *carrier, &[]);
    }
    index.assert_invariants();
    for property in [face, help_echo] {
        assert!(
            index
                .endpoint_records_strictly_within(
                    range(0, 100_000),
                    OverlayPropertyFilter::for_properties([property]),
                )
                .next()
                .is_none()
        );
    }

    // Replacing a class can lose and gain bits in one operation.
    let replacement = (0..257)
        .map(|i| Value::from_sym_id(intern(&format!("mask-republish-replacement-{i}"))))
        .find(|property| {
            overlay_property_signature_bit(*property) != overlay_property_signature_bit(face)
        })
        .expect("distinct signature class");
    let carrier = overlays[450];
    put_property_names(&mut index, carrier, &[face]);
    put_property_names(&mut index, carrier, &[replacement]);
    assert_carriers_pass_filter(&index, replacement, &[carrier]);
    assert!(
        index
            .endpoint_records_strictly_within(
                range(0, 100_000),
                OverlayPropertyFilter::for_properties([face]),
            )
            .next()
            .is_none()
    );
}

#[test]
fn property_mask_change_republishes_filters_without_a_full_summary_refresh() {
    crate::test_utils::init_test_tracing();
    let (mut index, overlays) = indexed_overlays(300);
    let carrier = overlays[150];
    index
        .with_published_endpoints(|endpoints| endpoints.records.reset_summary_refresh_count())
        .expect("endpoint index is published");

    // Diagnostic churn puts a few new names on each fresh overlay. Each new
    // name changes only the conservative filter class, never a position.
    put_property_names(&mut index, carrier, &[Value::symbol("face")]);
    put_property_names(
        &mut index,
        carrier,
        &[Value::symbol("face"), Value::symbol("help-echo")],
    );
    put_property_names(&mut index, carrier, &[Value::symbol("help-echo")]);

    let refreshes = index
        .with_published_endpoints(|endpoints| endpoints.records.summary_refresh_count())
        .expect("endpoint index is published");
    assert_eq!(
        refreshes, 0,
        "a filter-mask change rebuilt position summaries"
    );
}

#[test]
fn direct_property_counts_balance_duplicate_keys_and_remaining_carriers() {
    crate::test_utils::init_test_tracing();
    let category = Value::symbol("category");
    let face = Value::symbol("face");
    let first = overlay(1, 3);
    let second = overlay(5, 7);
    set_property_names(first, &[category, category]);
    set_property_names(second, &[category]);
    let mut index = OverlayIndex::new();
    assert!(index.attach(first, range(1, 3)));
    assert!(index.attach(second, range(5, 7)));
    assert!(index.may_contain_property(category));
    put_property_names(&mut index, first, &[face]);
    assert!(index.may_contain_property(category));
    assert!(index.detach(second).is_some());
    assert!(!index.may_contain_property(category));
    assert!(index.may_contain_property(face));
    assert!(index.detach(first).is_some());
    assert!(!index.may_contain_property(face));
    index.assert_invariants();
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "property token crossed an attachment change")]
fn property_key_token_refuses_detach_and_reattach() {
    crate::test_utils::init_test_tracing();
    let owner = overlay(1, 3);
    let mut index = OverlayIndex::new();
    assert!(index.attach(owner, range(1, 3)));
    let previous = index.property_keys_before_write(owner);
    assert!(index.detach(owner).is_some());
    assert!(index.attach(owner, range(1, 3)));
    index.overlay_properties_changed(owner, previous);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "property token crossed an index owner")]
fn property_key_token_refuses_a_foreign_index() {
    crate::test_utils::init_test_tracing();
    let owner = overlay(1, 3);
    let mut first = OverlayIndex::new();
    let mut second = OverlayIndex::new();
    assert!(first.attach(owner, range(1, 3)));
    let previous = first.property_keys_before_write(owner);
    assert!(first.detach(owner).is_some());
    assert!(second.attach(owner, range(1, 3)));
    second.overlay_properties_changed(owner, previous);
}
