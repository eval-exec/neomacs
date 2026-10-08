use super::*;

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
    set_property_names(overlay, names);
    index.overlay_properties_changed(overlay);
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
    // Publish the endpoint index: only a published index tracks masks.
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
        .map(|i| Value::symbol(&format!("mask-republish-replacement-{i}")))
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
    let endpoints = index.endpoints.get().expect("endpoint index is published");
    endpoints.records.reset_summary_refresh_count();

    // Diagnostic churn puts a few new names on each fresh overlay. Each new
    // name changes only the conservative filter class, never a position.
    put_property_names(&mut index, carrier, &[Value::symbol("face")]);
    put_property_names(
        &mut index,
        carrier,
        &[Value::symbol("face"), Value::symbol("help-echo")],
    );
    put_property_names(&mut index, carrier, &[Value::symbol("help-echo")]);

    let endpoints = index.endpoints.get().expect("endpoint index is published");
    assert_eq!(
        endpoints.records.summary_refresh_count(),
        0,
        "a filter-mask change rebuilt position summaries"
    );
}
