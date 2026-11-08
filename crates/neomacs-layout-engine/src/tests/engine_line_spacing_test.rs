//! Line spacing is row geometry, independent of point and region decoration.
use super::*;

/// A small buffer newline changes only its own row, independently of point.
#[test]
fn small_newline_face_shrinks_only_its_own_row() {
    for extend in ["nil", "t"] {
        let (mut eval, frame, _buffer, window) =
            incr_editing_frame("Summary: \n\nbody\n", 800, 240);
        realize_test_gui_frame(&mut eval, frame);
        eval.eval_str(&format!(
            "(put-text-property 11 12 'face '(:height 0.1 :inverse-video t :extend {extend}))"
        ))
        .unwrap();
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
        let rows = |engine: &mut LayoutEngine, eval: &mut Context| {
            let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(eval, frame)
            else {
                panic!("separator frame should prepare");
            };
            output
                .window_matrices
                .iter()
                .find(|entry| entry.window_id.get() == window.0 as i64)
                .unwrap()
                .matrix
                .rows
                .iter()
                .take(3)
                .map(|row| (row.pixel_y, row.height_px))
                .collect::<Vec<_>>()
        };
        let before = rows(&mut engine, &mut eval);
        assert!(
            before[1].1 < before[0].1 / 2.0,
            "small newline must not inherit the default row height: {before:?}"
        );
        assert_eq!(before[2].1, before[0].1);
        assert_eq!(before[2].0 - before[1].0, before[1].1);
        eval.eval_str("(set-window-point (selected-window) 11)")
            .unwrap();
        assert_eq!(
            rows(&mut engine, &mut eval),
            before,
            "point must not enlarge the separator"
        );
    }
}

/// Normal-size visible content still determines mixed-font row extents.
#[test]
fn small_newline_keeps_normal_sized_content_in_the_same_row() {
    let (mut eval, frame, _buffer, window) = incr_editing_frame("Summary: \nx\nbody\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str("(put-text-property 12 13 'face '(:height 0.1 :extend t))")
        .unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("mixed-font frame should prepare");
    };
    let rows = &output
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .unwrap()
        .matrix
        .rows;
    assert_eq!(rows[1].height_px, rows[0].height_px);
    assert_eq!(rows[2].height_px, rows[0].height_px);
}

/// Font height properties cannot shrink a terminal's logical row cells.
#[test]
fn small_newline_face_does_not_shrink_terminal_rows() {
    let (mut eval, frame, _buffer, window) = incr_editing_frame("Summary: \n\nbody\n", 800, 240);
    eval.eval_str("(put-text-property 11 12 'face '(:height 0.1 :extend t))")
        .unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("terminal frame should prepare");
    };
    let rows = &output
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .unwrap()
        .matrix
        .rows;
    assert_eq!(rows[1].height_px, rows[0].height_px);
    assert_eq!(rows[2].height_px, rows[0].height_px);
}

/// Terminal display ignores graphical raise/lower even with content-only line height.
#[test]
fn shifted_text_does_not_resize_terminal_cells() {
    for raise in [-0.5, 0.5] {
        let (mut eval, frame, _buffer, window) = incr_editing_frame("AAA\n\nBBB\n", 800, 240);
        eval.eval_str(&format!("(progn (put-text-property 1 4 'display '(raise {raise})) (put-text-property 4 5 'line-height t))")).unwrap();
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(&mut eval, frame)
        else {
            panic!("terminal frame should prepare")
        };
        let rows = &output
            .window_matrices
            .iter()
            .find(|entry| entry.window_id.get() == window.0 as i64)
            .unwrap()
            .matrix
            .rows;
        assert_eq!(rows[0].height_px, rows[1].height_px, "raise {raise}");
    }
}

