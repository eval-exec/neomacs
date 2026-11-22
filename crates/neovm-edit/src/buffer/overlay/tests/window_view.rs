//! Window-bounded overlay views for redisplay.
//!
//! A layout walk needs an immutable overlay set that is valid while the
//! evaluator keeps mutating the buffer. Copying the whole buffer's set makes
//! redisplay O(total overlays); these tests pin that the view is the same as
//! the full snapshot for every position inside its range, and that its copy
//! and digest costs follow the range.

use super::*;

use crate::buffer::position::EmacsBytePos;

fn serials_at(list: &OverlayList, pos: usize) -> Vec<u64> {
    overlays_at(list, pos)
        .into_iter()
        .map(|overlay| overlay_identity_key(overlay).get())
        .collect()
}

fn serials_in(list: &OverlayList, start: usize, end: usize) -> Vec<u64> {
    list.overlays_in_emacs_byte_range(emacs_byte_range(start, end))
        .into_iter()
        .map(|overlay| overlay_identity_key(overlay).get())
        .collect()
}

#[test]
fn window_view_matches_the_full_snapshot_inside_its_range() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for (start, end) in [
        (0, 10),
        (5, 6),
        (8, 8),
        (10, 20),
        (20, 20),
        (50, 60),
        (95, 105),
        (99, 99),
        (105, 106),
    ] {
        let overlay = alloc_overlay(start, end);
        list.insert_overlay(overlay);
    }

    let view = list.snapshot_in_range(emacs_byte_range(10, 100));
    for pos in 10..100 {
        assert_eq!(serials_at(&view, pos), serials_at(&list, pos), "pos {pos}");
    }
    for (start, end) in [(10, 20), (30, 60), (90, 99)] {
        assert_eq!(
            serials_in(&view, start, end),
            serials_in(&list, start, end),
            "range {start}..{end}"
        );
    }
}

#[test]
fn window_view_reports_a_query_below_its_capture_floor() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for (start, end) in [(0, 10), (50, 60), (95, 105), (1_000, 1_001)] {
        let overlay = alloc_overlay(start, end);
        list.insert_overlay(overlay);
    }

    let view = list.snapshot_in_range(emacs_byte_range(100, 1_000));
    // The capture starts one position before the span so an overlay ending
    // exactly at the span start is present for its after-string. Queries at
    // or above that floor are complete.
    let _ = view.overlays_at_emacs_byte_pos(EmacsBytePos::new(99));
    let _ = view.overlays_in_emacs_byte_range(emacs_byte_range(100, 200));
    let _ = view.next_boundary_after_emacs_byte_pos(EmacsBytePos::new(200));
    assert!(!view.overlay_view_query_underflowed());

    let _ = view.overlays_at_emacs_byte_pos(EmacsBytePos::new(98));
    assert!(
        view.overlay_view_query_underflowed(),
        "a point query below the view floor must request the whole-buffer retry"
    );

    let below = list.snapshot_in_range(emacs_byte_range(100, 1_000));
    let _ = below.overlays_in_emacs_byte_range(emacs_byte_range(50, 90));
    assert!(
        below.overlay_view_query_underflowed(),
        "a range query below the view floor must request the whole-buffer retry"
    );

    let full = list.snapshot();
    let _ = full.overlays_at_emacs_byte_pos(EmacsBytePos::ZERO);
    assert!(
        !full.overlay_view_query_underflowed(),
        "a whole-buffer snapshot has no capture floor"
    );
}

#[test]
fn window_view_copies_follow_the_range_not_the_buffer() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for index in 0..65_536 {
        let overlay = alloc_overlay(index, index + 1);
        list.insert_overlay(overlay);
    }
    reset_overlay_snapshot_copy_count();
    let _view = list.snapshot_in_range(emacs_byte_range(1_000, 9_192));
    let copied = overlay_snapshot_copy_count();
    assert!(
        copied <= 8_194,
        "a 8192-byte window copied {copied} overlays"
    );
    assert!(
        copied < 65_536 / 4,
        "the window view copied {copied} of 65536 overlays"
    );
}

