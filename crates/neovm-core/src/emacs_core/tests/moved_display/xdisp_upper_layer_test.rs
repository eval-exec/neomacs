//! xdisp_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::image_catalog::{
    ImageCatalog, ImageId, ImageLayoutExtent, ImageLoadAttempt, ImageLoadToken, ImageLookup,
    ImageMaskKind, ImageResolveRequest, ImageSizeLimit, ReadyImage, ResolvedImageMetadata,
};
use crate::emacs_core::xdisp::*;
use crate::window::DisplayFrameExt;
use crate::window::DisplayFrameManagerExt;
use crate::window::display::WindowDisplayContextExt;
use malachite::Integer;
use neovm_display::test_support::authority::{
    prefix_line_and_column_for_test as prefix_line_and_column,
    xdisp_interactive_context_for_test as interactive_context,
    xdisp_pixel_size_tty_context_for_test as pixel_size_tty_context,
};
use neovm_edit::buffer::buffer::BUFFER_SLOT_BUFFER_FILE_CODING_SYSTEM;
use neovm_edit::buffer::{BufferTextBackendKind, CharPos0, LispCharPos1};
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::DisplayHost;
use neovm_eval::emacs_core::eval::VariableView;
use neovm_eval::emacs_core::eval::data::display_spec::display_prop_replacing_p;
use neovm_value::error::FlowKind;
use neovm_value::intern::intern;
use neovm_value::value::Value;
use neovm_value::value::{
    StringTextPropertyRun, ValueKind, get_string_text_properties_table_for_value,
    set_string_text_properties_for_value,
};

fn assert_point_only_warm_query_reuse(setup: &str, reuse_after_move: bool) {
    assert_point_only_warm_query_reuse_with_face_setup(setup, None, reuse_after_move);
}