/// Overlay newlines use their own GUI font, but never shrink terminal cells.
#[test]
fn small_overlay_newline_uses_its_own_font_geometry() {
    for graphical in [false, true] {
        for property in ["before-string", "after-string"] {
            for extend in ["nil", "t"] {
                for prefix in ["", "x"] {
                    let (mut eval, frame, _buffer, window) =
                        incr_editing_frame("Summary: \nbody\n", 800, 240);
                    if graphical {
                        realize_test_gui_frame(&mut eval, frame);
                    }
                    eval.eval_str(&format!(
                        "(let ((overlay (make-overlay 11 11)))
                           (overlay-put overlay '{property}
                             (concat \"{prefix}\" (propertize \"\\n\" 'face
                               '(:height 0.1 :inverse-video t :extend {extend})))))"
                    ))
                    .unwrap();
                    let mut engine = crate::font::metrics::test_catalog::layout_engine();
                    if graphical {
                        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
                    }
                    let rows = |engine: &mut LayoutEngine, eval: &mut Context| {
                        let FrameLayoutAttempt::Prepared(output) =
                            engine.redisplay_frame_attempt(eval, frame)
                        else {
                            panic!("overlay frame should prepare");
                        };
                        output
                            .window_matrices
                            .iter()
                            .find(|entry| entry.window_id.get() == window.0 as i64)
                            .unwrap()
                            .matrix
                            .rows
                            .iter()
                            .take(3)
                            .map(|row| (row.pixel_y, row.height_px))
                            .collect::<Vec<_>>()
                    };
                    let before = rows(&mut engine, &mut eval);
                    if graphical && prefix.is_empty() {
                        assert!(
                            before[1].1 < before[0].1 / 2.0,
                            "small {property} newline must shrink its row: {before:?}"
                        );
                    } else {
                        assert_eq!(before[1].1, before[0].1);
                    }
                    assert_eq!(before[2].1, before[0].1);
                    assert_eq!(before[2].0 - before[1].0, before[1].1);
                    eval.eval_str("(set-window-point (selected-window) 11)")
                        .unwrap();
                    assert_eq!(
                        rows(&mut engine, &mut eval),
                        before,
                        "point must not change overlay row geometry"
                    );
                }
            }
        }
    }
}

/// Both smaller and larger empty newline fonts determine GUI row extents.
#[test]
fn tall_newline_grows_empty_buffer_and_string_rows() {
    for source in ["buffer", "overlay", "replacement"] {
        let (mut eval, frame, _, window) = incr_editing_frame("AAA\n\nBBB\n", 800, 240);
        realize_test_gui_frame(&mut eval, frame);
        let face = "'(:height 2.0 :inverse-video t :extend t)";
        let setup = match source {
            "buffer" => format!("(put-text-property 5 6 'face {face})"),
            "overlay" => format!(
                "(overlay-put (make-overlay 5 5) 'before-string (propertize \"\\n\" 'face {face}))"
            ),
            "replacement" => {
                format!("(put-text-property 5 6 'display (propertize \"\\n\" 'face {face}))")
            }
            _ => unreachable!(),
        };
        eval.eval_str(&setup).unwrap();
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
        let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(&mut eval, frame)
        else {
            panic!("tall newline frame should prepare");
        };
        let rows = &output
            .window_matrices
            .iter()
            .find(|entry| entry.window_id.get() == window.0 as i64)
            .unwrap()
            .matrix
            .rows;
        assert!(
            rows[1].height_px > rows[0].height_px * 1.5,
            "{source} newline should grow an empty row: normal={}, newline={}",
            rows[0].height_px,
            rows[1].height_px
        );
        assert_eq!(rows[2].pixel_y - rows[1].pixel_y, rows[1].height_px);
    }
}

/// Display-property newlines resolve their own font without shrinking mixed text or TTY cells.
#[test]
fn small_replacement_newline_uses_its_own_font_geometry() {
    for graphical in [false, true] {
        for extend in ["nil", "t"] {
            for prefix in ["", "x"] {
                let (mut eval, frame, _, window) = incr_editing_frame("AAA\n\nBBB\n", 800, 240);
                if graphical {
                    realize_test_gui_frame(&mut eval, frame);
                }
                eval.eval_str(&format!(
                    "(put-text-property 5 6 'display (concat \"{prefix}\" (propertize \"\\n\" 'face '(:height 0.1 :inverse-video t :extend {extend}))))"
                )).unwrap();
                let mut engine = crate::font::metrics::test_catalog::layout_engine();
                if graphical {
                    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
                }
                let FrameLayoutAttempt::Prepared(output) =
                    engine.redisplay_frame_attempt(&mut eval, frame)
                else {
                    panic!("replacement frame should prepare");
                };
                let rows = &output
                    .window_matrices
                    .iter()
                    .find(|entry| entry.window_id.get() == window.0 as i64)
                    .unwrap()
                    .matrix
                    .rows;
                if graphical && prefix.is_empty() {
                    assert!(
                        rows[1].height_px < rows[0].height_px / 2.0,
                        "replacement newline should shrink: normal={}, newline={}",
                        rows[0].height_px,
                        rows[1].height_px
                    );
                } else {
                    assert_eq!(rows[1].height_px, rows[0].height_px);
                }
                assert_eq!(rows[2].height_px, rows[0].height_px);
                assert_eq!(rows[2].pixel_y - rows[1].pixel_y, rows[1].height_px);
            }
        }
    }
}

/// Point-only replay must use the same full-row cursor bounds as fresh rendering.
#[test]
fn newline_cursor_replay_preserves_font_and_spacing_metrics() {
    for (height, box_spec, hide_text) in [
        ("0.1", "", false),
        ("1.0", "", false),
        ("2.0", "", false),
        ("0.1", ":box 1", false),
        ("0.1", "", true),
    ] {
        for spacing in [0, 5] {
            for extend in ["nil", "t"] {
                let (mut eval, frame, buffer, window) =
                    incr_editing_frame("alpha\n\nbeta\n", 800, 240);
                realize_test_gui_frame(&mut eval, frame);
                eval.eval_str(&format!(
                    "(put-text-property 7 8 'face '(:height {height} :extend {extend} {box_spec}))"
                ))
                .unwrap();
                if hide_text {
                    eval.eval_str("(put-text-property 8 10 'invisible t)")
                        .unwrap();
                }
                eval.buffer_manager_mut()
                    .get_mut(buffer)
                    .unwrap()
                    .set_buffer_local("line-spacing", Value::fixnum(spacing));
                let mut engine = crate::font::metrics::test_catalog::layout_engine();
                crate::font::metrics::test_catalog::enable_metrics(&mut engine);
                let FrameLayoutAttempt::Prepared(initial) =
                    engine.redisplay_frame_attempt(&mut eval, frame)
                else {
                    panic!("initial frame should prepare");
                };
                let initial_rows: Vec<_> = initial
                    .window_matrices
                    .iter()
                    .find(|entry| entry.window_id.get() == window.0 as i64)
                    .unwrap()
                    .matrix
                    .rows
                    .iter()
                    .map(|row| (row.pixel_y, row.height_px, row.ascent_px))
                    .collect();
                for point in [7, 1, 8, 11, 13, 7] {
                    eval.eval_str(&format!("(set-window-point (selected-window) {point})"))
                        .unwrap();
                    let FrameLayoutAttempt::Prepared(replayed) =
                        engine.redisplay_frame_attempt(&mut eval, frame)
                    else {
                        panic!("replay should prepare");
                    };
                    assert_eq!(engine.last_layout_stats().cursor_only_windows, 1);
                    assert!(engine.last_layout_stats().reused_rows > 0);
                    let rows: Vec<_> = replayed
                        .window_matrices
                        .iter()
                        .find(|entry| entry.window_id.get() == window.0 as i64)
                        .unwrap()
                        .matrix
                        .rows
                        .iter()
                        .map(|row| (row.pixel_y, row.height_px, row.ascent_px))
                        .collect();
                    assert_eq!(
                        rows, initial_rows,
                        "point motion must not change row geometry"
                    );
                    let mut fresh = crate::font::metrics::test_catalog::layout_engine();
                    crate::font::metrics::test_catalog::enable_metrics(&mut fresh);
                    let FrameLayoutAttempt::Prepared(rebuilt) =
                        fresh.redisplay_frame_attempt(&mut eval, frame)
                    else {
                        panic!("fresh should prepare");
                    };
                    let replayed = replayed.materialize();
                    let rebuilt = rebuilt.materialize();
                    let actual = replayed
                        .active_cursor()
                        .map(|cursor| (cursor.y, cursor.height, cursor.ascent));
                    let expected = rebuilt
                        .active_cursor()
                        .map(|cursor| (cursor.y, cursor.height, cursor.ascent));
                    if !hide_text || !(8..10).contains(&point) {
                        assert!(expected.is_some(), "visible point must have a cursor");
                    }
                    assert_eq!(
                        actual, expected,
                        "cursor replay: height={height}, spacing={spacing}, extend={extend}, box={box_spec}, hidden={hide_text}, point={point}"
                    );
                }
            }
        }
    }
}

/// Mixed-height rows must not change cursor geometry when only point moves.
#[test]
fn mixed_height_newline_cursor_replay_matches_fresh_capture() {
    for spacing in [0, 5] {
        for newline_height in ["0.1", "1.0"] {
            let (mut eval, frame, buffer, _) = incr_editing_frame("AAAx\nnext\n", 800, 240);
            realize_test_gui_frame(&mut eval, frame);
            eval.eval_str(&format!("(progn (put-text-property 1 4 'face '(:height 2.0)) (put-text-property 5 6 'face '(:height {newline_height})))")).unwrap();
            eval.buffer_manager_mut()
                .get_mut(buffer)
                .unwrap()
                .set_buffer_local("line-spacing", Value::fixnum(spacing));
            let mut engine = crate::font::metrics::test_catalog::layout_engine();
            crate::font::metrics::test_catalog::enable_metrics(&mut engine);
            assert!(matches!(
                engine.redisplay_frame_attempt(&mut eval, frame),
                FrameLayoutAttempt::Prepared(_)
            ));
            for point in [4, 5, 1, 5] {
                eval.eval_str(&format!("(set-window-point (selected-window) {point})"))
                    .unwrap();
                let FrameLayoutAttempt::Prepared(replay) =
                    engine.redisplay_frame_attempt(&mut eval, frame)
                else {
                    panic!("cursor replay should prepare")
                };
                assert_eq!(engine.last_layout_stats().cursor_only_windows, 1);
                assert!(engine.last_layout_stats().reused_rows > 0);
                let mut fresh = crate::font::metrics::test_catalog::layout_engine();
                crate::font::metrics::test_catalog::enable_metrics(&mut fresh);
                let FrameLayoutAttempt::Prepared(rebuilt) =
                    fresh.redisplay_frame_attempt(&mut eval, frame)
                else {
                    panic!("fresh frame should prepare")
                };
                let replay = replay.materialize();
                let rebuilt = rebuilt.materialize();
                let actual = replay.active_cursor().unwrap();
                let expected = rebuilt.active_cursor().unwrap();
                assert_eq!(
                    (actual.y, actual.height, actual.ascent),
                    (expected.y, expected.height, expected.ascent),
                    "spacing={spacing}, newline={newline_height}, point={point}"
                );
            }
        }
    }
}

/// Bidi replacement placement and vertical metrics must select the same glyph.
#[test]
fn rtl_replacement_cursor_uses_its_resolved_glyph_metrics() {
    let (mut eval, frame, _buffer, window) = incr_editing_frame("x\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str("(progn (put-text-property 1 2 'display (concat (propertize \"א\" 'face '(:height 2.0 :foreground \"white\" :background \"red\")) (propertize \"בג\" 'face '(:height 0.5 :foreground \"black\" :background \"blue\")))) (set-window-point (selected-window) 1))").unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let FrameLayoutAttempt::Prepared(prepared) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("replacement should prepare")
    };
    let row = &prepared
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .unwrap()
        .matrix
        .rows[0];
    let glyphs = &row.glyphs[GlyphArea::Text.index()];
    let replacements = || {
        glyphs.iter().filter(|glyph| {
            !glyph.padding
                && matches!(
                    glyph.provenance,
                    neomacs_display_protocol::glyph_matrix::GlyphProvenance::Str { .. }
                )
                && row.glyph_covers_buffer_charpos(glyph, 0)
        })
    };
    let first = replacements().next().expect("visual replacement glyph");
    let selected = replacements()
        .min_by_key(|glyph| match glyph.provenance {
            neomacs_display_protocol::glyph_matrix::GlyphProvenance::Str { index, .. } => index,
            _ => unreachable!(),
        })
        .expect("logical replacement glyph");
    assert_ne!(
        first.pixel_height, selected.pixel_height,
        "fixture must reorder differently sized glyphs"
    );
    let expected_height =
        (row.ascent_px + selected.pixel_height - selected.pixel_ascent).min(row.height_px);
    let expected_ascent = row.ascent_px;
    let expected_width = selected.pixel_width.max(1.0);
    let output = prepared.materialize();
    let cursor = output.active_cursor().unwrap().clone();
    assert_eq!(
        (cursor.height, cursor.ascent),
        (expected_height, expected_ascent)
    );
    assert_eq!(cursor.width, expected_width);
    assert_eq!(cursor.cursor_fg, Color::from_pixel(0xff0000));
    for point in [2, 1] {
        eval.eval_str(&format!("(set-window-point (selected-window) {point})"))
            .unwrap();
        let FrameLayoutAttempt::Prepared(replayed) =
            engine.redisplay_frame_attempt(&mut eval, frame)
        else {
            panic!("replacement replay should prepare")
        };
        assert_eq!(engine.last_layout_stats().cursor_only_windows, 1);
        assert!(engine.last_layout_stats().reused_rows > 0);
        if point == 1 {
            assert_eq!(replayed.materialize().active_cursor().unwrap(), &cursor);
        }
    }
}

/// Replacement placement must identify the selected glyph independently of
/// fresh/replay equality, and logical cursor anchors must use its finalized x.
#[test]
fn replacement_logical_cursor_tracks_the_selected_visual_slot() {
    for (replacement, visible) in ["אבג", "abc"]
        .into_iter()
        .flat_map(|replacement| [true, false].map(|visible| (replacement, visible)))
    {
        for line_numbers in [false, true] {
            let (mut eval, frame, _, window) = incr_editing_frame("ax\n", 800, 240);
            realize_test_gui_frame(&mut eval, frame);
            eval.buffer_manager_mut()
                .current_buffer_mut()
                .unwrap()
                .set_buffer_local(
                    "display-line-numbers",
                    if line_numbers { Value::T } else { Value::NIL },
                );
            eval.buffer_manager_mut()
                .current_buffer_mut()
                .unwrap()
                .set_buffer_local("cursor-type", if visible { Value::T } else { Value::NIL });
            eval.eval_str(&format!("(progn (put-text-property 2 3 'display \"{replacement}\") (set-window-point (selected-window) 2))")).unwrap();
            let mut engine = crate::font::metrics::test_catalog::layout_engine();
            crate::font::metrics::test_catalog::enable_metrics(&mut engine);
            let FrameLayoutAttempt::Prepared(prepared) =
                engine.redisplay_frame_attempt(&mut eval, frame)
            else {
                panic!("replacement frame")
            };
            let row = &prepared
                .window_matrices
                .iter()
                .find(|entry| entry.window_id.get() == window.0 as i64)
                .unwrap()
                .matrix
                .rows[0];
            let glyphs = &row.glyphs[GlyphArea::Text.index()];
            let selected = glyphs
                .iter()
                .enumerate()
                .filter(|(_, glyph)| !glyph.padding && row.glyph_covers_buffer_charpos(glyph, 1))
                .filter_map(|(i, glyph)| match glyph.provenance {
                    neomacs_display_protocol::glyph_matrix::GlyphProvenance::Str {
                        index, ..
                    } => Some((index, i)),
                    _ => None,
                })
                .min()
                .expect("replacement glyph")
                .1;
            let expected_col: u16 = row.glyphs[GlyphArea::LeftMargin.index()]
                .iter()
                .chain(glyphs[..selected].iter())
                .filter(|glyph| !glyph.padding)
                .map(|glyph| glyph.materialized_slot_span())
                .sum();
            let expected_x = row.pixel_x
                + glyphs[..selected]
                    .iter()
                    .filter(|glyph| !glyph.padding)
                    .map(|glyph| glyph.pixel_width)
                    .sum::<f32>();
            let output = prepared.materialize();
            if visible {
                assert_eq!(output.active_cursor().unwrap().slot_id.col, expected_col);
            } else {
                assert!(output.active_cursor().is_none());
            }
            let snapshot = eval
                .frame_manager()
                .get(frame)
                .unwrap()
                .redisplay_snapshot(window)
                .unwrap();
            let logical = snapshot.logical_cursor.as_ref().unwrap();
            // This fixture's line-number marks occupy display slots, not
            // buffer-text output columns. Keep this oracle independent of the resolver.
            let expected_output_col: u16 = glyphs[..selected]
                .iter()
                .filter(|glyph| {
                    !glyph.padding
                        && !matches!(
                            glyph.provenance,
                            neomacs_display_protocol::glyph_matrix::GlyphProvenance::Mark
                        )
                })
                .map(|glyph| glyph.materialized_slot_span())
                .sum();
            if line_numbers {
                assert!(expected_col > expected_output_col);
            }
            assert_eq!(
                logical.col,
                i64::from(expected_output_col),
                "{replacement}, visible={visible}, line_numbers={line_numbers}"
            );
            assert_eq!(
                logical.x,
                expected_x.round() as i64,
                "{replacement}, visible={visible}"
            );
            if let Some(physical) = snapshot.phys_cursor.as_ref() {
                assert_eq!(logical.x, physical.x, "{replacement}");
            }
        }
    }
}

/// Hidden-cursor anchors must use the replacement's dimensions, not a preceding
/// zero-width combining point that happens to share its logical column.
#[test]
fn hidden_rtl_cursor_anchor_uses_selected_glyph_dimensions() {
    use neovm_core::window::geometry::{PresentationId, VisualAnchor, VisualAnchorQuery};
    let (mut eval, frame, _, window) = incr_editing_frame("a\u{0301}x\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.buffer_manager_mut()
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("cursor-type", Value::NIL);
    eval.eval_str(
        "(progn (put-text-property 3 4 'display \"אבג\") (set-window-point (selected-window) 3))",
    )
    .unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let FrameLayoutAttempt::Prepared(prepared) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("hidden replacement")
    };
    let row = &prepared
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .unwrap()
        .matrix
        .rows[0];
    let selected = row.glyphs[GlyphArea::Text.index()]
        .iter()
        .filter(|glyph| !glyph.padding && row.glyph_covers_buffer_charpos(glyph, 2))
        .filter_map(|glyph| match glyph.provenance {
            neomacs_display_protocol::glyph_matrix::GlyphProvenance::Str { index, .. } => {
                Some((index, glyph))
            }
            _ => None,
        })
        .min_by_key(|(index, _)| *index)
        .unwrap()
        .1;
    let expected_width = selected.pixel_width.round();
    assert!(expected_width > 0.0);
    assert!(prepared.materialize().active_cursor().is_none());
    let snapshot = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .redisplay_snapshot(window)
        .unwrap()
        .clone();
    let owner = eval.frame_manager_mut().get_mut(frame).unwrap();
    // The layout attempt already prepared generation one.
    let presentation = PresentationId::new(2);
    owner
        .prepare_live_window_presentation(presentation, vec![snapshot])
        .unwrap();
    owner.activate_display_presentation(presentation).unwrap();
    let anchor = owner
        .resolve_active_visual_anchor(VisualAnchorQuery::new(
            presentation,
            VisualAnchor::CursorBottom { window },
        ))
        .unwrap();
    assert_eq!(anchor.bounds_in_frame().width().get(), expected_width);
}

/// Explicit string cursors retain one selected primitive's dimensions, not
/// row defaults or the aggregate advance of a multi-glyph source character.
#[test]
fn hidden_explicit_string_cursor_anchor_uses_selected_glyph_dimensions() {
    use neovm_core::window::geometry::{PresentationId, VisualAnchor, VisualAnchorQuery};
    for (source, selected_char) in [
        ("(propertize \"X\" 'cursor 1 'face '(:height 2.0))", 'X'),
        ("(propertize (string 1) 'cursor 1)", '^'),
        (
            "(concat \"א\" (propertize \"X\" 'cursor 1 'face '(:height 2.0)) \"ב\")",
            'X',
        ),
    ] {
        let (mut eval, frame, _, window) = incr_editing_frame("a\n", 800, 240);
        realize_test_gui_frame(&mut eval, frame);
        eval.buffer_manager_mut()
            .current_buffer_mut()
            .unwrap()
            .set_buffer_local("cursor-type", Value::NIL);
        eval.eval_str(&format!(
            "(progn (setq ctl-arrow t) (overlay-put (make-overlay 1 1) 'before-string {source}))"
        ))
        .unwrap();
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
        let FrameLayoutAttempt::Prepared(prepared) =
            engine.redisplay_frame_attempt(&mut eval, frame)
        else {
            panic!("explicit overlay")
        };
        let row = &prepared
            .window_matrices
            .iter()
            .find(|entry| entry.window_id.get() == window.0 as i64)
            .unwrap()
            .matrix
            .rows[0];
        let selected = row.glyphs[GlyphArea::Text.index()]
            .iter()
            .find(|glyph| matches!(&glyph.glyph_type, neomacs_display_protocol::glyph_matrix::GlyphType::Char { ch } if *ch == selected_char))
            .expect("selected primitive");
        let expected = selected.pixel_height.round();
        let expected_width = selected.pixel_width.round();
        assert!(prepared.materialize().active_cursor().is_none());
        let snapshot = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .clone();
        let owner = eval.frame_manager_mut().get_mut(frame).unwrap();
        let presentation = PresentationId::new(2);
        owner
            .prepare_live_window_presentation(presentation, vec![snapshot])
            .unwrap();
        owner.activate_display_presentation(presentation).unwrap();
        let anchor = owner
            .resolve_active_visual_anchor(VisualAnchorQuery::new(
                presentation,
                VisualAnchor::CursorBottom { window },
            ))
            .unwrap();
        assert_eq!(anchor.bounds_in_frame().height().get(), expected);
        assert_eq!(
            anchor.bounds_in_frame().width().get(),
            expected_width,
            "{source}"
        );
    }
}

/// An unchanged hidden explicit cursor still owns its position and extent.
/// Visible cursors remain a positive control for ordinary retained replay.
#[test]
fn hidden_explicit_cursor_survives_unchanged_replay() {
    use neovm_core::window::geometry::{PresentationId, VisualAnchor, VisualAnchorQuery};
    for visible in [true, false] {
        let (mut eval, frame, _, window) = incr_editing_frame("a\n", 800, 240);
        realize_test_gui_frame(&mut eval, frame);
        eval.buffer_manager_mut()
            .current_buffer_mut()
            .unwrap()
            .set_buffer_local("cursor-type", if visible { Value::T } else { Value::NIL });
        eval.eval_str("(overlay-put (make-overlay 1 1) 'before-string (propertize \"X\" 'cursor 1 'face '(:height 2.0)))").unwrap();
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
        engine.layout_frame_rust(&mut eval, frame);
        let before = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .clone();
        assert!(before.logical_cursor.is_some());
        assert!(before.logical_cursor_dimensions.is_some());
        let FrameLayoutAttempt::Prepared(replayed) =
            engine.redisplay_frame_attempt(&mut eval, frame)
        else {
            panic!("unchanged replay")
        };
        let after = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .clone();
        assert_eq!(
            engine.last_layout_stats().cursor_only_windows,
            1,
            "visible={visible}"
        );
        if visible {
            assert!(after.phys_cursor.is_some());
        } else {
            assert!(replayed.materialize().active_cursor().is_none());
            let position = |snapshot: &neovm_core::window::WindowDisplaySnapshot| {
                snapshot
                    .logical_cursor
                    .as_ref()
                    .map(|cursor| (cursor.x, cursor.y, cursor.row, cursor.col))
            };
            assert_eq!(position(&after), position(&before));
            assert_eq!(
                after.logical_cursor_dimensions,
                before.logical_cursor_dimensions
            );
            let expected = before.logical_cursor_dimensions.unwrap();
            let owner = eval.frame_manager_mut().get_mut(frame).unwrap();
            let presentation = PresentationId::new(3);
            owner
                .prepare_live_window_presentation(presentation, vec![after])
                .unwrap();
            owner.activate_display_presentation(presentation).unwrap();
            let anchor = owner
                .resolve_active_visual_anchor(VisualAnchorQuery::new(
                    presentation,
                    VisualAnchor::CursorBottom { window },
                ))
                .unwrap();
            assert_eq!(
                anchor.bounds_in_frame().width().get(),
                expected.width as f32
            );
            assert_eq!(
                anchor.bounds_in_frame().height().get(),
                expected.height as f32
            );
        }
    }
}

/// Mutating a retained string's cursor intervals is independent of buffer and
/// overlay ticks; unchanged point must still reselect the authoritative slot.
#[test]
fn hidden_explicit_cursor_reselects_after_string_property_mutation() {
    for (setup, mutation) in [
        (
            "(progn (setq neo-replay-string (propertize \"XY\" 'face '(:height 2.0))) (put-text-property 0 1 'cursor 1 neo-replay-string) (overlay-put (make-overlay 1 1) 'before-string neo-replay-string))",
            "(progn (put-text-property 0 1 'cursor nil neo-replay-string) (put-text-property 1 2 'cursor 1 neo-replay-string))",
        ),
        (
            "(progn (setq neo-replay-string (copy-sequence \"XY\")) (overlay-put (make-overlay 1 1) 'before-string neo-replay-string))",
            "(put-text-property 0 1 'cursor 1 neo-replay-string)",
        ),
    ] {
        let (mut eval, frame, _, window) = incr_editing_frame("a\n", 800, 240);
        realize_test_gui_frame(&mut eval, frame);
        eval.buffer_manager_mut()
            .current_buffer_mut()
            .unwrap()
            .set_buffer_local("cursor-type", Value::NIL);
        eval.eval_str(setup).unwrap();
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
        engine.layout_frame_rust(&mut eval, frame);
        eval.eval_str("(put-text-property 0 1 'face 'bold (copy-sequence \"unrelated\"))")
            .unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        assert_eq!(
            engine.last_layout_stats().cursor_only_windows,
            1,
            "unrelated string mutation must not invalidate observed sources"
        );
        eval.eval_str(mutation).unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        let actual = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .clone();
        let mut fresh = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut fresh);
        fresh.layout_frame_rust(&mut eval, frame);
        let expected = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap();
        assert!(actual.phys_cursor.is_none());
        let position = |snapshot: &neovm_core::window::WindowDisplaySnapshot| {
            snapshot
                .logical_cursor
                .as_ref()
                .map(|cursor| (cursor.x, cursor.y, cursor.row, cursor.col))
        };
        assert_eq!(position(&actual), position(expected));
        assert_eq!(
            actual.logical_cursor_dimensions,
            expected.logical_cursor_dimensions
        );
    }
}

/// A later leaf may mutate an earlier leaf's string without changing its
/// buffer or overlay ticks. Acceptance must revalidate the whole frame.
#[test]
fn hidden_cursor_sources_are_revalidated_after_later_window_callbacks() {
    let (mut eval, frame, _, window) = incr_editing_frame("a\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.buffer_manager_mut()
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("cursor-type", Value::NIL);
    eval.eval_str("(progn (setq neo-frame-string (propertize \"XY\" 'cursor 1) neo-frame-mutated nil noninteractive nil) (overlay-put (make-overlay 1 1) 'before-string neo-frame-string))").unwrap();
    let side = eval.buffer_manager_mut().create_buffer("*neo-later-leaf*");
    eval.buffer_manager_mut()
        .get_mut(side)
        .unwrap()
        .insert("b\n");
    eval.frame_manager_mut()
        .split_window(
            frame,
            window,
            neovm_core::window::SplitDirection::Horizontal,
            side,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    engine.layout_frame_rust(&mut eval, frame);
    let hook = eval.eval_str("(list (lambda (_pos) (if neo-frame-mutated nil (progn (setq neo-frame-mutated t) (put-text-property 0 1 'cursor nil neo-frame-string) (put-text-property 1 2 'cursor 1 neo-frame-string)))))").unwrap();
    eval.buffer_manager_mut()
        .get_mut(side)
        .unwrap()
        .set_buffer_local("fontification-functions", hook);
    engine.layout_frame_rust(&mut eval, frame);
    assert!(
        !eval.eval_str("neo-frame-mutated").unwrap().is_nil(),
        "later leaf hook must actually run"
    );
    let actual = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .redisplay_snapshot(window)
        .unwrap()
        .clone();
    let mut fresh = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut fresh);
    fresh.layout_frame_rust(&mut eval, frame);
    let expected = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .redisplay_snapshot(window)
        .unwrap();
    let position = |s: &neovm_core::window::WindowDisplaySnapshot| {
        s.logical_cursor.as_ref().map(|c| (c.x, c.y, c.row, c.col))
    };
    assert!(actual.phys_cursor.is_none());
    assert_eq!(position(&actual), position(expected));
    assert_eq!(
        actual.logical_cursor_dimensions,
        expected.logical_cursor_dimensions
    );
}

/// Global default properties can supply an explicit string cursor even when
/// the buffer has no intervals or overlays of its own.
#[test]
fn hidden_default_property_cursor_is_not_canonical() {
    let (mut eval, frame, _, window) = incr_editing_frame("ab\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.buffer_manager_mut()
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("cursor-type", Value::NIL);
    eval.eval_str("(setq default-text-properties (list 'display (let ((s (copy-sequence \"XY\"))) (put-text-property 1 2 'cursor 2 s) s)))").unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    engine.layout_frame_rust(&mut eval, frame);
    eval.eval_str("(set-window-point (selected-window) 2)")
        .unwrap();
    engine.layout_frame_rust(&mut eval, frame);
    let actual = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .redisplay_snapshot(window)
        .unwrap()
        .clone();
    let mut fresh = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut fresh);
    fresh.layout_frame_rust(&mut eval, frame);
    let expected = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .redisplay_snapshot(window)
        .unwrap();
    let position = |snapshot: &neovm_core::window::WindowDisplaySnapshot| {
        snapshot
            .logical_cursor
            .as_ref()
            .map(|cursor| (cursor.x, cursor.y, cursor.row, cursor.col))
    };
    assert_eq!(position(&actual), position(expected));
    assert_eq!(
        actual.logical_cursor_dimensions,
        expected.logical_cursor_dimensions
    );
    assert!(actual.phys_cursor.is_none());
}

/// Ordinary hidden selection uses finalized geometry without requiring paint
/// metrics; an empty last line must retain its real row origin too.
#[test]
fn hidden_canonical_cursor_replay_matches_fresh_positions() {
    for point in [2, 5] {
        let (mut eval, frame, _, window) = incr_editing_frame("abc\n", 800, 240);
        realize_test_gui_frame(&mut eval, frame);
        eval.buffer_manager_mut()
            .current_buffer_mut()
            .unwrap()
            .set_buffer_local("cursor-type", Value::NIL);
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame);
        eval.eval_str(&format!("(set-window-point (selected-window) {point})"))
            .unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        assert_eq!(engine.last_layout_stats().cursor_only_windows, 1);
        let actual = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .clone();
        assert!(actual.phys_cursor.is_none());
        let mut fresh = crate::font::metrics::test_catalog::layout_engine();
        fresh.layout_frame_rust(&mut eval, frame);
        let expected = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap();
        let position = |snapshot: &neovm_core::window::WindowDisplaySnapshot| {
            snapshot
                .logical_cursor
                .as_ref()
                .map(|cursor| (cursor.x, cursor.y, cursor.row, cursor.col))
        };
        assert_eq!(position(&actual), position(expected), "point={point}");
        assert_eq!(
            actual.logical_cursor_dimensions, expected.logical_cursor_dimensions,
            "point={point}"
        );
    }
}

/// Late source rejection occurs after speculative end publication; restore
/// the exact token before asking the frame coordinator to retry.
#[test]
fn late_source_rejection_restores_speculative_window_end() {
    use neovm_core::buffer::{EmacsBytePos, LispCharPos1};
    use neovm_core::window::{MatrixRow0, WindowEndRecord};
    let (mut eval, frame, buffer, window) = incr_editing_frame("a\n", 800, 240);
    let previous = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .find_window(window)
        .unwrap()
        .window_end_state();
    let mut attempt = eval.begin_redisplay_window_end_attempt(frame, window, buffer);
    assert!(attempt.is_some());
    eval.publish_redisplay_window_end(
        frame,
        window,
        WindowEndRecord::from_positions(
            LispCharPos1::new(3),
            EmacsBytePos::new(3),
            LispCharPos1::new(3),
            EmacsBytePos::new(3),
            MatrixRow0::new(1),
        ),
    );
    assert_ne!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .find_window(window)
            .unwrap()
            .window_end_state(),
        previous
    );
    eval.eval_str("(setq neo-end-source (propertize \"XY\" 'cursor 1))")
        .unwrap();
    let source = eval.eval_str("neo-end-source").unwrap();
    let capture = crate::cursor_source_dependencies::Capture::begin(&eval);
    crate::cursor_source_dependencies::string_properties(source);
    eval.eval_str("(put-text-property 0 1 'cursor nil neo-end-source)")
        .unwrap();
    assert!(!capture.unchanged());
    assert!(reject_changed_cursor_sources(
        &mut eval,
        &mut attempt,
        !capture.unchanged()
    ));
    assert!(attempt.is_none(), "rejected local token must be consumed");
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .find_window(window)
            .unwrap()
            .window_end_state(),
        previous
    );
}

/// Deleted owners must release source leases at idle maintenance, even when
/// the remaining frame is hidden and no presentation is accepted afterward.
#[test]
fn deleted_frame_source_roots_retire_during_idle_without_a_presentation() {
    use crate::cursor_source_dependencies::{Capture, string_properties};
    use crate::frame_visual_history::FrameVisualHistory;
    let (mut eval, frame, buffer, window) = incr_editing_frame("a\n", 800, 240);
    let other = eval
        .frame_manager_mut()
        .create_frame("hidden-survivor", 800, 240, buffer);
    eval.frame_manager_mut().get_mut(other).unwrap().visibility =
        neovm_core::window::FrameVisibility::Invisible;
    // Drain initialization's deferred reclamation before measuring our graph.
    for _ in 0..3 {
        eval.gc_collect_exact();
    }
    let count = || neovm_core::tagged::gc::with_tagged_heap(|heap| heap.allocated_count());
    let baseline = count();
    let source = Value::string("deleted-frame-unique-source");
    let capture = Capture::begin(&eval);
    string_properties(source);
    let certificate = capture.finish().unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.frame_visual_histories.commit(
        frame,
        FrameVisualHistory::default().with_cursor_sources(
            [(
                neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64),
                certificate,
            )]
            .into_iter()
            .collect(),
        ),
    );
    eval.gc_collect_exact();
    assert_eq!(count(), baseline + 1);
    // Deletion also retires ordinary frame parameters. Measure that separate
    // graph transition under a temporary root before testing our owner lease.
    let setup = eval.retain_gc_roots([source]);
    eval.frame_manager_mut().delete_frame(frame);
    for _ in 0..3 {
        eval.gc_collect_exact();
    }
    let with_source = count();
    drop(setup);
    assert!(engine.last_frame_display_state.is_none());
    engine.maintain_scroll_coverage(&eval);
    eval.gc_collect_exact();
    assert_eq!(
        count(),
        with_source - 1,
        "deleted frame's original source lease must retire without a new accepted frame"
    );
    assert!(engine.frame_visual_histories.get(frame).is_none());
}

