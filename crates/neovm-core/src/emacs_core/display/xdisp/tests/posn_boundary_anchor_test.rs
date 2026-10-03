//! Full-posn regression for canonical TTY iterator boundary offsets.
//! Live GNU expectation is also checked by the unchanged actual SGR oracle.
use super::*;
use crate::window::{
    DisplayPointRole, DisplayPointSnapshot, DisplayRowSnapshot, PosnObjectExtentMode,
};
use neomacs_display_protocol::posn_object_extent::{PosnMatrixRow, PosnMatrixSnapshot};

#[test]
fn tty_extent_on_retains_canonical_eol_and_synthetic_tail_offsets() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer = eval.buffers.current_buffer().expect("buffer").id;
    eval.buffers
        .get_mut(buffer)
        .expect("buffer")
        .insert("abcdef\n");
    let frame_id = eval.frames.create_frame("posn-tty-offset", 120, 40, buffer);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    let point = |pos, x, y, col, role| DisplayPointSnapshot {
        buffer_pos: LispCharPos1::new(pos),
        role,
        x,
        y,
        width: 1,
        height: 1,
        row: y,
        col,
    };
    let row = |y, start, end, end_x| DisplayRowSnapshot {
        row: y,
        y,
        height: 1,
        start_x: 0,
        start_col: 0,
        end_x,
        end_col: end_x,
        start_buffer_pos: Some(LispCharPos1::new(start)),
        end_buffer_pos: Some(LispCharPos1::new(end)),
        ..Default::default()
    };
    let snapshot = crate::window::WindowDisplaySnapshot {
        window_id,
        points: vec![
            point(2, 1, 0, 1, DisplayPointRole::Glyph),
            point(7, 6, 0, 6, DisplayPointRole::InsertionBoundary),
            point(8, 0, 1, 0, DisplayPointRole::InsertionBoundary),
        ],
        rows: vec![row(0, 1, 7, 6), row(1, 8, 8, 0)],
        posn_matrix: Some(std::sync::Arc::new(PosnMatrixSnapshot {
            rows: (0..2)
                .map(|y| PosnMatrixRow {
                    enabled: true,
                    y,
                    height: 1,
                    areas: [0, 120, 0],
                })
                .collect(),
        })),
        ..Default::default()
    };
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
        frame.commit_redisplay_cache_for_test(vec![snapshot.clone()]);
    }
    let query = |eval: &mut Context, x, y| {
        builtin_posn_at_x_y(
            eval,
            vec![
                Value::fixnum(x),
                Value::fixnum(y),
                Value::make_window(window_id.0),
            ],
        )
        .expect("canonical terminal query")
    };
    {
        let _off = PosnExtentFixtureGuard::set(PosnObjectExtentMode::Off);
        assert_eq!(
            crate::emacs_core::print::print_value(&query(&mut eval, 39, 0)),
            "(#<window 1> 7 (39 . 0) 0 nil 7 (39 . 0) nil (0 . 0) (1 . 1))"
        );
        assert_eq!(
            crate::emacs_core::print::print_value(&query(&mut eval, 1, 1)),
            "(#<window 1> 8 (1 . 1) 0 nil 8 (1 . 1) nil (0 . 0) (1 . 1))"
        );
    }
    let _on = PosnExtentFixtureGuard::set(PosnObjectExtentMode::On);
    // Exact baseline-red anchors from live GNU31.1 after common menu0 setup:
    // EOLx6 -> click39 gives dx33; empty EOBx0 -> click1 gives dx1.
    assert_eq!(
        crate::emacs_core::print::print_value(&query(&mut eval, 39, 0)),
        "(#<window 1> 7 (39 . 0) 0 nil 7 (39 . 0) nil (33 . 0) (1 . 0))"
    );
    assert_eq!(
        crate::emacs_core::print::print_value(&query(&mut eval, 1, 1)),
        "(#<window 1> 8 (1 . 1) 0 nil 8 (1 . 1) nil (1 . 0) (1 . 0))"
    );
    assert_eq!(
        crate::emacs_core::print::print_value(&query(&mut eval, 1, 0)),
        "(#<window 1> 2 (1 . 0) 0 nil 2 (1 . 0) nil (0 . 0) (1 . 0))"
    );
    // No materialized point on row1: the canonical row walker creates a
    // SyntheticBoundary at its own x/y. Click below EOB keeps iterator row1.
    let mut synthetic = snapshot.clone();
    synthetic
        .points
        .retain(|point| point.buffer_pos != LispCharPos1::new(8));
    eval.frames
        .get_mut(frame_id)
        .expect("frame")
        .replace_redisplay_cache_for_test(vec![synthetic]);
    assert_eq!(
        crate::emacs_core::print::print_value(&query(&mut eval, 3, 5)),
        "(#<window 1> 8 (3 . 5) 0 nil 8 (3 . 1) nil (3 . 4) (1 . 0))"
    );
    // Raw output rows include top chrome; offset Y uses the body origin.
    let mut below_chrome = snapshot;
    below_chrome.header_line_height = 1;
    below_chrome.tab_line_height = 1;
    for point in &mut below_chrome.points {
        point.row += 2;
        point.y += 2;
    }
    for row in &mut below_chrome.rows {
        row.row += 2;
        row.y += 2;
    }
    below_chrome.posn_matrix = Some(std::sync::Arc::new(PosnMatrixSnapshot {
        rows: (0..4)
            .map(|y| PosnMatrixRow {
                enabled: true,
                y,
                height: 1,
                areas: [0, 120, 0],
            })
            .collect(),
    }));
    eval.frames
        .get_mut(frame_id)
        .expect("frame")
        .replace_redisplay_cache_for_test(vec![below_chrome]);
    assert_eq!(
        crate::emacs_core::print::print_value(&query(&mut eval, 3, 7)),
        "(#<window 1> 8 (3 . 5) 0 nil 8 (3 . 1) nil (3 . 4) (1 . 0))"
    );
}

