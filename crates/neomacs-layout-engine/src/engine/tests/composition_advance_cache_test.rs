//! The retained automatic-composition advance cache must seed a full body
//! layout only while every input that decides the shaped width is unchanged,
//! and it must not retain entries for windows the accepted frame dropped
//! (N-orphan C1/C2/C3).

use super::*;

/// One fully populated cache whose validation inputs are easy to shift one at
/// a time.
fn baseline_advances() -> RetainedCompositionAdvances {
    RetainedCompositionAdvances {
        buffer_id: neovm_core::buffer::BufferId(7),
        buffer_modiff: 11,
        overlay_modified_tick: 13,
        face_change_count: 17,
        display_var_change_count: 19,
        body_redisplay: neovm_core::window::BodyRedisplayRevision::new(23, 29, 31),
        fontset_generation: 37,
        window_parameters_generation: 41,
        char_width_bits: 8.0f32.to_bits(),
        advances: rustc_hash::FxHashMap::default(),
    }
}

#[allow(clippy::too_many_arguments)]
fn current(
    cache: &RetainedCompositionAdvances,
    buffer_id: neovm_core::buffer::BufferId,
    buffer_modiff: i64,
    overlay_modified_tick: i64,
    face_change_count: u64,
    display_var_change_count: u64,
    body_redisplay: neovm_core::window::BodyRedisplayRevision,
    fontset_generation: u64,
    window_parameters_generation: u64,
    char_width_bits: u32,
) -> bool {
    cache.is_current_for(
        buffer_id,
        buffer_modiff,
        overlay_modified_tick,
        face_change_count,
        display_var_change_count,
        body_redisplay,
        fontset_generation,
        window_parameters_generation,
        char_width_bits,
    )
}

/// Every field of the cache key is load-bearing: moving exactly one of them
/// must reject the retained advances, including the face-remapping
/// (`display_var_change_count`) and forced-body-redisplay (`body_redisplay`)
/// terms that a `text-scale-adjust`/`force-window-update` moves without
/// touching any buffer/face/font tick.
#[test]
fn retained_composition_advances_reject_each_moved_input() {
    let cache = baseline_advances();
    let body = cache.body_redisplay;
    let buffer = cache.buffer_id;
    let bits = cache.char_width_bits;
    let unchanged = |cache: &RetainedCompositionAdvances| {
        current(
            cache,
            buffer,
            cache.buffer_modiff,
            cache.overlay_modified_tick,
            cache.face_change_count,
            cache.display_var_change_count,
            body,
            cache.fontset_generation,
            cache.window_parameters_generation,
            bits,
        )
    };
    assert!(unchanged(&cache), "the baseline key must accept itself");

    assert!(
        !current(
            &cache,
            buffer,
            cache.buffer_modiff + 1,
            cache.overlay_modified_tick,
            cache.face_change_count,
            cache.display_var_change_count,
            body,
            cache.fontset_generation,
            cache.window_parameters_generation,
            bits,
        ),
        "a text/property edit must reject the retained advances"
    );
    assert!(
        !current(
            &cache,
            buffer,
            cache.buffer_modiff,
            cache.overlay_modified_tick + 1,
            cache.face_change_count,
            cache.display_var_change_count,
            body,
            cache.fontset_generation,
            cache.window_parameters_generation,
            bits,
        ),
        "an overlay edit must reject the retained advances"
    );
    assert!(
        !current(
            &cache,
            buffer,
            cache.buffer_modiff,
            cache.overlay_modified_tick,
            cache.face_change_count + 1,
            cache.display_var_change_count,
            body,
            cache.fontset_generation,
            cache.window_parameters_generation,
            bits,
        ),
        "a face definition change must reject the retained advances"
    );
    assert!(
        !current(
            &cache,
            buffer,
            cache.buffer_modiff,
            cache.overlay_modified_tick,
            cache.face_change_count,
            cache.display_var_change_count + 1,
            body,
            cache.fontset_generation,
            cache.window_parameters_generation,
            bits,
        ),
        "a face-remapping-alist write (text scale) must reject the retained advances"
    );
    assert!(
        !current(
            &cache,
            buffer,
            cache.buffer_modiff,
            cache.overlay_modified_tick,
            cache.face_change_count,
            cache.display_var_change_count,
            neovm_core::window::BodyRedisplayRevision::new(23, 29, 32),
            cache.fontset_generation,
            cache.window_parameters_generation,
            bits,
        ),
        "a force-window-update on the buffer must reject the retained advances"
    );
    assert!(
        !current(
            &cache,
            buffer,
            cache.buffer_modiff,
            cache.overlay_modified_tick,
            cache.face_change_count,
            cache.display_var_change_count,
            neovm_core::window::BodyRedisplayRevision::new(23, 30, 31),
            cache.fontset_generation,
            cache.window_parameters_generation,
            bits,
        ),
        "a force-window-update on the window must reject the retained advances"
    );
    assert!(
        !current(
            &cache,
            buffer,
            cache.buffer_modiff,
            cache.overlay_modified_tick,
            cache.face_change_count,
            cache.display_var_change_count,
            body,
            cache.fontset_generation + 1,
            cache.window_parameters_generation,
            bits,
        ),
        "a fontset change must reject the retained advances"
    );
    assert!(
        !current(
            &cache,
            buffer,
            cache.buffer_modiff,
            cache.overlay_modified_tick,
            cache.face_change_count,
            cache.display_var_change_count,
            body,
            cache.fontset_generation,
            cache.window_parameters_generation + 1,
            bits,
        ),
        "a window face parameter change must reject the retained advances"
    );
    assert!(
        !current(
            &cache,
            buffer,
            cache.buffer_modiff,
            cache.overlay_modified_tick,
            cache.face_change_count,
            cache.display_var_change_count,
            body,
            cache.fontset_generation,
            cache.window_parameters_generation,
            9.0f32.to_bits(),
        ),
        "a window char-width change must reject the retained advances"
    );
    assert!(
        !current(
            &cache,
            neovm_core::buffer::BufferId(8),
            cache.buffer_modiff,
            cache.overlay_modified_tick,
            cache.face_change_count,
            cache.display_var_change_count,
            body,
            cache.fontset_generation,
            cache.window_parameters_generation,
            bits,
        ),
        "a different displayed buffer must reject the retained advances"
    );
}

