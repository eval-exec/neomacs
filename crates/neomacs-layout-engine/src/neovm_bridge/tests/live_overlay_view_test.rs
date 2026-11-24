//! P4c/P4d: `NEOMACS_LAYOUT_OVERLAY_SOURCE=live` capture, stats, and GC safety
//! across fontification callbacks that delete overlays.

use super::*;
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::xdisp::{
    ProbeRow, RedisplayFontificationRequest, ensure_fontified_for_redisplay_for_window_request,
};

fn owned_native_root_count(eval: &Context, target_bits: usize) -> usize {
    // SAFETY: single-threaded unit test on the evaluator thread owning the active
    // tagged heap (`eval` is live and no concurrent mutator or GC is running).
    let heap_id =
        unsafe { neovm_core::tagged::gc::with_tagged_heap(|heap| heap.tagged_heap_identity()) };
    let mut owned = 0usize;
    let current = std::cell::Cell::new("");
    eval.trace_roots(
        heap_id,
        |roots| {
            // SAFETY: single-threaded unit test tracing roots on the active
            // evaluator thread's tagged heap while no GC or mutator runs concurrently.
            unsafe {
                neovm_core::tagged::gc::with_tagged_heap(|heap| {
                    neovm_value::tagged::transport::root_table::collect_shared_root_gc_roots(
                        heap, roots,
                    );
                });
            }
        },
        &mut |group| current.set(group),
        &mut |value| {
            if value.bits() == target_bits && current.get() == "owned_native" {
                owned += 1;
            }
        },
    );
    owned
}

#[test]
fn overlay_source_mode_defaults_to_captured_and_honors_live_env() {
    assert_eq!(parse_overlay_source_mode(None), OverlaySourceMode::Captured);
    assert_eq!(
        parse_overlay_source_mode(Some(std::ffi::OsStr::new(""))),
        OverlaySourceMode::Captured
    );
    assert_eq!(
        parse_overlay_source_mode(Some(std::ffi::OsStr::new("captured"))),
        OverlaySourceMode::Captured
    );
    assert_eq!(
        parse_overlay_source_mode(Some(std::ffi::OsStr::new("live"))),
        OverlaySourceMode::Live
    );
    assert_eq!(
        parse_overlay_source_mode(Some(std::ffi::OsStr::new("LIVE"))),
        OverlaySourceMode::Live
    );
    // Default process mode with env unset must stay Captured.
    // SAFETY: nextest runs each test in its own process.
    unsafe {
        std::env::remove_var("NEOMACS_LAYOUT_OVERLAY_SOURCE");
    }
    assert_eq!(overlay_source_mode(), OverlaySourceMode::Captured);
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
fn live_overlay_snapshot_counts_zero_copies_entries_roots_and_nonzero_live_records() {
    let dir = ScopedTempDir::new("neomacs-live-overlay-stats");
    // SAFETY: nextest runs each test in its own process, before OnceLocks initialize.
    unsafe {
        std::env::set_var("NEOMACS_LAYOUT_STATS_FILE", dir.path().join("stats"));
        std::env::set_var("NEOMACS_LAYOUT_OVERLAY_SOURCE", "live");
    }
    assert_eq!(overlay_source_mode(), OverlaySourceMode::Live);
    neovm_core::buffer::overlay::reset_overlay_layout_stats();

    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    eval.eval_str(
        r#"(progn
             (insert (make-string 128 ?x))
             (overlay-put (make-overlay 1 10) 'face 'bold)
             (overlay-put (make-overlay 20 30) 'face 'italic))"#,
    )
    .unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let before = neovm_core::buffer::overlay::overlay_layout_stats();
    let snapshot = LayoutBufferSnapshot::from_buffer_for_window_with_symbols(
        buffer,
        eval.variables(),
        Some((0, 64)),
        Some((0, 64)),
        crate::display_property::DisplayPropertyTarget::Terminal,
        eval.symbol_mode(),
        &eval.owned_root_retainer(),
    );
    let overlays = snapshot.layout_overlays();
    let found = overlays.overlays_at_emacs_byte_pos(EmacsBytePos::new(5));
    assert_eq!(found.len(), 1);
    assert_eq!(
        owned_native_root_count(&eval, found[0].bits()),
        0,
        "live overlay view must not retain native root leases for buffer overlays"
    );
    let _ = overlays.next_boundary_after_emacs_byte_pos(EmacsBytePos::new(0));
    let _ = overlays
        .iter_overlays_at_emacs_byte_pos(EmacsBytePos::new(25))
        .count();

    let after = neovm_core::buffer::overlay::overlay_layout_stats();
    assert_eq!(
        after.copies - before.copies,
        0,
        "live mode must copy 0 overlays"
    );
    assert_eq!(
        after.view_entries - before.view_entries,
        0,
        "live mode must attach 0 view entries"
    );
    assert_eq!(
        after.roots - before.roots,
        0,
        "live mode must retain 0 overlay roots"
    );
    assert_eq!(
        after.rebuilds - before.rebuilds,
        0,
        "live mode must trigger 0 whole-buffer rebuilds"
    );
    assert!(
        after.live_records > before.live_records,
        "live mode must count visited live records: before={before:?} after={after:?}"
    );
    assert!(!snapshot.layout_overlay_view_stale());
    assert_eq!(snapshot.layout_overlay_view_end_char(), None);
}