#[test]
fn incremental_digest_matches_a_full_walk_after_every_mutation() {
    crate::test_utils::init_test_tracing();
    fn check(list: &OverlayList) {
        assert_eq!(
            list.content_digest(),
            list.content_digest_from_scratch_for_test(),
            "the incremental component sum drifted from a full walk"
        );
    }

    let face = Value::symbol("face");
    let mut list = OverlayList::new();
    let mut overlays = Vec::new();
    for index in 0..50 {
        let overlay = alloc_overlay(index * 3, index * 3 + 2);
        list.insert_overlay(overlay);
        overlays.push(overlay);
    }
    check(&list);

    list.overlay_put(overlays[3], face, Value::symbol("bold"))
        .unwrap();
    check(&list);
    list.move_overlay_to_emacs_byte_range(overlays[4], emacs_byte_range(100, 120));
    check(&list);
    assert!(list.detach_overlay(overlays[5]));
    check(&list);
    list.overlay_put(overlays[3], face, Value::symbol("italic"))
        .unwrap();
    check(&list);
    list.overlay_put(overlays[6], Value::symbol("category"), Value::symbol("x"))
        .unwrap();
    check(&list);
    assert!(list.content_digest().is_none());

    // The redisplay cache mirrors writes into an unshared snapshot copy; that
    // copy's own digest must stay in step with a full walk as well.
    let snapshot = list.snapshot();
    list.overlay_put(overlays[7], face, Value::symbol("underline"))
        .unwrap();
    check(&list);
    assert_eq!(
        snapshot.content_digest(),
        snapshot.content_digest_from_scratch_for_test()
    );

    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(7), emacs_byte_len(1), false);
    check(&list);
}

#[test]
fn overlay_move_does_not_rescan_the_content_components() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let mut list = OverlayList::new();
    let mut overlays = Vec::new();
    for index in 0..4_096 {
        let overlay = alloc_overlay(index * 2, index * 2 + 1);
        list.insert_overlay(overlay);
        list.overlay_put(overlay, face, Value::symbol("bold"))
            .unwrap();
        overlays.push(overlay);
    }
    assert!(list.content_digest().is_some());
    reset_overlay_content_component_scan_count();
    list.move_overlay_to_emacs_byte_range(overlays[100], emacs_byte_range(10_000, 10_010));
    assert!(list.content_digest().is_some());
    assert_eq!(
        overlay_content_component_scan_count(),
        0,
        "one overlay move rehashed the whole overlay set"
    );

    reset_overlay_content_component_scan_count();
    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(9), emacs_byte_len(1), false);
    assert!(list.content_digest().is_some());
    assert_eq!(
        overlay_content_component_scan_count(),
        4_096,
        "a text edit must rebuild the component sum once"
    );
}

/// A range view reuses unchanged overlay copies across captures. Scrolling
/// with no overlay or plist edits must not copy any plist per redisplay, as
/// GNU's redisplay reads the live overlay plists in place.
#[test]
fn window_view_reuses_unchanged_copies_across_identical_captures() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for index in 0..2_000 {
        let overlay = alloc_overlay(index * 3, index * 3 + 1);
        list.insert_overlay(overlay);
    }
    let range = emacs_byte_range(0, 6_000);
    let first = list.snapshot_in_range(range);
    reset_overlay_snapshot_copy_count();
    let second = list.snapshot_in_range(range);
    assert_eq!(
        overlay_snapshot_copy_count(),
        0,
        "an unchanged range view copied every overlay again"
    );
    assert_eq!(serials_in(&first, 0, 6_000), serials_in(&second, 0, 6_000));
    // The reused copies are the same immutable objects.
    assert_eq!(
        overlays_at(&first, 0)[0].bits(),
        overlays_at(&second, 0)[0].bits()
    );
}

/// A plist write invalidates only that overlay's copy: the next view sees the
/// new plist for it, reuses every other copy, and a view captured before the
/// mutation keeps its state.
#[test]
fn window_view_copy_reuse_stops_at_a_plist_mutation() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let face = Value::symbol("face");
    let help = Value::symbol("help-echo");
    for index in 0..50 {
        let overlay = alloc_overlay(index * 3, index * 3 + 1);
        list.insert_overlay(overlay);
        list.overlay_put(overlay, face, Value::symbol("error"))
            .unwrap();
    }
    let range = emacs_byte_range(0, 200);
    let before = list.snapshot_in_range(range);
    let target = overlays_at(&list, 0)[0];
    let copy_before = overlays_at(&before, 0)[0];
    assert_eq!(
        before.overlay_get_named(copy_before, help),
        None,
        "the captured view starts without the later property"
    );

    reset_overlay_snapshot_copy_count();
    list.overlay_put(target, help, Value::symbol("h")).unwrap();
    let after = list.snapshot_in_range(range);
    assert_eq!(
        overlay_snapshot_copy_count(),
        1,
        "a plist write must refresh only the written overlay's copy"
    );
    let copy_after = overlays_at(&after, 0)[0];
    assert_eq!(
        before.overlay_get_named(copy_before, help),
        None,
        "the older view must keep the state it captured"
    );
    assert_eq!(
        after.overlay_get_named(copy_after, help),
        Some(Value::symbol("h"))
    );

    // The revisions are stable again after the write.
    reset_overlay_snapshot_copy_count();
    let _third = list.snapshot_in_range(range);
    assert_eq!(overlay_snapshot_copy_count(), 0);
}

