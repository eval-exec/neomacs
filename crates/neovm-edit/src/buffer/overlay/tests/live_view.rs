//! P4a: generation-checked `LiveOverlayView` and the shared `OverlayRead`
//! surface.

use super::*;
use crate::buffer::{BufferId, BufferManager};

fn assert_same_values(actual: &[Value], expected: &[Value], label: &str) {
    let actual_bits: Vec<usize> = actual.iter().map(|v| v.bits()).collect();
    let expected_bits: Vec<usize> = expected.iter().map(|v| v.bits()).collect();
    assert_eq!(actual_bits, expected_bits, "{label}");
}

#[test]
fn live_view_matches_overlay_list_across_all_read_queries_and_chunks() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let face = Value::symbol("face");
    let mouse_face = Value::symbol("mouse-face");
    let priority = Value::symbol("priority");
    let window = Value::symbol("window");
    let missing = Value::symbol("nonexistent-prop");

    // 80 overlays produce 160 endpoints, exercising multi-chunk forward and
    // reverse endpoint scans (> ENDPOINT_CHUNK_SIZE = 64).
    let mut overlays = Vec::new();
    for i in 0..80usize {
        let start = 10 + i * 4;
        let end = start + 6;
        let ov = alloc_overlay(start, end);
        list.insert_overlay(ov);
        list.overlay_put(
            ov,
            face,
            Value::symbol(if i % 2 == 0 { "bold" } else { "italic" }),
        )
        .unwrap();
        if i % 3 == 0 {
            list.overlay_put(ov, mouse_face, Value::symbol("highlight"))
                .unwrap();
        }
        list.overlay_put(ov, priority, Value::fixnum((i % 7) as i64))
            .unwrap();
        if i % 10 == 0 {
            list.overlay_put(ov, window, Value::make_window(1)).unwrap();
        }
        overlays.push(ov);
    }

    let live = list.live_view();
    assert!(!live.is_stale());
    assert_eq!(live.generation_stamp(), list.overlay_generation());
    assert_eq!(OverlayRead::is_empty(&live), OverlayRead::is_empty(&list));
    assert_eq!(
        OverlayRead::may_contain_property(&live, face),
        OverlayRead::may_contain_property(&list, face)
    );
    assert_eq!(
        OverlayRead::may_contain_property(&live, missing),
        OverlayRead::may_contain_property(&list, missing)
    );

    assert_same_values(
        &OverlayRead::overlays_in_gnu_lists_order(&live),
        &OverlayRead::overlays_in_gnu_lists_order(&list),
        "overlays_in_gnu_lists_order",
    );

    for pos in [0usize, 10, 14, 15, 50, 120, 328, 400] {
        let bpos = emacs_byte_pos(pos);
        assert_same_values(
            &OverlayRead::overlays_at_emacs_byte_pos(&live, bpos),
            &OverlayRead::overlays_at_emacs_byte_pos(&list, bpos),
            "overlays_at",
        );
        let live_iter: Vec<Value> =
            OverlayRead::iter_overlays_at_emacs_byte_pos(&live, bpos).collect();
        let list_iter: Vec<Value> =
            OverlayRead::iter_overlays_at_emacs_byte_pos(&list, bpos).collect();
        assert_same_values(&live_iter, &list_iter, "iter_overlays_at");
        assert_eq!(
            OverlayRead::next_boundary_after_emacs_byte_pos(&live, bpos),
            OverlayRead::next_boundary_after_emacs_byte_pos(&list, bpos),
            "next_boundary_after({pos})"
        );
        for wid in [None, Some(1), Some(2)] {
            assert_eq!(
                OverlayRead::highest_priority_overlay_property_value_at_emacs_byte_pos(
                    &live, bpos, face, wid
                )
                .map(Value::bits),
                OverlayRead::highest_priority_overlay_property_value_at_emacs_byte_pos(
                    &list, bpos, face, wid
                )
                .map(Value::bits),
                "highest_priority({pos}, {wid:?})"
            );
        }
    }

    let span = emacs_byte_range(12, 200);
    assert_same_values(
        &OverlayRead::overlays_in_emacs_byte_range(&live, span),
        &OverlayRead::overlays_in_emacs_byte_range(&list, span),
        "overlays_in_emacs_byte_range",
    );
    let live_acc: Vec<Value> =
        OverlayRead::iter_overlays_in_accessible_emacs_byte_range(&live, span, emacs_byte_pos(400))
            .collect();
    let list_acc: Vec<Value> =
        OverlayRead::iter_overlays_in_accessible_emacs_byte_range(&list, span, emacs_byte_pos(400))
            .collect();
    assert_same_values(&live_acc, &list_acc, "iter_overlays_in_accessible");

    let sample = overlays[10];
    assert_eq!(
        OverlayRead::overlay_start_emacs_byte_pos(&live, sample),
        OverlayRead::overlay_start_emacs_byte_pos(&list, sample)
    );
    assert_eq!(
        OverlayRead::overlay_end_emacs_byte_pos(&live, sample),
        OverlayRead::overlay_end_emacs_byte_pos(&list, sample)
    );
    assert_eq!(
        OverlayRead::overlay_get_named(&live, sample, face).map(Value::bits),
        OverlayRead::overlay_get_named(&list, sample, face).map(Value::bits)
    );
    assert_eq!(
        OverlayRead::overlay_plist(&live, sample).map(Value::bits),
        OverlayRead::overlay_plist(&list, sample).map(Value::bits)
    );
    assert_eq!(
        OverlayRead::overlay_applies_to_window(&live, sample, Some(1)),
        OverlayRead::overlay_applies_to_window(&list, sample, Some(1))
    );
    assert_eq!(
        OverlayRead::overlay_applies_to_window(&live, sample, Some(2)),
        OverlayRead::overlay_applies_to_window(&list, sample, Some(2))
    );

    let mut live_sorted = overlays[0..12].to_vec();
    let mut list_sorted = live_sorted.clone();
    OverlayRead::sort_overlay_ids_by_priority_desc(&live, &mut live_sorted);
    OverlayRead::sort_overlay_ids_by_priority_desc(&list, &mut list_sorted);
    assert_same_values(
        &live_sorted,
        &list_sorted,
        "sort_overlay_ids_by_priority_desc",
    );

    // Multi-chunk property sweep & extent resolution must match identically.
    let bounds = emacs_byte_range(0, 400);
    let list_runs: Vec<OverlayPropertyRun> =
        match list.resolve_overlay_property_at_emacs_byte_pos(emacs_byte_pos(10), Some(1), |ov| {
            list.overlay_get_named(ov, mouse_face)
        }) {
            OverlayPropertyAtPoint::Present(res) => res.sweep(bounds).unwrap().collect(),
            OverlayPropertyAtPoint::Vacant(_) => panic!("expected present mouse-face at 10"),
        };
    let live_runs: Vec<OverlayPropertyRun> =
        match live.resolve_overlay_property_at_emacs_byte_pos(emacs_byte_pos(10), Some(1), |ov| {
            live.overlay_get_named(ov, mouse_face)
        }) {
            OverlayPropertyAtPoint::Present(res) => res.sweep(bounds).unwrap().collect(),
            OverlayPropertyAtPoint::Vacant(_) => panic!("expected present mouse-face at 10"),
        };
    assert_eq!(
        live_runs, list_runs,
        "multi-chunk property sweep must match"
    );
    assert!(live_runs.len() > 10);

    let list_extent =
        match list.resolve_overlay_property_at_emacs_byte_pos(emacs_byte_pos(12), Some(1), |ov| {
            list.overlay_get_named(ov, mouse_face)
        }) {
            OverlayPropertyAtPoint::Present(res) => res.extent(bounds),
            OverlayPropertyAtPoint::Vacant(_) => None,
        };
    let live_extent =
        match live.resolve_overlay_property_at_emacs_byte_pos(emacs_byte_pos(12), Some(1), |ov| {
            live.overlay_get_named(ov, mouse_face)
        }) {
            OverlayPropertyAtPoint::Present(res) => res.extent(bounds),
            OverlayPropertyAtPoint::Vacant(_) => None,
        };
    assert_eq!(live_extent, list_extent);
    assert!(!live.is_stale());
}