fn assert_point_only_warm_query_reuse_with_face_setup(
    setup: &str,
    face_setup: Option<&str>,
    reuse_after_move: bool,
) {
    use crate::window::{WindowLayoutQuery, WindowLayoutQueryOutcome, WindowLayoutQueryScope};

    let mut eval = interactive_context(crate::test_utils::new_context);
    let buffer = eval.edit.buffers.current_buffer().unwrap().id();
    eval.edit.buffers.get_mut(buffer).unwrap().insert("a\nb\n");
    let frame = eval
        .frame_manager_mut()
        .create_frame("point-only-row-query", 160, 96, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.frame_manager_mut().get_mut(frame).unwrap().initial = false;
    eval.set_variable("point-query-window", Value::make_window(window.0));
    if let Some(face_setup) = face_setup {
        let table = eval.frame_manager().get(frame).unwrap().face_hash_table();
        eval.set_variable("point-query-face-table", table);
        eval.eval_str(face_setup).unwrap();
    }
    eval.eval_str(
        "(progn
           (set-default 'display-line-numbers nil)
           (set-default 'mode-line-format nil)
           (set-default 'header-line-format nil)
           (set-default 'tab-line-format nil)
           (goto-char 1))",
    )
    .unwrap();
    eval.eval_str(setup).unwrap();
    match eval
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    {
        crate::window::Window::Leaf { point, .. } => *point = LispCharPos1::ONE,
        other => panic!("expected leaf window, got {other:?}"),
    }

    // Deliberately give the current buffer the opposite admission conditions.
    // The gate must inspect the queried window's buffer, not this buffer.
    let other_buffer = eval.edit.buffers.create_buffer("*point-query-other*");
    eval.set_current_buffer_unrecorded(other_buffer).unwrap();
    let other_value = if reuse_after_move {
        Value::T
    } else {
        Value::NIL
    };
    for name in [
        "display-line-numbers",
        "mode-line-format",
        "header-line-format",
        "tab-line-format",
    ] {
        eval.edit
            .buffers
            .set_buffer_local_property(other_buffer, name, other_value)
            .unwrap();
    }

    let freshness = eval
        .window_display_snapshot_freshness(frame, window, buffer)
        .unwrap();
    let mut warm = crate::window::WindowDisplaySnapshot {
        window_id: window,
        regions_materialized: true,
        points: vec![crate::window::DisplayPointSnapshot {
            role: crate::window::DisplayPointRole::Glyph,
            buffer_pos: LispCharPos1::new(3),
            x: 0,
            y: 16,
            width: 8,
            height: 16,
            row: 1,
            col: 0,
        }],
        rows: vec![crate::window::DisplayRowSnapshot {
            row: 1,
            y: 16,
            height: 16,
            start_buffer_pos: Some(LispCharPos1::new(3)),
            end_buffer_pos: Some(LispCharPos1::new(4)),
            ..Default::default()
        }],
        buffer_modiff: Some(eval.edit.buffers.get(buffer).unwrap().modified_tick()),
        layout_freshness: Some(freshness.clone()),
        ..Default::default()
    };
    warm.regions.text_body.width = 160.0;
    warm.regions.text_body.height = 32.0;
    neovm_display::test_support::authority::commit_redisplay_cache_for_test(
        eval.frame_manager_mut().get_mut(frame).unwrap(),
        vec![warm],
    );

    // Mock canonical bounds, not a font/chrome metric engine: the old body
    // admitted the target at y=16 in [0,32); the new visible body is [0,16),
    // with no target point. Header growth or current-line number face metrics
    // can invalidate that old admission without any display-variable mutation.
    let mut refreshed = crate::window::WindowDisplaySnapshot {
        window_id: window,
        regions_materialized: true,
        rows: vec![crate::window::DisplayRowSnapshot {
            row: 0,
            y: 0,
            height: 16,
            start_buffer_pos: Some(LispCharPos1::ONE),
            end_buffer_pos: Some(LispCharPos1::new(2)),
            ..Default::default()
        }],
        ..Default::default()
    };
    refreshed.regions.text_body.width = 160.0;
    refreshed.regions.text_body.height = 16.0;
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = calls.clone();
    eval.install_window_layout_query(move |_, fid, wid, scope| {
        assert_eq!((fid, wid), (frame, window));
        assert_eq!(
            scope,
            WindowLayoutQueryScope::Position {
                target: LispCharPos1::new(3)
            }
        );
        observed.set(observed.get() + 1);
        WindowLayoutQueryOutcome::Ready(WindowLayoutQuery::new(
            LispCharPos1::ONE,
            Some(refreshed.clone()),
        ))
    })
    .expect("install the layout-query handler");
    let query = |eval: &mut Context| {
        builtin_posn_at_point(eval, vec![Value::fixnum(3), Value::make_window(window.0)]).unwrap()
    };
    assert!(
        !query(&mut eval).is_nil(),
        "exact-fresh rows must answer: {setup}"
    );
    assert_eq!(calls.get(), 0, "exact-fresh rows must not query: {setup}");

    eval.edit
        .buffers
        .get_mut(buffer)
        .unwrap()
        .goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(3));
    match eval
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    {
        crate::window::Window::Leaf { point, .. } => {
            assert_eq!(*point, LispCharPos1::ONE);
            *point = LispCharPos1::new(3);
        }
        other => panic!("expected leaf window, got {other:?}"),
    }
    let moved = eval
        .window_display_snapshot_freshness(frame, window, buffer)
        .unwrap();
    assert_ne!(
        freshness, moved,
        "saved window point must change freshness: {setup}"
    );
    assert!(
        freshness.same_position_rows_ignoring_point(&moved),
        "only point may change: {setup}"
    );
    assert!(
        eval.fresh_window_display_snapshot(frame, window, buffer)
            .is_none()
    );
    assert_eq!(query(&mut eval).is_nil(), !reuse_after_move, "{setup}");
    assert_eq!(calls.get(), usize::from(!reuse_after_move), "{setup}");
    assert_eq!(eval.edit.buffers.current_buffer_id(), Some(other_buffer));
}

/// Commit "a\nb\n" rows as a redisplay at the current point: position 3
/// ("b") is a 14-pixel glyph on a row that something taller made 30 pixels.
fn commit_line_pixel_height_snapshot(
    ev: &mut Context,
    fid: crate::window::FrameId,
    wid: crate::window::WindowId,
    buf: neovm_edit::buffer::BufferId,
) {
    let (cl_frames, cl_buffers) = ev.frame_manager_and_buffers_mut();
    crate::emacs_core::window_cmds::remember_selected_window_point_in_state(
        cl_frames, cl_buffers, fid,
    );
    let freshness = ev.window_display_snapshot_freshness(fid, wid, buf).unwrap();
    let modiff = ev.edit.buffers.get(buf).unwrap().modified_tick();
    neovm_display::test_support::authority::commit_redisplay_cache_for_test(
        ev.frame_manager_mut().get_mut(fid).unwrap(),
        vec![crate::window::WindowDisplaySnapshot {
            window_id: wid,
            regions_materialized: true,
            points: vec![crate::window::DisplayPointSnapshot {
                role: crate::window::DisplayPointRole::Glyph,
                buffer_pos: LispCharPos1::new(3),
                x: 0,
                y: 14,
                width: 8,
                height: 14,
                row: 1,
                col: 0,
            }],
            rows: vec![
                line_pixel_height_row(0, 0, 14, 1, 2),
                line_pixel_height_row(1, 14, 30, 3, 4),
            ],
            buffer_modiff: Some(modiff),
            layout_freshness: Some(freshness),
            ..Default::default()
        }],
    );
}

/// A selected GUI frame (14-pixel lines) showing TEXT with point at POINT.
fn line_pixel_height_gui_context(
    text: &str,
    point: i64,
) -> (
    Context,
    crate::window::FrameId,
    crate::window::WindowId,
    neovm_edit::buffer::BufferId,
) {
    let mut ev = crate::test_utils::new_context();
    let buf = ev.edit.buffers.create_buffer("*lph*");
    ev.edit.buffers.set_current(buf);
    ev.edit.buffers.get_mut(buf).unwrap().insert(text);
    ev.eval_str(&format!("(goto-char {point})")).unwrap();
    let fid = ev.frame_manager_mut().create_frame("GUI", 800, 600, buf);
    let frame = ev.frame_manager_mut().get_mut(fid).unwrap();
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.char_height = 14.0;
    assert!(ev.frame_manager_mut().select_frame(fid));
    let (_, wid) = resolve_live_window_identity(&ev.frame_manager(), None)
        .unwrap()
        .expect("selected window");
    (ev, fid, wid, buf)
}

fn line_pixel_height_row(
    row: i64,
    y: i64,
    height: i64,
    start: i64,
    end: i64,
) -> crate::window::DisplayRowSnapshot {
    crate::window::DisplayRowSnapshot {
        row,
        y,
        height,
        start_buffer_pos: Some(LispCharPos1::new(start)),
        end_buffer_pos: Some(LispCharPos1::new(end)),
        ..Default::default()
    }
}

fn snapshot_string(value: Value) -> String {
    neovm_value::emacs_char::to_utf8_lossy(
        value.as_lisp_string().expect("string result").as_bytes(),
    )
}

#[test]
fn redisplay_fontification_walks_successive_unfontified_chunks() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let target = eval
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    eval.eval_str(
        r#"
        (insert "abcdefghij")
        (setq redisplay-fontify-calls nil)
        (setq fontification-functions
              (list (lambda (start)
                      (setq redisplay-fontify-calls
                            (cons start redisplay-fontify-calls))
                      (let ((end (min (point-max) (+ start 3))))
                        (put-text-property start end 'fontified t)
                        (put-text-property start end 'face 'bold)))))
        "#,
    )
    .unwrap_or_else(|err| panic!("install chunked fontification hook: {err}"));

    let other = eval.buffer_manager_mut().create_buffer("*other*");
    eval.set_current_buffer_unrecorded(other)
        .expect("switch to other buffer");

    let outcome =
        ensure_fontified_for_redisplay(&mut eval, target, 0, 10).expect("fontify target buffer");
    assert_eq!(outcome, RedisplayFontificationOutcome::Fontified);

    assert_eq!(eval.edit.buffers.current_buffer_id(), Some(other));
    eval.set_current_buffer_unrecorded(target)
        .expect("switch back to target");
    let result = eval
        .eval_str(
            r#"(list redisplay-fontify-calls
                     (text-property-not-all 1 (point-max) 'fontified t)
                     (get-text-property 10 'face))"#,
        )
        .expect("inspect fontification result");
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&result),
        "((10 7 4 1) nil bold)"
    );
}

#[test]
fn test_format_mode_line_size_and_process_specs_match_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context(crate::test_utils::new_context);
    let buffer_id = eval.edit.buffers.create_buffer("mode-line-metadata");
    eval.edit.buffers.set_current(buffer_id);
    {
        let buffer = eval.edit.buffers.get_mut(buffer_id).expect("buffer");
        buffer.insert(&"x".repeat(1536));
    }

    let no_process =
        builtin_format_mode_line_ctx(&mut eval, &vec![Value::string("%i|%I|%s")]).expect("specs");
    assert_eq!(no_process, Value::string("1536|1.5k|no process"));

    eval.processes_mut().create_process_record(
        neovm_value::heap_types::LispString::from_utf8("mode-line-proc"),
        Value::make_buffer(buffer_id),
        neovm_value::heap_types::LispString::from_utf8("cat"),
        vec![],
        neovm_eval::emacs_core::eval::data::process_state::ProcessKind::Real,
        None,
        neovm_eval::emacs_core::eval::data::process_state::ProcessCodingSystems {
            decode: Value::NIL,
            encode: Value::NIL,
        },
    );
    let with_process =
        builtin_format_mode_line_ctx(&mut eval, &vec![Value::string("%i|%I|%s")]).expect("specs");
    assert_eq!(with_process, Value::string("1536|1.5k|run"));
}

#[test]
fn format_mode_line_line_number_starts_at_accessible_buffer_beginning() {
    // GNU xdisp.c `decode_mode_spec` initializes `%l`'s line counter at
    // BUF_BEGV, not the full buffer beginning.  Thus narrowing makes the first
    // accessible line line 1 while `%i` reports the same accessible region.
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context(crate::test_utils::new_context);
    eval.eval_str(
        r#"(progn
  (erase-buffer)
  (insert "zero\none\ntwo\nthree\n")
  (narrow-to-region 6 20)
  (goto-char 14))"#,
    )
    .expect("create narrowed mode-line fixture");

    let rendered =
        builtin_format_mode_line_ctx(&mut eval, &vec![Value::string("%l|%c|%i")]).expect("specs");

    assert_eq!(rendered, Value::string("3|0|14"));
}

#[test]
fn line_pixel_height_on_a_gui_frame_uses_the_height_of_the_row_at_point() {
    crate::test_utils::init_test_tracing();
    let (mut ev, fid, wid, buf) = line_pixel_height_gui_context("a\nb\n", 3);
    commit_line_pixel_height_snapshot(&mut ev, fid, wid, buf);
    assert!(
        ev.fresh_window_display_snapshot(fid, wid, buf).is_some(),
        "the test snapshot must be fresh so the snapshot path answers"
    );

    // GNU's line_bottom_y measures the row, not the glyph at point.
    let result = builtin_line_pixel_height(&mut ev, &[]).unwrap();
    assert_eq!(result, Value::fixnum(30));
}

#[test]
fn line_pixel_height_ignores_rows_laid_out_for_an_older_point() {
    crate::test_utils::init_test_tracing();
    let (mut ev, fid, wid, buf) = line_pixel_height_gui_context("a\nb\n", 3);
    // The rows were laid out with point at 1.  `goto-char` then moves only
    // the buffer's point, not the window's point marker.
    ev.eval_str("(goto-char 1)").unwrap();
    commit_line_pixel_height_snapshot(&mut ev, fid, wid, buf);
    ev.eval_str("(goto-char 3)").unwrap();

    // Point-dependent display (the current line number face) can change a
    // row, so those rows must not answer; with no frontend the answer is the
    // frame's line height, not the stale row's 30.
    let result = builtin_line_pixel_height(&mut ev, &[]).unwrap();
    assert_eq!(result, Value::fixnum(14));
}

#[test]
fn line_pixel_height_lays_out_the_line_at_point_outside_the_window() {
    use crate::window::{WindowLayoutQuery, WindowLayoutQueryOutcome, WindowLayoutQueryScope};
    use std::sync::{Arc, Mutex};

    crate::test_utils::init_test_tracing();
    // Point is inside "bb" on the second source line; there are no current
    // rows (as when point sits before window-start after `goto-char`).
    let (mut ev, _fid, wid, _buf) = line_pixel_height_gui_context("aa\nbb\n", 5);
    let starts = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&starts);
    ev.install_window_layout_query(move |_, _, _, scope| {
        let WindowLayoutQueryScope::Rows { start, .. } = scope else {
            panic!("line-pixel-height lays out rows from point's line, got {scope:?}");
        };
        seen.lock().unwrap().push(start.as_i64());
        let snapshot = crate::window::WindowDisplaySnapshot {
            window_id: wid,
            rows: vec![line_pixel_height_row(0, 0, 30, 4, 6)],
            ..Default::default()
        };
        WindowLayoutQueryOutcome::Ready(WindowLayoutQuery::new(start, Some(snapshot)))
    })
    .expect("install the layout-query handler");

    let result = builtin_line_pixel_height(&mut ev, &[]).unwrap();
    assert_eq!(result, Value::fixnum(30));
    assert_eq!(
        *starts.lock().unwrap(),
        vec![4],
        "rows start at point's line"
    );
}