#[test]
fn live_overlay_view_survives_fontification_delete_and_gc_without_deref() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    eval.eval_str(
        r#"(progn
             (insert (make-string 1024 ?x))
             (setq live-gc-calls 0 live-gc-overlays nil)
             (let ((ov1 (make-overlay 2 10))
                   (ov2 (make-overlay 12 20)))
               (overlay-put ov1 'face 'bold)
               (overlay-put ov1 'before-string "A")
               (overlay-put ov2 'face 'italic)
               (setq live-gc-overlays (list ov1 ov2)))
             (setq fontification-functions
               (list (lambda (_pos)
                       (setq live-gc-calls (1+ live-gc-calls))
                       (while live-gc-overlays
                         (delete-overlay (car live-gc-overlays))
                         (setq live-gc-overlays (cdr live-gc-overlays)))
                       (garbage-collect)))))"#,
    )
    .unwrap();

    let buffer_id = eval.buffer_manager().current_buffer().unwrap().id();
    let snapshot = {
        let buffer = eval.buffer_manager().get(buffer_id).unwrap();
        LayoutBufferSnapshot::from_buffer_for_window_with_symbols_and_source(
            buffer,
            eval.variables(),
            Some((0, 64)),
            Some((0, 64)),
            crate::display_property::DisplayPropertyTarget::Terminal,
            eval.symbol_mode(),
            &eval.owned_root_retainer(),
            OverlaySourceMode::Live,
        )
    };

    let pre = snapshot
        .layout_overlays()
        .overlays_in_emacs_byte_range(EmacsByteRange::from_usize(0, 64));
    assert_eq!(
        pre.len(),
        2,
        "live view must observe both live overlays before callback"
    );
    assert!(!snapshot.layout_overlay_view_stale());

    let before_gc = eval.eval_str("gcs-done").unwrap().as_int().unwrap();
    ensure_fontified_for_redisplay_for_window_request(
        &mut eval,
        buffer_id,
        1,
        2,
        RedisplayFontificationRequest::VisibleWalk,
        ProbeRow::None,
        1,
    )
    .unwrap();
    assert_eq!(eval.eval_str("live-gc-calls").unwrap().as_int(), Some(1));
    assert!(
        eval.eval_str("gcs-done").unwrap().as_int().unwrap() > before_gc,
        "fontification callback must trigger a real GC after deleting overlays"
    );

    // The live view must now report Stale and short-circuit every read without
    // touching the deleted/collected overlay values.
    assert!(
        snapshot.layout_overlay_view_stale(),
        "deleting overlays in fontification-functions must mark the live view stale"
    );
    assert_eq!(
        snapshot.layout_overlay_view_unusable(CharPos0::new(10)),
        Some(super::super::OverlayViewUnusable::Stale)
    );
    let view = snapshot.layout_overlays();
    assert!(view.is_empty());
    assert!(
        view.overlays_at_emacs_byte_pos(EmacsBytePos::new(5))
            .is_empty()
    );
    assert!(
        view.overlays_in_emacs_byte_range(EmacsByteRange::from_usize(0, 64))
            .is_empty()
    );
    assert_eq!(
        view.next_boundary_after_emacs_byte_pos(EmacsBytePos::new(0)),
        None
    );
    assert_eq!(view.overlay_plist(pre[0]), None);
    assert_eq!(view.overlay_get_named(pre[0], Value::symbol("face")), None);
    assert!(!view.overlay_applies_to_window(pre[0], Some(1)));
    assert_eq!(view.overlay_start_emacs_byte_pos(pre[0]), None);
    assert_eq!(view.overlay_end_emacs_byte_pos(pre[0]), None);
}