/// Exhausted nested observation scopes mean unknown evidence, not an empty
/// certificate. Persistent failure must abort within the shared retry budget.
#[test]
fn incomplete_source_capture_aborts_without_publishing_speculative_state() {
    fn nested<T>(depth: usize, operation: &mut dyn FnMut() -> T) -> T {
        if depth == 0 {
            operation()
        } else {
            neovm_core::tagged::collection_reads::capture(|| nested(depth - 1, operation)).0
        }
    }
    let (mut eval, frame, _, window) = incr_editing_frame("a\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str("(overlay-put (make-overlay 1 1) 'before-string (propertize \"X\" 'cursor 1))")
        .unwrap();
    let previous = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .find_window(window)
        .unwrap()
        .window_end_state();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    let result = nested(8, &mut || engine.redisplay_frame_attempt(&mut eval, frame));
    assert!(
        matches!(result, FrameLayoutAttempt::Aborted),
        "unknown source evidence cannot authorize an accepted frame"
    );
    let frame_state = eval.frame_manager().get(frame).unwrap();
    assert!(!frame_state.has_prepared_display_presentations());
    assert!(frame_state.active_presentation().is_none());
    assert_eq!(
        frame_state.find_window(window).unwrap().window_end_state(),
        previous
    );
    assert!(engine.last_frame_display_state.is_none());
    // Leaving the exhausted outer scopes must allow the same producer to recover.
    assert!(matches!(
        engine.redisplay_frame_attempt(&mut eval, frame),
        FrameLayoutAttempt::Prepared(_)
    ));
}