#[test]
fn line_pixel_height_inside_a_layout_query_falls_back_instead_of_signaling() {
    use crate::window::{WindowLayoutQuery, WindowLayoutQueryOutcome};
    use std::sync::{Arc, Mutex};

    crate::test_utils::init_test_tracing();
    let (mut ev, _fid, wid, _buf) = line_pixel_height_gui_context("aa\nbb\n", 5);
    // Lisp that a layout query runs (a mode-line `:eval`, a fontification
    // function) calls `line-pixel-height` while the outer query is active.
    let nested = Arc::new(Mutex::new(None));
    let seen = Arc::clone(&nested);
    ev.install_window_layout_query(move |ctx, _, _, scope| {
        *seen.lock().unwrap() = Some(
            builtin_line_pixel_height(ctx, &[])
                .map_err(|flow| format!("{flow:?}"))
                .map(|value| value.as_fixnum()),
        );
        let crate::window::WindowLayoutQueryScope::Rows { start, .. } = scope else {
            panic!("expected a Rows query, got {scope:?}");
        };
        let snapshot = crate::window::WindowDisplaySnapshot {
            window_id: wid,
            rows: vec![line_pixel_height_row(0, 0, 30, 4, 6)],
            ..Default::default()
        };
        WindowLayoutQueryOutcome::Ready(WindowLayoutQuery::new(start, Some(snapshot)))
    })
    .expect("install the layout-query handler");

    let result = builtin_line_pixel_height(&mut ev, &[]).unwrap();
    assert_eq!(result, Value::fixnum(30));
    assert_eq!(
        *nested.lock().unwrap(),
        Some(Ok(Some(14))),
        "the nested call answers the frame's line height"
    );
}

