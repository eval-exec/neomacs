use super::*;
use crate::buffer_source::face_resolution::BufferSourceFaceResolutionContext;
use crate::buffer_source::owned_capture::capture_physical_line;
use crate::display_row::face_state::{
    DisplayRowFaceRealizer, DisplayRowGlyphMeasurer, DisplayRowMeasurementMode,
    DisplayRowMeasurementPolicy, stable_face_id_for_resolved,
};
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::glyph_advance::GlyphAdvanceQuantization;
use crate::row_layout::program::{RowProgram, RowProgramGeometry, RowProgramLimits};

#[test]
fn unseen_row_worker_glyphs_match_canonical_window_body() {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let display_window = neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64);
    let retained = engine.retained_window_matrices[&display_window].clone();
    let key = &retained.key;
    let sample = retained
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .unwrap();
    let metrics = DisplayRowFallbackMetrics::from_default_face_extents(
        key.char_width,
        sample.height_px,
        sample.ascent_px,
    );
    let frame_data = eval.frame_manager().get(frame).unwrap();
    let ws = frame_data
        .effective_window_system()
        .and_then(|value| value.as_symbol_name().map(str::to_owned));
    let mode = DisplayRowMeasurementMode::from_frame_window_system(ws.is_some());
    let resolver = crate::neovm_bridge::FaceResolver::new_with_font_sizing(
        eval.face_table(),
        0xffffff,
        0,
        key.font_pixel_size,
        ws,
        engine.font_sizing,
    );
    let mut attempt = engine.frame_face_arenas[&frame].begin_attempt();
    let base_id = stable_face_id_for_resolved(&mut attempt, resolver.default_face());
    let start = CharPos0::new(120 * line.len());
    let snapshot = crate::neovm_bridge::BorrowedLayoutBuffer::for_window(
        eval.buffer_manager().get(buffer).unwrap(),
        eval.obarray(),
        start,
        128,
        crate::display_property::DisplayPropertyTarget::for_window_system(
            mode.uses_concrete_font_geometry(),
        ),
    );
    let context = BufferSourceFaceResolutionContext::new(
        &snapshot,
        &resolver,
        DisplayRowMeasurementPolicy::for_mode(mode),
        resolver.default_face(),
        base_id,
        metrics,
        metrics,
        Default::default(),
    );
    let captured = capture_physical_line(
        buffer,
        window.0,
        start,
        128,
        32,
        context,
        &mut attempt,
        || false,
    )
    .unwrap();
    let mut realizer = DisplayRowFaceRealizer::new(&mut engine.font_metrics);
    let mut faces = vec![realizer.realize_face(
        base_id,
        resolver.default_face(),
        metrics.char_width(),
        metrics.ascent(),
        metrics.row_height(),
    )];
    for pending in captured.faces {
        if !faces.iter().any(|face| face.face_id == pending.face_id()) {
            faces.push(realizer.realize_face(
                pending.face_id(),
                pending.resolved(),
                metrics.char_width(),
                metrics.ascent(),
                metrics.row_height(),
            ));
        }
    }
    let mut measurer = DisplayRowGlyphMeasurer::with_mode(
        &faces,
        realizer.font_metrics_service_mut(),
        metrics.char_width(),
        GlyphAdvanceQuantization::PreserveLogicalPixels,
        mode,
    );
    let program = RowProgram::capture(
        RowProgramGeometry {
            inherited_line_spacing: 0.0,
            character_wrap: false,
            word_wrap: false,
            fringe: None,
            width: key.partition.text_body().width,
            metrics,
            tabs: crate::display_row::builder::DisplayTabPolicy::from_tab_width_and_stops(
                0.0,
                key.tab_width,
                &key.tab_stop_list,
            ),
            base_face: base_id,
            background: neomacs_display_protocol::types::Color::BLACK,
        },
        captured.items,
        faces.clone(),
        &mut measurer,
        RowProgramLimits {
            items: 32,
            text_bytes: 512,
            glyphs: 512,
        },
    )
    .unwrap();
    let region = retained.display_snapshot.regions.text_body;
    let window_top = retained.display_snapshot.regions.outer.y;
    let row_base = retained
        .matrix
        .rows
        .iter()
        .position(|row| row.enabled && row.role == GlyphRowRole::Text)
        .unwrap();
    let window_bounds = retained.display_snapshot.regions.outer;
    let ncols = retained.matrix.ncols;
    let actual = std::thread::spawn(move || {
        let row = program.compute(|| false)?;
        crate::window_output::prepared_body::position_buffer_rows(
            vec![row],
            row_base,
            region.x,
            region.y,
            window_top,
            window.0,
            window_bounds,
            ncols,
        )
    })
    .join()
    .unwrap()
    .unwrap();
    scroll_window_to(
        &mut eval,
        frame,
        window,
        buffer,
        start.get() as i64 + 1,
        start.get() + 5 * line.len(),
    );
    if let neovm_core::window::Window::Leaf { force_start, .. } = eval
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    {
        *force_start = true;
    }
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(&mut eval, frame);
    let expected = fresh.retained_window_matrices[&display_window]
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::Text)
        .unwrap();
    // Compare glyph semantics independently of speculative face numbering.
    let mut actual_row = actual.glyph_rows[0].clone();
    for glyph in actual_row.glyphs.iter_mut().flatten() {
        glyph.face_id = expected.glyphs[1][0].face_id;
    }
    assert_eq!(
        actual_row.glyphs[1].len(),
        expected.glyphs[1].len(),
        "actual text {:?}; expected text {:?}; expected row {}..{}",
        actual_row.glyphs[1]
            .iter()
            .map(|g| &g.glyph_type)
            .collect::<Vec<_>>(),
        expected.glyphs[1]
            .iter()
            .map(|g| &g.glyph_type)
            .collect::<Vec<_>>(),
        expected.start_charpos,
        expected.end_charpos
    );
    for (i, (actual, expected)) in actual_row.glyphs[1]
        .iter()
        .zip(&expected.glyphs[1])
        .enumerate()
    {
        assert_eq!(actual, expected, "glyph {i}");
    }
    assert_eq!(actual_row.height_px, expected.height_px);
    assert_eq!(actual_row.ascent_px, expected.ascent_px);
    assert_eq!(actual_row.start_charpos, expected.start_charpos);
    assert_eq!(actual_row.end_charpos, expected.end_charpos);
    let expected_snapshot = &fresh.retained_window_matrices[&display_window].display_snapshot;
    let expected_points: Vec<_> = expected_snapshot
        .iter_points()
        .filter(|point| point.row == row_base as i64)
        .collect();
    let actual_points = match &actual.geometry.point_rows {
        Some(rows) => rows.iter_points().collect::<Vec<_>>(),
        None => actual.geometry.points.clone(),
    };
    assert_eq!(actual_points, expected_points);
    assert_eq!(actual.geometry.rows[0], expected_snapshot.rows[row_base]);
}

#[test]
fn first_visit_to_worker_prepared_page_reuses_rows_and_matches_fresh_layout() {
    first_visit(None, "ordinary offscreen text\n", None);
}

#[test]
fn first_visit_to_worker_prepared_mixed_height_faces_matches_fresh_layout() {
    first_visit(
        Some("(:height 150 :foreground \"red\")"),
        "ordinary offscreen text\n",
        None,
    );
}

#[test]
fn first_visit_to_worker_prepared_unicode_page_matches_fresh_layout() {
    first_visit(None, "office café 好 á שלום سلام\n", None);
}

fn first_visit(face: Option<&str>, line: &str, change: Option<&str>) {
    first_visit_shifted(face, line, change, 0);
}

#[test]
fn first_visit_shifted_within_worker_page_matches_fresh_layout() {
    first_visit_shifted(None, "ordinary offscreen text\n", None, 1);
}

fn first_visit_shifted(face: Option<&str>, line: &str, change: Option<&str>, shift: usize) {
    first_visit_at(face, line, change, shift, 5);
}

#[test]
fn first_visit_page_command_reuses_worker_rows_with_point_at_page_start() {
    first_visit_at(None, "ordinary offscreen text\n", None, 0, 0);
}

fn first_visit_at(
    face: Option<&str>,
    line: &str,
    change: Option<&str>,
    shift: usize,
    point_row: usize,
) {
    first_visit_with_setup(face, line, change, shift, point_row, None);
}

#[test]
fn first_visit_worker_page_accepts_inactive_startup_overlay_arrows() {
    first_visit_with_setup(
        None,
        "ordinary offscreen text\n",
        None,
        0,
        5,
        Some(
            "(setq overlay-arrow-variable-list '(next-error-overlay-arrow-position overlay-arrow-position) next-error-overlay-arrow-position nil)",
        ),
    );
}

#[test]
fn worker_page_is_rejected_when_custom_overlay_arrow_becomes_active() {
    first_visit_with_setup(
        None,
        "ordinary offscreen text\n",
        Some("(setq worker-custom-arrow (copy-marker 2761))"),
        0,
        5,
        Some("(setq overlay-arrow-variable-list '(worker-custom-arrow) worker-custom-arrow nil)"),
    );
}

#[test]
fn first_visit_worker_page_preserves_overlapping_face_and_pointer_overlays() {
    first_visit_with_setup(
        None,
        "ordinary offscreen text\n",
        None,
        0,
        5,
        Some(
            "(let ((a (make-overlay 2761 3100)) (b (make-overlay 2780 3060)))
                 (overlay-put a 'face '(:family \"DejaVu Serif\" :height 150 :foreground \"red\"))
                 (overlay-put a 'mouse-face 'highlight)
                 (overlay-put a 'help-echo \"offscreen help\")
                 (overlay-put b 'priority 12)
                 (overlay-put b 'face '(:weight bold :underline t)))",
        ),
    );
}

#[test]
fn first_visit_worker_page_resolves_overlay_category_faces() {
    first_visit_with_setup(
        None,
        "ordinary offscreen text\n",
        None,
        0,
        5,
        Some(
            "(progn (put 'worker-overlay-category 'face '(:height 125 :slant italic))
                 (overlay-put (make-overlay 2761 3100) 'category 'worker-overlay-category))",
        ),
    );
}

#[test]
fn worker_capture_rejects_overlay_replacements_and_excessive_overlap() {
    use crate::row_layout::program::RowProgramError;
    for (setup, expected) in [
        (
            "(setq overlay-arrow-variable-list (make-list 33 'worker-custom-arrow))",
            RowProgramError::Unsupported,
        ),
        (
            "(setq overlay-arrow-variable-list '(worker-custom-arrow) worker-custom-arrow (copy-marker 2761))",
            RowProgramError::Unsupported,
        ),
        (
            "(overlay-put (make-overlay 2761 3100) 'before-string (make-string 129 ?p))",
            RowProgramError::Unsupported,
        ),
        (
            "(overlay-put (make-overlay 2761 3100) 'display \"replacement\")",
            RowProgramError::Unsupported,
        ),
        (
            "(progn (put 'worker-overlay-category 'after-string (make-string 129 ?s))
                 (overlay-put (make-overlay 2761 3100) 'category 'worker-overlay-category))",
            RowProgramError::Unsupported,
        ),
        (
            "(let ((i 0)) (while (< i 33) (make-overlay 2761 3100) (setq i (1+ i))))",
            RowProgramError::Budget,
        ),
        (
            "(put-text-property 2761 3100 'display '(when t (raise 0.25)))",
            RowProgramError::Unsupported,
        ),
        (
            "(put-text-property 2761 3100 'display '(raise (+ 1 2)))",
            RowProgramError::Unsupported,
        ),
        (
            "(put-text-property 2761 2765 'invisible t)",
            RowProgramError::Unsupported,
        ),
        (
            "(put-text-property 2766 3100 'invisible t)",
            RowProgramError::Unsupported,
        ),
        (
            "(progn (setq buffer-invisibility-spec (make-list 33 'worker-hidden))
                 (put-text-property 2766 2770 'invisible 'worker-hidden))",
            RowProgramError::Unsupported,
        ),
        (
            "(let ((o (make-overlay 2766 2770)))
                 (overlay-put o 'invisible t)
                 (overlay-put o 'window (selected-window)))",
            RowProgramError::Unsupported,
        ),
    ] {
        let (mut eval, frame, _, window) =
            incr_editing_frame(&"ordinary offscreen text\n".repeat(300), 800, 600);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        eval.eval_str(setup).unwrap();
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame);
        assert_eq!(
            engine.request_scroll_coverage(&eval, frame, window, CharPos0::new(2760)),
            Err(expected),
            "{setup}"
        );
    }
}

fn first_visit_with_setup(
    face: Option<&str>,
    line: &str,
    change: Option<&str>,
    shift: usize,
    point_row: usize,
    setup: Option<&str>,
) {
    first_visit_with_setup_and_gc(face, line, change, shift, point_row, setup, false);
}

fn first_visit_with_setup_and_gc(
    face: Option<&str>,
    line: &str,
    change: Option<&str>,
    shift: usize,
    point_row: usize,
    setup: Option<&str>,
    collect: bool,
) {
    let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let line_chars = line.chars().count();
    let start = 120 * line_chars;
    let start_byte = 120 * line.len();
    if let Some(face) = face {
        eval.eval_str(&format!(
            "(put-text-property {} {} 'face '{face})",
            start + 1,
            start + 12 * line_chars
        ))
        .unwrap();
    }
    if let Some(setup) = setup {
        eval.eval_str(setup).unwrap();
    }
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(start))
        .unwrap();
    if collect {
        eval.gc_collect_exact();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .expect("worker coverage failed")
    {
        assert!(
            std::time::Instant::now() < deadline,
            "unseen coverage was not admitted"
        );
        std::thread::yield_now();
    }
    if collect {
        eval.gc_collect_exact();
    }
    let start = start + shift * line_chars;
    let start_byte = start_byte + shift * line.len();
    let display_window = neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64);
    let mut requested_key = engine.retained_window_matrices[&display_window].key.clone();
    requested_key.window_start = start as i64;
    requested_key.point = (start + point_row * line_chars) as i64;
    if shift == 0 {
        if point_row == 0 {
            assert!(
                engine
                    .prepared_viewports
                    .replay(frame, display_window, &requested_key, false)
                    .is_none(),
                "ordinary point motion must still run viewport resolution"
            );
        }
        let (replay, faces) = engine
            .prepared_viewports
            .replay(frame, display_window, &requested_key, true)
            .expect("ready coverage must satisfy the requested key");
        let arena = &engine.frame_face_arenas[&frame];
        arena
            .begin_attempt()
            .admit_prepared(
                replay
                    .body_rows
                    .iter()
                    .flat_map(|(_, row)| row.glyphs.iter().flatten().map(|glyph| glyph.face_id)),
                &faces,
                arena,
            )
            .expect("prepared face namespace");
    }
    if let Some(change) = change {
        eval.eval_str(change).unwrap();
    }
    // Native scroll commands synchronously ask for window geometry before
    // publishing their new start. These queries must preserve idle coverage.
    for _ in 0..3 {
        engine
            .query_window_layout(
                &mut eval,
                frame,
                window,
                neovm_core::window::WindowLayoutQueryScope::Viewport,
            )
            .unwrap();
    }
    scroll_window_to(
        &mut eval,
        frame,
        window,
        buffer,
        start as i64 + 1,
        start_byte + point_row * line.len(),
    );
    if let neovm_core::window::Window::Leaf { force_start, .. } = eval
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    {
        *force_start = true;
    }
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(
        engine.last_layout_stats().prepared_windows,
        usize::from(change.is_none()),
        "requested {requested_key:?}, actual {:?}",
        engine.retained_window_matrices[&display_window].key
    );
    if change.is_none() {
        assert!(
            engine.last_layout_stats().reused_rows + engine.last_layout_stats().reused_shifted_rows
                > 10
        );
    }
    let actual = selected_window_layout_trace(&eval, &engine, frame);
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(&mut eval, frame);
    assert_eq!(actual, selected_window_layout_trace(&eval, &fresh, frame));
}

