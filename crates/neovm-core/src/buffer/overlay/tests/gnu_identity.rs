use super::*;

#[test]
fn gde_overlay_identity_tiebreak_uses_the_original_tagged_object() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let original = alloc_overlay(2, 5);
    list.insert_overlay(original);
    // GNU buffer.c:3281 compares XLI(overlay). A snapshot must keep that
    // same original identity even though its observer copy has a new address.
    assert_eq!(overlay_identity_key(original), original.bits() as u64);
    let copied = list
        .snapshot_clone()
        .overlays_at_emacs_byte_pos(emacs_byte_pos(3))[0];
    assert_ne!(copied.bits(), original.bits());
    assert_eq!(overlay_identity_key(copied), original.bits() as u64);
}

#[test]
fn gde_overlay_restored_live_identity_uses_its_relocated_object() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let original = eval
        .eval_str("(progn (insert \"abcd\") (setq gde-restored-overlay (make-overlay 2 4)))")
        .expect("live overlay");
    let snapshot = crate::emacs_core::pdump::snapshot_evaluator(&eval);
    let mut restored = crate::emacs_core::pdump::restore_snapshot(&snapshot)
        .expect("restore context containing a live overlay");
    let relocated = restored
        .eval_str("gde-restored-overlay")
        .expect("restored overlay");
    assert_ne!(original.bits(), relocated.bits());
    assert_eq!(overlay_identity_key(relocated), relocated.bits() as u64);
    for overlay in restored
        .buffers
        .current_buffer()
        .expect("restored buffer")
        .overlays
        .overlays_in_gnu_lists_order()
    {
        assert_eq!(overlay_identity_key(overlay), overlay.bits() as u64);
    }
}
