//! Real warmed Sync producer and complete renderer observations.
//! Constructor-site counters observe actual lookup allocations, not a model.

use super::*;
use crate::buffer_source::window_source::property_keys_budget_support::SourceBudgetGuard;
use crate::incremental_layout::lazy_proof_test_support::{self as sync_probes, Guard as SyncGuard};
use crate::neovm_bridge::property_keys_test_support::{self as probes, Counts, Guard};

/// Owned visual/source observations copied while one Context is exclusively
/// active. Faces own protocol data and strings; no Lisp Values or Context/heap
/// pointers escape. Independent test threads neither share nor publish this
/// fixture, and the returned immutable data can be compared after Context Drop.
#[derive(Debug, PartialEq)]
struct FrameObservation {
    rows: Vec<(i64, Vec<(usize, RowTrace)>)>,
    paint: Vec<(FrameGlyph, Option<Face>)>,
    faces: Vec<Face>,
    row_dependencies: Vec<(i64, usize, Vec<Face>)>,
    points: BackendLayoutTrace,
    ends: Vec<(i64, neovm_core::window::WindowEndState)>,
    geometry: serde_json::Value,
}

fn published_faces(state: &FrameDisplayState) -> Vec<Face> {
    let mut faces: Vec<_> = state.faces.values().cloned().map(normalize_face).collect();
    faces.sort_by_cached_key(|face| format!("{face:?}"));
    faces
}

fn frame_observation(
    eval: &Context,
    engine: &LayoutEngine,
    frame_id: neovm_core::window::FrameId,
) -> FrameObservation {
    let state = engine.last_frame_display_state.as_ref().unwrap();
    let mut row_dependencies = Vec::new();
    for entry in &state.window_matrices {
        for (index, row) in entry
            .matrix
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.enabled)
        {
            for id in row.referenced_face_ids() {
                assert!(
                    state.faces.contains_key(&id),
                    "complete row dependency must resolve: window={} face={id:?}",
                    entry.window_id.get()
                );
            }
            let ids: std::collections::BTreeSet<_> = row.referenced_face_ids().collect();
            let mut definitions: Vec<_> = ids
                .into_iter()
                .map(|id| normalize_face(state.faces.get(&id).unwrap().clone()))
                .collect();
            definitions.sort_by_cached_key(|face| format!("{face:?}"));
            row_dependencies.push((entry.window_id.get(), index, definitions));
        }
    }
    let fills: Vec<_> = state
        .face_fills
        .iter()
        .map(|fill| {
            let mut normalized = fill.clone();
            let face = state.faces.get(&fill.face_id).cloned().map(normalize_face);
            normalized.face_id = FaceId::new(0);
            (normalized, face)
        })
        .collect();
    let geometry = serde_json::json!({
        "frame_chrome": state.frame_chrome,
        "frame_cols": state.frame_cols,
        "frame_rows": state.frame_rows,
        "frame_pixel_width": state.frame_pixel_width,
        "frame_pixel_height": state.frame_pixel_height,
        "char_width": state.char_width,
        "char_height": state.char_height,
        "font_pixel_size": state.font_pixel_size,
        "background": state.background,
        "undecorated": state.undecorated,
        "border_width": state.border_width,
        "border_color": state.border_color,
        "outer_border_width": state.outer_border_width,
        "outer_border_color": state.outer_border_color,
        "background_alpha": state.background_alpha,
        "no_accept_focus": state.no_accept_focus,
        "window_infos": state.window_infos,
        "backgrounds": state.backgrounds,
        "face_fills": fills,
        "borders": state.borders,
        "cursors": state.cursors,
        "cursor_effects_by_window": state.cursor_effects_by_window,
        "scroll_bars": state.scroll_bars,
        "phys_cursor": state.phys_cursor,
        "fringe_bitmaps": state.fringe_bitmaps,
        "window_bounds": state.window_matrices.iter().map(|entry| (
            entry.window_id, entry.pixel_bounds, entry.text_pixel_bounds,
            entry.text_clip_bounds, entry.selected
        )).collect::<Vec<_>>(),
    });
    FrameObservation {
        rows: complete_frame_rows(state),
        paint: frame_paint(state),
        faces: published_faces(state),
        row_dependencies,
        points: selected_window_layout_trace(eval, engine, frame_id),
        ends: complete_window_ends(eval, frame_id, state),
        geometry,
    }
}

fn prepare_faces(eval: &mut Context, text: &str) {
    eval.eval_str(
        "(progn (setq bidi-paragraph-direction 'left-to-right) \
         (setq prove-first-mode-count 0 prove-first-font-count 0) \
         (setq header-line-format '(\"HEADER\") tab-line-format '(\"TAB\")) \
         (setq mode-line-format '((:eval (progn \
             (setq prove-first-mode-count (1+ prove-first-mode-count)) \"ML\")))) \
         (put-text-property 1 7 'mouse-face 'highlight))",
    )
    .unwrap();
    for (line, color) in ["red", "green", "blue"].iter().enumerate() {
        let start = text
            .split_inclusive('\n')
            .take(line + 1)
            .map(str::len)
            .sum::<usize>()
            + 1;
        eval.eval_str(&format!(
            "(put-text-property {start} {} 'face '(:foreground \"{color}\"))",
            start + 8
        ))
        .unwrap();
    }
    eval.eval_str(&format!("(goto-char {})", line_end(text, 8) + 1))
        .unwrap();
}

