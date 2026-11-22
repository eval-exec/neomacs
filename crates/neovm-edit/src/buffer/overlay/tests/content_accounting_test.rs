//! Restored/bulk-dirty content accounting must survive deletion before read.
use super::*;

fn accounting_overlay(start: usize, end: usize, plist: Value) -> Value {
    Value::make_overlay(neovm_value::heap_types::OverlayDataInit {
        serial: neovm_value::heap_types::OverlaySerialSource::Mint,
        plist,
        buffer: Some(BufferId(1)),
        start,
        end,
        front_advance: false,
        rear_advance: false,
    })
}

fn check_restored_category_deletion(remap: bool) {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let bold = Value::symbol("bold");
    let category = accounting_overlay(
        10,
        20,
        Value::list(vec![Value::symbol("category"), Value::NIL]),
    );
    let survivor = accounting_overlay(30, 40, Value::list(vec![face, bold]));
    let mut restored = OverlayList::from_dump(vec![category, survivor]);
    let shift = if remap { 100 } else { 0 };
    if remap {
        restored.remap_overlay_positions(&[
            OverlayPositionRemap {
                overlay: category,
                old_range: EmacsByteRange::from_usize(10, 20),
                range: EmacsByteRange::from_usize(110, 120),
                old_begin: CharPos0::new(10),
                new_begin: CharPos0::new(110),
            },
            OverlayPositionRemap {
                overlay: survivor,
                old_range: EmacsByteRange::from_usize(30, 40),
                range: EmacsByteRange::from_usize(130, 140),
                old_begin: CharPos0::new(30),
                new_begin: CharPos0::new(130),
            },
        ]);
    }
    // No digest access may initialize accounting before this deletion.
    // Debug builds used to panic here; release builds retained a stale sum.
    assert!(restored.delete_overlay(category));

    let mut fresh = OverlayList::new();
    fresh.insert_overlay(accounting_overlay(
        30 + shift,
        40 + shift,
        Value::list(vec![face, bold]),
    ));
    let expected = fresh.content_digest();
    assert!(
        expected.is_some(),
        "the survivor has no category indirection"
    );
    assert_ne!(
        expected,
        Some(0),
        "the control must detect a stale zero sum"
    );
    assert_eq!(restored.content_digest(), expected, "remapped={remap}");
    restored.assert_index_invariants_for_test();
}

#[test]
fn restored_category_deleted_before_digest_matches_fresh_incremental_owner() {
    check_restored_category_deletion(false);
}

#[test]
fn remapped_restored_category_deleted_before_digest_matches_fresh_incremental_owner() {
    check_restored_category_deletion(true);
}
