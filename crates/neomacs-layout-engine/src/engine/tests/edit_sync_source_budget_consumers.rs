//! Focused SourceBudget consumers. GNU source refs: xdisp.c:6837-6899
//! (static composition), :7212-7280 (overlay insertion iterator), and
//! :8440-8516 (display-table glyph vectors, including newline replacement).
use super::*;
use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::frame_glyphs::FrameGlyph;
use neomacs_display_protocol::glyph_matrix::FrameDisplayState;

fn budget_consumer_normalize_face(mut face: Face) -> Face {
    face.id = FaceId::new(0);
    face.default_resolved_font_id = face
        .default_resolved_font_id
        .map(|_| neomacs_display_protocol::font::ResolvedFontId(0));
    face
}

/// All painted frame content, with allocation-local face/resource identities
/// normalized while retaining exact face values, positions, clips and slots.
fn budget_consumer_frame_paint(state: &FrameDisplayState) -> Vec<(FrameGlyph, Option<Face>)> {
    let mut paint = Vec::new();
    state.for_each_glyph(|mut glyph| {
        let face = match &mut glyph {
            FrameGlyph::Char { face_id, .. }
            | FrameGlyph::Stretch { face_id, .. }
            | FrameGlyph::Image { face_id, .. }
            | FrameGlyph::Video { face_id, .. }
            | FrameGlyph::Xwidget { face_id, .. }
            | FrameGlyph::Surface { face_id, .. }
            | FrameGlyph::FringeBitmap { face_id, .. } => {
                let face = state
                    .faces
                    .get(face_id)
                    .cloned()
                    .map(budget_consumer_normalize_face);
                *face_id = FaceId::new(0);
                face
            }
            _ => None,
        };
        paint.push((glyph, face));
    });
    paint
}

fn budget_consumer_all_window_ends(
    frame: &SyncFrame,
) -> Vec<(i64, neovm_core::window::WindowEndState)> {
    let live = frame
        .eval
        .frame_manager()
        .get(frame.frame_id)
        .expect("frame");
    frame
        .engine
        .last_frame_display_state
        .as_ref()
        .expect("display state")
        .window_matrices
        .iter()
        .filter_map(|entry| {
            match live.find_window(neovm_core::window::WindowId(entry.window_id.get() as u64)) {
                Some(neovm_core::window::Window::Leaf { window_end, .. }) => {
                    Some((entry.window_id.get(), *window_end))
                }
                _ => None,
            }
        })
        .collect()
}

