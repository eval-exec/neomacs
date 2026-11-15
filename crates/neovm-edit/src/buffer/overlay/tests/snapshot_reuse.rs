//! The cached redisplay snapshot survives text edits.
//!
//! Layout takes `OverlayList::snapshot` on every attempt. These tests pin two
//! contracts: an edit that no observer is watching updates the cached copy
//! in place (no overlay object is re-created), and an observer that already
//! holds a snapshot keeps seeing the state it captured.
//!
//! The copy counter is one process-wide static. `cargo nextest` runs each test
//! in its own process, so the exact copy counts asserted here hold there; under
//! `cargo test`, another test taking a snapshot on a parallel thread could add
//! to the count.

use super::*;

/// Everything a layout walk can read from one indexed overlay, in the
/// index's raw ascending query order.
#[derive(Debug, PartialEq)]
struct ObservedOverlay {
    serial: u64,
    range: (usize, usize),
    front_advance: bool,
    rear_advance: bool,
    buffer: Option<BufferId>,
    plist: Vec<usize>,
}

fn observe(list: &OverlayList) -> Vec<ObservedOverlay> {
    list.index
        .all_ascending()
        .into_iter()
        .map(|overlay| {
            let data = overlay.as_overlay_data().unwrap();
            let range = list.index.range(overlay).unwrap();
            let mut plist = Vec::new();
            let mut tail = data.plist;
            while tail.is_cons() {
                plist.push(tail.cons_car().bits());
                tail = tail.cons_cdr();
            }
            ObservedOverlay {
                serial: data.serial.get(),
                range: (range.start().get(), range.end().get()),
                front_advance: data.front_advance,
                rear_advance: data.rear_advance,
                buffer: data.buffer,
                plist,
            }
        })
        .collect()
}

fn serials_at(list: &OverlayList, pos: usize) -> Vec<u64> {
    overlays_at(list, pos)
        .into_iter()
        .map(|overlay| overlay_identity_key(overlay).get())
        .collect()
}

/// One face overlay of width 1 every 7 bytes, as flycheck or diff-hl leave.
fn face_overlays(count: usize) -> OverlayList {
    let mut list = OverlayList::new();
    let face = Value::symbol("face");
    for index in 0..count {
        let overlay = alloc_overlay(7 * index, 7 * index + 1);
        list.insert_overlay(overlay);
        list.overlay_put(overlay, face, Value::symbol("error"))
            .unwrap();
    }
    list
}

#[test]
fn text_edit_keeps_the_cached_overlay_snapshot_without_copying_overlays() {
    crate::test_utils::init_test_tracing();
    let mut list = face_overlays(10_000);
    // The previous redisplay left its snapshot in the cache.
    drop(list.snapshot());
    reset_overlay_snapshot_copy_count();

    let typed = 7 * 5_000 + 3;
    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(typed), emacs_byte_len(1), false);
    let snapshot = list.snapshot();
    assert_eq!(
        overlay_snapshot_copy_count(),
        0,
        "one typed character re-created every overlay of the buffer"
    );
    assert_eq!(
        serials_at(&snapshot, 7 * 4_999),
        serials_at(&list, 7 * 4_999)
    );
    assert!(overlays_at(&snapshot, 7 * 5_001).is_empty());
    assert_eq!(overlays_at(&snapshot, 7 * 5_001 + 1).len(), 1);
    assert_eq!(observe(&snapshot), observe(&list));
    drop(snapshot);

    list.adjust_for_delete_emacs_byte_range(emacs_byte_range(typed, typed + 1));
    // Typing at an overlay's start and end, where the boundary cases run.
    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(7 * 200), emacs_byte_len(1), false);
    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(7 * 300 + 2), emacs_byte_len(1), false);
    let snapshot = list.snapshot();
    assert_eq!(overlay_snapshot_copy_count(), 0);
    assert_eq!(observe(&snapshot), observe(&list));
    let live = overlays_at(&list, 0)[0];
    let observed = overlays_at(&snapshot, 0)[0];
    assert_ne!(
        live.bits(),
        observed.bits(),
        "the snapshot must keep its own overlay objects"
    );
}

#[test]
fn walk_holding_the_snapshot_sees_the_pre_callback_overlay_state() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let face = Value::symbol("face");
    let first = alloc_overlay(10, 20);
    let second = alloc_overlay(30, 40);
    list.insert_overlay(first);
    list.insert_overlay(second);
    list.overlay_put(first, face, Value::symbol("old")).unwrap();
    drop(list.snapshot());

    // Layout holds the snapshot for the whole walk.
    let walk = list.snapshot();
    let before = observe(&walk);
    let frozen = overlays_at(&walk, 12)[0];

    // A Lisp callback (fontification, a `display` form) runs mid-walk.
    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(0), emacs_byte_len(5), false);
    list.overlay_put(first, face, Value::symbol("new")).unwrap();
    list.adjust_for_delete_emacs_byte_range(emacs_byte_range(36, 38));
    list.move_overlay_to_emacs_byte_range(second, emacs_byte_range(0, 3));

    assert_eq!(observe(&walk), before);
    assert_eq!(overlay_start(&walk, frozen), Some(10));
    assert_eq!(overlay_end(&walk, frozen), Some(20));
    assert_eq!(
        walk.overlay_get_named(frozen, face),
        Some(Value::symbol("old"))
    );
    assert_eq!(overlays_at(&walk, 35).len(), 1);
    assert!(overlays_at(&walk, 1).is_empty());
    drop(walk);

    let next = list.snapshot();
    assert_eq!(observe(&next), observe(&list));
    let moved = overlays_at(&next, 16)[0];
    assert_eq!(
        next.overlay_get_named(moved, face),
        Some(Value::symbol("new"))
    );
}