/// A text edit moves overlays; the copies must follow without a redisplay
/// having to rebuild them once per frame, and each captured view keeps the
/// ranges it observed.
#[test]
fn window_view_copy_reuse_stops_at_a_text_edit() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for index in 0..50 {
        let overlay = alloc_overlay(index * 3, index * 3 + 1);
        list.insert_overlay(overlay);
    }
    let range = emacs_byte_range(0, 200);
    let before = list.snapshot_in_range(range);
    let first = overlays_at(&list, 0)[0];
    let first_before = overlays_at(&before, 0)[0];
    let observed_before = list.index_range_for_test(first).unwrap();

    reset_overlay_snapshot_copy_count();
    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(0), emacs_byte_len(1), false);
    let after = list.snapshot_in_range(range);
    assert!(
        overlay_snapshot_copy_count() > 0,
        "a text edit must refresh the range-view copies"
    );
    assert_eq!(
        before.index_range_for_test(first_before),
        Some(observed_before),
        "the older view keeps the ranges it captured"
    );
    let first_after = overlays_at(&after, 1)[0];
    assert_eq!(
        after.index_range_for_test(first_after),
        list.index_range_for_test(first),
        "the new view follows the edited range"
    );

    reset_overlay_snapshot_copy_count();
    let _third = list.snapshot_in_range(range);
    assert_eq!(overlay_snapshot_copy_count(), 0);
}

/// The real redisplay workload: hl-line moves one highlight overlay on every
/// C-n while the buffer's other overlays stay put. Only the highlight may be
/// re-copied per capture; a list-wide invalidation would re-copy every
/// visible overlay on every key.
#[test]
fn window_view_copies_only_the_highlight_that_moves() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let mut list = OverlayList::new();
    for index in 0..500 {
        let overlay = alloc_overlay(index * 4, index * 4 + 1);
        list.overlay_put(overlay, face, Value::symbol("bold"))
            .unwrap();
        list.insert_overlay(overlay);
    }
    let highlight = alloc_overlay(0, 2);
    list.insert_overlay(highlight);
    list.overlay_put(highlight, face, Value::symbol("hl-line"))
        .unwrap();
    let highlight_serial = overlay_identity_key(highlight).get();
    let fixed_serial = overlay_identity_key(overlays_at(&list, 4)[0]).get();
    let range = emacs_byte_range(0, 2_000);
    let first = list.snapshot_in_range(range);
    let first_fixed = overlays_at(&first, 4)
        .into_iter()
        .find(|overlay| overlay_identity_key(*overlay).get() == fixed_serial)
        .expect("a fixed overlay is in the first view");
    let first_fixed_bits = first_fixed.bits();

    for step in 0..200 {
        let start = 4 * (step + 1);
        list.move_overlay_to_emacs_byte_range(highlight, emacs_byte_range(start, start + 2));
        reset_overlay_snapshot_copy_count();
        let view = list.snapshot_in_range(range);
        assert_eq!(
            overlay_snapshot_copy_count(),
            1,
            "step {step}: moving the highlight re-copied more than the highlight"
        );
        // An unchanged overlay keeps the exact object captured earlier.
        let fixed = overlays_at(&view, 4)
            .into_iter()
            .find(|overlay| overlay_identity_key(*overlay).get() == fixed_serial)
            .expect("a fixed overlay is in the view");
        assert_eq!(fixed.bits(), first_fixed_bits);
    }
    // The highlight itself does refresh, and its copy carries the new range.
    let last = list.snapshot_in_range(range);
    let moved = overlays_at(&last, 800)
        .into_iter()
        .find(|overlay| overlay_identity_key(*overlay).get() == highlight_serial)
        .expect("the highlight copy is at its new position");
    assert_eq!(
        last.index_range_for_test(moved),
        Some(emacs_byte_range(800, 802))
    );
}