#[test]
fn window_text_pixel_size_rejects_snapshot_after_overlay_change() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context(crate::test_utils::new_context);
    eval.frame_manager_mut()
        .selected_frame_mut()
        .expect("selected frame")
        .char_height = 16.0;
    let buf_id = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .id();
    eval.edit
        .buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\nc\n");
    let buffer_modiff = eval
        .edit
        .buffers
        .get(buf_id)
        .expect("buffer")
        .modified_tick();
    let overlay_modiff = eval
        .edit
        .buffers
        .get(buf_id)
        .expect("buffer")
        .overlay_modified_tick();

    let row = |row, y, height, start, end| crate::window::DisplayRowSnapshot {
        row,
        y,
        height,
        start_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(start)),
        end_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(end)),
        ..Default::default()
    };
    let window_id = crate::window::WindowId(selected_window as u64);
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame")
        .id;
    let layout_freshness = eval
        .window_display_snapshot_freshness(frame_id, window_id, buf_id)
        .expect("freshness token");
    neovm_display::test_support::authority::commit_redisplay_cache_for_test(
        eval.frame_manager_mut()
            .selected_frame_mut()
            .expect("selected frame"),
        vec![crate::window::WindowDisplaySnapshot {
            point_rows: None,
            window_id,
            rows: vec![
                row(0, 0, 10, 1, 2),
                row(1, 10, 30, 3, 4),
                row(2, 40, 10, 5, 6),
            ],
            buffer_modiff: Some(buffer_modiff),
            layout_freshness: Some(layout_freshness),
            ..Default::default()
        }],
    );

    eval.eval_str(
        "(let ((overlay (make-overlay 1 2)))\
           (overlay-put overlay 'help-echo \"changed\"))",
    )
    .expect("mutate overlay state after snapshot");
    let buffer = eval.edit.buffers.get(buf_id).expect("buffer");
    assert_eq!(buffer.modified_tick(), buffer_modiff);
    assert_ne!(buffer.overlay_modified_tick(), overlay_modiff);

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(1), Value::fixnum(15)),
            Value::fixnum(7),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(1), Value::fixnum(48), Value::fixnum(1),])
    );
}

#[test]
fn window_text_pixel_size_rejects_snapshot_after_narrowing() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context(crate::test_utils::new_context);
    eval.frame_manager_mut()
        .selected_frame_mut()
        .expect("selected frame")
        .char_height = 16.0;
    let buf_id = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .id();
    eval.edit
        .buffers
        .get_mut(buf_id)
        .expect("buffer")
        .insert("a\nb\nc\n");
    let buffer_modiff = eval
        .edit
        .buffers
        .get(buf_id)
        .expect("buffer")
        .modified_tick();

    let row = |row, y, start, end| crate::window::DisplayRowSnapshot {
        row,
        y,
        height: 10,
        start_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(start)),
        end_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(end)),
        ..Default::default()
    };
    let window_id = crate::window::WindowId(selected_window as u64);
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame")
        .id;
    let layout_freshness = eval
        .window_display_snapshot_freshness(frame_id, window_id, buf_id)
        .expect("freshness token");
    neovm_display::test_support::authority::commit_redisplay_cache_for_test(
        eval.frame_manager_mut()
            .selected_frame_mut()
            .expect("selected frame"),
        vec![crate::window::WindowDisplaySnapshot {
            point_rows: None,
            window_id,
            rows: vec![row(0, 0, 1, 2), row(1, 10, 3, 4), row(2, 20, 5, 6)],
            buffer_modiff: Some(buffer_modiff),
            layout_freshness: Some(layout_freshness),
            ..Default::default()
        }],
    );

    eval.eval_str("(narrow-to-region 3 7)")
        .expect("narrow after snapshot");
    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_none()
    );

    let result = builtin_window_text_pixel_size_ctx(
        &mut eval,
        vec![
            Value::make_window(selected_window as u64),
            Value::cons(Value::fixnum(3), Value::fixnum(-1)),
            Value::fixnum(7),
        ],
    )
    .expect("window-text-pixel-size");

    assert_eq!(
        result,
        Value::list(vec![Value::fixnum(1), Value::fixnum(32), Value::fixnum(3),])
    );
}

#[test]
fn retained_display_rows_reject_local_tab_width_reset() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context(crate::test_utils::new_context);
    eval.eval_str(
        "(set-default 'tab-width 8)
         (set (make-local-variable 'tab-width) 4)
         (insert \"\\t\")",
    )
    .expect("local tab-width and tabbed text");
    // GNU 31.1: current-column changes from 4 to 8 after resetting locals.
    assert_eq!(eval.eval_str("(current-column)").unwrap(), Value::fixnum(4));
    let buffer_id = eval.edit.buffers.current_buffer_id().unwrap();
    let frame_id = eval.frame_manager().selected_frame().unwrap().id;
    let window_id = crate::window::WindowId(selected_window as u64);
    let recorded = eval
        .window_display_snapshot_freshness(frame_id, window_id, buffer_id)
        .unwrap();
    neovm_display::test_support::authority::commit_redisplay_cache_for_test(
        eval.frame_manager_mut().selected_frame_mut().unwrap(),
        vec![crate::window::WindowDisplaySnapshot {
            window_id,
            layout_freshness: Some(recorded.clone()),
            ..Default::default()
        }],
    );
    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buffer_id)
            .is_some()
    );

    eval.eval_str("(kill-all-local-variables)").unwrap();
    // Evict current-column's last-point cache before checking live tab stops.
    eval.eval_str("(goto-char (point-min)) (current-column) (goto-char (point-max))")
        .unwrap();
    assert_eq!(eval.eval_str("(current-column)").unwrap(), Value::fixnum(8));
    let current = eval
        .window_display_snapshot_freshness(frame_id, window_id, buffer_id)
        .unwrap();
    let mut ignoring_presentation = current.clone();
    ignoring_presentation.redisplay_generation = recorded.redisplay_generation;
    // Diagnostic evidence, not a requirement that future freshness fixes
    // must preserve today's missing tab-width invalidation.
    tracing::warn!(
        matches_ignoring_presentation = ignoring_presentation == recorded,
        ?recorded,
        ?current,
        "local tab-width reset changes geometry without editing text"
    );
    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buffer_id)
            .is_none()
    );
}

#[test]
fn retained_display_rows_reject_window_display_table_changes() {
    crate::test_utils::init_test_tracing();
    let (mut eval, selected_window) = pixel_size_tty_context(crate::test_utils::new_context);
    let buf_id = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .id();
    let window_id = crate::window::WindowId(selected_window as u64);
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame")
        .id;
    let display_table = eval
        .eval_str("(make-char-table 'display-table)")
        .expect("make display table");
    eval.frame_manager_mut()
        .set_window_display_table(window_id, display_table);
    let layout_freshness = eval
        .window_display_snapshot_freshness(frame_id, window_id, buf_id)
        .expect("freshness token");
    neovm_display::test_support::authority::commit_redisplay_cache_for_test(
        eval.frame_manager_mut()
            .selected_frame_mut()
            .expect("selected frame"),
        vec![crate::window::WindowDisplaySnapshot {
            point_rows: None,
            window_id,
            layout_freshness: Some(layout_freshness),
            ..Default::default()
        }],
    );
    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_some()
    );

    neovm_text::emacs_core::chartable::builtin_set_char_table_range(
        vec![
            display_table,
            Value::fixnum('a' as i64),
            Value::vector(vec![Value::fixnum('A' as i64)]),
        ],
        None,
    )
    .expect("mutate installed display table in place");
    assert_eq!(
        eval.frame_manager().window_display_table(window_id).bits(),
        display_table.bits()
    );

    assert!(
        eval.fresh_window_display_snapshot(frame_id, window_id, buf_id)
            .is_none()
    );
}

