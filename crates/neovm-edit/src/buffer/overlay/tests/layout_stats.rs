//! P1 counters for the overlay layout view.
//!
//! nextest gives each test its own process, so the stats OnceLock sees only
//! this test's environment. Set or clear `NEOMACS_LAYOUT_STATS_FILE` before
//! the first read.

use super::*;

fn set_layout_stats_env() {
    let dir = std::env::temp_dir().join(format!(
        "neomacs-overlay-layout-stats-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("overlay layout stats tempdir");
    // SAFETY: nextest runs this test in its own process, and this runs before
    // any worker can read the once-per-process knob.
    unsafe {
        std::env::set_var("NEOMACS_LAYOUT_STATS_FILE", dir.join("stats"));
    }
}

fn clear_layout_stats_env() {
    // SAFETY: same process isolation as `set_layout_stats_env`.
    unsafe {
        std::env::remove_var("NEOMACS_LAYOUT_STATS_FILE");
    }
}

fn three_overlays() -> OverlayList {
    let mut list = OverlayList::new();
    for start in [0usize, 10, 20] {
        list.insert_overlay(alloc_overlay(start, start + 4));
    }
    list
}

#[test]
fn layout_stats_record_a_whole_buffer_rebuild_and_its_view() {
    set_layout_stats_env();
    crate::test_utils::init_test_tracing();
    assert!(
        overlay_layout_stats_enabled(),
        "the stats file must latch on before the snapshot"
    );
    reset_overlay_layout_stats();
    let list = three_overlays();
    let _first = list.snapshot();
    let stats = overlay_layout_stats();
    assert_eq!(stats.copies, 3, "each preserve-copy is counted: {stats:?}");
    assert_eq!(
        stats.view_entries, 3,
        "the clone attaches every entry: {stats:?}"
    );
    assert_eq!(
        stats.rebuilds, 1,
        "the cache miss is one whole-buffer rebuild: {stats:?}"
    );
    assert_eq!(stats.roots, 0, "the list does not retain roots: {stats:?}");
    assert_eq!(
        stats.live_records, 0,
        "the live walk is not this slice: {stats:?}"
    );

    let _second = list.snapshot();
    assert_eq!(
        overlay_layout_stats(),
        stats,
        "a cache hit copies nothing and attaches nothing"
    );
}

#[test]
fn layout_stats_range_view_attaches_entries_without_a_rebuild() {
    set_layout_stats_env();
    crate::test_utils::init_test_tracing();
    reset_overlay_layout_stats();
    let list = three_overlays();
    let _view = list.snapshot_in_range(emacs_byte_range(0, 30));
    let stats = overlay_layout_stats();
    assert_eq!(stats.rebuilds, 0, "{stats:?}");
    assert_eq!(stats.copies, 3, "{stats:?}");
    assert_eq!(stats.view_entries, stats.copies, "{stats:?}");
    assert_eq!(stats.live_records, 0, "{stats:?}");
}

#[test]
fn layout_stats_off_counts_copies_for_tests_and_leaves_the_view_row_at_zero() {
    clear_layout_stats_env();
    crate::test_utils::init_test_tracing();
    assert!(
        !overlay_layout_stats_enabled(),
        "an empty env must latch the stats row off"
    );
    reset_overlay_layout_stats();
    let list = three_overlays();
    let _snapshot = list.snapshot();
    let stats = overlay_layout_stats();
    assert_eq!(stats.view_entries, 0, "{stats:?}");
    assert_eq!(stats.rebuilds, 0, "{stats:?}");
    assert_eq!(stats.roots, 0, "{stats:?}");
    assert_eq!(stats.live_records, 0, "{stats:?}");
    assert_eq!(stats.copies, 3, "{stats:?}");
    assert_eq!(overlay_snapshot_copy_count(), 3);
}

#[test]
fn format_overlay_layout_stats_appends_stable_keys() {
    let line = format_overlay_layout_stats(OverlayLayoutStats {
        copies: 2,
        view_entries: 9,
        roots: 9,
        rebuilds: 1,
        live_records: 0,
    });
    assert_eq!(
        line,
        "overlay_copies=2 overlay_view_entries=9 overlay_roots=9 overlay_rebuilds=1 overlay_live_records=0"
    );
}