/// One unrelated plist write must not refresh any other visible overlay.
#[test]
fn window_view_copies_only_the_overlay_whose_plist_changed() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let help = Value::symbol("help-echo");
    let mut list = OverlayList::new();
    let mut overlays = Vec::new();
    for index in 0..500 {
        let overlay = alloc_overlay(index * 4, index * 4 + 1);
        list.overlay_put(overlay, face, Value::symbol("bold"))
            .unwrap();
        list.insert_overlay(overlay);
        overlays.push(overlay);
    }
    let fixed_serial = overlay_identity_key(overlays_at(&list, 4)[0]).get();
    let range = emacs_byte_range(0, 2_000);
    let first = list.snapshot_in_range(range);
    let witness = overlays_at(&first, 4)
        .into_iter()
        .find(|overlay| overlay_identity_key(*overlay).get() == fixed_serial)
        .expect("a fixed overlay is in the first view");
    let witness_bits = witness.bits();

    reset_overlay_snapshot_copy_count();
    list.overlay_put(overlays[400], help, Value::symbol("h"))
        .unwrap();
    let second = list.snapshot_in_range(range);
    assert_eq!(
        overlay_snapshot_copy_count(),
        1,
        "one unrelated overlay-put refreshed more than its own copy"
    );
    let witness_after = overlays_at(&second, 4)
        .into_iter()
        .find(|overlay| overlay_identity_key(*overlay).get() == fixed_serial)
        .expect("a fixed overlay is in the second view");
    assert_eq!(witness_after.bits(), witness_bits);
}

/// Every mutation class must make the next range view equal to a fresh copy
/// of the live state. A delete forgets that overlay's copy; re-inserting the
/// same object must not resurrect its pre-delete plist.
#[test]
fn window_view_follows_a_deletion_and_reinsertion() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let help = Value::symbol("help-echo");
    let mut list = OverlayList::new();
    let mut overlays = Vec::new();
    for index in 0..50 {
        let overlay = alloc_overlay(index * 3, index * 3 + 1);
        list.overlay_put(overlay, face, Value::symbol("error"))
            .unwrap();
        list.insert_overlay(overlay);
        overlays.push(overlay);
    }
    let range = emacs_byte_range(0, 200);
    let first = list.snapshot_in_range(range);
    reset_overlay_snapshot_copy_count();
    let _second = list.snapshot_in_range(range);
    assert_eq!(overlay_snapshot_copy_count(), 0);

    let gone = overlays[10];
    let gone_serial = overlay_identity_key(gone).get();
    list.delete_overlay(gone);
    reset_overlay_snapshot_copy_count();
    let after_delete = list.snapshot_in_range(range);
    assert_eq!(
        overlay_snapshot_copy_count(),
        0,
        "an unrelated deletion re-copied the surviving overlays"
    );
    assert_eq!(serials_in(&after_delete, 0, 200), serials_in(&list, 0, 200));
    assert!(
        !serials_in(&after_delete, 0, 200).contains(&gone_serial),
        "the deleted overlay is gone from the view"
    );

    // A plist write while detached is only observable after the reattach.
    list.overlay_put(gone, help, Value::symbol("h")).unwrap();
    list.insert_overlay(gone);
    reset_overlay_snapshot_copy_count();
    let reinserted = list.snapshot_in_range(range);
    assert_eq!(
        overlay_snapshot_copy_count(),
        1,
        "only the reinserted overlay needs a fresh copy"
    );
    let copy = overlays_at(&reinserted, 30)
        .into_iter()
        .find(|overlay| overlay_identity_key(*overlay).get() == gone_serial)
        .expect("the reinserted overlay is in the view");
    assert_eq!(
        reinserted.overlay_get_named(copy, help),
        Some(Value::symbol("h")),
        "the reinsertion reused the detached overlay's stale copy"
    );
    assert_eq!(
        overlays_at(&first, 30).len(),
        1,
        "the first view keeps what it captured"
    );
}

/// A delete text edit resizes or evaporates overlapping overlays; the next
/// view must equal the live state and reuse the untouched overlays.
#[test]
fn window_view_follows_a_delete_text_edit() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let mut list = OverlayList::new();
    for index in 0..50 {
        let overlay = alloc_overlay(index * 3, index * 3 + 1);
        list.overlay_put(overlay, face, Value::symbol("bold"))
            .unwrap();
        list.insert_overlay(overlay);
    }
    let _before = list.snapshot_in_range(emacs_byte_range(0, 200));

    reset_overlay_snapshot_copy_count();
    list.adjust_for_delete_emacs_byte_range(emacs_byte_range(0, 30));
    let after = list.snapshot_in_range(emacs_byte_range(0, 200));
    let copied = overlay_snapshot_copy_count();
    assert_eq!(
        copied, 50,
        "every overlay after the deleted span shifted; all copied ranges changed"
    );
    assert_eq!(serials_in(&after, 0, 200), serials_in(&list, 0, 200));
    for pos in 0..200 {
        assert_eq!(serials_at(&after, pos), serials_at(&list, pos), "pos {pos}");
    }
    reset_overlay_snapshot_copy_count();
    let _third = list.snapshot_in_range(emacs_byte_range(0, 200));
    assert_eq!(overlay_snapshot_copy_count(), 0);
}

