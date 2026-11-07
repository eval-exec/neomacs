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