/// Integer string coverage takes precedence over buffer point for visible as
/// well as hidden cursors; moved replay needs fresh semantic selection.
#[test]
fn moved_explicit_cursor_authority_matches_fresh_for_both_styles() {
    for visible in [false, true] {
        let (mut eval, frame, _, window) = incr_editing_frame("ab\n", 800, 240);
        realize_test_gui_frame(&mut eval, frame);
        eval.buffer_manager_mut()
            .current_buffer_mut()
            .unwrap()
            .set_buffer_local("cursor-type", if visible { Value::T } else { Value::NIL });
        eval.eval_str("(overlay-put (make-overlay 1 1) 'before-string (propertize \"X\" 'cursor 2 'face '(:height 2.0)))").unwrap();
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
        engine.layout_frame_rust(&mut eval, frame);
        eval.eval_str("(set-window-point (selected-window) 2)")
            .unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        let stats = engine.last_layout_stats().clone();
        let actual = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .clone();
        let mut fresh = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut fresh);
        fresh.layout_frame_rust(&mut eval, frame);
        let expected = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap();
        let position = |s: &neovm_core::window::WindowDisplaySnapshot| {
            s.logical_cursor.as_ref().map(|c| (c.x, c.y, c.row, c.col))
        };
        assert_eq!(position(&actual), position(expected), "visible={visible}");
        assert_eq!(
            actual.logical_cursor_dimensions, expected.logical_cursor_dimensions,
            "visible={visible}"
        );
        assert_eq!(
            actual.phys_cursor, expected.phys_cursor,
            "visible={visible}"
        );
        assert_eq!(
            stats.cursor_only_windows, 0,
            "moved override must be reselected"
        );
    }
}