/// An edit that moves no visible overlay's range leaves every copy reusable —
/// the case that made a per-edit generation clear unacceptable.
#[test]
fn window_view_copies_nothing_for_an_edit_outside_the_visible_ranges() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for index in 0..50 {
        list.insert_overlay(alloc_overlay(index * 3, index * 3 + 1));
    }
    let range = emacs_byte_range(0, 200);
    let _before = list.snapshot_in_range(range);

    reset_overlay_snapshot_copy_count();
    // Overlays live in 0..150; an insertion after them shifts none.
    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(500), emacs_byte_len(7), false);
    let after = list.snapshot_in_range(range);
    assert_eq!(
        overlay_snapshot_copy_count(),
        0,
        "an edit after every visible overlay re-copied the view"
    );
    assert_eq!(serials_in(&after, 0, 200), serials_in(&list, 0, 200));
    reset_overlay_snapshot_copy_count();
    let _third = list.snapshot_in_range(range);
    assert_eq!(overlay_snapshot_copy_count(), 0);
}

/// A replace edit shifts the overlays after the replaced span and resizes
/// those overlapping it; the next view must equal the live state.
#[test]
fn window_view_follows_a_replace_text_edit() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for index in 0..50 {
        list.insert_overlay(alloc_overlay(index * 3, index * 3 + 1));
    }
    let _before = list.snapshot_in_range(emacs_byte_range(0, 200));

    list.adjust_for_replace_at_emacs_byte_pos(
        emacs_byte_pos(15),
        emacs_byte_len(10),
        emacs_byte_len(4),
    );
    let after = list.snapshot_in_range(emacs_byte_range(0, 200));
    assert_eq!(serials_in(&after, 0, 200), serials_in(&list, 0, 200));
    for pos in 0..200 {
        assert_eq!(serials_at(&after, pos), serials_at(&list, pos), "pos {pos}");
    }
    reset_overlay_snapshot_copy_count();
    let _third = list.snapshot_in_range(emacs_byte_range(0, 200));
    assert_eq!(overlay_snapshot_copy_count(), 0);
}

/// Advance flags are part of the copied state: changing one refreshes that
/// overlay's copy only.
#[test]
fn window_view_follows_advance_flag_changes() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let mut overlays = Vec::new();
    for index in 0..50 {
        let overlay = alloc_overlay(index * 3, index * 3 + 1);
        list.insert_overlay(overlay);
        overlays.push(overlay);
    }
    let range = emacs_byte_range(0, 200);
    let _before = list.snapshot_in_range(range);

    reset_overlay_snapshot_copy_count();
    list.set_front_advance(overlays[5], true);
    list.set_rear_advance(overlays[6], true);
    let after = list.snapshot_in_range(range);
    assert_eq!(
        overlay_snapshot_copy_count(),
        2,
        "two advance-flag changes refreshed more than their own copies"
    );
    for (index, front, rear) in [(5usize, true, false), (6, false, true)] {
        let serial = overlay_identity_key(overlays[index]).get();
        let copy = overlays_at(&after, index * 3)
            .into_iter()
            .find(|overlay| overlay_identity_key(*overlay).get() == serial)
            .expect("the overlay is in the view");
        let copy = copy.as_overlay_data().unwrap();
        assert_eq!(copy.front_advance, front, "overlay {index}");
        assert_eq!(copy.rear_advance, rear, "overlay {index}");
    }
    reset_overlay_snapshot_copy_count();
    let _third = list.snapshot_in_range(range);
    assert_eq!(overlay_snapshot_copy_count(), 0);
}

/// Retargeting the buffer rewrites the copied `buffer` field of every overlay.
#[test]
fn window_view_follows_a_buffer_retarget() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for index in 0..50 {
        list.insert_overlay(alloc_overlay(index * 3, index * 3 + 1));
    }
    let range = emacs_byte_range(0, 200);
    let before = list.snapshot_in_range(range);
    assert_eq!(
        overlays_at(&before, 0)[0].as_overlay_data().unwrap().buffer,
        Some(BufferId(1))
    );

    reset_overlay_snapshot_copy_count();
    list.retarget_buffer(BufferId(1), BufferId(2));
    let after = list.snapshot_in_range(range);
    assert_eq!(
        overlay_snapshot_copy_count(),
        50,
        "a retarget rewrites every copied buffer field"
    );
    for pos in (0..150).step_by(3) {
        assert_eq!(
            overlays_at(&after, pos)[0]
                .as_overlay_data()
                .unwrap()
                .buffer,
            Some(BufferId(2)),
            "pos {pos}"
        );
    }
    assert_eq!(
        overlays_at(&before, 0)[0].as_overlay_data().unwrap().buffer,
        Some(BufferId(1)),
        "the older view keeps the buffer it captured"
    );
    reset_overlay_snapshot_copy_count();
    let _third = list.snapshot_in_range(range);
    assert_eq!(overlay_snapshot_copy_count(), 0);
}

