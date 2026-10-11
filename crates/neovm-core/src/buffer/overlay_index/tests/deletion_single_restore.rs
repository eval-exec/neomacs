use super::*;

fn identity_order(index: &OverlayIndex) -> Vec<usize> {
    index
        .all_ascending()
        .into_iter()
        .map(|overlay| overlay.bits())
        .collect()
}

#[test]
fn gde_review_erase_overlays_restore_each_record_once() {
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    for i in 0..257 {
        assert!(index.attach(overlay(i * 2, i * 2 + 1), range(i * 2, i * 2 + 1)));
    }
    let before = identity_order(&index);
    index
        .intervals
        .read()
        .records
        .reset_membership_mutation_count();
    index.adjust_for_text_edit(OverlayTextEdit::Delete {
        range: range(0, 514),
    });
    let mutations = index.intervals.read().records.membership_mutation_count();
    assert_eq!(
        mutations,
        257 * 2,
        "one removal and one restore per record, without a repair pass"
    );
    assert_eq!(
        identity_order(&index),
        before,
        "GNU deletion leaves structural in-order fixed"
    );
    assert!(index.values().all(|o| index.range(o) == Some(range(0, 0))));
    index.assert_invariants();
}

#[test]
fn gde_review_middle_delete_preserves_suffix_boundary_without_second_restore() {
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    for i in 0..256 {
        assert!(index.attach(overlay(i * 2, i * 2 + 1), range(i * 2, i * 2 + 1)));
    }
    let before = identity_order(&index);
    index
        .intervals
        .read()
        .records
        .reset_membership_mutation_count();
    index.adjust_for_text_edit(OverlayTextEdit::Delete {
        range: range(128, 384),
    });
    let mutations = index.intervals.read().records.membership_mutation_count();
    assert_eq!(
        mutations,
        129 * 2,
        "including starts exactly at END in the single restore pass"
    );
    assert_eq!(identity_order(&index), before);
    index.assert_invariants();
}
