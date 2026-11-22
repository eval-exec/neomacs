//! Copied overlay headers must survive actual redisplay fontifier callbacks.
use super::*;
use neovm_core::emacs_core::xdisp::{
    ProbeRow, RedisplayFontificationRequest, ensure_fontified_for_redisplay_for_window_request,
};

fn check_snapshot_fontification_roots(bounded: bool) {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    eval.eval_str(
        r#"(progn
             (insert (make-string 4096 ?x))
             (setq snapshot-root-calls 0 snapshot-root-overlays nil)
             (let ((i 0))
               (while (< i 2)
                 (let ((ov (make-overlay (+ 2 (* i 8)) (+ 6 (* i 8)))))
                   (overlay-put ov 'face 'bold)
                   (overlay-put ov 'before-string "m")
                   (setq snapshot-root-overlays (cons ov snapshot-root-overlays)))
                 (setq i (1+ i))))
             (setq fontification-functions
               (list (lambda (_pos)
                       (setq snapshot-root-calls (1+ snapshot-root-calls))
                       (garbage-collect)))))"#,
    )
    .unwrap();
    let buffer = eval.buffer_manager().current_buffer().unwrap();
    let buffer_id = buffer.id();
    let snapshot = LayoutBufferSnapshot::from_buffer_for_window_with_symbols(
        buffer,
        eval.variables(),
        Some((0, 64)),
        bounded.then_some((0, 64)),
        crate::display_property::DisplayPropertyTarget::Terminal,
        eval.symbol_mode(),
        &eval.owned_root_retainer(),
    );
    let snapshot = {
        let retained = snapshot.clone();
        drop(snapshot);
        retained
    };
    let copied = snapshot
        .layout_overlays()
        .overlays_in_emacs_byte_range(EmacsByteRange::from_usize(0, 64));
    assert_eq!(copied.len(), 2, "two independently copied overlay plists");
    let addresses: Vec<_> = copied
        .iter()
        .map(|copy| copy.as_veclike_ptr().unwrap() as usize)
        .collect();
    let mut plist_cells = Vec::new();
    for copy in &copied {
        let mut cell = copy.as_overlay_data().unwrap().plist;
        while cell.is_cons() {
            plist_cells.push(cell);
            cell = cell.cons_cdr();
        }
    }
    assert_eq!(
        plist_cells.len(),
        8,
        "four independent copied cells per overlay"
    );
    if !bounded {
        // Retiring the live buffer's full-snapshot cache must not unroot an
        // already-held view. The pending copy still reads its old property.
        eval.eval_str("(overlay-put (car snapshot-root-overlays) 'face 'italic)")
            .unwrap();
    }
    let before_gc = eval.eval_str("gcs-done").unwrap().as_int().unwrap();

    // This is the same callback-bearing helper used by the layout engine's
    // visible fontification pass, while its LayoutBufferSnapshot is alive.
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
    assert_eq!(
        eval.eval_str("snapshot-root-calls").unwrap().as_int(),
        Some(1),
        "the actual fontification function must have forced exact GC"
    );
    assert!(
        eval.eval_str("gcs-done").unwrap().as_int().unwrap() > before_gc,
        "the callback's garbage-collect must complete an actual collection"
    );
    // Check numeric allocation-catalog membership BEFORE any potentially
    // stale copied Value is dereferenced. A red run must fail without a UAF.
    neovm_core::tagged::gc::with_tagged_heap(|heap| {
        for &address in &addresses {
            assert!(
                heap.non_cons_object_addrs_contains_for_test(address),
                "snapshot copied overlay lost across fontification GC; bounded={bounded}"
            );
        }
        for cell in plist_cells {
            assert!(
                heap.owns_heap_value_for_test(cell),
                "snapshot copied plist cell lost across callback GC"
            );
            assert!(heap.value_is_marked_for_test(cell));
        }
    });
    for copy in copied {
        assert_eq!(
            snapshot
                .layout_overlays()
                .overlay_get_named(copy, Value::symbol("face")),
            Some(Value::symbol("bold")),
            "pending view must retain its copied plist after callback GC"
        );
    }
    drop(snapshot);
    eval.gc_collect_exact();
    neovm_core::tagged::gc::with_tagged_heap(|heap| {
        for address in addresses {
            assert!(
                !heap.non_cons_object_addrs_contains_for_test(address),
                "dropping the final pending snapshot must release its copied overlay roots"
            );
        }
    });
}

#[test]
fn bounded_snapshot_overlay_copies_survive_fontification_gc() {
    check_snapshot_fontification_roots(true);
}

#[test]
fn retired_full_snapshot_overlay_copies_survive_fontification_gc() {
    check_snapshot_fontification_roots(false);
}