/// Whole-list replacement drops every memo entry, and a later insert is
/// visible with a fresh copy.
#[test]
fn window_view_follows_a_whole_list_replacement() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for index in 0..50 {
        list.insert_overlay(alloc_overlay(index * 3, index * 3 + 1));
    }
    let range = emacs_byte_range(0, 200);
    let _before = list.snapshot_in_range(range);

    reset_overlay_snapshot_copy_count();
    list.delete_all_overlays();
    let empty = list.snapshot_in_range(range);
    assert_eq!(
        overlay_snapshot_copy_count(),
        0,
        "an empty list must not re-copy retained overlays"
    );
    assert!(serials_in(&empty, 0, 200).is_empty());

    for index in 0..5 {
        list.insert_overlay(alloc_overlay(index * 3, index * 3 + 1));
    }
    reset_overlay_snapshot_copy_count();
    let replaced = list.snapshot_in_range(range);
    assert_eq!(overlay_snapshot_copy_count(), 5);
    assert_eq!(serials_in(&replaced, 0, 200), serials_in(&list, 0, 200));
}

/// The range-view memo must never hand a snapshot copy to the Lisp-visible
/// list: a live overlay keeps its own object and plist across captures.
#[test]
fn window_view_never_replaces_a_live_overlay_object() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let mut list = OverlayList::new();
    let live = alloc_overlay(0, 2);
    list.overlay_put(live, face, Value::symbol("bold")).unwrap();
    list.insert_overlay(live);
    let live_bits = overlays_at(&list, 0)[0].bits();

    let view = list.snapshot_in_range(emacs_byte_range(0, 10));
    assert_eq!(
        overlays_at(&list, 0)[0].bits(),
        live_bits,
        "capturing a view replaced the live overlay object"
    );
    let copy = overlays_at(&view, 0)[0];
    assert_ne!(copy.bits(), live_bits, "the view owns a distinct object");
    assert_eq!(
        view.overlay_get_named(copy, face),
        Some(Value::symbol("bold"))
    );

    // A Lisp-visible `copy_overlays` must also own distinct objects, never a
    // memo copy (which several views share read-only).
    let lisp_copy = list.clone();
    let lisp_copy_overlay = overlays_at(&lisp_copy, 0)[0];
    assert_ne!(lisp_copy_overlay.bits(), live_bits);
    assert_ne!(lisp_copy_overlay.bits(), copy.bits());
    assert_eq!(
        lisp_copy.overlay_get_named(lisp_copy_overlay, face),
        Some(Value::symbol("bold"))
    );
}

/// The memo's copies are GC roots while reachable from it, and a write
/// releases the superseded copy. A view that already holds the old copy keeps
/// it alive through its own roots.
#[test]
fn window_view_copy_memo_values_are_rooted_and_replaced() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let mut list = OverlayList::new();
    let live = alloc_overlay(0, 2);
    list.overlay_put(live, face, Value::symbol("bold")).unwrap();
    list.insert_overlay(live);
    let view = list.snapshot_in_range(emacs_byte_range(0, 10));
    let memo_copy = overlays_at(&view, 0)[0];

    let mut roots = Vec::new();
    list.trace_roots(&mut roots);
    assert!(
        roots.iter().any(|value| eq_value(value, &memo_copy)),
        "the memo copy is not a root of the live list"
    );

    list.overlay_put(live, face, Value::symbol("italic"))
        .unwrap();
    roots.clear();
    list.trace_roots(&mut roots);
    assert!(
        !roots.iter().any(|value| eq_value(value, &memo_copy)),
        "a write left the superseded memo copy rooted"
    );
    let mut view_roots = Vec::new();
    view.trace_roots(&mut view_roots);
    assert!(
        view_roots.iter().any(|value| eq_value(value, &memo_copy)),
        "the observer view stopped rooting its captured copy"
    );

    let next = list.snapshot_in_range(emacs_byte_range(0, 10));
    let next_copy = overlays_at(&next, 0)[0];
    roots.clear();
    list.trace_roots(&mut roots);
    assert!(roots.iter().any(|value| eq_value(value, &next_copy)));
    assert_ne!(next_copy.bits(), memo_copy.bits());
}

