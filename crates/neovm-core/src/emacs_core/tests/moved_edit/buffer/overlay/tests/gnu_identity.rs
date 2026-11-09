use super::*;
use neovm_eval::emacs_core::eval::Context;

#[test]
fn gde_overlay_restored_live_identity_preserves_the_creation_serial() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let original = eval
        .eval_str("(progn (insert \"abcd\") (setq gde-restored-overlay (make-overlay 2 4)))")
        .expect("live overlay");
    let original_serial = overlay_identity_key(original);
    let snapshot = crate::emacs_core::pdump::snapshot_evaluator(&eval);
    let mut restored =
        crate::emacs_core::pdump::restore_snapshot(&snapshot, eval.hooks_installed())
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
            .edit
            .buffers
            .current_buffer()
            .expect("restored buffer")
            .overlays()
            .overlays_in_gnu_lists_order()
            .into_iter()
            .any(|overlay| overlay_identity_key(overlay) == original_serial),
        "the restored buffer must still hold the overlay under its preserved serial"
    );
}
