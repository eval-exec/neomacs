//! Accepted producer -> split -> query lifecycle for GNU's immutable frame pool.
use super::*;
use crate::window::{
    DisplayPointRole, DisplayPointSnapshot, DisplayRowSnapshot, PosnObjectExtentMode,
    SplitDirection, SplitPlacement, WindowDisplaySnapshot, WindowPresentationSnapshot,
};
use neomacs_display_protocol::glyph_matrix::{
    FrameDisplayState, GlyphMatrix, GlyphRow, MatrixRow, WindowMatrixEntry,
};
use neomacs_display_protocol::posn_frame_pool::PosnFramePool;
use neomacs_display_protocol::posn_object_extent::PosnMatrixSnapshot;
use neomacs_display_protocol::{DisplayWindowId, GlyphRowRole, Rect};
use std::sync::Arc;

#[test]
fn tty_posn_split_without_local_output_reads_the_accepted_frame_pool() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context();
    let buffer = eval.buffers.current_buffer().expect("buffer").id;
    let frame_id = eval.frames.create_frame("posn-pool-split", 12, 8, buffer);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        frame.initial = false;
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
        frame.set_window_layout_text_size(12, 8);
    }
    let bounds = *eval
        .frames
        .get(frame_id)
        .unwrap()
        .find_window(window_id)
        .unwrap()
        .bounds();
    let bounds = Rect::new(bounds.x, bounds.y, bounds.width, bounds.height);
    let mut state = FrameDisplayState::new(12, 8, 1.0, 1.0);
    let mut matrix = GlyphMatrix::new(bounds.height as usize, 12);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    row.ends_at_zv = true;
    row.height_px = 1.0;
    matrix.rows[0] = MatrixRow::new(row);
    state.window_matrices.push(WindowMatrixEntry {
        window_id: DisplayWindowId::new(window_id.0 as i64),
        matrix,
        pixel_bounds: bounds,
        text_pixel_bounds: bounds,
        text_clip_bounds: Some(bounds),
        selected: true,
    });
    let accepted = Arc::new(PosnFramePool::from_terminal_frame(
        &state,
        None,
        crate::encoding::char_width,
    ));
    let publication = WindowPresentationSnapshot::live(WindowDisplaySnapshot {
        window_id,
        posn_matrix: Some(Arc::new(PosnMatrixSnapshot::from_terminal_window(
            &state,
            &state.window_matrices[0],
            crate::encoding::char_width,
        ))),
        ..Default::default()
    });
    {
        let _on = PosnExtentFixtureGuard::set(PosnObjectExtentMode::On);
        eval.frames
            .get_mut(frame_id)
            .unwrap()
            .prepare_display_presentation_with_tty_posn_pool(
                crate::window::geometry::PresentationId::new(41),
                vec![publication],
                Some(accepted.clone()),
            )
            .expect("real accepted presentation");
    }
    assert!(
        eval.frames
            .get(frame_id)
            .unwrap()
            .redisplay_snapshot(window_id)
            .is_some()
    );
    // Lisp changes live source and topology before the new leaf produces any
    // redisplay output. Accepted numeric cells must survive both transitions.
    eval.buffers.get_mut(buffer).unwrap().insert("abcd\n");
    let child = eval
        .frames
        .split_window(
            frame_id,
            window_id,
            SplitDirection::Vertical,
            buffer,
            Some(3),
            SplitPlacement::AfterTarget,
        )
        .expect("split");
    eval.frames
        .apply_staged_split_sizes(
            frame_id,
            child,
            Some(3),
            Value::NIL,
            SplitDirection::Vertical,
        )
        .expect("applied GNU split sizes");
    assert!(
        eval.frames
            .get(frame_id)
            .unwrap()
            .redisplay_snapshot(child)
            .is_none()
    );
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = calls.clone();
    eval.install_window_layout_query(move |_eval, fid, wid, _scope| {
        assert_eq!((fid, wid), (frame_id, child));
        observed.set(observed.get() + 1);
        let mut points = (0..4)
            .map(|x| DisplayPointSnapshot {
                role: DisplayPointRole::Glyph,
                buffer_pos: LispCharPos1::new(x + 1),
                x,
                y: 0,
                width: 1,
                height: 1,
                row: 0,
                col: x,
            })
            .collect::<Vec<_>>();
        points.push(DisplayPointSnapshot {
            role: DisplayPointRole::InsertionBoundary,
            buffer_pos: LispCharPos1::new(5),
            x: 4,
            y: 0,
            width: 0,
            height: 1,
            row: 0,
            col: 4,
        });
        points.push(DisplayPointSnapshot {
            role: DisplayPointRole::InsertionBoundary,
            buffer_pos: LispCharPos1::new(6),
            x: 0,
            y: 1,
            width: 1,
            height: 1,
            row: 1,
            col: 0,
        });
        crate::window::WindowLayoutQueryOutcome::Ready(crate::window::WindowLayoutQuery::new(
            LispCharPos1::ONE,
            Some(WindowDisplaySnapshot {
                window_id: wid,
                points,
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
                        end_buffer_pos: Some(LispCharPos1::new(5)),
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
                        start_buffer_pos: Some(LispCharPos1::new(6)),
                        end_buffer_pos: Some(LispCharPos1::new(6)),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
        ))
    });
    let query = |eval: &mut Context, x, y| {
        crate::emacs_core::value::list_to_vec(
            &builtin_posn_at_x_y(
                eval,
                vec![
                    Value::fixnum(x),
                    Value::fixnum(y),
                    Value::make_window(child.0),
                ],
            )
            .expect("posn query"),
        )
        .unwrap()
    };
    for (x, y, expected) in [
        (0, 0, "(1 . 0)"),
        (1, 0, "(0 . 0)"),
        (3, 0, "(0 . 0)"),
        (7, 1, "(1 . 0)"),
    ] {
        let off = {
            let _off = PosnExtentFixtureGuard::set(PosnObjectExtentMode::Off);
            query(&mut eval, x, y)
        };
        let on = {
            let _on = PosnExtentFixtureGuard::set(PosnObjectExtentMode::On);
            query(&mut eval, x, y)
        };
        assert_eq!(
            on[..8],
            off[..8],
            "source and geometry remain the existing live query contract"
        );
        assert_eq!(crate::emacs_core::print::print_value(&off[9]), "(1 . 1)");
        assert_eq!(
            crate::emacs_core::print::print_value(&on[9]),
            expected,
            "GNU cold child cell metrics"
        );
    }
    let frame = eval.frames.get(frame_id).unwrap();
    assert!(Arc::ptr_eq(frame.tty_posn_pool().unwrap(), &accepted));
    assert!(
        frame.redisplay_snapshot(child).is_none(),
        "query-only rows cannot publish current matrix state"
    );
    assert_eq!(calls.get(), 8);
    // A refused identity is no publication authority either.
    let _on = PosnExtentFixtureGuard::set(PosnObjectExtentMode::On);
    assert!(
        eval.frames
            .get_mut(frame_id)
            .unwrap()
            .prepare_display_presentation_with_tty_posn_pool(
                crate::window::geometry::PresentationId::new(41),
                Vec::new(),
                Some(Arc::new((*accepted).clone())),
            )
            .is_err()
    );
    assert!(Arc::ptr_eq(
        eval.frames.get(frame_id).unwrap().tty_posn_pool().unwrap(),
        &accepted
    ));
}