/// The #702 counter row at Magit scale: one overlay per line plus hl-line,
/// 200 `next-line` keys from the midpoint of a small and a large log. Per-key
/// copies must follow the visible window (the entering line and the moved
/// highlight), not the buffer's overlay count, and must not grow with the log.
#[test]
fn hl_line_scrolling_copies_follow_the_window_not_the_buffer() {
    crate::test_utils::init_test_tracing();
    const LINE_BYTES: usize = 80;
    const WINDOW_LINES: usize = 50;
    const KEYS: usize = 200;

    fn copies_for(lines: usize) -> usize {
        let face = Value::symbol("face");
        let highlight = Value::symbol("highlight");
        let mut list = OverlayList::new();
        for line in 0..lines {
            list.insert_overlay(alloc_overlay(line * LINE_BYTES, line * LINE_BYTES + 1));
        }
        let first = lines / 2;
        let hl = alloc_overlay(first * LINE_BYTES, first * LINE_BYTES + 1);
        list.insert_overlay(hl);
        list.overlay_put(hl, face, highlight).unwrap();

        let mut copies = 0;
        for step in 0..KEYS {
            let line = first + step;
            list.move_overlay_to_emacs_byte_range(
                hl,
                emacs_byte_range(line * LINE_BYTES, line * LINE_BYTES + 1),
            );
            reset_overlay_snapshot_copy_count();
            let _view = list.snapshot_in_range(emacs_byte_range(
                line * LINE_BYTES,
                (line + WINDOW_LINES) * LINE_BYTES,
            ));
            copies += overlay_snapshot_copy_count();
        }
        copies
    }

    let small = copies_for(2_048);
    let large = copies_for(65_536);
    eprintln!("hl_line_scrolling copies: 2K={small} 64K={large} keys={KEYS}");
    assert!(
        small <= (WINDOW_LINES + 2) + 3 * KEYS,
        "2K-line scrolling copied {small} overlays for {KEYS} keys"
    );
    assert_eq!(
        small, large,
        "per-key copies grew with the buffer's overlay count"
    );
}

/// A position remap rewrites the copied ranges of every remapped overlay and
/// leaves no stale copy behind.
#[test]
fn window_view_follows_a_position_remap() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let mut overlays = Vec::new();
    for index in 0..50 {
        let overlay = alloc_overlay(index * 3, index * 3 + 1);
        list.insert_overlay(overlay);
        overlays.push(overlay);
    }
    let _before = list.snapshot_in_range(emacs_byte_range(0, 200));
    let remaps: Vec<OverlayPositionRemap> = overlays
        .iter()
        .enumerate()
        .map(|(index, overlay)| OverlayPositionRemap {
            overlay: *overlay,
            old_range: emacs_byte_range(index * 3, index * 3 + 1),
            range: emacs_byte_range(1_000 + index * 3, 1_001 + index * 3),
            old_begin: CharPos0::new(index * 3),
            new_begin: CharPos0::new(1_000 + index * 3),
        })
        .collect();

    reset_overlay_snapshot_copy_count();
    list.remap_overlay_positions(&remaps);
    let after = list.snapshot_in_range(emacs_byte_range(1_000, 1_200));
    assert_eq!(
        overlay_snapshot_copy_count(),
        50,
        "every remapped overlay needs a fresh copy"
    );
    assert_eq!(
        serials_in(&after, 1_000, 1_200),
        serials_in(&list, 1_000, 1_200)
    );
    for index in 0..50 {
        let serial = overlay_identity_key(overlays[index]).get();
        let copy = overlays_at(&after, 1_000 + index * 3)
            .into_iter()
            .find(|overlay| overlay_identity_key(*overlay).get() == serial)
            .expect("the remapped overlay is in the view");
        assert_eq!(
            after.index_range_for_test(copy),
            Some(emacs_byte_range(1_000 + index * 3, 1_001 + index * 3))
        );
    }
    reset_overlay_snapshot_copy_count();
    let _third = list.snapshot_in_range(emacs_byte_range(1_000, 1_200));
    assert_eq!(overlay_snapshot_copy_count(), 0);
}