#[test]
fn raised_buffer_rows_preserve_glyph_extents_with_and_without_wrapping() {
    for width in [120, 800] {
        let text = format!("{}\n", "raised words ".repeat(30));
        let (mut eval, frame, _, window) = incr_editing_frame(&text, width, 600);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        eval.eval_str(
            "(setq truncate-lines nil word-wrap t)
            (put-text-property 1 361 'face '(:height 150))
            (put-text-property 1 361 'display '(raise 0.25))",
        )
        .unwrap();
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame);
        let owner = neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64);
        let rows = &engine.retained_window_matrices[&owner].matrix.rows;
        let mut raised_rows = 0;
        for row in rows
            .iter()
            .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        {
            for glyph in row.glyphs.iter().flatten().filter(|glyph| !glyph.padding) {
                if glyph.vertical_offset_px == 0.0 {
                    continue;
                }
                raised_rows += 1;
                let ascent = (glyph.pixel_ascent - glyph.vertical_offset_px).max(0.0);
                let descent =
                    (glyph.pixel_height - glyph.pixel_ascent + glyph.vertical_offset_px).max(0.0);
                assert!(
                    row.ascent_px >= ascent,
                    "width={width}, row baseline lost raised glyph ascent"
                );
                assert!(
                    row.height_px - row.ascent_px >= descent,
                    "width={width}, row lost lowered glyph descent"
                );
            }
        }
        assert!(raised_rows > 0);
    }
}

#[test]
fn unchanged_raised_cursor_reuses_its_authoritative_presentation() {
    let (mut eval, frame, _, _) =
        incr_editing_frame(&"ordinary offscreen text\n".repeat(30), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str("(put-text-property 1 20 'display '(raise 0.25))")
        .unwrap();
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let expected = selected_window_layout_trace(&eval, &engine, frame);
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(engine.last_layout_stats().cursor_only_windows, 1);
    assert_eq!(
        selected_window_layout_trace(&eval, &engine, frame),
        expected
    );
}

#[test]
fn worker_page_preserves_literal_raised_text_and_overlay_geometry() {
    for setup in [
        "(put-text-property 2761 3100 'display '(raise 0.25))",
        "(overlay-put (make-overlay 2761 3100) 'display '(raise -0.25))",
    ] {
        first_visit_with_setup(
            Some("(:height 150 :weight bold)"),
            "ordinary offscreen text\n",
            None,
            0,
            16,
            Some(setup),
        );
    }
}

#[test]
fn worker_page_is_rejected_after_text_edit() {
    first_visit(
        None,
        "ordinary offscreen text\n",
        Some("(progn (goto-char 3000) (delete-region 3000 3001) (insert \"X\"))"),
    );
}

#[test]
fn worker_page_is_rejected_after_face_property_change() {
    first_visit(
        None,
        "ordinary offscreen text\n",
        Some("(put-text-property 2900 3000 'face '(:height 175))"),
    );
}

#[test]
fn worker_page_is_rejected_after_overlay_change() {
    first_visit(
        None,
        "ordinary offscreen text\n",
        Some("(overlay-put (make-overlay 2900 3000) 'face '(:background \"red\"))"),
    );
}

#[test]
fn worker_page_is_rejected_after_narrowing() {
    first_visit(
        None,
        "ordinary offscreen text\n",
        Some("(narrow-to-region 1 5000)"),
    );
}

#[test]
fn idle_capture_yields_between_rows_and_cancels_after_a_revision_change() {
    let (mut eval, frame, _buffer, window) =
        incr_editing_frame(&"ordinary text\n".repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine
        .begin_scroll_coverage(&eval, frame, window, CharPos0::new(120 * 14))
        .unwrap();
    assert!(
        engine.capture_scroll_step(&eval).unwrap(),
        "one step must leave the rest of the page for later"
    );
    eval.eval_str("(put-text-property 2000 2010 'face '(:height 175))")
        .unwrap();
    assert_eq!(
        engine.capture_scroll_step(&eval),
        Err(crate::row_layout::program::RowProgramError::Cancelled)
    );
    assert!(
        !engine.capture_scroll_step(&eval).unwrap(),
        "failed capture must be retired"
    );
    assert!(
        !engine
            .scroll_coverage
            .drain(&mut engine.prepared_viewports)
            .unwrap()
    );
}

#[test]
fn idle_maintenance_prepares_an_unseen_page_without_changing_the_live_viewport() {
    idle_first_visit(false, false);
}

#[test]
fn offscreen_capture_budget_ignores_unused_retained_matrix_capacity() {
    let (mut eval, frame, _, window) =
        incr_editing_frame(&"ordinary offscreen text\n".repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let window_id = neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64);
    let retained = engine.retained_window_matrices.get_mut(&window_id).unwrap();
    retained.matrix.resize(1000, retained.matrix.ncols);
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(120 * 23))
        .expect("unused matrix slots are not source rows to precompute");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    let mut key = engine.retained_window_matrices[&window_id].key.clone();
    key.window_start = 120 * 23;
    key.point = 125 * 23;
    let (replay, _) = engine
        .prepared_viewports
        .replay(frame, window_id, &key, false)
        .expect("unused slots must not exceed the prepared-cache capacity either");
    assert!(replay.body_rows.len() > 10);
}

#[test]
fn unchanged_redisplays_keep_retained_matrix_capacity_bounded() {
    let (mut eval, frame, _, window) =
        incr_editing_frame(&"ordinary offscreen text\n".repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let window_id = neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64);
    let rows = engine.retained_window_matrices[&window_id]
        .matrix
        .rows
        .len();
    for _ in 0..100 {
        engine.layout_frame_rust(&mut eval, frame);
    }
    assert_eq!(
        engine.retained_window_matrices[&window_id]
            .matrix
            .rows
            .len(),
        rows
    );
}

#[test]
fn idle_maintenance_prepares_an_unseen_backward_page() {
    idle_first_visit(true, false);
}

#[test]
fn idle_capture_survives_redisplays_with_unchanged_layout_inputs() {
    idle_first_visit(false, true);
}

fn idle_first_visit(backward: bool, redisplay_between_steps: bool) {
    idle_first_visit_step(backward, redisplay_between_steps, None);
}

#[test]
fn idle_worker_rows_complete_small_forward_scrolls() {
    for rows in [1, 3] {
        idle_first_visit_step(false, false, Some(rows));
    }
}

fn idle_first_visit_step(backward: bool, redisplay_between_steps: bool, step: Option<usize>) {
    idle_first_visit_styled(backward, redisplay_between_steps, step, false);
}

#[test]
fn idle_worker_rows_complete_small_scrolls_with_distinct_prefix_faces() {
    idle_first_visit_styled(false, false, Some(1), true);
    idle_first_visit_styled(false, false, Some(3), true);
}

fn idle_first_visit_styled(
    backward: bool,
    redisplay_between_steps: bool,
    step: Option<usize>,
    styled: bool,
) {
    idle_first_visit_projected(backward, redisplay_between_steps, step, styled, 0);
}

#[test]
fn idle_worker_rows_cover_fractional_scroll_placement() {
    for hidden in [1, 4, 12] {
        for step in [0, 1, 3] {
            idle_first_visit_projected(false, false, Some(step), false, hidden);
        }
    }
}

#[test]
fn idle_worker_fractional_scroll_preserves_mixed_face_geometry() {
    for step in [0, 1, 3] {
        idle_first_visit_projected(false, false, Some(step), true, 4);
    }
}

fn idle_first_visit_projected(
    backward: bool,
    redisplay_between_steps: bool,
    step: Option<usize>,
    styled: bool,
    hidden: i32,
) {
    idle_first_visit_projected_text(
        backward,
        redisplay_between_steps,
        step,
        styled,
        hidden,
        "ordinary offscreen text\n",
    );
}

#[test]
fn idle_worker_fractional_scroll_preserves_wrapped_rows_at_the_same_source_start() {
    let line = format!("{}\n", "ordinary offscreen text ".repeat(8));
    idle_first_visit_projected_text(false, false, Some(0), true, 4, &line);
}

fn idle_first_visit_projected_text(
    backward: bool,
    redisplay_between_steps: bool,
    step: Option<usize>,
    styled: bool,
    hidden: i32,
    line: &str,
) {
    let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    if styled {
        eval.eval_str(
            "(progn
            (put-text-property 1 70 'face '(:height 150 :family \"DejaVu Serif\"))
            (put-text-property 277 690 'face '(:height 125 :weight bold))
            (overlay-put (make-overlay 277 690) 'mouse-face 'highlight))",
        )
        .unwrap();
    }
    if backward {
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            (120 * line.len() + 1) as i64,
            125 * line.len(),
        );
        if let neovm_core::window::Window::Leaf { force_start, .. } = eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
        {
            *force_start = true;
        }
    }
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let before = selected_window_layout_trace(&eval, &engine, frame);
    let display_window = neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64);
    let rows: Vec<_> = engine.retained_window_matrices[&display_window]
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .collect();
    let start = if let Some(step) = step {
        step * line.len()
    } else if backward {
        (120 - rows.len().saturating_sub(2).max(1)) * line.len()
    } else {
        rows[rows.len() - 2].start_charpos
    };
    let row_count = rows.len();
    drop(rows);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut steps = 0;
    while engine.maintain_scroll_coverage(&eval).is_some() {
        if redisplay_between_steps {
            engine.layout_frame_rust(&mut eval, frame);
        }
        steps += 1;
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(
        steps + 1 >= row_count,
        "acquisition must yield between rows"
    );
    assert_eq!(before, selected_window_layout_trace(&eval, &engine, frame));
    scroll_window_to(
        &mut eval,
        frame,
        window,
        buffer,
        start as i64 + 1,
        start + 5 * line.len(),
    );
    let offsets = if hidden == 0 {
        vec![0]
    } else {
        vec![hidden, 2, 14, 1, 0, 6]
    };
    for hidden in offsets {
        if let neovm_core::window::Window::Leaf {
            force_start,
            vscroll,
            ..
        } = eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
        {
            *force_start = true;
            *vscroll = -hidden;
        }
        engine.layout_frame_rust(&mut eval, frame);
        assert_eq!(engine.last_layout_stats().prepared_windows, 1);
        let actual = selected_window_layout_trace(&eval, &engine, frame);
        let mut fresh = LayoutEngine::new();
        fresh.layout_frame_rust(&mut eval, frame);
        assert_eq!(actual, selected_window_layout_trace(&eval, &fresh, frame));
        let fringes = |engine: &LayoutEngine| {
            let state = engine.last_frame_display_state.as_ref().unwrap();
            state
                .window_matrices
                .iter()
                .find(|entry| entry.window_id == display_window)
                .unwrap()
                .matrix
                .rows
                .iter()
                .filter(|row| row.enabled)
                .map(|row| {
                    [
                        row.left_fringe_bitmap,
                        row.right_fringe_bitmap,
                        row.overlay_arrow_bitmap,
                    ]
                    .map(|bitmap| {
                        bitmap.map(|bitmap| {
                            let face = &state.faces[&bitmap.face_id];
                            (bitmap.bitmap_index, face.foreground, face.background)
                        })
                    })
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            fringes(&engine),
            fringes(&fresh),
            "fractional offset {hidden}"
        );
    }
}

#[test]
fn worker_page_is_rejected_after_font_change() {
    first_visit(
        None,
        "ordinary offscreen text\n",
        Some("(internal-set-lisp-face-attribute 'default :height 180 (selected-frame))"),
    );
}

#[test]
fn worker_page_is_rejected_after_horizontal_scroll() {
    first_visit(
        None,
        "ordinary offscreen text\n",
        Some("(set-window-hscroll nil 2)"),
    );
}

#[test]
fn worker_page_with_font_family_weight_and_slant_matches_fresh_layout() {
    first_visit(
        Some("(:family \"DejaVu Serif\" :weight bold :slant italic :height 125)"),
        "ordinary offscreen text\n",
        None,
    );
}

#[test]
fn worker_page_with_box_and_extended_background_matches_fresh_layout() {
    first_visit(
        Some("(:box (:line-width 2 :color \"blue\") :background \"red\" :extend t)"),
        "ordinary offscreen text\n",
        None,
    );
}

#[test]
fn worker_page_with_combined_decorations_matches_fresh_layout() {
    first_visit(
        Some(
            "(:underline (:style wave :color \"blue\") :overline t :strike-through t :inverse-video t)",
        ),
        "ordinary offscreen text\n",
        None,
    );
}

#[test]
fn worker_page_with_smaller_font_and_tabs_matches_fresh_layout() {
    first_visit(
        Some("(:family \"DejaVu Serif\" :height 125)"),
        "font\ttext\n",
        None,
    );
}

#[test]
fn repeated_fractional_scroll_across_prepared_pages_matches_fresh_layout() {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(400), 1000, 700);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.frame_manager_mut().get_mut(frame).unwrap().char_height = 17.0;
    let mut engine = LayoutEngine::new();
    for pixels in (0..=96)
        .map(|step| step * 4)
        .chain((0..96).rev().map(|step| step * 4))
    {
        let start = (pixels / 17) as usize * line.len();
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            start as i64 + 1,
            start + 5 * line.len(),
        );
        if let neovm_core::window::Window::Leaf {
            force_start,
            vscroll,
            ..
        } = eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
        {
            *force_start = true;
            *vscroll = -(pixels % 17);
        }
        engine.layout_frame_rust(&mut eval, frame);
        let actual = selected_window_layout_trace(&eval, &engine, frame);
        let mut fresh = LayoutEngine::new();
        fresh.layout_frame_rust(&mut eval, frame);
        assert_eq!(
            actual,
            selected_window_layout_trace(&eval, &fresh, frame),
            "offset {pixels}"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while engine.maintain_scroll_coverage(&eval).is_some() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
}

#[test]
fn idle_capture_finishes_while_the_viewport_keeps_moving() {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(400), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let display_window = neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64);
    let mut completed = false;
    // Each service opportunity is separated by a real viewport change and
    // redisplay. A continuously moving viewport must not restart acquisition
    // at its first row forever.
    for step in 0..150 {
        engine.maintain_scroll_coverage(&eval);
        if engine
            .prepared_viewports
            .has_computed(frame, display_window)
        {
            completed = true;
            break;
        }
        let start = (step % 3 + 1) * line.len();
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            start as i64 + 1,
            start + 5 * line.len(),
        );
        engine.layout_frame_rust(&mut eval, frame);
        std::thread::yield_now();
    }
    assert!(completed, "viewport motion starved the offscreen worker");
    let actual = selected_window_layout_trace(&eval, &engine, frame);
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(&mut eval, frame);
    assert_eq!(actual, selected_window_layout_trace(&eval, &fresh, frame));
}

#[test]
fn worker_page_is_rejected_after_category_symbol_properties_change() {
    first_visit_with_setup(
        None,
        "ordinary offscreen text\n",
        Some("(put 'worker-overlay-category 'face '(:height 200))"),
        0,
        5,
        Some(
            "(progn (put 'worker-overlay-category 'face '(:height 125)) (overlay-put (make-overlay 2761 3100) 'category 'worker-overlay-category))",
        ),
    );
}

#[test]
fn idle_preparation_covers_unselected_windows_even_when_selected_rows_are_unsupported() {
    for unsupported_selected in [false, true] {
        let line = "ordinary offscreen text\n";
        let (mut eval, frame, buffer, selected) = incr_editing_frame(&line.repeat(300), 800, 600);
        let other_buffer = eval.buffer_manager_mut().create_buffer("offscreen-other");
        eval.buffer_manager_mut()
            .get_mut(other_buffer)
            .unwrap()
            .insert(&line.repeat(300));
        let other = eval
            .frame_manager_mut()
            .split_window(
                frame,
                selected,
                neovm_core::window::SplitDirection::Horizontal,
                other_buffer,
                None,
                neovm_core::window::SplitPlacement::AfterTarget,
            )
            .unwrap();
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        if unsupported_selected {
            eval.eval_str("(put-text-property 1 (point-max) 'display \"replacement\")")
                .unwrap();
        }
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame);
        let before = selected_window_layout_trace(&eval, &engine, frame);
        let owner = neomacs_display_protocol::types::DisplayWindowId::new(other.0 as i64);
        let rows: Vec<_> = engine.retained_window_matrices[&owner]
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
            .collect();
        let target = rows[rows.len() - 2].start_charpos;
        let mut key = engine.retained_window_matrices[&owner].key.clone();
        key.window_start = target as i64;
        key.point = (target + line.len()) as i64;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while engine.maintain_scroll_coverage(&eval).is_some() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(
            engine
                .prepared_viewports
                .replay(frame, owner, &key, false)
                .is_some(),
            "unselected window must have prepared coverage; unsupported_selected={unsupported_selected}"
        );
        assert_eq!(before, selected_window_layout_trace(&eval, &engine, frame));
        assert_eq!(
            eval.frame_manager().get(frame).unwrap().selected_window,
            selected
        );
        assert_eq!(eval.buffer_manager().current_buffer().unwrap().id(), buffer);
    }
}

