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