/// The published map is bounded by the current snapshot's spans: a resolved
/// span the snapshot still has wins, a span this pass did not resolve keeps
/// its validated seed (a replay band must not erase a full-screen pass), and
/// anything outside the current spans is dropped.
#[test]
fn retained_advances_keep_seeded_widths_only_for_current_spans() {
    let mut resolved = rustc_hash::FxHashMap::default();
    resolved.insert(0usize, 7.0f32);
    resolved.insert(20usize, 9.0f32);
    resolved.insert(40usize, 11.0f32); // no longer in the snapshot
    let mut seeded = rustc_hash::FxHashMap::default();
    seeded.insert((0usize, 2usize), 1.0f32); // resolution wins
    seeded.insert((10usize, 14usize), 3.0f32); // seeded, unresolved this pass
    seeded.insert((40usize, 44usize), 5.0f32); // outside the snapshot
    let spans = [(0usize, 2usize), (10usize, 14usize)];
    let retained = retained_composition_advances_for_spans(spans.into_iter(), &resolved, &seeded);
    assert_eq!(retained.get(&(0, 2)), Some(&7.0));
    assert_eq!(retained.get(&(10, 14)), Some(&3.0));
    assert_eq!(retained.len(), 2, "spans outside the snapshot are dropped");
}

/// The accepted break drops parked advances for windows this frame no longer
/// has, so a deleted window cannot leave its shaped widths behind.
#[test]
fn retained_advance_pruning_drops_windows_the_frame_no_longer_has() {
    let mut caches = rustc_hash::FxHashMap::default();
    for id in 1..=3 {
        caches.insert(DisplayWindowId::new(id), baseline_advances());
    }
    prune_retained_composition_advances(&mut caches, |window| window.get() == 2);
    assert_eq!(caches.len(), 1);
    assert!(caches.contains_key(&DisplayWindowId::new(2)));
}

/// A real layout records the key the C1 invalidation reads, and each
/// production mutation path moves exactly the term it owns. The point-only
/// control keeps seeding: it is deliberately not part of the key because
/// GNU's matrix-row reuse survives cursor motion.
#[test]
fn composition_cache_key_tracks_real_display_inputs() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    crate::test_composition::install_rules(&mut eval);
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    eval.buffer_manager_mut()
        .get_mut(buf_id)
        .expect("buffer")
        .insert("e\u{301}b\n");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("composition-cache-key", 640, 160, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    realize_test_gui_frame(&mut eval, frame_id);
    let window = DisplayWindowId::new(window_id.0 as i64);

    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame_id);

    let key = |engine: &LayoutEngine| {
        let cache = engine
            .retained_composition_advances
            .get(&window)
            .expect("every laid-out window has a composition cache entry");
        (
            cache.buffer_modiff,
            cache.overlay_modified_tick,
            cache.face_change_count,
            cache.display_var_change_count,
            cache.body_redisplay,
        )
    };

    let before = key(&engine);
    // `setq-local` is a Lisp macro from subr.el and this is an unbootstrapped
    // VM; `set` writes the buffer-local slot through the same variable-set
    // chokepoint (`mark_redisplay_dirty_if_display_var`).
    eval.eval_str("(set 'face-remapping-alist '((default (:height 2.0))))")
        .expect("face-remapping-alist is a display-affecting buffer-local");
    engine.retained_window_matrices.clear();
    engine.layout_frame_rust(&mut eval, frame_id);
    let after = key(&engine);
    assert_ne!(
        after.3, before.3,
        "a face-remapping-alist write must move display_var_change_count"
    );

    let before = after;
    eval.force_body_redisplay(neovm_core::window::ForcedBodyRedisplay::AllWindows);
    engine.retained_window_matrices.clear();
    engine.layout_frame_rust(&mut eval, frame_id);
    let after = key(&engine);
    assert_ne!(
        after.4, before.4,
        "force-window-update must move body_redisplay"
    );

    let before = after;
    eval.eval_str("(progn (goto-char (point-max)) (insert \"x\"))")
        .expect("edit");
    engine.retained_window_matrices.clear();
    engine.layout_frame_rust(&mut eval, frame_id);
    let after = key(&engine);
    assert_ne!(after.0, before.0, "an edit must move buffer_modiff");

    let before = after;
    eval.eval_str("(overlay-put (make-overlay 1 2) 'face 'bold)")
        .expect("overlay");
    engine.retained_window_matrices.clear();
    engine.layout_frame_rust(&mut eval, frame_id);
    let after = key(&engine);
    assert_ne!(
        after.1, before.1,
        "an overlay change must move overlay_modified_tick"
    );

    let before = after;
    eval.eval_str("(set-window-point (selected-window) 2)")
        .expect("point motion");
    engine.retained_window_matrices.clear();
    engine.layout_frame_rust(&mut eval, frame_id);
    let after = key(&engine);
    assert_eq!(
        after, before,
        "point/repaint-only motion must keep every composition input unchanged"
    );
}