#[test]
fn live_view_becomes_stale_after_writes_and_returns_empty_safely() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let ov = alloc_overlay(10, 20);
    list.insert_overlay(ov);
    list.overlay_put(ov, Value::symbol("face"), Value::symbol("bold"))
        .unwrap();

    let live = list.live_view();
    assert!(!live.is_stale());
    assert_eq!(live.overlays_at_emacs_byte_pos(emacs_byte_pos(12)).len(), 1);

    // Mutate the list; the live view must detect staleness without deref.
    list.overlay_put(ov, Value::symbol("face"), Value::symbol("italic"))
        .unwrap();
    assert!(live.is_stale());
    assert!(
        live.overlays_at_emacs_byte_pos(emacs_byte_pos(12))
            .is_empty()
    );
    assert!(
        live.overlays_in_emacs_byte_range(emacs_byte_range(0, 30))
            .is_empty()
    );
    assert!(live.overlays_in_gnu_lists_order().is_empty());
    assert_eq!(
        live.next_boundary_after_emacs_byte_pos(emacs_byte_pos(0)),
        None
    );
    assert!(!live.may_contain_property(Value::symbol("face")));
    assert!(live.is_empty());
    // Stale overlay reads must short-circuit before touching overlay memory.
    assert_eq!(live.overlay_plist(Value::NIL), None);
    assert_eq!(
        live.overlay_get_named(Value::NIL, Value::symbol("face")),
        None
    );
    assert!(!live.overlay_applies_to_window(Value::NIL, Some(1)));
    assert_eq!(live.overlay_start_emacs_byte_pos(Value::NIL), None);
    assert_eq!(live.overlay_end_emacs_byte_pos(Value::NIL), None);

    // delete_all_overlays replaces `intervals` Arc while bumping `core`:
    // an older live view must still detect staleness immediately.
    let live2 = list.live_view();
    assert!(!live2.is_stale());
    list.delete_all_overlays();
    assert!(live2.is_stale());
    assert!(
        live2
            .overlays_at_emacs_byte_pos(emacs_byte_pos(12))
            .is_empty()
    );

    // buffer-swap-text on an empty overlay list must also invalidate its live view.
    let mut manager = BufferManager::new();
    let empty_id = manager.create_buffer("*live-swap-empty*");
    let other_id = manager.create_buffer("*live-swap-other*");
    manager.insert_into_buffer(empty_id, "abcdef").unwrap();
    manager.insert_into_buffer(other_id, "abcdef").unwrap();
    let live_empty = manager.get(empty_id).unwrap().overlays.live_view();
    assert!(!live_empty.is_stale());
    manager.swap_buffer_text(empty_id, other_id).unwrap();
    assert!(
        live_empty.is_stale(),
        "buffer-swap-text must invalidate a live view captured from an empty overlay list"
    );
}

