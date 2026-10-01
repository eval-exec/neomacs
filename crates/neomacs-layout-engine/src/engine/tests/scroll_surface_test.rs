use super::*;

fn await_coverage(engine: &mut LayoutEngine) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !engine
        .scroll_coverage
        .drain(&mut engine.prepared_viewports)
        .unwrap()
    {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn overlapping_worker_wraps_cannot_disconnect_an_exported_surface() {
    check_overlapping_worker_wraps(false);
}

#[test]
fn overlapping_mixed_face_worker_wraps_preserve_exported_coverage() {
    check_overlapping_worker_wraps(true);
}

fn check_overlapping_worker_wraps(rich: bool) {
    let line = format!("{}\n", "ordinary wrapped text ".repeat(10));
    let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(100), 320, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    if rich {
        eval.eval_str(
            "(progn
            (put-text-property 20 100 'face '(:family \"DejaVu Serif\" :height 180 :weight bold))
            (put-text-property 100 180 'face '(:family \"DejaVu Sans\" :height 85 :slant italic))
            (let ((outer (make-overlay 10 190)) (inner (make-overlay 35 75)))
              (overlay-put outer 'face '(:background \"#243040\"))
              (overlay-put inner 'priority 30)
              (overlay-put inner 'face '(:underline t))))",
        )
        .unwrap();
    }
    scroll_window_to(&mut eval, frame, window, buffer, 6, 5);
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
    let owner = DisplayWindowId::new(window.0 as i64);
    let end = engine.retained_window_matrices[&owner]
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .last()
        .unwrap()
        .start_charpos;
    let forward = end / line.len() * line.len();
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(forward))
        .unwrap();
    await_coverage(&mut engine);
    engine.layout_frame_rust(&mut eval, frame);
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
    let before = coverage_end(&engine).expect("forward rows must extend the visible body");
    let visible = selected_window_layout_trace(&eval, &engine, frame);
    // This worker starts at the physical line's origin. The live viewport
    // starts inside it, so their first paragraph has different wrap boundaries.
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(0))
        .unwrap();
    await_coverage(&mut engine);
    engine.layout_frame_rust(&mut eval, frame);
    assert_eq!(
        coverage_end(&engine),
        Some(before),
        "a conflicting worker paragraph must not interleave the accepted row chain"
    );
    assert_eq!(visible, selected_window_layout_trace(&eval, &engine, frame));
    assert_eq!(
        engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .materialize()
            .scroll_surfaces
            .len(),
        1
    );
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(&mut eval, frame);
    assert_eq!(visible, selected_window_layout_trace(&eval, &fresh, frame));
}

#[test]
fn exported_scroll_surface_moves_paint_and_source_hits_without_changing_viewport() {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame_id, buffer, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let window_id = DisplayWindowId::new(window.0 as i64);
    let old = engine.retained_window_matrices[&window_id].clone();
    let rows: Vec<_> = old
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .collect();
    let target = rows[rows.len() - 2].start_charpos;
    let row_height = rows[0].height_px;
    engine
        .request_scroll_coverage(&eval, frame_id, window, CharPos0::new(target))
        .unwrap();
    await_coverage(&mut engine);
    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine.last_frame_display_state.as_ref().unwrap();
    assert_eq!(
        state.scroll_coverage.len(),
        1,
        "a joined worker page must be exported"
    );
    let mut frame = state.materialize();
    assert_eq!(
        frame.scroll_surfaces.len(),
        1,
        "valid coverage must materialize"
    );
    let surface = frame.scroll_surfaces[0].clone();
    let viewport = surface.coverage().viewport;
    let before_hit = state.presented_hit_index.clone();
    assert_eq!(
        engine.retained_window_matrices[&window_id].key.window_start,
        old.key.window_start
    );
    assert_eq!(
        engine.retained_window_matrices[&window_id]
            .display_snapshot
            .rows,
        old.display_snapshot.rows
    );
    let offset = surface.clamp_offset(row_height);
    assert_eq!(offset, row_height);
    surface.paint(&mut frame, offset).unwrap();
    let point = settled_point(frame.presentation_id, viewport.x + 2.0, viewport.y + 2.0);
    let hit = surface.hit(point, offset).unwrap().unwrap();
    assert_eq!(
        hit.text_position().unwrap().buffer_position(),
        line.len() as i64 + 1
    );
    assert_eq!(hit.text_position().unwrap().row(), 0);
    assert_eq!(hit.text_position().unwrap().bounds().y(), viewport.y);
    assert_eq!(hit.region().bounds().raw(), viewport);
    assert_eq!(
        before_hit
            .resolve(neomacs_display_protocol::PresentedHitQuery::new(point))
            .unwrap()
            .unwrap()
            .text_position()
            .unwrap()
            .buffer_position(),
        1
    );
    assert!(
        surface
            .hit(
                settled_point(
                    frame.presentation_id,
                    viewport.x + 2.0,
                    viewport.bottom() + 1.0
                ),
                offset
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(surface.clamp_offset(-100_000.0), 0.0);
    assert!(surface.clamp_offset(100_000.0) > row_height);
    assert_eq!(surface.clamp_offset(f32::NAN), 0.0);

    scroll_window_to(
        &mut eval,
        frame_id,
        window,
        buffer,
        line.len() as i64 + 1,
        line.len(),
    );
    let mut canonical = LayoutEngine::new();
    canonical.layout_frame_rust(&mut eval, frame_id);
    let expected = canonical
        .last_frame_display_state
        .as_ref()
        .unwrap()
        .materialize();
    let chars = |frame: &neomacs_display_protocol::FrameGlyphBuffer| {
        frame
            .glyphs
            .iter()
            .filter_map(|glyph| match glyph {
                neomacs_display_protocol::FrameGlyph::Char {
                    window_id: owner,
                    row_role: GlyphRowRole::Text,
                    char: ch,
                    x,
                    y,
                    baseline,
                    width,
                    height,
                    ..
                } if *owner == window_id && *y < viewport.bottom() && *y + *height > viewport.y => {
                    Some((*ch, *x, *y, *baseline, *width, *height))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(chars(&frame), chars(&expected));
}

#[test]
fn exported_scroll_surface_rejects_non_contiguous_or_foreign_hit_geometry() {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame_id, _, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let window_id = DisplayWindowId::new(window.0 as i64);
    let rows: Vec<_> = engine.retained_window_matrices[&window_id]
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .collect();
    let start = rows[rows.len() - 2].start_charpos;
    engine
        .request_scroll_coverage(&eval, frame_id, window, CharPos0::new(start))
        .unwrap();
    await_coverage(&mut engine);
    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine.last_frame_display_state.as_ref().unwrap();
    let original = state.scroll_coverage[0].clone();
    let mut broken = (*original).clone();
    neomacs_display_protocol::glyph_matrix::MatrixRow::make_mut(
        &mut broken.content.matrix.rows[1],
    )
    .pixel_y += 1.0;
    assert!(std::sync::Arc::new(broken).materialize(state).is_none());
    let mut broken = (*original).clone();
    broken.content.window_id = DisplayWindowId::new(9_999);
    assert!(std::sync::Arc::new(broken).materialize(state).is_none());
    eval.eval_str("(goto-char 1) (insert \"changed\")").unwrap();
    engine.layout_frame_rust(&mut eval, frame_id);
    assert!(
        engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .scroll_coverage
            .is_empty()
    );
}

#[test]
fn exported_backward_coverage_keeps_nonnegative_storage_and_visible_row_hit_coordinates() {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame_id, buffer, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str(&format!(
        "(put-text-property {} {} 'face '(:height 180 :family \"monospace\" :weight bold))",
        50 * line.len() + 1,
        53 * line.len()
    ))
    .unwrap();
    scroll_window_to(
        &mut eval,
        frame_id,
        window,
        buffer,
        40 * line.len() as i64 + 1,
        40 * line.len(),
    );
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    engine
        .request_scroll_coverage(&eval, frame_id, window, CharPos0::new(30 * line.len()))
        .unwrap();
    await_coverage(&mut engine);
    let window_id = DisplayWindowId::new(window.0 as i64);
    let rows: Vec<_> = engine.retained_window_matrices[&window_id]
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .collect();
    let target = rows[rows.len() - 2].start_charpos;
    engine
        .request_scroll_coverage(&eval, frame_id, window, CharPos0::new(target))
        .unwrap();
    await_coverage(&mut engine);
    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine.last_frame_display_state.as_ref().unwrap();
    assert_eq!(state.scroll_coverage.len(), 1);
    let frame = state.materialize();
    let surface = &frame.scroll_surfaces[0];
    assert!(surface.coverage().origin > 0.0);
    assert!(surface.clamp_offset(-1000.0) < -4.0);
    let viewport = surface.coverage().viewport;
    let point = settled_point(frame.presentation_id, viewport.x + 2.0, viewport.y + 1.0);
    let hit = surface.hit(point, -4.0).unwrap().unwrap();
    assert_eq!(
        hit.text_position().unwrap().buffer_position(),
        39 * line.len() as i64 + 1
    );
    assert_eq!(hit.text_position().unwrap().row(), 0);
    assert_eq!(hit.text_position().unwrap().bounds().y(), viewport.y);
    assert_eq!(hit.region().bounds().raw(), viewport);
    assert!(
        surface
            .coverage()
            .content
            .matrix
            .rows
            .iter()
            .any(|row| row.height_px > frame.char_height)
    );
}

#[test]
fn resolved_scroll_preview_requires_current_source_and_maps_source_start_to_coverage() {
    resolved_scroll_preview_for_window(false);
}

#[test]
fn resolved_scroll_preview_supports_an_unselected_window_without_raw_prediction() {
    resolved_scroll_preview_for_window(true);
}

fn resolved_scroll_preview_for_window(other_window: bool) {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame, buffer, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    let selected = window;
    let window = if other_window {
        eval.frame_manager_mut()
            .split_window(
                frame,
                selected,
                neovm_core::window::SplitDirection::Horizontal,
                buffer,
                None,
                neovm_core::window::SplitPlacement::AfterTarget,
            )
            .unwrap()
    } else {
        window
    };
    eval.eval_str("(setq neomacs-compositor-scrolling t)")
        .unwrap();
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame);
    let owner = DisplayWindowId::new(window.0 as i64);
    let rows: Vec<_> = engine.retained_window_matrices[&owner]
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .collect();
    let height = rows[0].height_px;
    let target = rows[rows.len() - 2].start_charpos;
    engine
        .request_scroll_coverage(&eval, frame, window, CharPos0::new(target))
        .unwrap();
    await_coverage(&mut engine);
    let FrameLayoutAttempt::Prepared(published) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("frame publication failed")
    };
    let old_presentation = published.presentation_id;
    assert!(
        engine.last_frame_display_state.is_none(),
        "production transfers the complete frame to transport"
    );
    scroll_window_to(
        &mut eval,
        frame,
        window,
        buffer,
        line.len() as i64 + 1,
        line.len(),
    );
    let stream = neomacs_display_protocol::input_progress::InputStream::default();
    let delivery = stream.issue().unwrap();
    // Echo/chrome repaint requests do not change the certified body rows.
    eval.eval_str("(force-mode-line-update t)").unwrap();
    let intent = engine
        .resolved_scroll_preview(&eval, frame, window, vec![delivery.receipt()])
        .expect("unchanged source with a committed destination should preview");
    assert_eq!(intent.window, owner);
    assert_eq!(
        eval.frame_manager().get(frame).unwrap().selected_window,
        selected
    );
    if other_window {
        assert!(!eval.permits_compositor_pixel_scroll(window));
    }
    assert_eq!(intent.offset, height);
    assert_eq!(intent.presentation, old_presentation);
    assert!(engine.last_frame_display_state.is_none());
    eval.eval_str("(setq neomacs-compositor-scrolling nil)")
        .unwrap();
    assert!(
        engine
            .resolved_scroll_preview(&eval, frame, window, vec![delivery.receipt()])
            .is_none()
    );
    eval.eval_str("(setq neomacs-compositor-scrolling t)")
        .unwrap();
    assert!(
        engine
            .resolved_scroll_preview(&eval, frame, window, vec![delivery.receipt()])
            .is_some()
    );
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("changed source");
    assert!(
        engine
            .resolved_scroll_preview(&eval, frame, window, vec![delivery.receipt()])
            .is_none()
    );
}

#[test]
fn wrapped_scroll_surface_exports_contiguous_visual_rows() {
    let line = format!("{}\n", "wide words ".repeat(18));
    let (mut eval, frame_id, _, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str("(setq word-wrap t)").unwrap();
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let owner = DisplayWindowId::new(window.0 as i64);
    let original = engine.retained_window_matrices[&owner].clone();
    let rows: Vec<_> = original
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .collect();
    assert!(
        rows[0].continued,
        "visual wrapping must be recorded on the transported row"
    );
    let target = rows
        .iter()
        .rev()
        .find(|row| row.start_charpos % line.len() == 0)
        .unwrap()
        .start_charpos;
    engine
        .request_scroll_coverage(&eval, frame_id, window, CharPos0::new(target))
        .unwrap();
    await_coverage(&mut engine);
    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine.last_frame_display_state.as_ref().unwrap();
    assert_eq!(
        state.scroll_coverage.len(),
        1,
        "wrapped seams must join the worker page"
    );
    let mut frame = state.materialize();
    assert_eq!(frame.scroll_surfaces.len(), 1);
    let surface = frame.scroll_surfaces[0].clone();
    let viewport = surface.coverage().viewport;
    let offset = surface.clamp_offset(rows[0].height_px);
    assert_eq!(offset, rows[0].height_px);
    let fringe_count = surface
        .coverage_glyphs()
        .iter()
        .filter(|glyph| matches!(glyph, FrameGlyph::FringeBitmap { .. }))
        .count();
    assert!(
        fringe_count > 0,
        "wrapped coverage owns its continuation arrows"
    );
    let bars: Vec<_> = frame
        .glyphs
        .iter()
        .filter(|glyph| matches!(glyph, FrameGlyph::ScrollBar { .. }))
        .cloned()
        .collect();
    frame.fringe_bitmaps.clear();
    surface.paint(&mut frame, offset).unwrap();
    let mut painted = 0;
    for glyph in &frame.glyphs {
        if let FrameGlyph::FringeBitmap {
            window_id,
            y,
            height,
            clip_rect,
            bitmap_index,
            ..
        } = glyph
        {
            if *window_id != owner {
                continue;
            }
            painted += 1;
            assert!(*y < viewport.bottom() && *y + *height > viewport.y);
            let clip = clip_rect.unwrap();
            assert_eq!(clip.y, viewport.y);
            assert_eq!(clip.height, viewport.height);
            assert!(clip.x <= viewport.x && clip.right() >= viewport.right());
            assert!(frame.fringe_bitmaps.contains_key(bitmap_index));
        }
    }
    assert!(painted > 0 && painted < fringe_count);
    assert_eq!(
        frame
            .glyphs
            .iter()
            .filter(|glyph| matches!(glyph, FrameGlyph::ScrollBar { .. }))
            .cloned()
            .collect::<Vec<_>>(),
        bars
    );
    let point = settled_point(frame.presentation_id, viewport.x + 1.0, viewport.y + 1.0);
    let hit = surface.hit(point, offset).unwrap().unwrap();
    assert_eq!(
        hit.text_position().unwrap().buffer_position(),
        rows[1].start_charpos as i64 + 1
    );
    assert_eq!(
        engine.retained_window_matrices[&owner].key.window_start,
        original.key.window_start
    );
}

#[test]
fn scroll_surface_translates_rich_hover_to_projected_glyphs() {
    assert_scroll_surface_hover_projection(false);
}

#[test]
fn scroll_surface_translates_owned_insertion_hover_to_projected_glyphs() {
    assert_scroll_surface_hover_projection(true);
}

fn assert_scroll_surface_hover_projection(insertions: bool) {
    let line = "ordinary offscreen text\n";
    let (mut eval, frame_id, _, window) = incr_editing_frame(&line.repeat(300), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str("(progn (put-text-property (point-min) (point-max) 'mouse-face 'highlight) (put-text-property 3 8 'face '(:height 2.0)))")
        .unwrap();
    if insertions {
        eval.eval_str(
            "(let ((p (point-min))) (while (< p (point-max))
            (let ((ov (make-overlay (+ p 5) (+ p 5))))
                (overlay-put ov 'before-string (propertize \"AA\" 'mouse-face 'highlight)))
            (setq p (+ p 23))))",
        )
        .unwrap();
    }
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let owner = DisplayWindowId::new(window.0 as i64);
    let original = engine.retained_window_matrices[&owner].clone();
    let rows: Vec<_> = original
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .collect();
    engine
        .request_scroll_coverage(
            &eval,
            frame_id,
            window,
            CharPos0::new(rows[rows.len() - 2].start_charpos),
        )
        .unwrap();
    await_coverage(&mut engine);
    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine.last_frame_display_state.as_ref().unwrap();
    let mut frame = state.materialize();
    assert_eq!(
        frame.scroll_surfaces.len(),
        1,
        "hover faces need owned scroll transport"
    );
    let surface = frame.scroll_surfaces[0].clone();
    for offset in [0.5, rows[0].height_px - 0.5, rows[0].height_px + 0.5] {
        let mut projected = frame.clone();
        surface.paint(&mut projected, offset).unwrap();
        assert!(!projected.presented_pointer().is_empty());
    }
    let offset = rows[0].height_px + 0.5;
    surface.paint(&mut frame, offset).unwrap();
    assert!(!frame.presented_pointer().is_empty());
    if insertions {
        assert!(surface.coverage().content.matrix.rows.iter().any(|row| {
            row.pointer_appearances().iter().any(|appearance| {
                appearance.source.kind
                    == neomacs_display_protocol::glyph_matrix::GlyphPointerSourceKind::LispString
            })
        }));
    }
    let viewport = surface.coverage().viewport;
    for appearance in frame.presented_pointer().appearances() {
        for span in appearance.paint_spans() {
            assert!(span.clip().y() >= viewport.y);
            assert!(span.clip().bottom() <= viewport.bottom());
            for glyph in &frame.glyphs[span.first() as usize..(span.first() + span.len()) as usize]
            {
                assert_eq!(glyph.window_id(), Some(owner));
                assert_eq!(glyph.row_role(), Some(GlyphRowRole::Text));
            }
        }
    }
    // A transported primitive-index map has no retained slot ownership.
    // Reject scrolling before changing pixels rather than reusing stale indices.
    frame
        .install_presented_pointer_map(frame.presented_pointer().clone())
        .unwrap();
    let unchanged = frame.clone();
    assert_eq!(
        surface.paint(&mut frame, offset),
        Err(neomacs_display_protocol::PresentedPointerMapError::MissingSourceMap)
    );
    assert_eq!(frame, unchanged);
}

#[test]
fn certified_worker_rows_answer_motion_without_a_new_walk() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    for decoration in [
        "nil",
        "(put-text-property 1 (point-max) 'face '(:height 1.5 :weight bold))",
        "(let ((o (make-overlay 921 930))) (overlay-put o 'before-string \"prefix \") (overlay-put o 'face '(:height 2.0)))",
    ] {
        let line = "ordinary offscreen text\n";
        let (mut eval, frame, _buffer, window) = incr_editing_frame(&line.repeat(300), 800, 600);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        eval.eval_str(decoration).unwrap();
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame);
        let target = line.len() * 40;
        engine
            .request_scroll_coverage(&eval, frame, window, CharPos0::new(target))
            .unwrap();
        await_coverage(&mut engine);
        let scope = WindowLayoutQueryScope::Rows {
            start: LispCharPos1::from_one_based_usize(target + 1),
            count: std::num::NonZeroUsize::new(8).unwrap(),
        };
        probe::reset();
        let actual = engine
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(
            probe::max_depth(),
            0,
            "certified rows should avoid synchronous interpretation"
        );
        let mut fresh = WindowLayoutQueryEngine::new();
        let expected = fresh
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(actual.end(), expected.end());
        assert_eq!(
            actual.geometry().unwrap().rows,
            expected.geometry().unwrap().rows
        );
        assert_eq!(
            actual.geometry().unwrap().points,
            expected.geometry().unwrap().points
        );
        eval.eval_str(&format!(
            "(put-text-property {} {} 'display \"replacement\")",
            target + 1,
            target + 5
        ))
        .unwrap();
        probe::reset();
        let changed = engine
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert!(
            probe::max_depth() > 0,
            "changed properties must reject the worker certificate"
        );
        let expected = fresh
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(changed.geometry(), expected.geometry());
    }
}

#[test]
fn certified_worker_pixels_answer_motion_without_a_new_walk() {
    use crate::engine::viewport_retry_depth_probe as probe;
    use neovm_core::{buffer::LispCharPos1, window::WindowLayoutQueryScope};
    for decoration in [
        "nil",
        "(put-text-property 1 (point-max) 'face '(:height 1.5 :weight bold))",
        "(let ((o (make-overlay 921 930))) (overlay-put o 'before-string \"prefix \") (overlay-put o 'face '(:height 2.0)))",
    ] {
        let line = "ordinary offscreen text\n";
        let (mut eval, frame, _buffer, window) = incr_editing_frame(&line.repeat(300), 800, 600);
        eval.frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .window_system = Some(Value::symbol("neomacs"));
        eval.eval_str(decoration).unwrap();
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame);
        let target = line.len() * 40;
        engine
            .request_scroll_coverage(&eval, frame, window, CharPos0::new(target))
            .unwrap();
        await_coverage(&mut engine);
        let scope = WindowLayoutQueryScope::Pixels {
            start: LispCharPos1::from_one_based_usize(target + 1),
            height: std::num::NonZeroUsize::new(120).unwrap(),
        };
        probe::reset();
        let actual = engine
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(
            probe::max_depth(),
            0,
            "certified rows should avoid synchronous interpretation"
        );
        let mut fresh = WindowLayoutQueryEngine::new();
        let expected = fresh
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(actual.end(), expected.end());
        assert_eq!(
            actual.geometry().unwrap().rows,
            expected.geometry().unwrap().rows
        );
        assert_eq!(
            actual.geometry().unwrap().points,
            expected.geometry().unwrap().points
        );
        let uncovered = WindowLayoutQueryScope::Pixels {
            start: LispCharPos1::from_one_based_usize(target + 1),
            height: std::num::NonZeroUsize::new(1200).unwrap(),
        };
        probe::reset();
        let extended = engine
            .query_window_layout(&mut eval, frame, window, uncovered)
            .unwrap();
        assert!(probe::max_depth() > 0, "short worker prefix must fall back");
        let expected = fresh
            .query_window_layout(&mut eval, frame, window, uncovered)
            .unwrap();
        assert_eq!(extended.end(), expected.end());
        assert_eq!(extended.geometry(), expected.geometry());
        eval.eval_str(&format!(
            "(put-text-property {} {} 'display \"replacement\")",
            target + 1,
            target + 5
        ))
        .unwrap();
        probe::reset();
        let changed = engine
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert!(
            probe::max_depth() > 0,
            "changed properties must reject the worker certificate"
        );
        let expected = fresh
            .query_window_layout(&mut eval, frame, window, scope)
            .unwrap();
        assert_eq!(changed.geometry(), expected.geometry());
    }
}