#[test]
fn tty_extent_on_empty_hscrolled_eob_offsets_preserve_off_geometry() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer = eval.buffers.current_buffer().expect("buffer").id;
    eval.buffers.get_mut(buffer).expect("buffer").insert("\t\n");
    let frame_id = eval
        .frames
        .create_frame("posn-empty-hscroll", 120, 40, buffer);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    let snapshot = crate::window::WindowDisplaySnapshot {
        window_id,
        points: vec![
            DisplayPointSnapshot {
                buffer_pos: LispCharPos1::ONE,
                role: DisplayPointRole::Glyph,
                x: 0,
                y: 0,
                width: 4,
                height: 1,
                row: 0,
                col: 0,
            },
            DisplayPointSnapshot {
                buffer_pos: LispCharPos1::new(2),
                role: DisplayPointRole::InsertionBoundary,
                x: 4,
                y: 0,
                width: 0,
                height: 1,
                row: 0,
                col: 4,
            },
            DisplayPointSnapshot {
                buffer_pos: LispCharPos1::new(3),
                role: DisplayPointRole::InsertionBoundary,
                x: 0,
                y: 1,
                width: 1,
                height: 1,
                row: 1,
                col: 0,
            },
        ],
        rows: vec![
            DisplayRowSnapshot {
                row: 0,
                y: 0,
                height: 1,
                start_x: 0,
                start_col: 0,
                end_x: 4,
                end_col: 4,
                start_buffer_pos: Some(LispCharPos1::ONE),
                end_buffer_pos: Some(LispCharPos1::new(2)),
                ..Default::default()
            },
            DisplayRowSnapshot {
                row: 1,
                y: 1,
                height: 1,
                start_x: 0,
                start_col: 0,
                end_x: 0,
                end_col: 0,
                start_buffer_pos: Some(LispCharPos1::new(3)),
                end_buffer_pos: Some(LispCharPos1::new(3)),
                ..Default::default()
            },
        ],
        posn_matrix: Some(std::sync::Arc::new(PosnMatrixSnapshot {
            rows: (0..2)
                .map(|y| PosnMatrixRow {
                    enabled: true,
                    y,
                    height: 1,
                    areas: [0, 120, 0],
                })
                .collect(),
        })),
        ..Default::default()
    };
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
        let Window::Leaf { hscroll, .. } = frame.find_window_mut(window_id).expect("window") else {
            panic!("window is a leaf");
        };
        *hscroll = 9;
        frame.commit_redisplay_cache_for_test(vec![snapshot.clone()]);
    }
    let query = |eval: &mut Context, x, y| {
        let posn = builtin_posn_at_x_y(
            eval,
            vec![
                Value::fixnum(x),
                Value::fixnum(y),
                Value::make_window(window_id.0),
            ],
        )
        .expect("empty EOB query");
        crate::emacs_core::value::list_to_vec(&posn).expect("full posn")
    };
    for (x, y, dx, dy) in [(0, 1, 9, 0), (3, 6, 12, 5)] {
        let baseline = {
            let _off = PosnExtentFixtureGuard::set(PosnObjectExtentMode::Off);
            query(&mut eval, x, y)
        };
        let current = {
            let _on = PosnExtentFixtureGuard::set(PosnObjectExtentMode::On);
            query(&mut eval, x, y)
        };
        // Legacy source/XY/column/visibility is outside this sidecar change.
        for cell in [0, 1, 2, 3, 4, 5, 6, 7] {
            assert_eq!(
                crate::emacs_core::print::print_value(&current[cell]),
                crate::emacs_core::print::print_value(&baseline[cell])
            );
        }
        assert_eq!(
            crate::emacs_core::print::print_value(&current[8]),
            format!("({dx} . {dy})")
        );
        assert_eq!(
            crate::emacs_core::print::print_value(&current[9]),
            "(1 . 0)"
        );
    }
    {
        let _on = PosnExtentFixtureGuard::set(PosnObjectExtentMode::On);
        let ordinary_tab = query(&mut eval, 1, 0);
        assert_eq!(
            crate::emacs_core::print::print_value(&ordinary_tab[8]),
            "(0 . 0)"
        );
        let ordinary_newline = query(&mut eval, 7, 0);
        assert_eq!(
            crate::emacs_core::print::print_value(&ordinary_newline[8]),
            "(3 . 0)"
        );
    }
    assert_eq!(
        eval.frames
            .get(frame_id)
            .expect("frame")
            .redisplay_snapshot(window_id)
            .unwrap(),
        &snapshot
    );
}