/// One accepted capped attempt must match a replay-free complete frame: all
/// enabled rows at their exact indices, all paint/cursor/window metadata, and
/// every complete live WindowEndState (currentness, char+byte offsets and row).
fn budget_consumer_step(frame: &mut SyncFrame, form: &str, expected_retries: Option<u64>) {
    use crate::buffer_source::window_source::{
        reset_sync_source_budget_horizon_reads_for_test, reset_sync_source_budget_retries_for_test,
        sync_source_budget_horizon_reads_for_test, sync_source_budget_retries_for_test,
    };
    reset_sync_source_budget_horizon_reads_for_test();
    reset_sync_source_budget_retries_for_test();
    frame
        .eval
        .eval_str(form)
        .unwrap_or_else(|error| panic!("{form}: {error:?}"));
    frame
        .engine
        .layout_frame_rust(&mut frame.eval, frame.frame_id);
    let reads = sync_source_budget_horizon_reads_for_test();
    let retries = sync_source_budget_retries_for_test();
    assert!(
        reads > 0,
        "{form}: fixture must actually copy an artificial sync horizon, stats={:?}",
        frame.engine.last_layout_stats()
    );
    if let Some(expected) = expected_retries {
        assert_eq!(retries, expected, "{form}: local retry count");
    }
    let incremental = frame
        .engine
        .last_frame_display_state
        .as_ref()
        .expect("incremental frame")
        .clone();
    let incremental_ends = budget_consumer_all_window_ends(frame);
    assert!(
        incremental_ends.iter().all(|(_, end)| end.is_current()),
        "{form}: stale window end"
    );
    let incremental_selected =
        selected_window_layout_trace(&frame.eval, &frame.engine, frame.frame_id);
    let matrix_trace = |state: &FrameDisplayState| {
        state
            .window_matrices
            .iter()
            .map(|entry| {
                (
                    entry.window_id,
                    entry.pixel_bounds,
                    entry.text_pixel_bounds,
                    entry.text_clip_bounds,
                    entry
                        .matrix
                        .rows
                        .iter()
                        .enumerate()
                        .filter(|(_, row)| row.enabled)
                        .map(|(index, row)| (index, RowTrace::from_row(row, &state.faces)))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };
    let mut full = LayoutEngine::new();
    full.layout_frame_rust(&mut frame.eval, frame.frame_id);
    let reference = full.last_frame_display_state.as_ref().expect("full frame");
    assert_eq!(
        incremental_selected,
        selected_window_layout_trace(&frame.eval, &full, frame.frame_id),
        "{form}"
    );
    assert_eq!(
        matrix_trace(&incremental),
        matrix_trace(reference),
        "{form}: complete frame matrices"
    );
    assert_eq!(
        budget_consumer_frame_paint(&incremental),
        budget_consumer_frame_paint(reference),
        "{form}: complete frame paint"
    );
    assert_eq!(
        incremental.window_infos, reference.window_infos,
        "{form}: window metadata"
    );
    assert_eq!(
        incremental.phys_cursor, reference.phys_cursor,
        "{form}: active cursor"
    );
    assert_eq!(
        format!("{:?}", incremental.cursors),
        format!("{:?}", reference.cursors),
        "{form}: decorative cursors"
    );
    assert_eq!(
        incremental.cursor_effects_by_window, reference.cursor_effects_by_window,
        "{form}: cursor effects"
    );
    let live = frame.eval.frame_manager().get(frame.frame_id).unwrap();
    let reference_ends = reference
        .window_matrices
        .iter()
        .filter_map(|entry| {
            match live.find_window(neovm_core::window::WindowId(entry.window_id.get() as u64)) {
                Some(neovm_core::window::Window::Leaf { window_end, .. }) => {
                    Some((entry.window_id.get(), *window_end))
                }
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        incremental_ends, reference_ends,
        "{form}: complete frame window ends"
    );
}

#[test]
fn source_budget_newline_remapping_display_table_retries_before_semantic_eob() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let _budget = SourceBudgetGuard::set(true);
    // GNU whitespace-mode's supported shape displays an arrow before each
    // real source newline. Keep genuine line boundaries for sync admission;
    // deleting that source newline then makes the converged suffix rise one
    // row, which the existing negative-dy gate refuses, exhausting the cap.
    let text = tabbed_source(80);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
    let table = Value::make_char_table(Value::symbol("display-table"), Value::NIL, 6);
    neovm_core::emacs_core::chartable::ct_set_single(
        &table,
        '\n' as i64,
        Value::vector(vec![Value::fixnum('→' as i64), Value::fixnum('\n' as i64)]),
    );
    frame
        .eval
        .buffer_manager_mut()
        .current_buffer_mut()
        .expect("buffer")
        .set_buffer_local("buffer-display-table", table);
    frame
        .engine
        .layout_frame_rust(&mut frame.eval, frame.frame_id);
    let before = selected_window_layout_trace(&frame.eval, &frame.engine, frame.frame_id);
    assert!(
        backend_trace_text_area_text(&before).contains('→'),
        "fixture must render the newline's replacement"
    );
    let main = frame
        .engine
        .last_frame_display_state
        .as_ref()
        .expect("display state")
        .window_matrices
        .iter()
        .find(|entry| {
            entry.window_id.get()
                == frame
                    .eval
                    .frame_manager()
                    .get(frame.frame_id)
                    .unwrap()
                    .selected_window
                    .0 as i64
        })
        .expect("selected matrix");
    assert!(
        main.matrix
            .rows
            .iter()
            .filter(|row| row.enabled
                && row.role == GlyphRowRole::Text
                && row.displays_text
                && !row.continued)
            .count()
            > 20,
        "fixture must retain many genuine text rows, rather than wrapping one joined line"
    );
    budget_consumer_step(&mut frame, "(delete-region (point) (1+ (point)))", Some(1));
}

#[test]
fn source_budget_static_composition_and_owned_overlay_strings_match_complete_frame() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let _budget = SourceBudgetGuard::set(true);
    let text = tabbed_source(80);
    let line_start = |line: usize| {
        text.split_inclusive('\n')
            .take(line)
            .map(|part| part.chars().count())
            .sum::<usize>()
    };
    let start = line_start(15);
    let end = line_start(17);
    // Point stays outside the static composition, as GNU's handler requires.
    // Two source newlines belong to one composed glyph, followed by ordinary
    // text on that visual row. Its insertion re-walks the full composition.
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 17));
    frame
        .eval
        .eval_str(&format!(
            "(compose-region-internal {} {} ?Ω)",
            start + 1,
            end + 1
        ))
        .unwrap();
    // Empty overlays at the composition's end own UTF8/tab insertion strings.
    // Keep them on this same visual row: source-newline collapsing is exercised
    // by the composition, while an edit later than this fixed empty anchor
    // leaves the overlay digest unchanged and permits a genuine sync plan.
    frame
        .eval
        .eval_str(&format!(
            "(let ((first (make-overlay {0} {0})) (second (make-overlay {0} {0}))) \
           (overlay-put first 'priority 2) \
           (overlay-put first 'before-string \"Bé中\\tPRE \") \
           (overlay-put second 'priority 1) \
           (overlay-put second 'after-string \" POSTAé\\t\"))",
            end + 1,
        ))
        .unwrap();
    frame
        .engine
        .layout_frame_rust(&mut frame.eval, frame.frame_id);
    let before = selected_window_layout_trace(&frame.eval, &frame.engine, frame.frame_id);
    let rendered = backend_trace_text_area_text(&before);
    assert!(
        rendered.contains("Bé") && rendered.contains("Aé"),
        "both owned overlay strings must be consumed"
    );
    assert!(
        rendered.contains('Ω'),
        "fixture must consume the static composition"
    );
    budget_consumer_step(&mut frame, "(insert \"中\")", Some(0));
}
