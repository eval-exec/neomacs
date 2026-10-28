use super::*;

#[test]
fn gde_overlay_snapshot_clone_preserves_the_creation_serial() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let original = alloc_overlay(2, 5);
    list.insert_overlay(original);
    // The creation serial is the deterministic replacement for GNU's
    // address-order tie-break (buffer.c:3281), and allocation mints it.
    let serial = overlay_identity_key(original);
    assert_ne!(serial.get(), 0);
    assert_eq!(serial, original.as_overlay_data().unwrap().serial);
    // A redisplay snapshot owns a different observer object but must keep the
    // identity of the overlay it observes, so both orders agree.
    let copied = list
        .snapshot_clone()
        .overlays_at_emacs_byte_pos(emacs_byte_pos(3))[0];
    assert_ne!(copied.bits(), original.bits());
    assert_eq!(overlay_identity_key(copied), serial);
}

#[test]
fn gde_overlay_restored_live_identity_preserves_the_creation_serial() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let original = eval
        .eval_str("(progn (insert \"abcd\") (setq gde-restored-overlay (make-overlay 2 4)))")
        .expect("live overlay");
    let original_serial = overlay_identity_key(original);
    let snapshot = crate::emacs_core::pdump::snapshot_evaluator(&eval);
    let mut restored = crate::emacs_core::pdump::restore_snapshot(&snapshot)
        .expect("restore context containing a live overlay");
    let relocated = restored
        .eval_str("gde-restored-overlay")
        .expect("restored overlay");
    assert_ne!(original.bits(), relocated.bits());
    // The image preserves the creation serial, so the tie-break survives a
    // restart even though the object address was relocated.
    assert_eq!(overlay_identity_key(relocated), original_serial);
    assert!(
        restored
            .buffers
            .current_buffer()
            .expect("restored buffer")
            .overlays
            .overlays_in_gnu_lists_order()
            .into_iter()
            .any(|overlay| overlay_identity_key(overlay) == original_serial),
        "the restored buffer must still hold the overlay under its preserved serial"
    );
}

#[test]
fn gde_equal_overlays_are_ordered_by_creation_serial_after_delete_and_recreate() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let first = alloc_overlay(2, 5);
    let second = alloc_overlay(2, 5);
    let third = alloc_overlay(2, 5);
    for value in [first, second, third] {
        list.insert_overlay(value);
    }
    // Fully equal overlays: later creation wins, so the newest is reported
    // first by the precedence-ordered query.
    assert_eq!(overlays_at(&list, 3), vec![third, second, first]);
    assert!(list.delete_overlay(second));
    let replacement = alloc_overlay(2, 5);
    list.insert_overlay(replacement);
    assert!(
        overlay_identity_key(replacement) > overlay_identity_key(third),
        "a deleted overlay's creation serial is never reissued"
    );
    assert_eq!(overlays_at(&list, 3), vec![replacement, third, first]);
    assert!(list.delete_overlay(replacement));
    assert!(list.delete_overlay(third));
    assert!(list.delete_overlay(first));
    assert!(overlays_at(&list, 3).is_empty());
}

#[test]
fn gde_copy_overlays_mints_fresh_serials_while_snapshots_preserve_them() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let original = alloc_overlay(2, 5);
    list.insert_overlay(original);
    let original_serial = overlay_identity_key(original);

    // Redisplay snapshots observe the same overlay identity.
    let observer = list.snapshot_clone();
    let observed = observer.overlays_at_emacs_byte_pos(emacs_byte_pos(3))[0];
    assert_eq!(overlay_identity_key(observed), original_serial);

    // GNU `copy_overlays` owns distinct Lisp objects; each copy is a new
    // creation and therefore takes a later creation serial.
    let copied = list.clone();
    let copy = copied.overlays_at_emacs_byte_pos(emacs_byte_pos(3))[0];
    assert_ne!(copy.bits(), original.bits());
    assert!(
        overlay_identity_key(copy) > original_serial,
        "copy-overlays must mint a fresh, later creation serial"
    );
    assert_eq!(overlay_identity_key(original), original_serial);
}
