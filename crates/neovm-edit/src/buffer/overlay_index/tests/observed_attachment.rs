use super::*;
use crate::buffer::BufferId;
use neovm_value::heap_types::OverlayDataInit;
use neovm_value::tagged::collection_reads::{self, CollectionReads};

fn fresh_overlay(start: usize, end: usize, face: Value, before_string: Value) -> Value {
    // Build the complete plist before capture without reading the new overlay.
    // Duplicate keys must remain balanced in the direct-key refcounts.
    let plist = Value::list(vec![
        face,
        Value::T,
        face,
        Value::NIL,
        before_string,
        Value::NIL,
    ]);
    Value::make_overlay(OverlayDataInit {
        serial: neovm_value::heap_types::OverlaySerialSource::Mint,
        plist,
        buffer: Some(BufferId(1)),
        start,
        end,
        front_advance: false,
        rear_advance: false,
    })
}

fn assert_owner_observed(reads: &CollectionReads, overlay: Value) {
    let mut found = false;
    assert!(reads.all_dependency_bits(|bits| {
        found |= bits == overlay.bits();
        true
    }));
    assert!(found, "attachment must observe the actual overlay owner");
}

fn assert_attachment(index: &OverlayIndex, overlay: Value, start: usize, end: usize) {
    let data = overlay.as_overlay_data().expect("fixture is an overlay");
    let handle = data
        .position_handle()
        .as_ref()
        .expect("attachment must install its position handle");
    assert!(handle.belongs_to(overlay, &index.intervals));
    assert_eq!(
        index.range(overlay),
        Some(EmacsByteRange::from_usize(start, end))
    );
}

fn assert_later_mutation_invalidates(reads: &CollectionReads, overlay: Value) {
    overlay
        .with_overlay_data_mut(|data| data.front_advance = true)
        .expect("observed overlay remains live");
    assert!(
        !reads.unchanged(),
        "later mutation of the observed owner must invalidate its certificate"
    );
}

#[test]
fn attachment_publishes_handle_before_observed_property_reads() {
    crate::test_utils::init_test_tracing();
    for published in [false, true] {
        let mut index = OverlayIndex::new();
        if published {
            let _ = index.endpoint_index();
        }
        let face = Value::symbol("face");
        let before_string = Value::symbol("before-string");
        let overlay = fresh_overlay(2, 8, face, before_string);
        let (attached, reads) =
            collection_reads::capture(|| index.attach(overlay, EmacsByteRange::from_usize(2, 8)));
        assert!(attached);
        let reads = reads.expect("attachment must return a coherent read certificate");
        assert!(reads.unchanged(), "attachment invalidated its own reads");
        assert_owner_observed(&reads, overlay);
        assert_attachment(&index, overlay, 2, 8);
        assert_eq!(index.direct_property_keys.get(&face.bits()), Some(&2));
        assert_eq!(
            index.direct_property_keys.get(&before_string.bits()),
            Some(&1)
        );
        assert!(index.may_contain_property(face));
        assert!(index.may_contain_property(before_string));
        assert_eq!(
            index.next_boundary_after(EmacsBytePos::ZERO, EmacsBytePos::new(10)),
            Some(EmacsBytePos::new(2))
        );
        assert_eq!(
            index.previous_boundary_before(EmacsBytePos::new(10), EmacsBytePos::ZERO),
            Some(EmacsBytePos::new(8))
        );
        assert_eq!(index.published_endpoints().is_some(), published);
        assert!(reads.unchanged(), "read-only checks changed the attachment");
        assert_later_mutation_invalidates(&reads, overlay);
    }
}

#[test]
fn batch_attachment_publishes_handles_before_observed_property_reads() {
    crate::test_utils::init_test_tracing();
    for order in [
        OverlayBatchOrder::AttachmentSequence,
        OverlayBatchOrder::AscendingQueryOrder,
    ] {
        let mut index = OverlayIndex::new();
        let face = Value::symbol("face");
        let before_string = Value::symbol("before-string");
        let first = fresh_overlay(2, 8, face, before_string);
        let second = fresh_overlay(4, 6, face, before_string);
        let entries = [
            (first, EmacsByteRange::from_usize(2, 8)),
            (second, EmacsByteRange::from_usize(4, 6)),
        ];
        let (attached, reads) = collection_reads::capture(|| index.attach_batch(&entries, order));
        assert!(attached);
        let reads = reads.expect("batch attachment must return a coherent read certificate");
        assert!(reads.unchanged(), "batch invalidated its own reads");
        assert_owner_observed(&reads, first);
        assert_owner_observed(&reads, second);
        assert_attachment(&index, first, 2, 8);
        assert_attachment(&index, second, 4, 6);
        assert_eq!(index.direct_property_keys.get(&face.bits()), Some(&4));
        assert_eq!(
            index.direct_property_keys.get(&before_string.bits()),
            Some(&2)
        );
        assert!(index.may_contain_property(face));
        assert!(index.may_contain_property(before_string));
        assert_eq!(
            index.next_boundary_after(EmacsBytePos::new(2), EmacsBytePos::new(10)),
            Some(EmacsBytePos::new(4))
        );
        assert_eq!(
            index.previous_boundary_before(EmacsBytePos::new(8), EmacsBytePos::ZERO),
            Some(EmacsBytePos::new(6))
        );
        assert!(
            index.published_endpoints().is_none(),
            "geometry must stay cold"
        );
        assert!(reads.unchanged(), "read-only checks changed the batch");
        assert_later_mutation_invalidates(&reads, first);
    }
}
