//! P3: every overlay-index write advances the generation. A remap whose
//! numeric begins and byte ranges are unchanged does not.

use super::*;

fn fresh() -> (OverlayList, Value) {
    let mut list = OverlayList::new();
    let overlay = alloc_overlay(10, 20);
    list.insert_overlay(overlay);
    (list, overlay)
}

fn assert_bumped(before: u64, after: u64, kind: &str) {
    assert!(
        after > before,
        "{kind} must advance the generation: before={before} after={after}"
    );
}

#[test]
fn each_overlay_write_advances_generation_and_noop_remap_does_not() {
    crate::test_utils::init_test_tracing();
    let (mut list, overlay) = fresh();
    let attached = list.overlay_generation();
    assert_bumped(0, attached, "attach");

    list.overlay_put(overlay, Value::symbol("face"), Value::symbol("bold"))
        .unwrap();
    let put = list.overlay_generation();
    assert_bumped(attached, put, "put");

    list.move_overlay_to_emacs_byte_range(overlay, emacs_byte_range(12, 24));
    let moved = list.overlay_generation();
    assert_bumped(put, moved, "move");

    list.set_front_advance(overlay, true);
    let advanced = list.overlay_generation();
    assert_bumped(moved, advanced, "advance");

    list.retarget_buffer(BufferId(1), BufferId(2));
    let retargeted = list.overlay_generation();
    assert_bumped(advanced, retargeted, "retarget");

    let noop = list.overlay_generation();
    list.remap_overlay_positions(&[OverlayPositionRemap {
        overlay,
        old_range: emacs_byte_range(12, 24),
        range: emacs_byte_range(12, 24),
        old_begin: CharPos0::new(12),
        new_begin: CharPos0::new(12),
    }]);
    assert_eq!(
        list.overlay_generation(),
        noop,
        "a no-op remap must not advance the generation"
    );

    list.remap_overlay_positions(&[OverlayPositionRemap {
        overlay,
        old_range: emacs_byte_range(12, 24),
        range: emacs_byte_range(30, 42),
        old_begin: CharPos0::new(12),
        new_begin: CharPos0::new(30),
    }]);
    let remapped = list.overlay_generation();
    assert_bumped(noop, remapped, "remap");

    list.adjust_for_text_edit(OverlayTextEdit::Insert {
        position: emacs_byte_pos(32),
        length: emacs_byte_len(1),
        before_markers: false,
    });
    let edited = list.overlay_generation();
    assert_bumped(remapped, edited, "text edit");

    list.overlay_put(overlay, Value::symbol("evaporate"), Value::t())
        .unwrap();
    let evaporate_put = list.overlay_generation();
    assert_bumped(edited, evaporate_put, "put evaporate");
    list.move_overlay_to_emacs_byte_range(overlay, emacs_byte_range(40, 40));
    let evaporated = list.overlay_generation();
    assert_bumped(evaporate_put, evaporated, "evaporate");
    assert!(
        list.get(overlay).is_none(),
        "an empty evaporating overlay must leave the index"
    );

    let (mut list, overlay) = fresh();
    let attached = list.overlay_generation();
    assert!(list.detach_overlay(overlay));
    assert_bumped(attached, list.overlay_generation(), "detach");

    let (mut list, _) = fresh();
    let before_clear = list.overlay_generation();
    list.delete_all_overlays();
    assert_bumped(before_clear, list.overlay_generation(), "delete-all");
}