#[test]
fn pos_visible_queries_target_rows_but_last_row_still_queries_the_viewport() {
    use crate::window::{WindowLayoutQueryOutcome, WindowLayoutQueryScope};
    let mut eval = interactive_context(crate::test_utils::new_context);
    let buffer = eval.edit.buffers.current_buffer().unwrap().id();
    eval.edit
        .buffers
        .get_mut(buffer)
        .unwrap()
        .insert("first\nsecond\nthird\n");
    let frame = eval
        .frame_manager_mut()
        .create_frame("target-query", 160, 96, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.eval_str("(goto-char 5)").unwrap();
    let scopes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = scopes.clone();
    eval.install_window_layout_query(move |_, _, _, scope| {
        observed.borrow_mut().push(scope);
        WindowLayoutQueryOutcome::Unavailable
    })
    .expect("install the layout-query handler");
    for pos in [Value::fixnum(3), Value::NIL, Value::T] {
        builtin_pos_visible_in_window_p_ctx(
            &mut eval,
            vec![pos, Value::make_window(window.0), Value::T],
        )
        .unwrap();
    }
    assert_eq!(
        scopes.borrow().as_slice(),
        &[
            WindowLayoutQueryScope::Position {
                target: LispCharPos1::new(3)
            },
            WindowLayoutQueryScope::Position {
                target: LispCharPos1::new(5)
            },
            WindowLayoutQueryScope::Viewport,
        ]
    );
}

#[test]
fn posn_at_point_reuses_warm_rows_for_default_absolute_numbers_after_point_moves() {
    // GNU's default `line-number-current-line` is `(t :inherit line-number)`,
    // so `try_cursor_movement` accepts the point-only shortcut for absolute
    // numbering; this port matches that lface comparison.
    for setup in [
        "(set-default 'display-line-numbers t)",
        "(set-default 'display-line-numbers 'absolute)",
        "(setq display-line-numbers t)",
        "(setq display-line-numbers 'absolute)",
    ] {
        assert_point_only_warm_query_reuse(setup, true);
    }
}

#[test]
fn posn_at_point_requeries_warm_rows_for_distinct_current_line_face() {
    // A taller current-line face moves the row's baseline when point enters
    // it, so the retained rows must not answer (src/xdisp.c:19912-19918).
    let face_setup = "(let ((vector (make-vector 20 'unspecified)))
                         (aset vector 0 'face)
                         (aset vector 4 2)
                         (puthash 'line-number-current-line vector point-query-face-table))";
    for setup in [
        "(set-default 'display-line-numbers t)",
        "(setq display-line-numbers 'absolute)",
    ] {
        assert_point_only_warm_query_reuse_with_face_setup(setup, Some(face_setup), false);
    }
}

#[test]
fn posn_at_point_requeries_warm_rows_after_point_moves_with_default_relative_numbers() {
    use crate::window::{WindowLayoutQueryOutcome, WindowLayoutQueryScope};

    let mut eval = interactive_context(crate::test_utils::new_context);
    let buffer = eval.edit.buffers.current_buffer().unwrap().id();
    eval.edit.buffers.get_mut(buffer).unwrap().insert("a\nb\n");
    let frame = eval
        .frame_manager_mut()
        .create_frame("relative-row-query", 160, 96, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    eval.frame_manager_mut().get_mut(frame).unwrap().initial = false;
    eval.eval_str("(progn (set-default 'display-line-numbers 'relative) (goto-char 1))")
        .unwrap();
    let freshness = eval
        .window_display_snapshot_freshness(frame, window, buffer)
        .unwrap();
    let warm = crate::window::WindowDisplaySnapshot {
        window_id: window,
        points: vec![crate::window::DisplayPointSnapshot {
            role: crate::window::DisplayPointRole::Glyph,
            buffer_pos: LispCharPos1::new(3),
            x: 0,
            y: 16,
            width: 8,
            height: 16,
            row: 1,
            col: 0,
        }],
        rows: vec![crate::window::DisplayRowSnapshot {
            row: 1,
            y: 16,
            height: 16,
            start_buffer_pos: Some(LispCharPos1::new(3)),
            end_buffer_pos: Some(LispCharPos1::new(4)),
            ..Default::default()
        }],
        buffer_modiff: Some(eval.edit.buffers.get(buffer).unwrap().modified_tick()),
        layout_freshness: Some(freshness.clone()),
        ..Default::default()
    };
    neovm_display::test_support::authority::commit_redisplay_cache_for_test(
        eval.frame_manager_mut().get_mut(frame).unwrap(),
        vec![warm],
    );
    let scopes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = scopes.clone();
    eval.install_window_layout_query(move |_, fid, wid, scope| {
        assert_eq!((fid, wid), (frame, window));
        observed.borrow_mut().push(scope);
        WindowLayoutQueryOutcome::Unavailable
    })
    .expect("install the layout-query handler");
    let query = |eval: &mut Context| {
        builtin_posn_at_point(eval, vec![Value::fixnum(3), Value::make_window(window.0)]).unwrap()
    };
    assert!(!query(&mut eval).is_nil());
    assert!(
        scopes.borrow().is_empty(),
        "unchanged warm rows answer directly"
    );

    eval.eval_str("(goto-char 3)").unwrap();
    // Keep the non-selected test window's saved point in sync with buffer point.
    match eval
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
    {
        crate::window::Window::Leaf { point, .. } => *point = LispCharPos1::new(3),
        other => panic!("expected leaf window, got {other:?}"),
    }
    let moved = eval
        .window_display_snapshot_freshness(frame, window, buffer)
        .unwrap();
    assert_ne!(
        freshness, moved,
        "the fixture must change snapshot freshness"
    );
    assert!(freshness.same_position_rows_ignoring_point(&moved));
    query(&mut eval);
    assert_eq!(
        scopes.borrow().as_slice(),
        &[WindowLayoutQueryScope::Position {
            target: LispCharPos1::new(3),
        }],
        "default relative numbering must requery after point moves"
    );
}

#[test]
fn posn_at_point_requeries_warm_rows_after_face_overlay_height_changes_or_moves() {
    use crate::window::{WindowLayoutQuery, WindowLayoutQueryOutcome, WindowLayoutQueryScope};

    for (setup, mutation) in [
        (
            "(setq geometry-overlay (make-overlay 1 2))",
            "(overlay-put geometry-overlay 'face '(:height 2.0))",
        ),
        (
            "(progn (setq geometry-overlay (make-overlay 3 4)) (overlay-put geometry-overlay 'face '(:height 2.0)))",
            "(move-overlay geometry-overlay 1 2)",
        ),
    ] {
        let mut eval = interactive_context(crate::test_utils::new_context);
        let buffer = eval.edit.buffers.current_buffer().unwrap().id();
        eval.edit.buffers.get_mut(buffer).unwrap().insert("a\nb\n");
        let frame = eval
            .frame_manager_mut()
            .create_frame("overlay-row-query", 160, 96, buffer);
        let window = eval.frame_manager().get(frame).unwrap().selected_window;
        eval.frame_manager_mut().get_mut(frame).unwrap().initial = false;
        eval.eval_str(setup).unwrap();
        let buffer_modiff = eval.edit.buffers.get(buffer).unwrap().modified_tick();
        let overlay_modiff = eval
            .edit
            .buffers
            .get(buffer)
            .unwrap()
            .overlay_modified_tick();
        let freshness = eval
            .window_display_snapshot_freshness(frame, window, buffer)
            .unwrap();
        let snapshot = |y| crate::window::WindowDisplaySnapshot {
            window_id: window,
            points: vec![crate::window::DisplayPointSnapshot {
                role: crate::window::DisplayPointRole::Glyph,
                buffer_pos: LispCharPos1::new(3),
                x: 0,
                y,
                width: 8,
                height: 16,
                row: 1,
                col: 0,
            }],
            rows: vec![crate::window::DisplayRowSnapshot {
                row: 1,
                y,
                height: 16,
                start_buffer_pos: Some(LispCharPos1::new(3)),
                end_buffer_pos: Some(LispCharPos1::new(4)),
                ..Default::default()
            }],
            buffer_modiff: Some(buffer_modiff),
            ..Default::default()
        };
        let mut warm = snapshot(16);
        warm.layout_freshness = Some(freshness);
        neovm_display::test_support::authority::commit_redisplay_cache_for_test(
            eval.frame_manager_mut().get_mut(frame).unwrap(),
            vec![warm],
        );
        let refreshed = snapshot(32);
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let observed = calls.clone();
        eval.install_window_layout_query(move |_, fid, wid, scope| {
            assert_eq!((fid, wid), (frame, window));
            assert_eq!(
                scope,
                WindowLayoutQueryScope::Position {
                    target: LispCharPos1::new(3),
                }
            );
            observed.set(observed.get() + 1);
            WindowLayoutQueryOutcome::Ready(WindowLayoutQuery::new(
                LispCharPos1::ONE,
                Some(refreshed.clone()),
            ))
        })
        .expect("install the layout-query handler");
        let query = |eval: &mut Context| {
            builtin_posn_at_point(eval, vec![Value::fixnum(3), Value::make_window(window.0)])
                .unwrap()
                .cons_cdr()
                .cons_cdr()
                .cons_car()
        };
        assert_eq!(
            query(&mut eval),
            Value::cons(Value::fixnum(0), Value::fixnum(16))
        );
        assert_eq!(calls.get(), 0, "unchanged warm rows must answer directly");
        eval.eval_str("(goto-char 1)").unwrap();
        assert_eq!(
            query(&mut eval),
            Value::cons(Value::fixnum(0), Value::fixnum(16))
        );
        assert_eq!(calls.get(), 0, "point-only changes must retain warm rows");

        // Face-only overlay changes preserve text but can double the preceding
        // row's height; retained render-plan reuse cannot validate query rows.
        eval.eval_str(mutation).unwrap();
        let live = eval.edit.buffers.get(buffer).unwrap();
        assert_eq!(live.modified_tick(), buffer_modiff);
        assert_ne!(live.overlay_modified_tick(), overlay_modiff);
        assert_eq!(
            query(&mut eval),
            Value::cons(Value::fixnum(0), Value::fixnum(32)),
            "{mutation}"
        );
        assert_eq!(calls.get(), 1, "stale rows must query layout: {mutation}");
    }
}

#[test]
fn posn_at_x_y_decodes_frame_or_window_the_way_gnu_dispatches_it() {
    // GNU `Fposn_at_x_y` (src/keyboard.c) dispatches on WINDOWP, not on
    // "is it a frame":
    //
    //     if (NILP (frame_or_window)) frame_or_window = selected_window;
    //     if (WINDOWP (frame_or_window))
    //       { struct window *w = decode_live_window (frame_or_window);
    //         ... ; frame_or_window = w->frame; }
    //     CHECK_LIVE_FRAME (frame_or_window);
    //
    // So a WINDOW-shaped argument is checked against `window-live-p` -- which
    // rejects internal and deleted windows -- and everything else falls
    // through to `frame-live-p`.  Measured on GNU Emacs 31.1:
    //
    //     nil   cons                 live   cons
    //     mini  cons                 frame  cons
    //     internal ERR:window-live-p dead   ERR:window-live-p
    //     symbol   ERR:frame-live-p  buffer ERR:frame-live-p
    //
    // Neomacs guarded the window branch on "not a frame" rather than "is a
    // window", so anything that is NEITHER -- a symbol, a buffer, a deleted
    // window -- entered the window decoder and reported its predicate.
    //
    // The predicate lives in the signal DATA, so this drives the subr from
    // Lisp: `(car (cdr err))` is the only place the two arms are visibly
    // different, and asserting the condition alone (`wrong-type-argument`)
    // passes on both the correct and the broken implementation.
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context(crate::test_utils::new_context);
    let out = eval
        .eval_str_each(
            "(progn ((lambda (old size side normal) (set-window-new-pixel old (- (if (memq side '(t left right)) (window-pixel-width old) (window-pixel-height old)) size)) (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil) t)
         (if (consp (posn-at-x-y 0 0 nil)) 'cons 'other)
         (if (consp (posn-at-x-y 0 0 (selected-window))) 'cons 'other)
         (if (consp (posn-at-x-y 0 0 (minibuffer-window))) 'cons 'other)
         (if (consp (posn-at-x-y 0 0 (selected-frame))) 'cons 'other)
         (condition-case err (posn-at-x-y 0 0 (window-parent (selected-window)))
           (error (car (cdr err))))
         (let ((doomed ((lambda (old size side normal) (set-window-new-pixel old (- (if (memq side '(t left right)) (window-pixel-width old) (window-pixel-height old)) size)) (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
           (delete-window-internal doomed)
           (condition-case err (posn-at-x-y 0 0 doomed) (error (car (cdr err)))))
         (condition-case err (posn-at-x-y 0 0 'foo) (error (car (cdr err))))
         (condition-case err (posn-at-x-y 0 0 (current-buffer)) (error (car (cdr err))))",
        )
        .iter()
        .map(|result| match result {
            Ok(value) => format!("OK {}", neovm_eval::emacs_core::print::print_value(value)),
            Err(err) => format!("ERR {err:?}"),
        })
        .collect::<Vec<_>>();

    assert_eq!(out[1], "OK cons", "nil means the selected window");
    assert_eq!(out[2], "OK cons", "a live window is accepted");
    assert_eq!(out[3], "OK cons", "so is the minibuffer window");
    assert_eq!(
        out[4], "OK cons",
        "a live FRAME is accepted -- this is why the argument is FRAME-OR-WINDOW"
    );
    assert_eq!(
        out[5], "OK window-live-p",
        "an internal window IS a window, so it is rejected by decode_live_window"
    );
    assert_eq!(
        out[6], "OK window-live-p",
        "and so is a deleted window -- not framep"
    );
    assert_eq!(
        out[7], "OK frame-live-p",
        "a symbol is not a window, so it falls through to CHECK_LIVE_FRAME"
    );
    assert_eq!(
        out[8], "OK frame-live-p",
        "nor is a buffer -- neither may reach the window decoder"
    );
}

#[test]
fn test_vertical_motion_eval_uses_live_redisplay_rows() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context(crate::test_utils::new_context);
    let buf_id = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("xdisp-vertical-motion-rows", 160, 96, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let buf = eval.edit.buffers.get_mut(buf_id).expect("buffer");
        buf.insert(&"a\n".repeat(100));
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    }
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        neovm_display::test_support::authority::commit_redisplay_cache_for_test(
            frame,
            vec![crate::window::WindowDisplaySnapshot {
                point_rows: None,
                window_id: selected_window,
                rows: vec![
                    crate::window::DisplayRowSnapshot {
                        truncated_end_buffer_pos: None,
                        row: 0,
                        y: 0,
                        height: 16,
                        extra_line_spacing: 0,
                        start_x: 0,
                        start_col: 0,
                        end_x: 8,
                        end_col: 1,
                        start_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(1)),
                        end_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(1)),
                        end_source: Default::default(),
                        fringe: Default::default(),
                    },
                    crate::window::DisplayRowSnapshot {
                        truncated_end_buffer_pos: None,
                        row: 1,
                        y: 16,
                        height: 16,
                        extra_line_spacing: 0,
                        start_x: 0,
                        start_col: 0,
                        end_x: 8,
                        end_col: 1,
                        start_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(40)),
                        end_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(40)),
                        end_source: Default::default(),
                        fringe: Default::default(),
                    },
                    crate::window::DisplayRowSnapshot {
                        truncated_end_buffer_pos: None,
                        row: 2,
                        y: 32,
                        height: 16,
                        extra_line_spacing: 0,
                        start_x: 0,
                        start_col: 0,
                        end_x: 8,
                        end_col: 1,
                        start_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(80)),
                        end_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(80)),
                        end_source: Default::default(),
                        fringe: Default::default(),
                    },
                ],
                ..crate::window::WindowDisplaySnapshot::default()
            }],
        );
    }

    let result = eval
        .eval_str("(progn (goto-char 1) (list (vertical-motion 2) (point)))")
        .unwrap();
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&result),
        "(2 80)"
    );
}

#[test]
fn test_vertical_motion_eval_uses_live_redisplay_goal_column() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context(crate::test_utils::new_context);
    let buf_id = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("xdisp-vertical-motion-column", 160, 96, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let buf = eval.edit.buffers.get_mut(buf_id).expect("buffer");
        buf.insert(&"a\n".repeat(100));
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    }
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        neovm_display::test_support::authority::commit_redisplay_cache_for_test(
            frame,
            vec![crate::window::WindowDisplaySnapshot {
                point_rows: None,
                window_id: selected_window,
                points: vec![
                    crate::window::DisplayPointSnapshot {
                        role: crate::window::DisplayPointRole::Glyph,
                        buffer_pos: neovm_edit::buffer::LispCharPos1::new(40),
                        x: 0,
                        y: 16,
                        width: 8,
                        height: 16,
                        row: 1,
                        col: 0,
                    },
                    crate::window::DisplayPointSnapshot {
                        role: crate::window::DisplayPointRole::Glyph,
                        buffer_pos: neovm_edit::buffer::LispCharPos1::new(43),
                        x: 24,
                        y: 16,
                        width: 8,
                        height: 16,
                        row: 1,
                        col: 3,
                    },
                    crate::window::DisplayPointSnapshot {
                        role: crate::window::DisplayPointRole::Glyph,
                        buffer_pos: neovm_edit::buffer::LispCharPos1::new(45),
                        x: 40,
                        y: 16,
                        width: 8,
                        height: 16,
                        row: 1,
                        col: 5,
                    },
                ],
                rows: vec![
                    crate::window::DisplayRowSnapshot {
                        truncated_end_buffer_pos: None,
                        row: 0,
                        y: 0,
                        height: 16,
                        extra_line_spacing: 0,
                        start_x: 0,
                        start_col: 0,
                        end_x: 8,
                        end_col: 1,
                        start_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(1)),
                        end_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(1)),
                        end_source: Default::default(),
                        fringe: Default::default(),
                    },
                    crate::window::DisplayRowSnapshot {
                        truncated_end_buffer_pos: None,
                        row: 1,
                        y: 16,
                        height: 16,
                        extra_line_spacing: 0,
                        start_x: 0,
                        start_col: 0,
                        end_x: 48,
                        end_col: 6,
                        start_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(40)),
                        end_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(45)),
                        end_source: Default::default(),
                        fringe: Default::default(),
                    },
                ],
                ..crate::window::WindowDisplaySnapshot::default()
            }],
        );
    }

    let result = eval
        .eval_str("(progn (goto-char 1) (list (vertical-motion (cons 3 1)) (point)))")
        .unwrap();
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&result),
        "(1 43)"
    );
}