/// A reused hidden point row needs its own logical capture, not a fallback
/// published by the newly walked tail. Require actual partial reuse.
#[test]
fn hidden_partial_cursor_on_reused_row_matches_fresh() {
    let (mut eval, frame, buffer, window) = incr_editing_frame(&"abc\n".repeat(80), 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.buffer_manager_mut()
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("cursor-type", Value::NIL);
    scroll_window_to(&mut eval, frame, window, buffer, 1, 21);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    engine.layout_frame_rust(&mut eval, frame);
    scroll_window_to(&mut eval, frame, window, buffer, 9, 21);
    engine.layout_frame_rust(&mut eval, frame);
    let stats = engine.last_layout_stats().clone();
    assert_eq!(stats.scroll_windows, 1);
    assert!(stats.reused_rows > 0);
    let actual = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .redisplay_snapshot(window)
        .unwrap()
        .clone();
    assert!(actual.phys_cursor.is_none());
    assert!(actual.logical_cursor.is_some());
    let mut fresh = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut fresh);
    fresh.layout_frame_rust(&mut eval, frame);
    let expected = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .redisplay_snapshot(window)
        .unwrap();
    let position = |s: &neovm_core::window::WindowDisplaySnapshot| {
        s.logical_cursor.as_ref().map(|c| (c.x, c.y, c.row, c.col))
    };
    assert_eq!(position(&actual), position(expected));
    assert_eq!(
        actual.logical_cursor_dimensions,
        expected.logical_cursor_dimensions
    );
}