#[test]
fn moving_selected_window_does_not_starve_other_window_preparation() {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame, buffer, selected) = incr_editing_frame(&line.repeat(300), 800, 600);
    let other = eval
        .frame_manager_mut()
        .split_window(
            frame,
            selected,
            neovm_core::window::SplitDirection::Horizontal,
            buffer,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .unwrap();
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let owner = neomacs_display_protocol::types::DisplayWindowId::new(other.0 as i64);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut steps = 0;
    while !engine.prepared_viewports.has_computed(frame, owner) {
        assert!(engine.maintain_scroll_coverage(&eval).is_some());
        assert!(std::time::Instant::now() < deadline);
        // Change the selected viewport while its bounded page is in flight.
        // A global "observed window" would continually retarget that window.
        {
            let start = (steps % 60 + 1) * line.len();
            scroll_window_to(&mut eval, frame, selected, buffer, start as i64 + 1, start);
            if let neovm_core::window::Window::Leaf { force_start, .. } = eval
                .frame_manager_mut()
                .get_mut(frame)
                .unwrap()
                .find_window_mut(selected)
                .unwrap()
            {
                *force_start = true;
            }
            engine.layout_frame_rust(&mut eval, frame);
        }
        steps += 1;
        std::thread::yield_now();
    }
}

#[test]
fn deleting_capture_owner_allows_remaining_window_preparation() {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame, buffer, selected) = incr_editing_frame(&line.repeat(300), 800, 600);
    let other = eval
        .frame_manager_mut()
        .split_window(
            frame,
            selected,
            neovm_core::window::SplitDirection::Horizontal,
            buffer,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .unwrap();
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    assert!(engine.maintain_scroll_coverage(&eval).is_some());
    assert!(eval.frame_manager_mut().delete_window(frame, selected));
    engine.layout_frame_rust(&mut eval, frame);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while engine.maintain_scroll_coverage(&eval).is_some() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(engine.prepared_viewports.has_computed(
        frame,
        neomacs_display_protocol::types::DisplayWindowId::new(other.0 as i64)
    ));
    assert!(!engine.prepared_viewports.has_computed(
        frame,
        neomacs_display_protocol::types::DisplayWindowId::new(selected.0 as i64)
    ));
}

#[test]
fn worker_page_is_rejected_after_in_place_display_property_mutation() {
    first_visit_with_setup(
        None,
        "ordinary offscreen text\n",
        Some("(setcar (cdr worker-raise-spec) 0.75)"),
        0,
        16,
        Some(
            "(progn (setq worker-raise-spec (list 'raise 0.25)) (put-text-property 2761 3100 'display worker-raise-spec))",
        ),
    );
}