#[test]
fn text_deletion_and_evaporating_delete_advance_generation() {
    crate::test_utils::init_test_tracing();
    let (mut list, overlay) = fresh();
    let before_delete = list.overlay_generation();
    list.adjust_for_text_edit(OverlayTextEdit::Delete {
        range: emacs_byte_range(12, 15),
    });
    let after_delete = list.overlay_generation();
    assert_bumped(before_delete, after_delete, "text delete");

    list.overlay_put(overlay, Value::symbol("evaporate"), Value::t())
        .unwrap();
    let before_evap = list.overlay_generation();
    list.adjust_for_text_edit(OverlayTextEdit::Delete {
        range: emacs_byte_range(10, 17),
    });
    let after_evap = list.overlay_generation();
    assert_bumped(before_evap, after_evap, "evaporating text delete");
    assert!(
        list.get(overlay).is_none(),
        "deleted evaporating overlay must leave the index"
    );
}

#[test]
fn evaporate_on_unchanged_empty_move_advances_generation() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let overlay = alloc_overlay(20, 20);
    list.insert_overlay(overlay);
    list.overlay_put(overlay, Value::symbol("evaporate"), Value::t())
        .unwrap();
    // Moving an already-empty overlay to its same range does not change the
    // interval range (move_to is Unchanged); the generation bump must come
    // from the evaporating detach itself.
    let before = list.overlay_generation();
    list.move_overlay_to_emacs_byte_range(overlay, emacs_byte_range(20, 20));
    assert_bumped(before, list.overlay_generation(), "evaporate detach");
    assert!(list.get(overlay).is_none());
}

#[test]
fn no_change_operations_do_not_advance_generation() {
    crate::test_utils::init_test_tracing();
    let mut empty = OverlayList::new();
    let empty_gen = empty.overlay_generation();
    empty.adjust_for_text_edit(OverlayTextEdit::Insert {
        position: emacs_byte_pos(0),
        length: emacs_byte_len(4),
        before_markers: false,
    });
    empty.adjust_for_text_edit(OverlayTextEdit::Delete {
        range: emacs_byte_range(0, 4),
    });
    assert_eq!(
        empty.overlay_generation(),
        empty_gen,
        "text edits on an empty overlay list must not bump generation"
    );

    let (mut list, overlay) = fresh();
    list.overlay_put(overlay, Value::symbol("face"), Value::symbol("bold"))
        .unwrap();
    let stable = list.overlay_generation();

    list.insert_overlay(overlay);
    assert_eq!(
        list.overlay_generation(),
        stable,
        "re-attaching an already-attached overlay must not bump generation"
    );

    let unattached = alloc_overlay(30, 40);
    assert!(!list.detach_overlay(unattached));
    assert_eq!(
        list.overlay_generation(),
        stable,
        "detaching an unattached overlay must not bump generation"
    );

    list.move_overlay_to_emacs_byte_range(overlay, emacs_byte_range(10, 20));
    assert_eq!(
        list.overlay_generation(),
        stable,
        "moving an overlay to its unchanged range must not bump generation"
    );

    list.overlay_put(overlay, Value::symbol("face"), Value::symbol("bold"))
        .unwrap();
    assert_eq!(
        list.overlay_generation(),
        stable,
        "putting the same eq value for an existing property must not bump generation"
    );

    list.retarget_buffer(BufferId(99), BufferId(100));
    assert_eq!(
        list.overlay_generation(),
        stable,
        "standalone retarget_buffer with no matching overlays must not bump generation"
    );

    list.adjust_for_text_edit(OverlayTextEdit::Insert {
        position: emacs_byte_pos(15),
        length: emacs_byte_len(0),
        before_markers: false,
    });
    list.adjust_for_text_edit(OverlayTextEdit::Delete {
        range: emacs_byte_range(15, 15),
    });
    assert_eq!(
        list.overlay_generation(),
        stable,
        "zero-length text edits must not bump generation"
    );
}