/// Hidden point rows above or below a localized edit must retain canonical
/// cursor facts while the edit plan actually reuses retained rows.
#[test]
fn hidden_partial_edit_cursor_on_reused_rows_matches_fresh() {
    use crate::incremental_layout::edit_sync::{
        EditSyncMode, set_edit_sync_mode_for_test, set_prove_first_for_test,
    };
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            set_edit_sync_mode_for_test(None);
            set_prove_first_for_test(None);
        }
    }
    let _restore = Restore;
    set_edit_sync_mode_for_test(Some(EditSyncMode::Sync));
    set_prove_first_for_test(Some(true));
    for dirty_line in [2, 9, 14] {
        let (mut eval, frame, buffer, window) = incr_editing_frame(&"abc\n".repeat(80), 800, 240);
        realize_test_gui_frame(&mut eval, frame);
        eval.buffer_manager_mut()
            .current_buffer_mut()
            .unwrap()
            .set_buffer_local("cursor-type", Value::NIL);
        scroll_window_to(&mut eval, frame, window, buffer, 1, 21);
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
        engine.layout_frame_rust(&mut eval, frame);
        eval.eval_str(&format!(
            "(save-excursion (goto-char {}) (insert \"X\"))",
            dirty_line * 4 + 1
        ))
        .unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        assert_eq!(
            engine.last_layout_stats().edit_windows,
            1,
            "dirty_line={dirty_line}"
        );
        assert!(engine.last_layout_stats().reused_rows > 0);
        let actual = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .clone();
        assert!(actual.phys_cursor.is_none());
        assert!(actual.logical_cursor.is_some());
        let mut fresh = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut fresh);
        fresh.layout_frame_rust(&mut eval, frame);
        let expected = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap();
        let position = |s: &neovm_core::window::WindowDisplaySnapshot| {
            s.logical_cursor.as_ref().map(|c| (c.x, c.y, c.row, c.col))
        };
        assert_eq!(
            position(&actual),
            position(expected),
            "dirty_line={dirty_line}"
        );
        assert_eq!(
            actual.logical_cursor_dimensions, expected.logical_cursor_dimensions,
            "dirty_line={dirty_line}"
        );
    }
}

/// Mutating an original face plist must invalidate the source projection even
/// when neither the source string nor its interval table is replaced.
#[test]
fn retained_string_face_payload_mutation_matches_fresh_layout() {
    for mutation in [
        "(setcar (cdr neo-payload-face) 0.1)",
        "(setcdr neo-payload-face (list 0.1 :foreground \"blue\"))",
    ] {
        let (mut eval, frame, _, window) = incr_editing_frame("a\n", 800, 240);
        realize_test_gui_frame(&mut eval, frame);
        eval.buffer_manager_mut()
            .current_buffer_mut()
            .unwrap()
            .set_buffer_local("cursor-type", Value::NIL);
        eval.eval_str("(progn (setq neo-payload-face (list :height 2.0 :foreground \"red\")) (setq neo-payload-string (propertize \"XY\" 'face neo-payload-face 'cursor 1)) (overlay-put (make-overlay 1 1) 'before-string neo-payload-string))").unwrap();
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut engine);
        engine.layout_frame_rust(&mut eval, frame);
        eval.eval_str("(setcar (cdr (list :height 1.0)) 0.5)")
            .unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        assert_eq!(
            engine.last_layout_stats().cursor_only_windows,
            1,
            "unrelated plist remains irrelevant"
        );
        let before = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .logical_cursor_dimensions;
        eval.eval_str(mutation).unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        let actual = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .clone();
        let mut fresh = crate::font::metrics::test_catalog::layout_engine();
        crate::font::metrics::test_catalog::enable_metrics(&mut fresh);
        fresh.layout_frame_rust(&mut eval, frame);
        let expected = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap();
        assert_ne!(
            before, expected.logical_cursor_dimensions,
            "mutation changes actual geometry"
        );
        assert_eq!(
            actual.logical_cursor_dimensions, expected.logical_cursor_dimensions,
            "{mutation}"
        );
        assert_eq!(engine.last_layout_stats().cursor_only_windows, 0);
    }
}