#[test]
fn test_vertical_motion_goal_column_past_row_end_lands_on_the_row_end_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = interactive_context(crate::test_utils::new_context);
    let buf_id = eval
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("xdisp-vertical-motion-row-end", 160, 96, buf_id);
    let selected_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let buf = eval.edit.buffers.get_mut(buf_id).expect("buffer");
        buf.insert("hello world\nsecond\n");
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    }
    {
        // "hello world" is 11 glyphs at columns 0..=10; the newline at position
        // 12 draws nothing, so the row's pen stops at column 11.
        let points = (0..11)
            .map(|index| crate::window::DisplayPointSnapshot {
                role: crate::window::DisplayPointRole::Glyph,
                buffer_pos: neovm_edit::buffer::LispCharPos1::new(1 + index),
                x: 8 * index,
                y: 0,
                width: 8,
                height: 16,
                row: 0,
                col: index,
            })
            .collect::<Vec<_>>();
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        neovm_display::test_support::authority::commit_redisplay_cache_for_test(
            frame,
            vec![crate::window::WindowDisplaySnapshot {
                point_rows: None,
                window_id: selected_window,
                points,
                rows: vec![crate::window::DisplayRowSnapshot {
                    truncated_end_buffer_pos: None,
                    row: 0,
                    y: 0,
                    height: 16,
                    extra_line_spacing: 0,
                    start_x: 0,
                    start_col: 0,
                    end_x: 88,
                    end_col: 11,
                    start_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(1)),
                    end_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(12)),
                    end_source: Default::default(),
                    fringe: Default::default(),
                }],
                ..crate::window::WindowDisplaySnapshot::default()
            }],
        );
    }

    // A goal column beyond the row: GNU stops at the newline (position 12).
    let past_end = eval
        .eval_str("(progn (goto-char 3) (list (vertical-motion (cons 40 0)) (point)))")
        .unwrap();
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&past_end),
        "(0 12)"
    );

    // A goal column inside the row still selects the glyph at that column.
    let inside = eval
        .eval_str("(progn (goto-char 3) (list (vertical-motion (cons 6 0)) (point)))")
        .unwrap();
    assert_eq!(neovm_eval::emacs_core::print::print_value(&inside), "(0 7)");
}