struct ScopedTempDir(std::path::PathBuf);

impl ScopedTempDir {
    fn new(prefix: &str) -> Self {
        let base = std::env::var_os("CARGO_TARGET_TMPDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!("{prefix}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scoped test tempdir");
        Self(dir)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for ScopedTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn live_view_records_zero_copies_zero_view_entries_and_nonzero_live_records() {
    crate::test_utils::init_test_tracing();
    let stats_dir = ScopedTempDir::new("neomacs-live-view-stats");
    let stats_path = stats_dir.path().join("stats.log");
    // SAFETY: nextest runs each test in its own process, before `ENABLED` is initialized.
    unsafe {
        std::env::set_var("NEOMACS_LAYOUT_STATS_FILE", &stats_path);
    }
    reset_overlay_layout_stats();

    let mut list = OverlayList::new();
    for i in 0..10usize {
        let ov = alloc_overlay(i * 4, i * 4 + 3);
        list.insert_overlay(ov);
        list.overlay_put(ov, Value::symbol("face"), Value::symbol("bold"))
            .unwrap();
    }
    let live = list.live_view();
    let _ = live.overlays_at_emacs_byte_pos(emacs_byte_pos(5));
    let _ = live.next_boundary_after_emacs_byte_pos(emacs_byte_pos(0));
    let _ = live
        .iter_overlays_at_emacs_byte_pos(emacs_byte_pos(9))
        .count();

    let stats = overlay_layout_stats();
    assert_eq!(stats.copies, 0, "live view must not copy overlays");
    assert_eq!(
        stats.view_entries, 0,
        "live view must not build view entries"
    );
    assert_eq!(stats.roots, 0, "live view must not retain overlay roots");
    assert_eq!(
        stats.rebuilds, 0,
        "live view must not trigger whole-buffer rebuilds"
    );
    assert!(
        stats.live_records > 0,
        "live view reads must count visited live records: {stats:?}"
    );
}