/// Moving an evaporating overlay to an empty range deletes it through the
/// ordinary owner path: the memo forgets it, and re-inserting the same object
/// cannot resurrect its pre-deletion copy.
#[test]
fn window_view_forgets_an_evaporated_overlay() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let mut list = OverlayList::new();
    let mut overlays = Vec::new();
    for index in 0..10 {
        let overlay = alloc_overlay(index * 3, index * 3 + 1);
        list.overlay_put(overlay, face, Value::symbol("bold"))
            .unwrap();
        list.insert_overlay(overlay);
        overlays.push(overlay);
    }
    let target = overlays[4];
    let serial = overlay_identity_key(target).get();
    list.overlay_put(target, Value::symbol("evaporate"), Value::T)
        .unwrap();
    let _before = list.snapshot_in_range(emacs_byte_range(0, 100));

    reset_overlay_snapshot_copy_count();
    list.move_overlay_to_emacs_byte_range(target, emacs_byte_range(20, 20));
    assert!(
        !list.index.contains(target),
        "the evaporating move must delete the overlay"
    );
    let after = list.snapshot_in_range(emacs_byte_range(0, 100));
    assert_eq!(
        overlay_snapshot_copy_count(),
        0,
        "deleting an evaporated overlay re-copied the survivors"
    );
    assert!(!serials_in(&after, 0, 100).contains(&serial));

    // A detached write is only observable after the reattach, and the forget
    // means no pre-deletion copy can be reused.
    list.overlay_put(target, face, Value::symbol("italic"))
        .unwrap();
    list.insert_overlay(target);
    list.move_overlay_to_emacs_byte_range(target, emacs_byte_range(40, 41));
    reset_overlay_snapshot_copy_count();
    let reinserted = list.snapshot_in_range(emacs_byte_range(0, 100));
    assert_eq!(overlay_snapshot_copy_count(), 1);
    let copy = overlays_at(&reinserted, 40)
        .into_iter()
        .find(|overlay| overlay_identity_key(*overlay).get() == serial)
        .expect("the reinserted overlay is in the view");
    assert_eq!(
        reinserted.overlay_get_named(copy, face),
        Some(Value::symbol("italic"))
    );
}

/// Two concurrent range views may share one memoized copy. Attaching the
/// second view re-points the copy's weak position handle, but each view must
/// keep answering from its own index and its own captured state.
#[test]
fn two_concurrent_views_share_copies_without_cross_talk() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let mut list = OverlayList::new();
    let mut overlays = Vec::new();
    for index in 0..200 {
        let overlay = alloc_overlay(index * 3, index * 3 + 1);
        list.overlay_put(overlay, face, Value::symbol("bold"))
            .unwrap();
        list.insert_overlay(overlay);
        overlays.push(overlay);
    }
    let range = emacs_byte_range(0, 600);
    let first = list.snapshot_in_range(range);
    reset_overlay_snapshot_copy_count();
    let second = list.snapshot_in_range(range);
    assert_eq!(
        overlay_snapshot_copy_count(),
        0,
        "the second view must reuse the first view's copies"
    );
    for pos in (0..600).step_by(3) {
        assert_eq!(
            serials_at(&first, pos),
            serials_at(&second, pos),
            "pos {pos}"
        );
    }

    let target = overlays[10];
    let serial = overlay_identity_key(target).get();
    list.overlay_put(target, face, Value::symbol("italic"))
        .unwrap();
    reset_overlay_snapshot_copy_count();
    let third = list.snapshot_in_range(range);
    assert_eq!(overlay_snapshot_copy_count(), 1);
    let copy_in = |view: &OverlayList| {
        overlays_at(view, 30)
            .into_iter()
            .find(|overlay| overlay_identity_key(*overlay).get() == serial)
            .expect("the overlay is in the view")
    };
    let first_copy = copy_in(&first);
    let second_copy = copy_in(&second);
    let third_copy = copy_in(&third);
    assert_eq!(
        first.overlay_get_named(first_copy, face),
        Some(Value::symbol("bold")),
        "the first view keeps its captured plist"
    );
    assert_eq!(
        second.overlay_get_named(second_copy, face),
        Some(Value::symbol("bold"))
    );
    assert_eq!(
        third.overlay_get_named(third_copy, face),
        Some(Value::symbol("italic"))
    );
    // The shared copy's weak handle may point at the second view's tree, but
    // the first view still reports the range it captured.
    assert_eq!(
        first.index_range_for_test(first_copy),
        Some(emacs_byte_range(30, 31))
    );
    assert_eq!(
        second.index_range_for_test(second_copy),
        Some(emacs_byte_range(30, 31))
    );
}

/// Range-view copies keep the source serial for precedence identity, but they
/// are unreachable from the live list: no live accessor returns one, and the
/// live plist accessor refuses one, so Lisp cannot obtain and re-insert it.
#[test]
fn range_view_copies_cannot_be_obtained_through_the_live_list() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for index in 0..10 {
        list.insert_overlay(alloc_overlay(index * 3, index * 3 + 1));
    }
    let view = list.snapshot_in_range(emacs_byte_range(0, 100));
    for pos in 0..100 {
        let live = overlays_at(&list, pos);
        for copy in overlays_at(&view, pos) {
            assert!(
                !list.index.contains(copy),
                "a range-view copy is attached to the live index"
            );
            assert_eq!(
                list.overlay_plist(copy),
                None,
                "the live list exposed a range-view copy's plist"
            );
            assert!(
                !live.iter().any(|value| value.bits() == copy.bits()),
                "a live query returned a range-view copy"
            );
        }
    }
    // The serials match (precedence identity is preserved); object identity
    // does not.
    assert_eq!(serials_in(&list, 0, 100), serials_in(&view, 0, 100));
}