#[test]
fn worker_capture_rejects_mutation_between_idle_steps() {
    let (mut eval, frame, _, window) =
        incr_editing_frame(&"ordinary offscreen text\n".repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str("(progn (setq worker-raise-spec (list 'raise 0.25)) (put-text-property 2761 3500 'display worker-raise-spec))").unwrap();
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine
        .begin_scroll_coverage(&eval, frame, window, CharPos0::new(2880))
        .unwrap();
    assert_eq!(engine.capture_scroll_step(&eval), Ok(true));
    eval.eval_str("(setcar (cdr worker-raise-spec) 0.75)")
        .unwrap();
    assert_eq!(
        engine.capture_scroll_step(&eval),
        Err(crate::row_layout::program::RowProgramError::Cancelled)
    );
}

#[test]
fn worker_admission_rejects_mutation_after_capture() {
    let (mut eval, frame, _, window) =
        incr_editing_frame(&"ordinary offscreen text\n".repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str("(progn (setq worker-raise-spec (list 'raise 0.25)) (put-text-property 2761 3500 'display worker-raise-spec))").unwrap();
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(2880))
        .unwrap();
    eval.eval_str("(setcar (cdr worker-raise-spec) 0.75)")
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match engine.scroll_coverage.drain(&mut engine.prepared_viewports) {
            Ok(false) => {
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
            result => {
                assert_eq!(
                    result,
                    Err(crate::row_layout::program::RowProgramError::Cancelled)
                );
                break;
            }
        }
    }
}

#[test]
fn idle_mutation_withdraws_published_worker_coverage() {
    let (mut eval, frame, _, window) =
        incr_editing_frame(&"ordinary offscreen text\n".repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str("(progn (setq worker-raise-spec (list 'raise 0.25)) (put-text-property 2761 3500 'display worker-raise-spec))").unwrap();
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    // Establish idle ownership before manually requesting the distant page.
    engine.maintain_scroll_coverage(&eval);
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(2880))
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(engine.take_scroll_coverage_publication());
    eval.eval_str("(setcar (cdr worker-raise-spec) 0.75)")
        .unwrap();
    engine.maintain_scroll_coverage(&eval);
    assert!(
        engine.take_scroll_coverage_publication(),
        "discarding prepared rows must also withdraw the compositor's old certificate"
    );
}

#[test]
fn worker_page_preserves_numeric_line_height() {
    for height in ["1.3", "30"] {
        first_visit_with_setup(
            None,
            "ordinary offscreen text\n",
            None,
            0,
            16,
            Some(&format!(
                "(put-text-property 2761 3500 'line-height {height})"
            )),
        );
    }
}

#[test]
fn buffer_newline_numeric_height_uses_default_font_for_factors() {
    let (mut eval, frame, _, window) = incr_editing_frame("small\nnext\n", 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let owner = DisplayWindowId::new(window.0 as i64);
    let first_row = |engine: &LayoutEngine| {
        let row = engine.retained_window_matrices[&owner]
            .matrix
            .rows
            .iter()
            .find(|row| row.enabled && row.role == GlyphRowRole::Text)
            .unwrap();
        (row.height_px, row.ascent_px)
    };
    let baseline = first_row(&engine);
    eval.eval_str("(put-text-property 6 7 'line-height 2.0)")
        .unwrap();
    engine.layout_frame_rust(&mut eval, frame);
    let enlarged = first_row(&engine);
    let newline_cell_height = |engine: &LayoutEngine| {
        engine.retained_window_matrices[&owner]
            .display_snapshot
            .iter_points()
            .find(|point| {
                point.buffer_pos == neovm_core::buffer::LispCharPos1::from_one_based_usize(6)
            })
            .unwrap()
            .height
    };
    assert_eq!(newline_cell_height(&engine), enlarged.0 as i64);
    assert_eq!(enlarged.0, (baseline.0 * 2.0).floor());
    assert_eq!(
        enlarged.0 - enlarged.1,
        baseline.0 - baseline.1,
        "extra height belongs above the baseline"
    );
    eval.eval_str("(progn (put-text-property 6 7 'face '(:height 3.0)) (put-text-property 6 7 'line-height 1.0))").unwrap();
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(
        first_row(&engine),
        baseline,
        "a float overrides the newline's face with the frame default metrics"
    );
    assert_eq!(newline_cell_height(&engine), baseline.0 as i64);
}

#[test]
fn scaled_line_height_overflow_keeps_finite_geometry() {
    let metrics = crate::display_row::metrics::resolve_line_height(
        crate::display_item::DisplayLineHeightPolicy::Scale(f32::MAX),
        (23.0, 17.0),
        (23.0, 17.0),
        (23.0, 17.0),
    );
    assert_eq!((metrics.height, metrics.ascent), (23.0, 17.0));
    assert_eq!(
        (metrics.newline_height, metrics.newline_ascent),
        (23.0, 17.0)
    );
}

#[test]
fn worker_keeps_complete_prefix_before_unsupported_rows() {
    let line = "ordinary offscreen text\n";
    let start = 120 * line.len();
    for boundary in ["replacement", "contextual-fragment"] {
        let mut text = line.repeat(300);
        if boundary == "contextual-fragment" {
            text.replace_range(
                start + 3 * line.len()..start + 4 * line.len(),
                &("س".repeat(129) + "\n"),
            );
        }
        let (mut eval, frame, buffer, window) = incr_editing_frame(&text, 800, 600);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        if boundary == "replacement" {
            eval.eval_str(&format!(
                "(overlay-put (make-overlay {} {}) 'before-string (propertize \"prefix\" 'cursor 1))",
                start + 3 * line.len() + 1,
                start + 4 * line.len()
            ))
            .unwrap();
        }
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame);
        engine
            .request_scroll_coverage(&eval, frame, window, CharPos0::new(start))
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !engine
            .scroll_coverage
            .drain(&mut engine.prepared_viewports)
            .unwrap()
        {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        let owner = DisplayWindowId::new(window.0 as i64);
        assert!(engine.prepared_viewports.has_computed(frame, owner));
        let mut key = engine.retained_window_matrices[&owner].key.clone();
        key.window_start = start as i64;
        key.point = start as i64;
        assert!(
            engine
                .prepared_viewports
                .replay(frame, owner, &key, true)
                .is_none(),
            "three complete rows are useful coverage, not a complete viewport"
        );
        key.window_start += line.len() as i64;
        key.point = key.window_start;
        let (prefix, _) = engine
            .prepared_viewports
            .scroll_replay(frame, owner, &key, 0)
            .unwrap();
        assert_eq!(prefix.reused_rows.len(), 2, "{boundary}");
        assert_eq!(
            prefix.reused_rows.last().unwrap().1.end_charpos + 1,
            start + 3 * line.len()
        );
        assert!(
            prefix.exposed_row_count > 1,
            "the rest of the viewport still needs layout"
        );
        let destination = start + line.len();
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            destination as i64 + 1,
            destination + 16 * line.len(),
        );
        if let neovm_core::window::Window::Leaf { force_start, .. } = eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
        {
            *force_start = true;
        }
        engine.layout_frame_rust(&mut eval, frame);
        assert_eq!(
            engine.last_layout_stats().reused_shifted_rows,
            2,
            "{boundary}"
        );
        let actual = selected_window_layout_trace(&eval, &engine, frame);
        let mut fresh = LayoutEngine::new();
        fresh.layout_frame_rust(&mut eval, frame);
        assert_eq!(
            actual,
            selected_window_layout_trace(&eval, &fresh, frame),
            "{boundary}"
        );
    }
}

#[test]
fn worker_page_preserves_literal_line_spacing() {
    for spacing in ["3", "0.3", "-2"] {
        for height in ["nil", "t", "1.3"] {
            first_visit_with_setup(
                Some("(:height 150)"),
                "ordinary offscreen text\n",
                None,
                0,
                5,
                Some(&format!(
                    "(progn (put-text-property 2761 3500 'line-spacing {spacing}) (put-text-property 2761 3500 'line-height {height}))"
                )),
            );
        }
    }
}

#[test]
fn worker_page_preserves_inherited_line_spacing_and_overrides() {
    for spacing in ["nil", "0", "0.3"] {
        for height in ["nil", "t"] {
            first_visit_with_setup(
                Some("(:height 150)"),
                "ordinary offscreen text\n",
                None,
                0,
                5,
                Some(&format!(
                    "(progn (make-local-variable 'line-spacing) (setq line-spacing 3) (put-text-property 2761 3500 'line-spacing {spacing}) (put-text-property 2761 3500 'line-height {height}))"
                )),
            );
        }
    }
}

#[test]
fn worker_admission_rejects_fontset_change_after_capture() {
    let (mut eval, frame, _, window) =
        incr_editing_frame(&"ordinary offscreen text\n".repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(2880))
        .unwrap();
    let before = neovm_core::emacs_core::fontset::fontset_generation();
    eval.eval_str("(set-fontset-font t #x25cb '(nil . \"iso10646-1\"))")
        .unwrap();
    assert_ne!(
        before,
        neovm_core::emacs_core::fontset::fontset_generation()
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match engine.scroll_coverage.drain(&mut engine.prepared_viewports) {
            Ok(false) => {
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
            result => {
                assert_eq!(
                    result,
                    Err(crate::row_layout::program::RowProgramError::Cancelled)
                );
                break;
            }
        }
    }
}

#[test]
fn worker_capture_rejects_fontset_change_between_idle_steps() {
    let (mut eval, frame, _, window) =
        incr_editing_frame(&"ordinary offscreen text\n".repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine
        .begin_scroll_coverage(&eval, frame, window, CharPos0::new(2880))
        .unwrap();
    assert_eq!(engine.capture_scroll_step(&eval), Ok(true));
    eval.eval_str("(set-fontset-font t #x25cb '(nil . \"iso10646-1\"))")
        .unwrap();
    assert_eq!(
        engine.capture_scroll_step(&eval),
        Err(crate::row_layout::program::RowProgramError::Cancelled)
    );
}

#[test]
fn idle_fontset_change_withdraws_published_worker_coverage() {
    let (mut eval, frame, _, window) =
        incr_editing_frame(&"ordinary offscreen text\n".repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine.maintain_scroll_coverage(&eval);
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(2880))
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(engine.take_scroll_coverage_publication());
    let display_window = neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64);
    assert!(
        engine
            .prepared_viewports
            .has_computed(frame, display_window)
    );
    eval.eval_str("(set-fontset-font t #x25cb '(nil . \"iso10646-1\"))")
        .unwrap();
    engine.maintain_scroll_coverage(&eval);
    assert!(
        !engine
            .prepared_viewports
            .has_computed(frame, display_window)
    );
    assert!(
        engine.take_scroll_coverage_publication(),
        "retiring font measurements must withdraw the compositor certificate"
    );
}

#[test]
fn ascii_worker_capture_does_not_measure_glyphs_on_evaluator() {
    let (mut eval, frame, _, window) =
        incr_editing_frame(&"ordinary offscreen text\n".repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    use crate::display_row::face_state::GLYPH_MEASURE_CALLS;
    GLYPH_MEASURE_CALLS.with(|calls| calls.set(0));
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(2880))
        .unwrap();
    assert_eq!(
        GLYPH_MEASURE_CALLS.with(|calls| calls.get()),
        0,
        "offscreen glyph measurement belongs on the row worker"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
}

#[test]
fn worker_admission_rejects_font_family_alternatives_after_capture() {
    let (mut eval, frame, _, window) =
        incr_editing_frame(&"ordinary offscreen text\n".repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(2880))
        .unwrap();
    eval.eval_str("(internal-set-alternative-font-family-alist '((\"monospace\" \"serif\")))")
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match engine.scroll_coverage.drain(&mut engine.prepared_viewports) {
            Ok(false) => {
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
            result => {
                assert_eq!(
                    result,
                    Err(crate::row_layout::program::RowProgramError::Cancelled)
                );
                break;
            }
        }
    }
}

#[test]
fn worker_admission_rejects_font_registry_alternatives_after_capture() {
    let (mut eval, frame, _, window) =
        incr_editing_frame(&"ordinary offscreen text\n".repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(2880))
        .unwrap();
    eval.eval_str(
        "(internal-set-alternative-font-registry-alist '((\"iso10646-1\" \"iso8859-1\")))",
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match engine.scroll_coverage.drain(&mut engine.prepared_viewports) {
            Ok(false) => {
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
            result => {
                assert_eq!(
                    result,
                    Err(crate::row_layout::program::RowProgramError::Cancelled)
                );
                break;
            }
        }
    }
}

#[test]
fn unicode_worker_capture_does_not_measure_glyphs_on_evaluator() {
    let (mut eval, frame, _, window) =
        incr_editing_frame(&"中文 offscreen text\n".repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    use crate::display_row::face_state::GLYPH_MEASURE_CALLS;
    GLYPH_MEASURE_CALLS.with(|calls| calls.set(0));
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(2880))
        .unwrap();
    assert_eq!(
        GLYPH_MEASURE_CALLS.with(|calls| calls.get()),
        0,
        "offscreen glyph measurement belongs on the row worker"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
}

#[test]
fn worker_unicode_page_with_mixed_font_attributes_matches_fresh_layout() {
    first_visit(
        Some("(:family \"serif\" :height 150 :weight bold :slant italic)"),
        "中文 café text\n",
        None,
    );
}

#[test]
fn first_visit_to_worker_prepared_wrapped_page_matches_fresh_layout() {
    first_visit(None, &format!("{}\n", "W".repeat(100)), None);
}

#[test]
fn worker_prepared_wrapped_mixed_font_page_matches_fresh_layout() {
    first_visit(
        Some("(:family \"serif\" :height 150 :weight bold :slant italic)"),
        &format!("{}\n", "W".repeat(100)),
        None,
    );
}

#[test]
fn first_visit_to_worker_prepared_word_wrapped_page_matches_fresh_layout() {
    first_visit_with_setup(
        None,
        &format!("{}\n", "wide words ".repeat(10)),
        None,
        0,
        5,
        Some("(setq word-wrap t)"),
    );
}

#[test]
fn worker_word_wrapping_rewinds_across_font_changes() {
    let line = format!("{}\n", "wide words ".repeat(10));
    let start = 120 * line.len() + 1;
    for face_at in [80, 90, 95, 100] {
        first_visit_with_setup(
            None,
            &line,
            None,
            0,
            3,
            Some(&format!(
                "(progn (setq word-wrap t) (let ((p {start}))
               (while (< p {})
                 (put-text-property (+ p {face_at}) (+ p {}) 'face
                   '(:family \"serif\" :height 130 :weight bold :slant italic))
                 (setq p (+ p {})))))",
                start + 12 * line.len(),
                face_at + 5,
                line.len()
            )),
        );
    }
}

#[test]
fn worker_word_wrapping_preserves_long_words_and_whitespace_boundaries() {
    for line in [
        format!("{} short\n", "W".repeat(90)),
        format!("{}word {}\n", " ".repeat(30), "W".repeat(75)),
        format!("{}\n", "wide   words ".repeat(9)),
    ] {
        first_visit_with_setup(None, &line, None, 0, 3, Some("(setq word-wrap t)"));
    }
}

#[test]
fn worker_wrapped_page_rejects_mutated_fringe_policy() {
    first_visit_with_setup(
        None,
        &format!("{}\n", "W".repeat(100)),
        Some("(setcar (cdr (assq 'continuation fringe-indicator-alist)) 'left-curly-arrow)"),
        0,
        5,
        Some("(setq fringe-indicator-alist '((continuation left-arrow right-arrow)))"),
    );
}

#[test]
fn canonical_word_wrap_rewinds_source_end_and_output_pen_with_glyphs() {
    let line = format!("{}word {}\n", " ".repeat(30), "W".repeat(75));
    let (mut eval, frame, _, _) = incr_editing_frame(&line.repeat(30), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str("(setq word-wrap t)").unwrap();
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let trace = selected_window_layout_trace(&eval, &engine, frame);
    let row = &trace.matrix_rows[0];
    assert_eq!(
        row.end_charpos, 35,
        "the overflowing word belongs wholly to the next row"
    );
    let snapshot = &trace.output_rows[0];
    assert_eq!(snapshot.end_col, 35);
    let width: f32 = row.glyph_areas[1]
        .iter()
        .map(|glyph| f32::from_bits(glyph.pixel_width_bits))
        .sum();
    assert_eq!(
        snapshot.end_x,
        width.round() as i64,
        "the pen stops at the last retained glyph"
    );
}

#[test]
fn worker_prepares_long_physical_lines_across_bounded_capture_steps() {
    first_visit_with_setup(
        None,
        &format!("{}\n", "W".repeat(180)),
        None,
        0,
        2,
        Some("(setq word-wrap t)"),
    );
}

#[test]
fn worker_prepares_tabs_after_wrapped_text_with_physical_line_origin() {
    for word_wrap in ["nil", "t"] {
        for width in [48, 49, 50, 51, 74, 75, 76, 180] {
            for tail in ["\tend", "\t\tend", "word \t tail"] {
                first_visit_with_setup(
                    None,
                    &format!("{}{}\n", "W".repeat(width), tail),
                    None,
                    0,
                    2,
                    Some(&format!("(setq word-wrap {word_wrap})")),
                );
            }
        }
    }
}

#[test]
fn worker_prepares_wrapped_raised_and_spaced_rows() {
    let line = format!("{}\tend\n", "words W ".repeat(24));
    let start = 120 * line.len();
    for properties in [
        "'display '(raise 0.2)",
        "'display '(raise -0.2)",
        "'line-height 1.3",
        "'line-spacing 3",
    ] {
        first_visit_with_setup(
            None,
            &line,
            None,
            0,
            2,
            Some(&format!(
                "(setq word-wrap t) (put-text-property {} {} {properties})",
                start + 1,
                start + 12 * line.len()
            )),
        );
    }
}

#[test]
fn worker_prepares_wrapped_box_and_extended_faces() {
    for face in [
        "(:background \"red\" :extend t)",
        "(:box (:line-width 2 :color \"blue\") :background \"red\" :extend t)",
        "(:family \"DejaVu Serif\" :weight bold :slant italic :height 125)",
    ] {
        first_visit_with_setup(
            Some(face),
            &format!("{}\tend\n", "words W ".repeat(24)),
            None,
            0,
            1,
            Some("(setq word-wrap t)"),
        );
    }
}

#[test]
fn worker_prepares_wrapped_rows_with_inherited_spacing() {
    for spacing in ["3", "0.5"] {
        first_visit_with_setup(
            None,
            &format!("{}\tend\n", "words W ".repeat(24)),
            None,
            0,
            1,
            Some(&format!("(setq word-wrap t line-spacing {spacing})")),
        );
    }
}

#[test]
fn worker_word_wrapping_rewinds_extended_face_boundaries() {
    let line = format!("{}\n", "wide words ".repeat(18));
    let start = 120 * line.len() + 1;
    for face_at in [70, 80, 90, 100] {
        first_visit_with_setup(
            None,
            &line,
            None,
            0,
            1,
            Some(&format!(
                "(setq word-wrap t)
             (let ((p {start}) (i 0))
               (while (< i 12)
                 (put-text-property (+ p {face_at}) (+ p {face_at} 20)
                    'face '(:height 180 :background \"red\" :extend t :box (:line-width 2)))
                 (setq i (+ i 1) p (+ p {}))))",
                line.len()
            )),
        );
    }
}

#[test]
fn worker_prepares_wrapped_unicode_source_offsets() {
    for text in [
        "café café ",
        "好好 words ",
        "á words ",
        "שלום words ",
        "سلام words ",
        "👩‍💻 words ",
    ] {
        for word_wrap in ["nil", "t"] {
            let line = format!("{}\n", text.repeat(10));
            first_visit_with_setup(
                None,
                &line,
                None,
                0,
                3,
                Some(&format!("(setq word-wrap {word_wrap})")),
            );
        }
    }
}

#[test]
fn worker_unicode_wrap_boundaries_and_mixed_fonts_match_fresh_layout() {
    for prefix in 62..=76 {
        let line = format!("{} á 好 سلام 👩‍💻 tail tail\n", "W".repeat(prefix));
        let chars = line.chars().count();
        let start = 120 * chars + 1;
        for wrap in ["nil", "t"] {
            first_visit_with_setup(
                None,
                &line,
                None,
                0,
                3,
                Some(&format!(
                    "(progn (setq word-wrap {wrap}) (let ((p {start}))
                  (while (< p {})
                    (put-text-property (+ p {}) (+ p {}) 'face
                      '(:family \"serif\" :height 130 :weight bold :slant italic))
                    (setq p (+ p {chars})))))",
                    start + 12 * chars,
                    prefix + 1,
                    prefix + 9
                )),
            );
        }
    }
}

#[test]
fn worker_prepares_long_unicode_source_fragments() {
    for text in [
        "café words ",
        "好好 words ",
        "á words ",
        "שלום words ",
        "سلام words ",
        "👩‍💻 words ",
    ] {
        for wrap in ["nil", "t"] {
            first_visit_with_setup(
                None,
                &format!("{}\n", text.repeat(30)),
                None,
                0,
                2,
                Some(&format!("(setq word-wrap {wrap})")),
            );
        }
    }
}

#[test]
fn worker_long_unicode_capture_boundaries_preserve_rich_faces() {
    for prefix in 124..=129 {
        let line = format!(
            "{}á 👩‍💻 سلام 好 {}\n",
            "W".repeat(prefix),
            "á words ".repeat(20)
        );
        for wrap in ["nil", "t"] {
            first_visit_with_setup(
                Some(
                    "(:family \"serif\" :height 130 :weight bold :box (:line-width 2) :background \"red\" :extend t)",
                ),
                &line,
                None,
                0,
                2,
                Some(&format!("(setq word-wrap {wrap})")),
            );
        }
    }
}

#[test]
fn worker_prepares_owned_overlay_insertions() {
    let line = "ordinary offscreen text\n";
    let start = 120 * line.len() + 1;
    for offset in [0, 5] {
        first_visit_with_setup_and_gc(None, line, None, 0, 2, Some(&format!(
            "(let ((p {start}) (i 0))
               (while (< i 12)
                 (let ((ov (make-overlay (+ p {offset}) (+ p {offset}))))
                   (overlay-put ov 'before-string (propertize \"[before]\" 'face '(:family \"serif\" :height 130 :weight bold)))
                   (overlay-put ov 'after-string (propertize \"[after]\" 'face '(:foreground \"red\"))))
                 (setq i (+ i 1) p (+ p {}))))", line.len())), true);
    }
}

#[test]
fn worker_overlay_insertions_reject_mutated_captured_sources() {
    let line = "ordinary offscreen text\n";
    let start = 120 * line.len() + 1;
    let setup = format!(
        "(progn (setq worker-insertion (propertize \"before\" 'face '(:height 130)))
        (setq worker-overlay (make-overlay {start} {start}))
        (overlay-put worker-overlay 'before-string worker-insertion))"
    );
    for change in [
        "(aset worker-insertion 0 ?X)",
        "(put-text-property 0 6 'face '(:height 180) worker-insertion)",
        "(progn (delete-overlay worker-overlay) (setq worker-overlay nil worker-insertion nil) (garbage-collect))",
    ] {
        first_visit_with_setup_and_gc(None, line, Some(change), 0, 2, Some(&setup), true);
    }
}

#[test]
fn worker_overlay_insertions_preserve_box_and_hover_faces() {
    let line = "ordinary offscreen text\n";
    let start = 120 * line.len() + 1;
    first_visit_with_setup_and_gc(None, line, None, 0, 2, Some(&format!(
        "(let ((p {start})) (while (< p {})
            (let ((ov (make-overlay (+ p 5) (+ p 5))))
                (overlay-put ov 'before-string (propertize \"AA\" 'face '(:height 140 :box (:line-width 2) :background \"blue\") 'mouse-face 'highlight))
                (overlay-put ov 'after-string (propertize \"BB\" 'face '(:height 90 :slant italic))))
            (setq p (+ p {}))))", start + 12 * line.len(), line.len())), true);
}

#[test]
fn worker_overlay_insertions_preserve_unstyled_default_face() {
    let line = "ordinary offscreen text\n";
    let start = 120 * line.len() + 1;
    first_visit_with_setup_and_gc(
        None,
        line,
        None,
        0,
        2,
        Some(&format!(
            "(overlay-put (make-overlay {start} {}) 'before-string \"plain\")",
            start + 100
        )),
        true,
    );
}

#[test]
fn worker_prepares_owned_display_replacement_strings() {
    let line = "ordinary offscreen text\n";
    let start = 120 * line.len() + 1;
    for overlay in [false, true] {
        let install = if overlay {
            "(overlay-put (make-overlay (+ p 5) (+ p 9)) 'display text)"
        } else {
            "(put-text-property (+ p 5) (+ p 9) 'display text)"
        };
        first_visit_with_setup_and_gc(None, line, None, 0, 2, Some(&format!(
            "(let ((p {start}) (text (propertize \"replace\" 'face '(:height 130 :weight bold))))
                (while (< p {}) {install} (setq p (+ p {}))))", start + 12 * line.len(), line.len())), true);
    }
}

#[test]
fn worker_replacement_strings_preserve_multiple_faces_and_unicode() {
    let line = "ordinary offscreen text\n";
    let start = 120 * line.len() + 1;
    for text in [
        r#"(concat (propertize "AA" 'face '(:height 140 :box (:line-width 2) :background "blue") 'mouse-face 'highlight) (propertize "BB" 'face '(:height 90 :slant italic)))"#,
        r#"(propertize "好á" 'face '(:height 130 :weight bold))"#,
    ] {
        first_visit_with_setup_and_gc(
            None,
            line,
            None,
            0,
            2,
            Some(&format!(
                "(let ((p {start}) (text {text})) (while (< p {}) (put-text-property (+ p 5) (+ p 9) 'display text) (setq p (+ p {}))))",
                start + 12 * line.len(),
                line.len()
            )),
            true,
        );
    }
}

#[test]
fn worker_replacement_strings_reject_mutated_sources() {
    let line = "ordinary offscreen text\n";
    let start = 120 * line.len() + 1;
    let setup = format!(
        "(progn (setq worker-replacement (propertize \"replace\" 'face '(:height 130))) (put-text-property (+ {start} 5) (+ {start} 9) 'display worker-replacement))"
    );
    for change in [
        "(aset worker-replacement 0 ?X)",
        "(put-text-property 0 7 'face '(:height 180) worker-replacement)",
    ] {
        first_visit_with_setup_and_gc(None, line, Some(change), 0, 2, Some(&setup), true);
    }
}

#[test]
fn idle_capture_aligns_forward_wrapped_rows_to_physical_source_start() {
    let mut checked_continuation = false;
    for length in [100, 180, 240] {
        let line = format!("{}\n", "W".repeat(length));
        let (mut eval, frame, _, window) = incr_editing_frame(&line.repeat(300), 800, 600);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame);
        let display_window = neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64);
        let rows: Vec<_> = engine.retained_window_matrices[&display_window]
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
            .collect();
        let anchor = rows[rows.len() - 2].start_charpos;
        if anchor % line.len() == 0 {
            continue;
        }
        checked_continuation = true;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while engine.maintain_scroll_coverage(&eval).is_some() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(
            engine
                .prepared_viewports
                .has_computed(frame, display_window),
            "forward visual row {anchor} inside a {length}-character physical line must produce coverage"
        );
    }
    assert!(checked_continuation);
}

#[test]
fn worker_prepares_lisp_selected_automatic_compositions() {
    for line in [
        "ordinary é xy text\n".to_owned(),
        format!("{}\n", "words é xy ".repeat(24)),
    ] {
        first_visit_with_setup(
            None,
            &line,
            None,
            0,
            2,
            Some(
                r#"(progn
            (setq auto-composition-mode t auto-composition-function 'auto-compose-chars
                  composition-function-table (make-char-table nil))
            (aset composition-function-table #x301 (list (vector ".́" 1 'font-shape-gstring)))
            (aset composition-function-table ?x (list (vector "xy" 0 'font-shape-gstring))))"#,
            ),
        );
    }
}

#[test]
fn worker_automatic_compositions_preserve_wrap_and_capture_boundaries() {
    for prefix in [70, 71, 72, 125, 126, 127, 128] {
        for wrap in ["nil", "t"] {
            let line = format!("{}xy é {}\n", "W".repeat(prefix), "words xy ".repeat(12));
            first_visit_with_setup(
                Some("(:height 130 :box (:line-width 2) :background \"blue\")"),
                &line,
                None,
                0,
                2,
                Some(&format!(
                    r#"(progn
                (setq word-wrap {wrap} auto-composition-mode t auto-composition-function 'auto-compose-chars
                      composition-function-table (make-char-table nil))
                (aset composition-function-table #x301 (list (vector ".́" 1 'font-shape-gstring)))
                (aset composition-function-table ?x (list (vector "xy" 0 'font-shape-gstring))))"#
                )),
            );
        }
    }
}

#[test]
fn worker_automatic_composition_rule_mutation_invalidates_coverage() {
    first_visit_with_setup(
        None,
        "ordinary xy xyz é text\n",
        Some("(aset composition-function-table ?x (list (vector \"xyz\" 0 'font-shape-gstring)))"),
        0,
        2,
        Some(
            r#"(progn
            (setq auto-composition-mode t auto-composition-function 'auto-compose-chars
                  composition-function-table (make-char-table nil))
            (aset composition-function-table ?x (list (vector "xy" 0 'font-shape-gstring))))"#,
        ),
    );
}

#[test]
fn worker_overlay_insertions_preserve_automatic_compositions() {
    let line = "ordinary offscreen text\n";
    let start = 120 * line.len() + 1;
    first_visit_with_setup_and_gc(
        None,
        line,
        None,
        0,
        2,
        Some(&format!(
            r#"(progn
        (setq auto-composition-mode t auto-composition-function 'auto-compose-chars
              composition-function-table (make-char-table nil))
        (aset composition-function-table #x301 (list (vector ".́" 1 'font-shape-gstring)))
        (let ((p {start})) (while (< p {})
            (overlay-put (make-overlay (+ p 5) (+ p 5)) 'before-string
                (propertize "é" 'face '(:height 130)))
            (setq p (+ p {})))))"#,
            start + 12 * line.len(),
            line.len()
        )),
        true,
    );
}

#[test]
fn worker_wraps_buffer_text_after_owned_strings() {
    for replacement in [false, true] {
        for wrap in ["nil", "t"] {
            let line = format!("ordinary offscreen {}\n", "long words ".repeat(24));
            let start = 120 * line.len() + 1;
            let install = if replacement {
                "(put-text-property (+ p 5) (+ p 9) 'display text)"
            } else {
                "(overlay-put (make-overlay (+ p 5) (+ p 5)) 'before-string text)"
            };
            first_visit_with_setup_and_gc(
                None,
                &line,
                None,
                0,
                2,
                Some(&format!(
                    "(progn (setq word-wrap {wrap}) (let ((p {start}) (text (propertize \"string\" 'face '(:height 130 :box (:line-width 2))))) (while (< p {}) {install} (setq p (+ p {})))))",
                    start + 12 * line.len(),
                    line.len()
                )),
                true,
            );
        }
    }
}

#[test]
fn worker_word_wrap_replays_owned_insertion_before_its_buffer_anchor() {
    for prefix in [45, 55, 60, 65] {
        let line = format!("{} {} tail words\n", "W".repeat(prefix), "x".repeat(45));
        let start = 120 * line.len() + 1;
        first_visit_with_setup_and_gc(
            None,
            &line,
            None,
            0,
            2,
            Some(&format!(
                r#"(progn
            (setq word-wrap t)
            (let ((p {start})) (while (< p {})
                (let ((ov (make-overlay (+ p {}) (+ p {}))))
                    (overlay-put ov 'before-string
                        (concat (propertize "AA" 'face '(:height 130 :box (:line-width 2) :background "blue" :extend t))
                                (propertize "B" 'face '(:height 90))))
                    (overlay-put ov 'after-string "C"))
                (setq p (+ p {})))))"#,
                start + 12 * line.len(),
                prefix + 1,
                prefix + 1,
                line.len()
            )),
            true,
        );
    }
}

#[test]
fn worker_prepares_bounded_invisible_spans_and_ellipses() {
    let line = "ordinary offscreen text\n";
    let start = 120 * line.len() + 1;
    for overlay in [false, true] {
        for ellipsis in ["nil", "t"] {
            let install = if overlay {
                "(overlay-put (make-overlay (+ p 5) (+ p 9)) 'invisible 'worker-hidden)"
            } else {
                "(put-text-property (+ p 5) (+ p 9) 'invisible 'worker-hidden)"
            };
            first_visit_with_setup_and_gc(
                None,
                line,
                None,
                0,
                2,
                Some(&format!(
                    "(progn (setq buffer-invisibility-spec '((worker-hidden . {ellipsis})))
                    (let ((p {start})) (while (< p {}) {install} (setq p (+ p {})))))",
                    start + 12 * line.len(),
                    line.len()
                )),
                true,
            );
        }
    }
}

#[test]
fn worker_invisible_spans_preserve_wrapping_faces_and_boundary_strings() {
    for overlay in [false, true] {
        for wrap in ["nil", "t"] {
            let line = format!("ordinary offscreen {}\n", "long words ".repeat(24));
            let start = 120 * line.len() + 1;
            let install = if overlay {
                "(overlay-put (make-overlay (+ p 40) (+ p 46)) 'invisible 'worker-hidden)"
            } else {
                "(put-text-property (+ p 40) (+ p 46) 'invisible 'worker-hidden)"
            };
            first_visit_with_setup_and_gc(
                None,
                &line,
                None,
                0,
                2,
                Some(&format!(
                    r#"(progn (setq word-wrap {wrap} buffer-invisibility-spec '((worker-hidden . t)))
                    (let ((p {start})) (while (< p {})
                        (put-text-property (+ p 12) (+ p 38) 'face
                            '(:family "DejaVu Serif" :height 130 :box (:line-width 2)))
                        (put-text-property (+ p 5) (+ p 8) 'display '(raise 0.2))
                        (let ((o (make-overlay (+ p 20) (+ p 20))))
                            (overlay-put o 'before-string (propertize "[before]" 'face '(:height 150)))
                            (overlay-put o 'after-string "[after]"))
                        (let ((o (make-overlay (+ p 40) (+ p 46))))
                            (overlay-put o 'before-string "edge-before")
                            (overlay-put o 'after-string "edge-after"))
                        {install}
                        (setq p (+ p {})))))"#,
                    start + 12 * line.len(),
                    line.len()
                )),
                true,
            );
        }
    }
}

#[test]
fn worker_invisible_spans_coalesce_and_invalidate_when_visibility_changes() {
    let line = "ordinary offscreen text\n";
    let start = 120 * line.len() + 1;
    let setup = format!(
        "(progn (setq buffer-invisibility-spec '((worker-hidden . t) worker-hidden-tail))
        (let ((p {start})) (while (< p {})
            (put-text-property (+ p 5) (+ p 7) 'invisible 'worker-hidden)
            (put-text-property (+ p 7) (+ p 9) 'invisible 'worker-hidden-tail)
            (setq p (+ p {})))))",
        start + 12 * line.len(),
        line.len()
    );
    for change in [
        None,
        Some("(setq buffer-invisibility-spec nil)"),
        Some("(put-text-property 2886 2888 'invisible nil)"),
    ] {
        first_visit_with_setup_and_gc(None, line, change, 0, 2, Some(&setup), true);
    }
}

#[test]
fn worker_invisible_ellipsis_uses_preceding_buffer_face_not_hidden_face() {
    let line = "ordinary offscreen text\n";
    let start = 120 * line.len() + 1;
    first_visit_with_setup_and_gc(
        None,
        line,
        None,
        0,
        2,
        Some(&format!(
            r#"(progn (setq buffer-invisibility-spec '((worker-hidden . t)))
            (let ((p {start})) (while (< p {})
                (put-text-property p (+ p 5) 'face '(:family "DejaVu Serif" :height 150 :box (:line-width 2)))
                (put-text-property (+ p 5) (+ p 9) 'face '(:height 240))
                (put-text-property (+ p 5) (+ p 9) 'invisible 'worker-hidden)
                (setq p (+ p {})))))"#,
            start + 12 * line.len(),
            line.len()
        )),
        true,
    );
}

#[test]
fn hidden_box_neighbour_face_does_not_enlarge_visible_rows() {
    let measure = |hidden_height| {
        let (mut eval, frame, _, window) =
            incr_editing_frame(&"ordinary offscreen text\n".repeat(30), 800, 600);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        eval.eval_str(&format!(
            "(progn (setq buffer-invisibility-spec '((worker-hidden . t)))
             (put-text-property 1 6 'face '(:height 150 :box (:line-width 2)))
             (put-text-property 6 10 'face '(:height {hidden_height}))
             (put-text-property 6 10 'invisible 'worker-hidden))"
        ))
        .unwrap();
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame);
        let owner = neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64);
        engine.retained_window_matrices[&owner]
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
            .map(|row| (row.height_px, row.ascent_px))
            .collect::<Vec<_>>()
    };
    assert_eq!(measure(100), measure(240));
}

#[test]
fn idle_worker_publishes_a_closed_prefix_before_full_page_capture() {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame, _, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str(
        "(progn (setq worker-raise-spec (list 'raise 0.25))
        (put-text-property 1 (point-max) 'display worker-raise-spec))",
    )
    .unwrap();
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let before = selected_window_layout_trace(&eval, &engine, frame);
    for _ in 0..4 {
        assert!(engine.maintain_scroll_coverage(&eval).is_some());
    }
    eval.gc_collect_exact();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "four closed rows should be published before acquiring the rest of the page"
        );
        std::thread::yield_now();
    }
    assert!(engine.take_scroll_coverage_publication());
    let owner = neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64);
    assert!(engine.prepared_viewports.has_computed(frame, owner));
    assert_eq!(before, selected_window_layout_trace(&eval, &engine, frame));
    // Growing closed prefixes extend coverage during acquisition instead of
    // waiting for the whole page. Double the publication frontier so total
    // worker replay remains linear in the final page size.
    for additional_rows in [4, 8] {
        for _ in 0..additional_rows {
            assert!(engine.maintain_scroll_coverage(&eval).is_some());
        }
        eval.gc_collect_exact();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !engine
            .scroll_coverage
            .drain(&mut engine.prepared_viewports)
            .unwrap()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "a growing closed prefix should publish before full-page acquisition"
            );
            std::thread::yield_now();
        }
        assert!(engine.take_scroll_coverage_publication());
        assert_eq!(before, selected_window_layout_trace(&eval, &engine, frame));
    }
    eval.eval_str("(setcar (cdr worker-raise-spec) 0.75)")
        .unwrap();
    engine.maintain_scroll_coverage(&eval);
    assert!(
        engine.take_scroll_coverage_publication(),
        "withdraw stale prefix"
    );
    assert!(!engine.prepared_viewports.has_computed(frame, owner));
}

#[test]
fn rich_visible_rows_do_not_clone_the_active_face_per_glyph() {
    let line = format!("{}\n", "ordinary characters ".repeat(10));
    let (mut eval, frame, _, _) = incr_editing_frame(&line.repeat(30), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str(
        "(progn
        (put-text-property 1 (point-max) 'face '(:height 130 :box (:line-width 2)))
        (put-text-property 1 (point-max) 'display '(raise 0.2)))",
    )
    .unwrap();
    let mut engine = LayoutEngine::new();
    crate::display_row::face_state::take_active_face_clone_count();
    engine.layout_frame_rust(&mut eval, frame);
    let clones = crate::display_row::face_state::take_active_face_clone_count();
    assert!(
        clones < 32,
        "ordinary source glyphs copied the active face {clones} times"
    );
}

#[test]
fn idle_precomputation_prioritizes_the_current_scroll_direction() {
    let line = "ordinary offscreen text\n";
    for next_line in [119usize, 121] {
        let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(400), 800, 600);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            (120 * line.len() + 1) as i64,
            125 * line.len(),
        );
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while engine.maintain_scroll_coverage(&eval).is_some() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            (next_line * line.len() + 1) as i64,
            (next_line + 5) * line.len(),
        );
        engine.layout_frame_rust(&mut eval, frame);
        engine.maintain_scroll_coverage(&eval);
        let start = engine
            .scroll_coverage
            .active_capture_start_for_test()
            .expect("new page capture");
        if next_line < 120 {
            assert!(
                start < next_line * line.len(),
                "backward scrolling prioritized a forward page at {start}"
            );
            let frontier = engine
                .last_frame_display_state
                .as_ref()
                .unwrap()
                .scroll_coverage
                .iter()
                .find(|coverage| coverage.content.window_id.get() == window.0 as i64)
                .unwrap()
                .content
                .matrix
                .rows
                .first()
                .unwrap()
                .start_charpos;
            assert!(
                start < frontier,
                "backward preparation at {start} must extend coverage before {frontier}, not recapture already prepared rows"
            );
        } else {
            assert!(
                start > next_line * line.len(),
                "forward scrolling prioritized a backward page at {start}"
            );
        }
        let actual = selected_window_layout_trace(&eval, &engine, frame);
        let mut fresh = LayoutEngine::new();
        fresh.layout_frame_rust(&mut eval, frame);
        assert_eq!(actual, selected_window_layout_trace(&eval, &fresh, frame));
    }
}

#[test]
fn backward_reversal_retargets_an_active_forward_capture() {
    check_active_capture_reversal(false, false, false, false);
}

#[test]
fn fractional_backward_reversal_retargets_an_active_forward_capture() {
    check_active_capture_reversal(false, true, false, false);
}

#[test]
fn rich_backward_reversal_retargets_an_active_forward_capture() {
    check_active_capture_reversal(true, false, false, false);
}

#[test]
fn backward_reversal_keeps_forward_capture_with_sufficient_headroom() {
    check_active_capture_reversal(false, false, true, false);
}

#[test]
fn backward_reversal_preserves_admitted_forward_prefix() {
    check_active_capture_reversal(false, false, false, true);
}

fn check_active_capture_reversal(rich: bool, fractional: bool, prepared: bool, preview: bool) {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame, buffer, window) = if rich {
        shared_rich_scrolling_frame(664, 646)
    } else {
        let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(400), 800, 600);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        (eval, frame, buffer, window)
    };
    let origin = if rich {
        5574160 - 48 * 110080
    } else {
        120 * line.len()
    };
    let place = |eval: &mut Context, start: usize, hidden: i32| {
        let point = eval
            .buffer_manager()
            .get(buffer)
            .unwrap()
            .char_pos_to_emacs_byte_pos_clamped(CharPos0::new(start + 100))
            .get();
        scroll_window_to(eval, frame, window, buffer, start as i64 + 1, point);
        if let neovm_core::window::Window::Leaf {
            force_start,
            vscroll,
            ..
        } = eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
        {
            *force_start = true;
            *vscroll = -hidden;
        }
    };
    place(&mut eval, origin, 8);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    assert!(engine.maintain_scroll_coverage(&eval).is_some());
    let owner = DisplayWindowId::new(window.0 as i64);
    if preview {
        for _ in 0..3 {
            engine.maintain_scroll_coverage(&eval);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !engine
            .scroll_coverage
            .drain(&mut engine.prepared_viewports)
            .unwrap()
        {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(engine.prepared_viewports.has_computed(frame, owner));
        eval.gc_collect_exact();
        engine.layout_frame_rust(&mut eval, frame);
    }
    if prepared {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while engine.maintain_scroll_coverage(&eval).is_some() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        engine.layout_frame_rust(&mut eval, frame);
        let start = engine.retained_window_matrices[&owner]
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
            .last()
            .unwrap()
            .start_charpos;
        let forward = eval
            .eval_str(&format!(
                "(save-excursion (goto-char {}) (line-beginning-position))",
                start + 1
            ))
            .unwrap()
            .as_fixnum()
            .unwrap() as usize
            - 1;
        engine
            .begin_scroll_coverage(&eval, frame, window, CharPos0::new(forward))
            .unwrap();
    }
    let forward = engine
        .scroll_coverage
        .active_capture_start_for_test()
        .unwrap();
    assert!(forward > origin, "establish an active forward capture");
    let start = if fractional {
        origin
    } else if rich {
        eval.eval_str(&format!(
            "(save-excursion (goto-char {}) (forward-line -1) (point))",
            origin + 1
        ))
        .unwrap()
        .as_fixnum()
        .unwrap() as usize
            - 1
    } else {
        origin - line.len()
    };
    place(&mut eval, start, 4);
    engine.layout_frame_rust(&mut eval, frame);
    let visible = selected_window_layout_trace(&eval, &engine, frame);
    let headroom = |engine: &LayoutEngine| {
        engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .materialize()
            .scroll_surfaces
            .iter()
            .find(|surface| surface.coverage().content.window_id == owner)
            .map_or(0.0, |surface| -surface.clamp_offset(-f32::MAX))
    };
    if prepared {
        assert!(
            headroom(&engine) > 100.0,
            "prepared backward rows leave time for forward acquisition"
        );
    } else {
        assert!(headroom(&engine) < 34.0, "backward preparation is urgent");
    }
    let coverage_end = |engine: &LayoutEngine| {
        engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .scroll_coverage
            .iter()
            .find(|coverage| coverage.content.window_id == owner)
            .map(|coverage| coverage.content.matrix.rows.last().unwrap().end_charpos)
    };
    let original_end = coverage_end(&engine);
    if preview {
        assert!(
            original_end.is_some(),
            "published forward prefix connects to the viewport"
        );
    }
    engine.maintain_scroll_coverage(&eval);
    let active = engine
        .scroll_coverage
        .pending_source_start_for_test()
        .unwrap();
    if prepared {
        assert_eq!(
            active, forward,
            "adequate backward headroom must preserve the active forward capture"
        );
    } else {
        assert!(
            active < start,
            "reversal with little headroom must start a backward bridge instead of continuing forward capture at {active}"
        );
    }
    if preview {
        eval.gc_collect_exact();
        engine.layout_frame_rust(&mut eval, frame);
        assert_eq!(
            coverage_end(&engine),
            original_end,
            "yielding a producer must retain its already admitted prefix across GC"
        );
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while engine.maintain_scroll_coverage(&eval).is_some() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    engine.layout_frame_rust(&mut eval, frame);
    assert!(
        headroom(&engine) > 34.0,
        "the worker must provide usable backward pixel coverage, got {}",
        headroom(&engine)
    );
    assert!(
        visible == selected_window_layout_trace(&eval, &engine, frame),
        "idle preparation must preserve the visible layout"
    );
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(&mut eval, frame);
    assert!(
        visible == selected_window_layout_trace(&eval, &fresh, frame),
        "visible layout must match fresh layout"
    );
}

#[test]
fn urgent_backward_bridge_publishes_after_one_physical_line() {
    check_short_urgent_backward_bridge(false);
}

#[test]
fn rich_fractional_reversal_starts_the_nearest_physical_bridge() {
    check_short_urgent_backward_bridge(true);
}

#[test]
fn fractional_reversal_extends_the_prepared_edge_instead_of_recapturing_it() {
    check_fractional_reversal_prepared_edge(false, false);
}

#[test]
fn fractional_reversal_restarts_preparation_after_the_queue_is_idle() {
    check_fractional_reversal_prepared_edge(true, false);
}

#[test]
fn backward_preparation_follows_a_published_bridge_without_viewport_motion() {
    check_fractional_reversal_prepared_edge(false, true);
}

fn check_fractional_reversal_prepared_edge(idle: bool, extend_again: bool) {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(400), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    if extend_again {
        eval.eval_str("(put-text-property (point-min) (point-max) 'face '(:height 80))")
            .unwrap();
    }
    let origin = 120 * line.len();
    let point = eval
        .buffer_manager()
        .get(buffer)
        .unwrap()
        .char_pos_to_emacs_byte_pos_clamped(CharPos0::new(origin + 100))
        .get();
    scroll_window_to(&mut eval, frame, window, buffer, origin as i64 + 1, point);
    let hide = |eval: &mut Context, pixels: i32| {
        if let neovm_core::window::Window::Leaf {
            force_start,
            vscroll,
            ..
        } = eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
        {
            *force_start = true;
            *vscroll = -pixels;
        }
    };
    hide(&mut eval, 8);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine.maintain_scroll_coverage(&eval);
    let mut prepared_start = origin - line.len();
    engine
        .request_scroll_bridge(
            &eval,
            frame,
            window,
            CharPos0::new(prepared_start),
            CharPos0::new(origin),
        )
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    engine.layout_frame_rust(&mut eval, frame);
    let owner = DisplayWindowId::new(window.0 as i64);
    assert_eq!(
        engine.prepared_viewports.backward_start(
            frame,
            owner,
            &engine.retained_window_matrices[&owner].key
        ),
        Some(CharPos0::new(prepared_start))
    );
    if idle {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while engine.maintain_scroll_coverage(&eval).is_some() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        engine.layout_frame_rust(&mut eval, frame);
        assert!(engine.maintain_scroll_coverage(&eval).is_none());
        prepared_start = engine
            .prepared_viewports
            .backward_start(frame, owner, &engine.retained_window_matrices[&owner].key)
            .unwrap()
            .get();
    } else {
        engine
            .begin_scroll_coverage(
                &eval,
                frame,
                window,
                CharPos0::new(origin + 60 * line.len()),
            )
            .unwrap();
    }
    hide(&mut eval, 4);
    engine.layout_frame_rust(&mut eval, frame);
    let visible = selected_window_layout_trace(&eval, &engine, frame);
    engine.maintain_scroll_coverage(&eval);
    let next = engine
        .scroll_coverage
        .pending_source_start_for_test()
        .expect("fractional reversal must schedule work even after the queue is idle");
    assert!(
        next < prepared_start,
        "fractional reversal must acquire outside the prepared edge {prepared_start}, got {next}"
    );
    if !idle {
        assert_eq!(
            next,
            prepared_start - line.len(),
            "urgent preparation takes the nearest line"
        );
    }
    if extend_again {
        let start = engine.retained_window_matrices[&owner].key.window_start;
        let vscroll = engine.retained_window_matrices[&owner].key.vscroll;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !engine
            .scroll_coverage
            .drain(&mut engine.prepared_viewports)
            .unwrap()
        {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        engine.layout_frame_rust(&mut eval, frame);
        assert_eq!(
            engine.retained_window_matrices[&owner].key.window_start,
            start
        );
        assert_eq!(engine.retained_window_matrices[&owner].key.vscroll, vscroll);
        let edge = engine
            .prepared_viewports
            .backward_start(frame, owner, &engine.retained_window_matrices[&owner].key)
            .unwrap()
            .get();
        assert_eq!(
            edge, next,
            "the completed bridge must extend exported coverage"
        );
        engine.maintain_scroll_coverage(&eval);
        let (bridge, end) = engine
            .scroll_coverage
            .pending_backward_bridge_for_test(owner)
            .expect("publication must queue another useful bridge without viewport motion");
        assert!(bridge.get() < edge);
        assert_eq!(
            end,
            CharPos0::new(edge),
            "the next bridge must end at the newly exported edge"
        );
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while engine.maintain_scroll_coverage(&eval).is_some() {
        assert!(std::time::Instant::now() < deadline);
        if extend_again && engine.take_scroll_coverage_publication() {
            engine.layout_frame_rust(&mut eval, frame);
        }
        std::thread::yield_now();
    }
    eval.gc_collect_exact();
    engine.layout_frame_rust(&mut eval, frame);
    if extend_again {
        assert!(
            engine.maintain_scroll_coverage(&eval).is_none(),
            "bounded prepared coverage must eventually stop extending at a stationary viewport"
        );
    }
    assert_eq!(visible, selected_window_layout_trace(&eval, &engine, frame));
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(&mut eval, frame);
    assert_eq!(visible, selected_window_layout_trace(&eval, &fresh, frame));
}

fn check_short_urgent_backward_bridge(rich: bool) {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame, buffer, window) = if rich {
        shared_rich_scrolling_frame(664, 646)
    } else {
        let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(400), 800, 600);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        (eval, frame, buffer, window)
    };
    let origin = if rich {
        5565000 - 48 * 110080
    } else {
        120 * line.len()
    };
    let point = eval
        .buffer_manager()
        .get(buffer)
        .unwrap()
        .char_pos_to_emacs_byte_pos_clamped(CharPos0::new(origin + 100))
        .get();
    scroll_window_to(&mut eval, frame, window, buffer, origin as i64 + 1, point);
    let set_hidden = |eval: &mut Context, hidden: i32| {
        if let neovm_core::window::Window::Leaf {
            force_start,
            vscroll,
            ..
        } = eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
        {
            *force_start = true;
            *vscroll = -hidden;
        }
    };
    set_hidden(&mut eval, 8);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine.maintain_scroll_coverage(&eval);
    set_hidden(&mut eval, 4);
    engine.layout_frame_rust(&mut eval, frame);
    let visible = selected_window_layout_trace(&eval, &engine, frame);
    let nearest = eval
        .eval_str(&format!(
            "(save-excursion (goto-char {}) (forward-line -1) (point))",
            origin + 1
        ))
        .unwrap()
        .as_fixnum()
        .unwrap() as usize
        - 1;
    engine.maintain_scroll_coverage(&eval);
    assert_eq!(
        engine.scroll_coverage.pending_source_start_for_test(),
        Some(nearest),
        "low backward headroom must acquire the nearest physical line first"
    );
    if !rich {
        assert!(
            engine
                .scroll_coverage
                .active_capture_start_for_test()
                .is_none(),
            "one plain physical line must reach the worker in a single capture slice"
        );
    }
    engine.take_scroll_coverage_publication();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "urgent bridge never published"
        );
        if engine
            .scroll_coverage
            .drain(&mut engine.prepared_viewports)
            .unwrap()
        {
            break;
        }
        if rich {
            engine.maintain_scroll_coverage(&eval);
            if engine.take_scroll_coverage_publication() {
                break;
            }
        }
        std::thread::yield_now();
    }
    eval.gc_collect_exact();
    engine.layout_frame_rust(&mut eval, frame);
    let owner = DisplayWindowId::new(window.0 as i64);
    let coverage = engine
        .last_frame_display_state
        .as_ref()
        .unwrap()
        .scroll_coverage
        .iter()
        .find(|coverage| coverage.content.window_id == owner)
        .expect("connected bridge");
    assert_eq!(
        coverage.content.matrix.rows[0].start_charpos, nearest,
        "published coverage must extend to the nearest acquired physical line"
    );
    assert!(
        visible == selected_window_layout_trace(&eval, &engine, frame),
        "bridge publication must preserve visible geometry"
    );
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(&mut eval, frame);
    assert!(
        visible == selected_window_layout_trace(&eval, &fresh, frame),
        "visible geometry must match a fresh layout"
    );
}

#[test]
fn urgent_bridge_resumes_forward_capture_progress_after_gc() {
    check_paused_capture_resume(false, false, PausedCaptureMutation::None);
}

#[test]
fn urgent_bridge_preempts_and_resumes_distant_backward_capture() {
    check_paused_capture_resume(false, true, PausedCaptureMutation::None);
}

#[test]
fn rich_urgent_bridge_resumes_captured_fragments_after_gc() {
    check_paused_capture_resume(true, false, PausedCaptureMutation::None);
}

#[test]
fn paused_capture_rejects_in_place_property_mutation_before_resume() {
    check_paused_capture_resume(false, false, PausedCaptureMutation::InPlace);
}

#[test]
fn paused_capture_is_retired_after_buffer_revision() {
    check_paused_capture_resume(false, false, PausedCaptureMutation::BufferRevision);
}

enum PausedCaptureMutation {
    None,
    InPlace,
    BufferRevision,
}

fn check_paused_capture_resume(rich: bool, backward: bool, mutation: PausedCaptureMutation) {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame, buffer, window) = if rich {
        shared_rich_scrolling_frame(664, 646)
    } else {
        let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(400), 800, 600);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        (eval, frame, buffer, window)
    };
    let origin = if rich {
        5565000 - 48 * 110080
    } else {
        120 * line.len()
    };
    let distance = if backward { -40 } else { 60 };
    let producer = eval
        .eval_str(&format!(
            "(save-excursion (goto-char {}) (forward-line {distance}) (point))",
            origin + 1
        ))
        .unwrap()
        .as_fixnum()
        .unwrap() as usize
        - 1;
    if matches!(mutation, PausedCaptureMutation::InPlace) {
        eval.eval_str(&format!(
            "(progn (setq paused-raise-spec (list 'raise 0.25)) (put-text-property {} {} 'display paused-raise-spec))",
            producer + 1, producer + 10 * line.len()
        )).unwrap();
    }
    let point = eval
        .buffer_manager()
        .get(buffer)
        .unwrap()
        .char_pos_to_emacs_byte_pos_clamped(CharPos0::new(origin + 100))
        .get();
    scroll_window_to(&mut eval, frame, window, buffer, origin as i64 + 1, point);
    let set_hidden = |eval: &mut Context, hidden: i32| {
        if let neovm_core::window::Window::Leaf {
            force_start,
            vscroll,
            ..
        } = eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
        {
            *force_start = true;
            *vscroll = -hidden;
        }
    };
    set_hidden(&mut eval, 8);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine.maintain_scroll_coverage(&eval);
    engine
        .begin_scroll_coverage(&eval, frame, window, CharPos0::new(producer))
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while engine
        .scroll_coverage
        .active_capture_progress_for_test()
        .unwrap()
        .2
        < 3
    {
        assert!(std::time::Instant::now() < deadline);
        assert_eq!(engine.capture_scroll_step(&eval), Ok(true));
    }
    let before = engine
        .scroll_coverage
        .active_capture_progress_for_test()
        .unwrap();
    assert!(
        before.1 > producer,
        "the producer has already acquired source"
    );
    set_hidden(&mut eval, 4);
    engine.layout_frame_rust(&mut eval, frame);
    let mut visible = selected_window_layout_trace(&eval, &engine, frame);
    let nearest = eval
        .eval_str(&format!(
            "(save-excursion (goto-char {}) (forward-line -1) (point))",
            origin + 1
        ))
        .unwrap()
        .as_fixnum()
        .unwrap() as usize
        - 1;
    engine.maintain_scroll_coverage(&eval);
    assert_eq!(
        engine.scroll_coverage.pending_source_start_for_test(),
        Some(nearest),
        "urgent connecting work must preempt both forward and distant backward producers"
    );
    eval.gc_collect_exact();
    engine.take_scroll_coverage_publication();
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "urgent bridge never published"
        );
        if engine
            .scroll_coverage
            .drain(&mut engine.prepared_viewports)
            .unwrap()
        {
            break;
        }
        engine.maintain_scroll_coverage(&eval);
        if engine.take_scroll_coverage_publication() {
            break;
        }
        std::thread::yield_now();
    }
    engine.layout_frame_rust(&mut eval, frame);
    if matches!(mutation, PausedCaptureMutation::InPlace) {
        eval.eval_str("(setcar (cdr paused-raise-spec) 0.75)")
            .unwrap();
    }
    if matches!(mutation, PausedCaptureMutation::BufferRevision) {
        eval.eval_str("(save-excursion (goto-char (point-max)) (insert \"new revision\\n\"))")
            .unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        // Appending changes the window-end offsets from Z, even though the
        // visible text is identical. Compare against this new revision.
        visible = selected_window_layout_trace(&eval, &engine, frame);
    }
    eval.gc_collect_exact();
    engine.maintain_scroll_coverage(&eval);
    // Newly published coverage can still leave less than two rows of
    // headroom. Allow another connecting bridge before resuming the saved
    // producer, while preserving the mutation and GC checks below.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while engine.scroll_coverage.has_deferred_capture_for_test() {
        assert!(std::time::Instant::now() < deadline);
        engine.maintain_scroll_coverage(&eval);
        if engine.take_scroll_coverage_publication() {
            engine.layout_frame_rust(&mut eval, frame);
        }
        std::thread::yield_now();
    }
    assert!(
        !engine.scroll_coverage.has_deferred_capture_for_test(),
        "saved acquisition must be resumed or retired after servicing the bridge"
    );
    if matches!(mutation, PausedCaptureMutation::BufferRevision) {
        assert!(
            engine
                .scroll_coverage
                .active_capture_progress_for_test()
                .is_none_or(|progress| progress.2 < before.2),
            "a new buffer revision must discard all paused old fragments"
        );
    } else if matches!(mutation, PausedCaptureMutation::InPlace) {
        assert!(
            engine
                .scroll_coverage
                .active_capture_progress_for_test()
                .is_none(),
            "a paused read certificate must reject in-place mutation before extending capture"
        );
        assert!(
            engine
                .scroll_coverage
                .pending_source_start_for_test()
                .is_none(),
            "mutated saved programs must never be submitted"
        );
    } else {
        let resumed = engine
            .scroll_coverage
            .active_capture_progress_for_test()
            .expect("resume the bounded unfinished producer");
        assert_eq!(
            resumed.0, producer,
            "resume saved work before distant fresh acquisition"
        );
        assert!(
            resumed.1 >= before.1 && resumed.2 > before.2,
            "resuming must preserve source progress and captured fragments: {before:?} -> {resumed:?}"
        );
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while engine.maintain_scroll_coverage(&eval).is_some() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    engine.layout_frame_rust(&mut eval, frame);
    assert!(
        visible == selected_window_layout_trace(&eval, &engine, frame),
        "paused work must preserve visible geometry"
    );
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(&mut eval, frame);
    assert!(
        visible == selected_window_layout_trace(&eval, &fresh, frame),
        "visible geometry must match fresh layout after resuming"
    );
}

#[test]
fn repeated_idle_preview_preserves_a_complete_prepared_page() {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame, _, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine.maintain_scroll_coverage(&eval);
    let start = engine
        .scroll_coverage
        .active_capture_start_for_test()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while engine.maintain_scroll_coverage(&eval).is_some() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    let owner = neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64);
    let mut key = engine.retained_window_matrices[&owner].key.clone();
    key.window_start = start as i64;
    key.point = (start + 5 * line.len()) as i64;
    let (before, _) = engine
        .prepared_viewports
        .replay(frame, owner, &key, false)
        .expect("initial full page");
    assert!(before.body_rows.len() > 10);
    // Retargeting may repeat a still-valid physical start. Its early preview
    // must not withdraw the larger page already available to scrolling.
    engine.scroll_coverage.cancel();
    for _ in 0..4 {
        engine.maintain_scroll_coverage(&eval);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    let (after, _) = engine
        .prepared_viewports
        .replay(frame, owner, &key, false)
        .expect("short preview must preserve the complete prepared page");
    assert_eq!(before.body_rows.len(), after.body_rows.len());
}

#[test]
fn backward_precomputation_connects_fragmented_lines_to_the_viewport() {
    let line = format!("{}\n", "W".repeat(180));
    let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    scroll_window_to(
        &mut eval,
        frame,
        window,
        buffer,
        (120 * line.len() + 1) as i64,
        121 * line.len(),
    );
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let before = selected_window_layout_trace(&eval, &engine, frame);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while engine.maintain_scroll_coverage(&eval).is_some() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    engine.layout_frame_rust(&mut eval, frame);
    let coverage = engine
        .last_frame_display_state
        .as_ref()
        .unwrap()
        .scroll_coverage
        .iter()
        .find(|coverage| coverage.content.window_id.get() == window.0 as i64)
        .expect("prepared coverage");
    assert!(
        coverage.anchor_row > 0,
        "backward preparation must connect to the viewport even when a far page exhausts the fragment budget"
    );
    assert_eq!(before, selected_window_layout_trace(&eval, &engine, frame));
}

#[test]
fn repeated_idle_preview_preserves_a_longer_partial_prepared_page() {
    let line = format!("{}\n", "W".repeat(180));
    let (mut eval, frame, _, window) = incr_editing_frame(&line.repeat(300), 4000, 1000);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while engine.maintain_scroll_coverage(&eval).is_some() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    engine.layout_frame_rust(&mut eval, frame);
    let coverage_end = |engine: &LayoutEngine| {
        engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .scroll_coverage
            .iter()
            .find(|coverage| coverage.content.window_id.get() == window.0 as i64)
            .expect("prepared coverage")
            .content
            .matrix
            .rows
            .last()
            .unwrap()
            .end_charpos
    };
    let before = coverage_end(&engine);
    let prepare_far = |engine: &mut LayoutEngine, eval: &neovm_core::emacs_core::Context, start| {
        engine
            .request_scroll_coverage(eval, frame, window, CharPos0::new(start))
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !engine
            .scroll_coverage
            .drain(&mut engine.prepared_viewports)
            .unwrap()
        {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
    };
    // Fill the bounded cache, leaving the current useful prefix oldest.
    for index in 0..7 {
        prepare_far(&mut engine, &eval, (100 + index * 10) * line.len());
    }
    engine.take_scroll_coverage_publication();
    engine.scroll_coverage.cancel();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine.take_scroll_coverage_publication() {
        engine.maintain_scroll_coverage(&eval);
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(
        coverage_end(&engine),
        before,
        "a smaller preview must not withdraw still-valid partial coverage"
    );
    // Accepting the repeated prefix is still a visit to this page. A new
    // unrelated page should evict an older target, not this useful coverage.
    prepare_far(&mut engine, &eval, 250 * line.len());
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(
        coverage_end(&engine),
        before,
        "a retained larger prefix must inherit the new preview's cache recency"
    );
}

fn shared_rich_scrolling_frame(
    width: u32,
    height: u32,
) -> (
    Context,
    neovm_core::window::FrameId,
    BufferId,
    neovm_core::window::WindowId,
) {
    let (mut eval, frame, buffer, window) = incr_editing_frame("", width, height);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    // The context has no frontend font-list callback. Actual layout still uses
    // these installed native families; content and face/overlay construction
    // come from the same fixture as the GUI latency test.
    let source = include_str!("../../../../neomacs-perf/fixtures/scrolling-content.el")
        .replace("(display-graphic-p)", "t")
        .replace(
            "(font-family-list)",
            "'(\"DejaVu Sans Mono\" \"DejaVu Serif\" \"DejaVu Sans\")",
        );
    for file in [
        "emacs-lisp/byte-run.el",
        "emacs-lisp/backquote.el",
        "subr.el",
        "emacs-lisp/macroexp.el",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../lisp")
            .join(file);
        eval.eval_str(&format!("(load {:?} nil nil t)", path.to_str().unwrap()))
            .unwrap();
    }
    eval.eval_str(&source).unwrap();
    eval.eval_str("(neomacs-scroll-content-insert 3000)")
        .unwrap();
    (eval, frame, buffer, window)
}

#[test]
fn shared_rich_fixture_worker_prepares_the_overlay_line() {
    let (mut eval, frame, buffer, window) = shared_rich_scrolling_frame(1000, 800);
    // Line 2000 has the same block/overlay phase as GUI line 50000.
    let start = 2 * 110080;
    let point = eval
        .buffer_manager()
        .get(buffer)
        .unwrap()
        .char_pos_to_emacs_byte_pos_clamped(CharPos0::new(start as usize))
        .get();
    scroll_window_to(&mut eval, frame, window, buffer, start + 1, point);
    if let neovm_core::window::Window::Leaf { force_start, .. } = eval
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    {
        *force_start = true;
    }
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(start as usize + 1760))
        .expect("capture the complex overlay line");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match engine.scroll_coverage.drain(&mut engine.prepared_viewports) {
            Ok(true) => break,
            Ok(false) => {
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
            Err(error) => panic!("the overlay row was rejected: {error:?}"),
        }
    }
    let point = eval
        .buffer_manager()
        .get(buffer)
        .unwrap()
        .char_pos_to_emacs_byte_pos_clamped(CharPos0::new(start as usize + 2020))
        .get();
    scroll_window_to(&mut eval, frame, window, buffer, start + 2021, point);
    if let neovm_core::window::Window::Leaf { force_start, .. } = eval
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    {
        *force_start = true;
    }
    engine.layout_frame_rust(&mut eval, frame);
    let coverage = engine
        .last_frame_display_state
        .as_ref()
        .unwrap()
        .scroll_coverage
        .iter()
        .find(|coverage| coverage.content.window_id.get() == window.0 as i64)
        .expect("worker rows connect across the wrapped overlay line");
    assert!(
        coverage.anchor_row > 0,
        "wrapped overlay rows remain connected above the viewport"
    );
}

#[test]
fn captured_rich_worker_positions_preserve_connected_coverage() {
    let (mut eval, frame, buffer, window) = shared_rich_scrolling_frame(664, 646);
    // Remove 48 repeated 1000-line blocks from the 100000-line native fixture.
    // This preserves the text, face, and 32-line overlay phases at these jobs.
    let shift = 48 * 110080;
    let jobs = [
        5514560, 5515120, 5513060, 5512500, 5515020, 5514820, 5512600, 5514160, 5513760, 5511700,
    ];
    let owner = DisplayWindowId::new(window.0 as i64);
    for origin in [5514560, 5513060, 5512500] {
        let start = origin - shift + 5;
        let point = eval
            .buffer_manager()
            .get(buffer)
            .unwrap()
            .char_pos_to_emacs_byte_pos_clamped(CharPos0::new(start))
            .get();
        scroll_window_to(&mut eval, frame, window, buffer, start as i64 + 1, point);
        if let neovm_core::window::Window::Leaf {
            force_start,
            vscroll,
            ..
        } = eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
        {
            *force_start = true;
            *vscroll = 0;
        }
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame);
        let end = engine.retained_window_matrices[&owner]
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
            .last()
            .unwrap()
            .start_charpos;
        let forward = eval
            .eval_str(&format!(
                "(save-excursion (goto-char {}) (line-beginning-position))",
                end + 1
            ))
            .unwrap()
            .as_fixnum()
            .unwrap() as usize
            - 1;
        let prepare = |engine: &mut LayoutEngine, eval: &Context, source| {
            engine
                .request_scroll_coverage(eval, frame, window, CharPos0::new(source))
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !engine
                .scroll_coverage
                .drain(&mut engine.prepared_viewports)
                .unwrap()
            {
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
        };
        prepare(&mut engine, &eval, forward);
        engine.layout_frame_rust(&mut eval, frame);
        let bounds = |engine: &LayoutEngine| {
            engine
                .last_frame_display_state
                .as_ref()
                .unwrap()
                .scroll_coverage
                .iter()
                .find(|coverage| coverage.content.window_id == owner)
                .map(|coverage| {
                    let rows = &coverage.content.matrix.rows;
                    (
                        rows.first().unwrap().start_charpos,
                        rows.last().unwrap().end_charpos,
                    )
                })
        };
        let original = bounds(&engine).expect("establish useful rich forward coverage");
        let headroom = |engine: &LayoutEngine| {
            let frame = engine
                .last_frame_display_state
                .as_ref()
                .unwrap()
                .materialize();
            let surface = frame
                .scroll_surfaces
                .first()
                .expect("materialized rich coverage");
            (
                surface.clamp_offset(-f32::MAX),
                surface.clamp_offset(f32::MAX),
            )
        };
        let original_headroom = headroom(&engine);
        let visible = selected_window_layout_trace(&eval, &engine, frame);
        for source in jobs {
            prepare(&mut engine, &eval, source - shift);
            engine.layout_frame_rust(&mut eval, frame);
            let current = bounds(&engine)
                .unwrap_or_else(|| panic!("worker {source} removed coverage at viewport {origin}"));
            assert!(
                current.0 <= original.0 && current.1 >= original.1,
                "worker {source} shrank coverage at viewport {origin}: {original:?} -> {current:?}"
            );
            assert_eq!(visible, selected_window_layout_trace(&eval, &engine, frame));
            let current_headroom = headroom(&engine);
            assert!(
                current_headroom.0 <= original_headroom.0 + 0.01
                    && current_headroom.1 >= original_headroom.1 - 0.01,
                "worker {source} reduced scroll headroom at {origin}: {original_headroom:?} -> {current_headroom:?}"
            );
        }
        let mut fresh = LayoutEngine::new();
        fresh.layout_frame_rust(&mut eval, frame);
        assert_eq!(visible, selected_window_layout_trace(&eval, &fresh, frame));
    }
}

#[test]
fn leaving_a_prepared_viewport_preserves_its_compositor_coverage() {
    let line = "prepared row for backward scrolling\n";
    let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(0))
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    scroll_window_to(
        &mut eval,
        frame,
        window,
        buffer,
        line.len() as i64 + 1,
        line.len(),
    );
    if let neovm_core::window::Window::Leaf { force_start, .. } = eval
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    {
        *force_start = true;
    }
    engine.layout_frame_rust(&mut eval, frame);
    // Export again after the previous viewport has entered the history cache.
    engine.layout_frame_rust(&mut eval, frame);
    let coverage = engine
        .last_frame_display_state
        .as_ref()
        .unwrap()
        .scroll_coverage
        .iter()
        .find(|coverage| coverage.content.window_id.get() == window.0 as i64)
        .expect("leaving a prepared viewport must retain its certified scroll surface");
    assert!(
        coverage.anchor_row > 0,
        "previously prepared rows remain above the viewport"
    );
}

#[test]
fn scroll_history_does_not_evict_prepared_compositor_coverage() {
    let line = "prepared row for backward scrolling\n";
    let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(0))
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    for row in 1..=10 {
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            (row * line.len()) as i64 + 1,
            row * line.len(),
        );
        if let neovm_core::window::Window::Leaf { force_start, .. } = eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
        {
            *force_start = true;
        }
        engine.layout_frame_rust(&mut eval, frame);
    }
    engine.layout_frame_rust(&mut eval, frame);
    let coverage = engine
        .last_frame_display_state
        .as_ref()
        .unwrap()
        .scroll_coverage
        .iter()
        .find(|coverage| coverage.content.window_id.get() == window.0 as i64)
        .expect("leaving a prepared viewport must retain its certified scroll surface");
    assert!(
        coverage.anchor_row > 0,
        "previously prepared rows remain above the viewport"
    );
}

#[test]
fn connected_prepared_pages_export_a_bounded_surface_around_the_viewport() {
    let line = "bounded coverage\n";
    for anchor in [0, 140, 300] {
        let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(500), 800, 1200);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            (anchor * line.len()) as i64 + 1,
            anchor * line.len(),
        );
        if let neovm_core::window::Window::Leaf { force_start, .. } = eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
        {
            *force_start = true;
        }
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame);
        for page in 0..8 {
            engine
                .request_scroll_coverage(
                    &eval,
                    frame,
                    window,
                    CharPos0::new(page * 40 * line.len()),
                )
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !engine
                .scroll_coverage
                .drain(&mut engine.prepared_viewports)
                .unwrap()
            {
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
        }
        engine.layout_frame_rust(&mut eval, frame);
        let state = engine.last_frame_display_state.as_ref().unwrap();
        let coverage = state
            .scroll_coverage
            .iter()
            .find(|coverage| coverage.content.window_id.get() == window.0 as i64)
            .expect("extra connected pages must not remove all compositor coverage");
        assert!(coverage.content.matrix.rows.len() <= 192);
        assert_eq!(
            coverage.content.matrix.rows[coverage.anchor_row].start_charpos,
            anchor * line.len()
        );
        let current = &engine.retained_window_matrices[&DisplayWindowId::new(window.0 as i64)];
        let last_visible = current
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
            .last()
            .unwrap();
        assert!(
            coverage.content.matrix.rows.last().unwrap().end_charpos >= last_visible.end_charpos
        );
        assert!(anchor == 0 || coverage.anchor_row > 0);
    }
}

#[test]
fn full_height_worker_preview_finishes_acquisition() {
    let line = format!("{}\n", "wrapped offscreen text ".repeat(12));
    let (mut eval, frame, _, window) = incr_editing_frame(&line.repeat(300), 240, 180);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let owner = neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "worker preview never filled the viewport"
        );
        engine.maintain_scroll_coverage(&eval);
        if engine.prepared_viewports.has_computed(frame, owner) {
            assert!(
                engine
                    .scroll_coverage
                    .active_capture_start_for_test()
                    .is_none(),
                "a full-height preview must release acquisition for the next coverage target"
            );
            break;
        }
        std::thread::yield_now();
    }
}

#[test]
fn backward_bridge_stops_acquisition_at_existing_coverage() {
    backward_bridge_connects("ordinary offscreen text\n", 800, 600);
}

#[test]
fn backward_bridge_preserves_its_connected_tail_when_taller_than_the_viewport() {
    let line = format!("{}\n", "wrapped offscreen text ".repeat(6));
    backward_bridge_connects(&line, 320, 200);
}

fn backward_bridge_connects(line: &str, width: u32, height: u32) {
    let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(400), width, height);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    scroll_window_to(
        &mut eval,
        frame,
        window,
        buffer,
        (120 * line.len() + 1) as i64,
        120 * line.len() + 5,
    );
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while engine.maintain_scroll_coverage(&eval).is_some() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    scroll_window_to(
        &mut eval,
        frame,
        window,
        buffer,
        (119 * line.len() + 1) as i64,
        119 * line.len() + 5,
    );
    engine.layout_frame_rust(&mut eval, frame);
    let frontier = engine
        .last_frame_display_state
        .as_ref()
        .unwrap()
        .scroll_coverage
        .iter()
        .find(|coverage| coverage.content.window_id.get() == window.0 as i64)
        .map(|coverage| coverage.content.matrix.rows[0].start_charpos)
        .unwrap_or_else(|| {
            engine.retained_window_matrices
                [&neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64)]
                .key
                .window_start as usize
        });
    engine.take_scroll_coverage_publication();
    engine.maintain_scroll_coverage(&eval);
    let start = engine
        .scroll_coverage
        .active_capture_start_for_test()
        .unwrap();
    assert_eq!(
        frontier / line.len() * line.len() - start,
        4 * line.len(),
        "prepare the missing bridge"
    );
    loop {
        assert!(std::time::Instant::now() < deadline);
        engine.maintain_scroll_coverage(&eval);
        if engine.take_scroll_coverage_publication() {
            assert!(
                engine
                    .scroll_coverage
                    .active_capture_start_for_test()
                    .is_none(),
                "a connected backward bridge must yield acquisition instead of recapturing covered rows"
            );
            break;
        }
        std::thread::yield_now();
    }
    engine.layout_frame_rust(&mut eval, frame);
    let extended = engine
        .last_frame_display_state
        .as_ref()
        .unwrap()
        .scroll_coverage
        .iter()
        .find(|coverage| coverage.content.window_id.get() == window.0 as i64)
        .map(|coverage| coverage.content.matrix.rows[0].start_charpos)
        .unwrap_or(frontier);
    assert!(
        extended < frontier,
        "the backward bridge must connect to existing coverage, not publish a disconnected prefix: {extended} >= {frontier}"
    );
    let actual = selected_window_layout_trace(&eval, &engine, frame);
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(&mut eval, frame);
    assert_eq!(actual, selected_window_layout_trace(&eval, &fresh, frame));
}

#[test]
fn distant_worker_bridges_do_not_evict_the_connected_viewport_seam() {
    let line = "nearby prepared row\n";
    for (backward, full_bridge) in [(true, false), (false, false), (true, true), (false, true)] {
        let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(400), 800, 600);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        let anchor = 120 * line.len();
        scroll_window_to(&mut eval, frame, window, buffer, anchor as i64 + 1, anchor);
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame);
        let owner = DisplayWindowId::new(window.0 as i64);
        let visible_end = engine.retained_window_matrices[&owner]
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
            .last()
            .unwrap()
            .next_buffer_row_start()
            .unwrap();
        // The nearest bridge may be a full page or a four-line prefix.
        // Farther worker results contribute four adjacent physical rows.
        // Keep the live viewport
        // fixed while enough pages arrive to exhaust the bounded cache.
        for page in 0..12 {
            let start = if backward {
                anchor - (page + 1) * 4 * line.len()
            } else {
                visible_end + page * 4 * line.len()
            };
            if full_bridge && page == 0 {
                engine
                    .request_scroll_coverage(&eval, frame, window, CharPos0::new(start))
                    .unwrap();
            } else {
                engine
                    .request_scroll_bridge(
                        &eval,
                        frame,
                        window,
                        CharPos0::new(start),
                        CharPos0::new(start + 4 * line.len()),
                    )
                    .unwrap();
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !engine
                .scroll_coverage
                .drain(&mut engine.prepared_viewports)
                .unwrap()
            {
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
            engine.layout_frame_rust(&mut eval, frame);
            let coverage = engine
                .last_frame_display_state
                .as_ref()
                .unwrap()
                .scroll_coverage
                .iter()
                .find(|coverage| coverage.content.window_id == owner)
                .expect("retain usable offscreen coverage");
            if backward {
                assert!(
                    coverage.anchor_row >= 4,
                    "farther page {page} evicted the nearby backward seam"
                );
            } else {
                assert!(
                    coverage.content.matrix.rows.last().unwrap().end_charpos
                        >= visible_end + 3 * line.len(),
                    "farther page {page} evicted the nearby forward seam"
                );
            }
        }
        // A full page is also a query/replay result for an upcoming command.
        // Keep its insertion recency even when partial paint bridges are
        // closer to the currently visible source range.
        let target = 250 * line.len();
        engine
            .request_scroll_coverage(&eval, frame, window, CharPos0::new(target))
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !engine
            .scroll_coverage
            .drain(&mut engine.prepared_viewports)
            .unwrap()
        {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        scroll_window_to(
            &mut eval,
            frame,
            window,
            buffer,
            target as i64 + 1,
            target + 5 * line.len(),
        );
        if let neovm_core::window::Window::Leaf { force_start, .. } = eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
        {
            *force_start = true;
        }
        engine.layout_frame_rust(&mut eval, frame);
        assert_eq!(
            engine.last_layout_stats().prepared_windows,
            1,
            "new full page must remain reusable after admission under bridge pressure"
        );
        let actual = selected_window_layout_trace(&eval, &engine, frame);
        let mut fresh = LayoutEngine::new();
        fresh.layout_frame_rust(&mut eval, frame);
        assert_eq!(actual, selected_window_layout_trace(&eval, &fresh, frame));
    }
}

#[test]
fn overlapping_prepared_pages_do_not_discard_a_new_backward_bridge() {
    check_backward_bridge_admission_under_overlapping_page_pressure(false);
}

#[test]
fn rich_overlapping_prepared_pages_do_not_discard_a_new_backward_bridge() {
    check_backward_bridge_admission_under_overlapping_page_pressure(true);
}

fn check_backward_bridge_admission_under_overlapping_page_pressure(rich: bool) {
    let line = "nearby prepared row\n";
    let (mut eval, frame, buffer, window) = if rich {
        shared_rich_scrolling_frame(1000, 900)
    } else {
        let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(400), 800, 600);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        (eval, frame, buffer, window)
    };
    assert!(eval.frame_manager_mut().select_frame(frame));
    let base = if rich { 2 * 110080 } else { 116 * line.len() };
    let starts: Vec<_> = (0..=8)
        .map(|offset| {
            let value = eval
                .eval_str(&format!(
                    "(save-excursion (goto-char {}) (forward-line {offset}) (point))",
                    base + 1
                ))
                .unwrap();
            CharPos0::new(value.as_fixnum().unwrap() as usize - 1)
        })
        .collect();
    let visible_start = starts[5];
    let point = eval
        .buffer_manager()
        .get(buffer)
        .unwrap()
        .char_pos_to_emacs_byte_pos_clamped(visible_start)
        .get();
    scroll_window_to(
        &mut eval,
        frame,
        window,
        buffer,
        visible_start.get() as i64 + 1,
        point,
    );
    if let neovm_core::window::Window::Leaf {
        point,
        force_start,
        vscroll,
        ..
    } = eval
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    {
        *point = LispCharPos1::from_one_based_usize(visible_start.get() + 1);
        *force_start = true;
        *vscroll = -4;
    }
    let mut engine = LayoutEngine::new();
    engine.set_font_sizing(crate::font::sizing::FontSizing::wayland());
    engine.layout_frame_rust(&mut eval, frame);
    // Every cached page overlaps the visible source range, so its eviction
    // distance is zero. The new bridge lies farther away but is the only way
    // to extend the already published backward seam.
    for &start in &starts[1..] {
        engine
            .request_scroll_coverage(&eval, frame, window, start)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !engine
            .scroll_coverage
            .drain(&mut engine.prepared_viewports)
            .unwrap()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "worker page never ready"
            );
            std::thread::yield_now();
        }
    }
    engine.layout_frame_rust(&mut eval, frame);
    let owner = DisplayWindowId::new(window.0 as i64);
    let before = selected_window_layout_trace(&eval, &engine, frame);
    assert_eq!(
        engine.prepared_viewports.backward_start(
            frame,
            owner,
            &engine.retained_window_matrices[&owner].key
        ),
        Some(starts[1])
    );
    engine
        .request_scroll_bridge(&eval, frame, window, starts[0], starts[1])
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "worker bridge never ready"
        );
        std::thread::yield_now();
    }
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(
        engine.prepared_viewports.backward_start(
            frame,
            owner,
            &engine.retained_window_matrices[&owner].key
        ),
        Some(starts[0]),
        "new bridge was discarded before publication under overlapping-page pressure"
    );
    assert!(
        selected_window_layout_trace(&eval, &engine, frame) == before,
        "bridge changed accepted visible geometry"
    );
}

#[test]
fn discarded_distant_bridge_does_not_request_a_coverage_publication() {
    check_discarded_bridge_publication(false);
}

#[test]
fn discarded_bridge_preserves_an_earlier_unconsumed_publication() {
    check_discarded_bridge_publication(true);
}

fn check_discarded_bridge_publication(pending: bool) {
    let line = "nearby prepared row\n";
    let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(400), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let anchor = 120 * line.len();
    scroll_window_to(&mut eval, frame, window, buffer, anchor as i64 + 1, anchor);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let owner = DisplayWindowId::new(window.0 as i64);
    // Unique four-line pages exhaust the bounded cache. A farther page
    // cannot replace the already connected seam without breaking coverage.
    for page in 0..9 {
        let start = anchor - (page + 1) * 4 * line.len();
        if !pending {
            engine.take_scroll_coverage_publication();
        }
        let visible_before = selected_window_layout_trace(&eval, &engine, frame);
        let edge_before = engine.prepared_viewports.backward_start(
            frame,
            owner,
            &engine.retained_window_matrices[&owner].key,
        );
        engine
            .request_scroll_bridge(
                &eval,
                frame,
                window,
                CharPos0::new(start),
                CharPos0::new(start + 4 * line.len()),
            )
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !engine
            .scroll_coverage
            .drain(&mut engine.prepared_viewports)
            .unwrap()
        {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        if page == 8 {
            assert_eq!(
                engine.take_scroll_coverage_publication(),
                pending,
                "discarded work must preserve the previous publication decision"
            );
        }
        engine.layout_frame_rust(&mut eval, frame);
        if page == 8 {
            assert!(
                selected_window_layout_trace(&eval, &engine, frame) == visible_before,
                "discarded work changed accepted geometry"
            );
            assert_eq!(
                engine.prepared_viewports.backward_start(
                    frame,
                    owner,
                    &engine.retained_window_matrices[&owner].key
                ),
                edge_before,
                "discarded work changed the exported edge"
            );
        }
    }
    assert!(
        engine
            .prepared_viewports
            .backward_start(frame, owner, &engine.retained_window_matrices[&owner].key)
            .unwrap()
            .get()
            < anchor
    );
}