#[test]
fn mirrored_plist_write_advances_snapshot_generation() {
    crate::test_utils::init_test_tracing();
    let (mut list, overlay) = fresh();
    let (gen_before, ptr_before) = {
        let snap_before = list.snapshot();
        (
            snap_before.overlay_generation(),
            &*snap_before as *const OverlayList,
        )
    };
    // 1. Adding a new property key mirrors key-filter + generation update.
    list.overlay_put(overlay, Value::symbol("face"), Value::symbol("bold"))
        .unwrap();
    let gen_after_add = {
        let snap_after = list.snapshot();
        assert_eq!(
            &*snap_after as *const OverlayList, ptr_before,
            "overlay_put should reuse and mirror into the cached snapshot"
        );
        snap_after.assert_index_invariants_for_test();
        assert!(snap_after.may_contain_property(Value::symbol("face")));
        let snap_ov = snap_after.overlays_at_emacs_byte_pos(emacs_byte_pos(12))[0];
        assert_eq!(
            snap_after.overlay_get_named(snap_ov, Value::symbol("face")),
            Some(Value::symbol("bold"))
        );
        let g = snap_after.overlay_generation();
        assert_bumped(
            gen_before,
            g,
            "mirrored new-key plist write on cached snapshot",
        );
        g
    };
    // 2. Overwriting an existing property key preserves key invariants and
    //    still advances the cached snapshot's generation.
    list.overlay_put(overlay, Value::symbol("face"), Value::symbol("italic"))
        .unwrap();
    let snap_after_update = list.snapshot();
    assert_eq!(
        &*snap_after_update as *const OverlayList, ptr_before,
        "existing-key overlay_put should keep reusing the cached snapshot"
    );
    snap_after_update.assert_index_invariants_for_test();
    assert!(snap_after_update.may_contain_property(Value::symbol("face")));
    let snap_ov = snap_after_update.overlays_at_emacs_byte_pos(emacs_byte_pos(12))[0];
    assert_eq!(
        snap_after_update.overlay_get_named(snap_ov, Value::symbol("face")),
        Some(Value::symbol("italic"))
    );
    assert_bumped(
        gen_after_add,
        snap_after_update.overlay_generation(),
        "mirrored existing-key plist write on cached snapshot",
    );
}

#[test]
fn buffer_swap_text_advances_generation_for_empty_overlay_list() {
    crate::test_utils::init_test_tracing();
    let mut manager = crate::buffer::BufferManager::new();
    let empty_id = manager.create_buffer("*p4-m2-empty*");
    let non_empty_id = manager.create_buffer("*p4-m2-nonempty*");
    manager.insert_into_buffer(empty_id, "hello world").unwrap();
    manager
        .insert_into_buffer(non_empty_id, "hello world")
        .unwrap();
    let ov = Value::make_overlay(neovm_value::heap_types::OverlayDataInit {
        serial: neovm_value::heap_types::OverlaySerialSource::Mint,
        plist: Value::NIL,
        buffer: Some(non_empty_id),
        start: 1,
        end: 5,
        front_advance: false,
        rear_advance: false,
    });
    manager
        .get_mut(non_empty_id)
        .unwrap()
        .overlays
        .insert_overlay(ov);

    let empty_gen_before = manager.get(empty_id).unwrap().overlays.overlay_generation();
    let non_empty_gen_before = manager
        .get(non_empty_id)
        .unwrap()
        .overlays
        .overlay_generation();

    manager.swap_buffer_text(empty_id, non_empty_id).unwrap();

    // After the swap, empty_id's old empty OverlayList now lives in non_empty_id,
    // and non_empty_id's old list now lives in empty_id. Both cores must advance
    // unconditionally so a live view captured for the previously empty buffer
    // goes stale.
    let moved_empty_gen = manager
        .get(non_empty_id)
        .unwrap()
        .overlays
        .overlay_generation();
    let moved_non_empty_gen = manager.get(empty_id).unwrap().overlays.overlay_generation();
    assert_bumped(
        empty_gen_before,
        moved_empty_gen,
        "M2: buffer-swap-text on empty overlay list",
    );
    assert_bumped(
        non_empty_gen_before,
        moved_non_empty_gen,
        "M2: buffer-swap-text on non-empty overlay list",
    );
}
