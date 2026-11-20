//! Window-bounded overlay views for redisplay.
//!
//! A layout walk needs an immutable overlay set that is valid while the
//! evaluator keeps mutating the buffer. Copying the whole buffer's set makes
//! redisplay O(total overlays); these tests pin that the view is the same as
//! the full snapshot for every position inside its range, and that its copy
//! and digest costs follow the range.

use super::*;

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