#[test]
fn frame_snapshot_errors_without_display_hook() {
    let mut eval = crate::test_utils::new_context();
    assert!(
        eval.eval_str("(neomacs--frame-snapshot)").is_err(),
        "must signal in batch mode (no frontend hook installed)"
    );
}

#[test]
fn frame_snapshot_forwards_request_to_installed_hook() {
    use crate::emacs_core::xdisp::{SnapshotFormat, SnapshotTarget};
    use std::cell::RefCell;
    use std::rc::Rc;

    let mut eval = crate::test_utils::new_context();
    let seen: Rc<RefCell<Vec<(SnapshotTarget, SnapshotFormat)>>> = Rc::default();
    let sink = seen.clone();
    eval.frame_snapshot_fn = Some(Box::new(move |_eval, request| {
        sink.borrow_mut().push((request.target, request.format));
        Ok("SNAP".to_string())
    }));

    let value = eval
        .eval_str("(neomacs--frame-snapshot)")
        .expect("selected/text snapshot");
    assert_eq!(snapshot_string(value), "SNAP");
    eval.eval_str("(neomacs--frame-snapshot t 'json)")
        .expect("all/json snapshot");
    eval.eval_str("(neomacs--frame-snapshot nil 'text-faces)")
        .expect("selected/text-faces snapshot");
    eval.eval_str("(neomacs--frame-snapshot t 'json-geometry)")
        .expect("all/geometry snapshot");

    assert_eq!(
        *seen.borrow(),
        vec![
            (SnapshotTarget::Selected, SnapshotFormat::Text),
            (SnapshotTarget::All, SnapshotFormat::Json),
            (SnapshotTarget::Selected, SnapshotFormat::TextFaces),
            (SnapshotTarget::All, SnapshotFormat::JsonGeometry),
        ]
    );
}