/// A moved point or translated retained row cannot certify an old explicit
/// selection. Hidden overrides must be reselected, and retry must discard rows.
#[test]
fn hidden_explicit_cursor_reselects_after_point_and_start_changes() {
    let (mut eval, frame, _, window) =
        incr_editing_frame("ab\ncd\nef\ngh\nij\nkl\nmn\nop\n", 800, 80);
    realize_test_gui_frame(&mut eval, frame);
    eval.buffer_manager_mut()
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("cursor-type", Value::NIL);
    eval.eval_str("(progn (overlay-put (make-overlay 1 1) 'before-string (propertize \"X\" 'cursor 2 'face '(:height 2.0))) (overlay-put (make-overlay 4 4) 'before-string (propertize \"X\" 'cursor 2 'face '(:height 1.5))))").unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    engine.layout_frame_rust(&mut eval, frame);
    let mut last = None;
    for (change, height) in [
        ("(set-window-point (selected-window) 2)", 24),
        ("(set-window-point (selected-window) 5)", 18),
        ("(set-window-start (selected-window) 4 t)", 18),
    ] {
        eval.eval_str(change).unwrap();
        let FrameLayoutAttempt::Prepared(prepared) =
            engine.redisplay_frame_attempt(&mut eval, frame)
        else {
            panic!("changed hidden selection")
        };
        assert!(prepared.materialize().active_cursor().is_none());
        assert_eq!(engine.last_layout_stats().full_windows, 1, "{change}");
        let actual = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .clone();
        assert_eq!(actual.logical_cursor.as_ref().unwrap().x, 0, "{change}");
        assert_eq!(
            actual.logical_cursor_dimensions.unwrap().height,
            height,
            "{change}"
        );
        let rows = prepared
            .window_matrices
            .iter()
            .find(|entry| entry.window_id.get() == window.0 as i64)
            .unwrap()
            .matrix
            .rows
            .iter()
            .map(|row| {
                (
                    row.pixel_y,
                    row.height_px,
                    row.glyphs[GlyphArea::Text.index()]
                        .iter()
                        .map(|glyph| {
                            (
                                glyph.glyph_type.clone(),
                                glyph.pixel_width,
                                glyph.pixel_height,
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        last = Some((actual, rows));
    }
    let (actual, rows) = last.unwrap();
    let mut fresh = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut fresh);
    let FrameLayoutAttempt::Prepared(expected_frame) =
        fresh.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("fresh hidden selection")
    };
    let expected = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .redisplay_snapshot(window)
        .unwrap();
    let position = |snapshot: &neovm_core::window::WindowDisplaySnapshot| {
        snapshot
            .logical_cursor
            .as_ref()
            .map(|cursor| (cursor.x, cursor.y, cursor.row, cursor.col))
    };
    assert_eq!(position(&actual), position(expected));
    assert_eq!(
        actual.logical_cursor_dimensions,
        expected.logical_cursor_dimensions
    );
    let expected_rows = expected_frame
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .unwrap()
        .matrix
        .rows
        .iter()
        .map(|row| {
            (
                row.pixel_y,
                row.height_px,
                row.glyphs[GlyphArea::Text.index()]
                    .iter()
                    .map(|glyph| {
                        (
                            glyph.glyph_type.clone(),
                            glyph.pixel_width,
                            glyph.pixel_height,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(rows, expected_rows);
}

/// Moved-point reconstruction must share fresh stretch-width policy, including
/// preserving narrow stretches and respecting x-stretch-cursor.
#[test]
fn stretch_cursor_only_replay_matches_fresh_width_policy() {
    for stretch in [false, true] {
        for (width, style) in [0.01, 0.25, 5.0]
            .into_iter()
            .flat_map(|width| ["box", "bar", "hbar"].map(|style| (width, style)))
        {
            let (mut eval, frame, _, window) = incr_editing_frame("אxב\n", 800, 240);
            realize_test_gui_frame(&mut eval, frame);
            let cursor_type = match style {
                "box" => Value::T,
                "bar" => Value::cons(Value::symbol("bar"), Value::fixnum(2)),
                _ => Value::symbol("hbar"),
            };
            eval.buffer_manager_mut()
                .current_buffer_mut()
                .unwrap()
                .set_buffer_local("cursor-type", cursor_type);
            eval.eval_str(&format!("(progn (setq x-stretch-cursor {}) (put-text-property 2 3 'display '(space :width {width})) (set-window-point (selected-window) 1))", if stretch { "t" } else { "nil" })).unwrap();
            let mut engine = crate::font::metrics::test_catalog::layout_engine();
            crate::font::metrics::test_catalog::enable_metrics(&mut engine);
            engine.layout_frame_rust(&mut eval, frame);
            let column_width = eval
                .eval_str("(frame-char-width)")
                .unwrap()
                .as_fixnum()
                .unwrap() as f32;
            assert!(column_width >= 1.0);
            eval.eval_str("(set-window-point (selected-window) 2)")
                .unwrap();
            let FrameLayoutAttempt::Prepared(replayed) =
                engine.redisplay_frame_attempt(&mut eval, frame)
            else {
                panic!("stretch replay")
            };
            assert_eq!(engine.last_layout_stats().cursor_only_windows, 1);
            assert!(engine.last_layout_stats().reused_rows > 0);
            let actual = replayed.materialize().active_cursor().unwrap().width;
            let row = &replayed
                .window_matrices
                .iter()
                .find(|entry| entry.window_id.get() == window.0 as i64)
                .unwrap()
                .matrix
                .rows[0];
            let glyph_width = row.glyphs[GlyphArea::Text.index()]
                .iter()
                .find(|glyph| {
                    row.glyph_covers_buffer_charpos(glyph, 1)
                        && matches!(
                            glyph.glyph_type,
                            neomacs_display_protocol::glyph_matrix::GlyphType::Stretch { .. }
                        )
                })
                .unwrap()
                .pixel_width;
            let expected_width = if style == "bar" {
                2.0
            } else if stretch {
                glyph_width.max(1.0)
            } else {
                glyph_width.max(1.0).min(column_width)
            };
            assert_eq!(
                actual, expected_width,
                "stretch={stretch} width={width} style={style}"
            );
            if width < 0.1 && style != "bar" {
                assert_eq!(actual, 1.0);
            }
            if width > 1.0 {
                assert!(glyph_width > column_width);
            } else {
                assert!(glyph_width < column_width);
            }
            let mut fresh = crate::font::metrics::test_catalog::layout_engine();
            crate::font::metrics::test_catalog::enable_metrics(&mut fresh);
            let FrameLayoutAttempt::Prepared(rebuilt) =
                fresh.redisplay_frame_attempt(&mut eval, frame)
            else {
                panic!("fresh stretch")
            };
            assert_eq!(
                rebuilt.materialize().active_cursor().unwrap().width,
                expected_width,
                "fresh stretch={stretch} width={width} style={style}"
            );
        }
    }
}

/// An unchanged authoritative cursor needs no new point-font measurement.
#[test]
fn unchanged_unmeasured_cursor_preserves_retained_replay() {
    let (mut eval, frame, _buffer, _) = incr_editing_frame("text\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let mut engine = crate::LayoutEngine::new_without_font_metrics();
    let FrameLayoutAttempt::Prepared(first) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("first frame should prepare")
    };
    let first = first.materialize().active_cursor().unwrap().clone();
    let FrameLayoutAttempt::Prepared(second) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("unchanged frame should prepare")
    };
    assert_eq!(engine.last_layout_stats().cursor_only_windows, 1);
    assert!(engine.last_layout_stats().reused_rows > 0);
    assert_eq!(second.materialize().active_cursor().unwrap(), &first);
}

/// A scroll retaining point's row must use its point glyph, not the whole row.
#[test]
fn mixed_height_scroll_cursor_matches_fresh_capture() {
    for spacing in [0, 5] {
        for point_offset in [4, 5] {
            let text = "AAAx\n".repeat(80);
            let point = 11 * 5 + point_offset;
            let (mut eval, frame, buffer, window) = incr_editing_frame(&text, 800, 600);
            realize_test_gui_frame(&mut eval, frame);
            eval.eval_str("(put-text-property 56 59 'face '(:height 2.0))")
                .unwrap();
            eval.buffer_manager_mut()
                .get_mut(buffer)
                .unwrap()
                .set_buffer_local("line-spacing", Value::fixnum(spacing));
            let mut engine = crate::font::metrics::test_catalog::layout_engine();
            crate::font::metrics::test_catalog::enable_metrics(&mut engine);
            scroll_window_to(&mut eval, frame, window, buffer, 1, point - 1);
            assert!(matches!(
                engine.redisplay_frame_attempt(&mut eval, frame),
                FrameLayoutAttempt::Prepared(_)
            ));
            scroll_window_to(&mut eval, frame, window, buffer, 21, point - 1);
            let FrameLayoutAttempt::Prepared(replay) =
                engine.redisplay_frame_attempt(&mut eval, frame)
            else {
                panic!("scroll should prepare")
            };
            assert_eq!(engine.last_layout_stats().scroll_windows, 1);
            assert!(engine.last_layout_stats().reused_rows > 0);
            let mut fresh = crate::font::metrics::test_catalog::layout_engine();
            crate::font::metrics::test_catalog::enable_metrics(&mut fresh);
            let FrameLayoutAttempt::Prepared(rebuilt) =
                fresh.redisplay_frame_attempt(&mut eval, frame)
            else {
                panic!("fresh frame should prepare")
            };
            let replay = replay.materialize();
            let rebuilt = rebuilt.materialize();
            let actual = replay.active_cursor().unwrap();
            let expected = rebuilt.active_cursor().unwrap();
            assert_eq!(
                (actual.y, actual.height, actual.ascent),
                (expected.y, expected.height, expected.ascent),
                "spacing={spacing}, point offset={point_offset}"
            );
        }
    }
}

/// An unmeasured point glyph must decline replay rather than invent its descent.
#[test]
fn unmeasured_point_glyphs_decline_cursor_replay() {
    for stretch in [false, true] {
        for scroll in [false, true] {
            let (mut eval, frame, buffer, window) =
                incr_editing_frame(&"AAAx\n".repeat(80), 800, 600);
            realize_test_gui_frame(&mut eval, frame);
            eval.eval_str("(put-text-property 56 59 'face '(:height 2.0))")
                .unwrap();
            if stretch {
                eval.eval_str(
                    "(put-text-property 56 57 'display '(space :width 1 :height (40) :ascent 75))",
                )
                .unwrap();
            }
            let mut engine = crate::LayoutEngine::new_without_font_metrics();
            scroll_window_to(
                &mut eval,
                frame,
                window,
                buffer,
                1,
                if scroll { 58 } else { 0 },
            );
            assert!(matches!(
                engine.redisplay_frame_attempt(&mut eval, frame),
                FrameLayoutAttempt::Prepared(_)
            ));
            if scroll {
                scroll_window_to(&mut eval, frame, window, buffer, 21, 58);
            } else {
                eval.eval_str("(set-window-point (selected-window) 59)")
                    .unwrap();
            }
            let FrameLayoutAttempt::Prepared(replayed) =
                engine.redisplay_frame_attempt(&mut eval, frame)
            else {
                panic!("replay should prepare")
            };
            assert_eq!(engine.last_layout_stats().full_windows, 1);
            assert_eq!(engine.last_layout_stats().scroll_windows, 0);
            assert_eq!(engine.last_layout_stats().cursor_only_windows, 0);
            let mut fresh = crate::LayoutEngine::new_without_font_metrics();
            let FrameLayoutAttempt::Prepared(rebuilt) =
                fresh.redisplay_frame_attempt(&mut eval, frame)
            else {
                panic!("fresh frame should prepare")
            };
            let replayed = replayed.materialize();
            let rebuilt = rebuilt.materialize();
            let actual = replayed.active_cursor().unwrap();
            let expected = rebuilt.active_cursor().unwrap();
            assert_eq!(
                (actual.y, actual.height, actual.ascent),
                (expected.y, expected.height, expected.ascent),
                "scroll={scroll}, stretch={stretch}"
            );
        }
    }
}

#[test]
fn text_cursor_keeps_its_metrics_when_line_spacing_enlarges_the_row() {
    let (mut eval, frame, buffer, _window) = incr_editing_frame("alpha\nbeta\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let FrameLayoutAttempt::Prepared(unspaced) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("unspaced frame should prepare");
    };
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .set_buffer_local("line-spacing", Value::fixnum(5));
    let FrameLayoutAttempt::Prepared(spaced) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("spaced frame should prepare");
    };
    let unspaced = unspaced.materialize();
    let spaced = spaced.materialize();
    let before = unspaced.active_cursor().expect("unspaced text cursor");
    let after = spaced.active_cursor().expect("spaced text cursor");
    let (_, row_y, _, row_height) = spaced
        .slot_glyph(after.slot_id)
        .unwrap()
        .cell_rect()
        .unwrap();
    assert_eq!(after.height, before.height, "spacing is not cursor height");
    assert_eq!(
        after.ascent, before.ascent,
        "spacing must not move the baseline"
    );
    assert_eq!(row_height, after.height + 5.0);
    assert_eq!(after.y, row_y);
}

#[test]
fn newline_spacing_is_preserved_when_point_enters_an_empty_line() {
    let (mut eval, frame, _buffer, window) = incr_editing_frame("alpha\n\nbeta\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let rows = |engine: &mut LayoutEngine, eval: &mut Context| {
        let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(eval, frame)
        else {
            panic!("frame should prepare");
        };
        output
            .window_matrices
            .iter()
            .find(|entry| entry.window_id.get() == window.0 as i64)
            .unwrap()
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && !row.mode_line)
            .take(3)
            .map(|row| (row.pixel_y, row.height_px))
            .collect::<Vec<_>>()
    };
    let unspaced = rows(&mut engine, &mut eval);
    eval.eval_str("(put-text-property 1 13 'line-spacing 5)")
        .unwrap();
    let before = rows(&mut engine, &mut eval);
    assert_eq!(before.len(), 3, "three text rows: {before:?}");
    eval.eval_str("(set-window-point (selected-window) 7)")
        .unwrap();
    let after = rows(&mut engine, &mut eval);
    let mut fresh = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut fresh);
    assert_eq!(
        after,
        rows(&mut fresh, &mut eval),
        "retained and fresh row geometry agree"
    );
    assert_eq!(
        before[0].1,
        unspaced[0].1 + 5.0,
        "line-spacing belongs to the logical row height: {before:?}"
    );
    assert_eq!(before, after, "moving point must not change line spacing");
    for pair in before.windows(2) {
        assert_eq!(
            pair[1].0 - pair[0].0,
            pair[0].1,
            "GNU row height includes spacing: adjacent backgrounds must meet"
        );
    }
}

#[test]
fn relative_default_line_spacing_uses_whole_pixels_without_moving_the_baseline() {
    let (mut eval, frame, _, window) = incr_editing_frame("alpha\n\nbeta\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let FrameLayoutAttempt::Prepared(unspaced) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("unspaced frame should prepare");
    };
    let baseline = &unspaced
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .unwrap()
        .matrix
        .rows[0];
    // Arrange 2.75px of requested spacing with whatever native font the
    // platform realizes. GNU contributes two pixels, without raising ascent.
    let factor = 2.75 / f64::from(baseline.height_px);
    eval.eval_str(&format!(
        "(setq default-text-properties '(line-spacing {factor}))"
    ))
    .unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("frame should prepare");
    };
    let rows = &output
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .unwrap()
        .matrix
        .rows;
    assert_eq!(rows[0].height_px, baseline.height_px + 2.0);
    assert_eq!(rows[1].pixel_y - rows[0].pixel_y, baseline.height_px + 2.0);
    assert_eq!(rows[2].pixel_y - rows[1].pixel_y, baseline.height_px + 2.0);
    assert_eq!(rows[0].ascent_px, baseline.ascent_px);
}

#[test]
fn routed_row_keeps_tall_glyph_metrics_when_newline_returns_to_default_face() {
    let line = "tall text\n";
    let (mut eval, frame, buffer, _) = incr_editing_frame(&line.repeat(50), 800, 600);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = Some(Value::symbol("neomacs"));
    eval.eval_str(&format!(
        "(put-text-property 1 {} 'face '(:height 150))",
        3 * line.len()
    ))
    .unwrap();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(5 * line.len()));
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    let retained = &engine.retained_window_matrices
        [&neomacs_display_protocol::types::DisplayWindowId::new(window.0 as i64)];
    let row = retained
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.start_charpos == 2 * line.len())
        .unwrap();
    let glyph_height = row
        .glyphs
        .iter()
        .flatten()
        .map(|glyph| glyph.pixel_height)
        .fold(0.0f32, f32::max);
    assert!(
        glyph_height > retained.key.char_height,
        "fixture must use a taller face"
    );
    assert!(
        row.height_px >= glyph_height,
        "row height {} discarded a {}px glyph",
        row.height_px,
        glyph_height
    );
}