fn observed_sync(
    enabled: bool,
) -> (
    FrameObservation,
    String,
    LayoutStats,
    Counts,
    crate::incremental_layout::lazy_proof_test_support::Counts,
) {
    let _policy = ProducerGuard::set(false);
    let _sync = SyncGuard::set(true);
    let _keys = Guard::set(enabled);
    let _budget = SourceBudgetGuard::set(true);
    assert_eq!(probes::forced(), Some(enabled));
    let text = source();
    let (mut eval, frame_id, _buffer, _window) = incr_editing_frame(&text, 800, 600);
    prepare_faces(&mut eval, &text);
    eval.buffer_manager_mut()
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("char-property-alias-alist", Value::NIL);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    activate_last_engine_presentation(&mut eval, &engine, frame_id);
    eval.eval_str("(setq prove-first-mode-count 0 prove-first-font-count 0)")
        .unwrap();
    probes::reset_counts();
    eval.eval_str("(insert \"x\")").unwrap();
    engine.layout_frame_rust(&mut eval, frame_id);
    let counts = probes::counts();
    let sync_counts = sync_probes::counts();
    let stats = engine.last_layout_stats().clone();
    activate_last_engine_presentation(&mut eval, &engine, frame_id);
    let incremental = frame_observation(&eval, &engine, frame_id);
    assert!(incremental.ends.iter().all(|(_, end)| end.is_current()));
    assert!(
        engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .faces
            .len()
            > 3
    );
    assert!(
        !engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .presented_pointer_source
            .appearances()
            .is_empty()
    );
    let callbacks = eval
        .eval_str(
            "(prin1-to-string (list prove-first-mode-count prove-first-font-count \
         (window-end nil t) (let ((p (posn-at-point))) \
         (and p (list (nth 1 p) (nth 2 p) (nth 8 p))))))",
        )
        .unwrap()
        .as_str_owned()
        .unwrap();
    // A fresh engine re-walks the same Context and source; it cannot borrow the
    // measured engine's matrices or frame-face arena. No fontifiers are installed.
    let mut full = LayoutEngine::new();
    full.layout_frame_rust(&mut eval, frame_id);
    let canonical = frame_observation(&eval, &full, frame_id);
    // A fresh renderer also publishes an unused default-face definition. It
    // does not belong to the retained-row dependency namespace. Compare every
    // complete visible/source observation and resolved row/pointer/fringe face
    // here; the complete published face tables remain an OFF/ON comparison.
    assert!(
        incremental.rows == canonical.rows,
        "complete canonical matrix rows"
    );
    assert!(
        incremental.paint == canonical.paint,
        "complete canonical paint"
    );
    assert!(
        incremental.row_dependencies == canonical.row_dependencies,
        "all canonical row, pointer, fringe and chrome face definitions"
    );
    assert!(
        incremental.points == canonical.points,
        "complete canonical display points"
    );
    assert!(
        incremental.ends == canonical.ends,
        "complete canonical window ends"
    );
    assert!(
        incremental.geometry == canonical.geometry,
        "complete canonical frame geometry"
    );
    (incremental, callbacks, stats, counts, sync_counts)
}

#[test]
fn accepted_sync_inlines_canonical_keys_after_complete_output_and_callbacks() {
    let off = observed_sync(false);
    let on = observed_sync(true);
    assert_eq!(on.0, off.0, "complete OFF/ON frame and face definitions");
    assert_eq!(on.1, off.1, "mode-line/fontifier and positional observers");
    assert_eq!(off.2.edit_windows, 1);
    assert_eq!(on.2.edit_windows, 1);
    assert!(off.2.reused_rows > 0 && on.2.reused_rows > 0);
    assert!(
        off.4.sync_admissions > 0 && on.4.sync_admissions > 0,
        "both arms must admit real Sync: off={:?} on={:?}",
        off.4,
        on.4
    );
    assert!(
        off.3.heap_materializations > 0,
        "actual constructor allocation control: {:?}",
        off.3
    );
    // Proposed constructor-counter RED on the actual future G29-based test-only predecessor,
    // after complete output/callback and positive Sync admission validation.
    // No execution is claimed; fixture failures need separate review.
    assert_eq!(
        on.3.heap_materializations, 0,
        "canonical key construction: {:?}",
        on.3
    );
    assert!(on.3.inline_constructions > 0);
}

#[cfg(test)]
#[path = "edit_sync_dense_index_engine_test.rs"]
mod dense_index_tests;

#[cfg(test)]
#[path = "edit_sync_fontify_coverage_engine_test.rs"]
mod fontify_coverage_tests;