#[test]
fn write_frame_snapshot_writes_file_and_returns_t() {
    let mut eval = crate::test_utils::new_context();
    eval.frame_snapshot_fn = Some(Box::new(|_, _| Ok("SNAPSHOT-CONTENT".to_string())));
    let path = std::env::temp_dir().join(format!(
        "neomacs-frame-snapshot-test-{}.txt",
        std::process::id()
    ));
    let form = format!(
        "(neomacs--write-frame-snapshot {:?} nil 'text)",
        path.to_str().expect("utf8 temp path")
    );
    let value = eval.eval_str(&form).expect("write snapshot");
    assert!(value.is_t());
    assert_eq!(
        std::fs::read_to_string(&path).expect("artifact readable"),
        "SNAPSHOT-CONTENT"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn line_number_anchor_counts_from_recent_line_and_survives_edits_below() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let text = "line\n".repeat(2000);
    eval.eval_str(&format!(
        // `switch-to-buffer' is lisp/window.el:9558 and has no subr any more
        // (DIVERGENCES.md 154); this test only needs the buffer current, which
        // is `set-buffer' (src/buffer.c:2416) -- one of the three C primitives
        // GNU's `switch-to-buffer' body calls.
        "(progn (set-buffer (get-buffer-create \"anchor-test\")) (insert {:?}))",
        text
    ))
    .expect("setup");
    let point_at = |eval: &Context, charpos: usize| {
        let buf = eval.edit.buffers.current_buffer().expect("buffer");
        buf.char_pos_to_emacs_byte_pos_clamped(neovm_edit::buffer::CharPos0::new(charpos))
    };
    let buf = eval.edit.buffers.current_buffer().expect("buffer");
    // First computation seeds the anchor at point's line (5000 chars / 5 = 1000 newlines).
    let p = point_at(&eval, 5000);
    assert_eq!(
        prefix_line_and_column(buf, buf.accessible_emacs_byte_region(), p),
        1001
    );
    assert!(buf.line_number_anchor().get().is_some(), "anchor seeded");
    // Simulate the accepted-frame ack, then an edit BELOW the anchor: the
    // anchor stays valid and the count stays exact.
    buf.reset_unchanged_region();
    eval.eval_str("(progn (goto-char 9000) (insert \"x\"))")
        .expect("edit below");
    let buf = eval.edit.buffers.current_buffer().expect("buffer");
    let p = point_at(&eval, 5000);
    assert_eq!(
        prefix_line_and_column(buf, buf.accessible_emacs_byte_region(), p),
        1001
    );
    // An edit ABOVE the anchor invalidates it: a newline inserted at the
    // start must shift the count, never show a stale number.
    eval.eval_str("(progn (goto-char 1) (insert \"\\n\"))")
        .expect("edit above");
    let buf = eval.edit.buffers.current_buffer().expect("buffer");
    let p = point_at(&eval, 5001);
    assert_eq!(
        prefix_line_and_column(buf, buf.accessible_emacs_byte_region(), p),
        1002
    );
}
