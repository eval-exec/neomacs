//! Offscreen source projection through the public row-query API.
use neomacs_layout_engine::WindowLayoutQueryEngine;
use neovm_core::window::DisplayFrameManagerExt;
use neovm_core::{
    buffer::LispCharPos1,
    emacs_core::{Context, Value},
    window::WindowLayoutQueryScope,
};

#[test]
fn text_extent_end_and_geometry_belong_to_projected_source() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let live = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(live)
        .unwrap()
        .insert(&"live-window\n".repeat(20));
    let source = eval.buffer_manager_mut().create_buffer("offscreen");
    eval.buffer_manager_mut()
        .get_mut(source)
        .unwrap()
        .insert("abcd efgh ijkl");
    let frame = eval
        .frame_manager_mut()
        .create_frame("text-extent", 400, 240, live);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("neomacs")));
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    let before = eval.eval_str("(list (window-buffer) (window-start) (window-point) (window-hscroll) (window-vscroll nil t))").unwrap();
    let end = LispCharPos1::new(5);
    let mut engine = WindowLayoutQueryEngine::new_without_font_metrics();
    let query = engine
        .query_window_layout(
            &mut eval,
            frame,
            window,
            WindowLayoutQueryScope::TextExtent {
                buffer: source,
                start: LispCharPos1::ONE,
                end,
                width: None,
                height: None,
            },
        )
        .unwrap();
    assert_eq!(
        query.end(),
        end,
        "query end belongs to offscreen clipped source"
    );
    let snapshot = query.geometry().expect("source geometry");
    assert!(!snapshot.rows.is_empty());
    assert!(snapshot.iter_points().all(|point| point.buffer_pos <= end));
    assert_eq!(eval.eval_str("(list (window-buffer) (window-start) (window-point) (window-hscroll) (window-vscroll nil t))").unwrap(), before);
    let source = eval.buffer_manager_mut().create_buffer("tall-offscreen");
    eval.buffer_manager_mut()
        .get_mut(source)
        .unwrap()
        .insert(&"a\n".repeat(30));
    let end = LispCharPos1::new(61);
    let tall = engine
        .query_window_layout(
            &mut eval,
            frame,
            window,
            WindowLayoutQueryScope::TextExtent {
                buffer: source,
                start: LispCharPos1::ONE,
                end,
                width: None,
                height: None,
            },
        )
        .unwrap();
    assert_eq!(tall.end(), end);
    assert!(
        tall.geometry().unwrap().rows.len() >= 30,
        "source measurement must extend past physical viewport"
    );

    assert!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .is_none(),
        "query must not publish a frame"
    );
}

#[test]
fn window_text_pixel_size_measure_path_separates_rewind_edge_from_x_limit() {
    use neovm_core::window::display::WindowDisplayContextExt;
    use std::cell::RefCell;
    use std::rc::Rc;

    let run_measured = |window_system: Option<&'static str>,
                        setup: &dyn Fn(&mut Context),
                        from: i64,
                        to: Option<i64>,
                        x_limit: Option<i64>,
                        y_limit: Option<i64>,
                        require_query: bool|
     -> (i64, i64) {
        let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
        let buf = eval.buffer_manager().current_buffer().unwrap().id();
        setup(&mut eval);
        let frame = eval
            .frame_manager_mut()
            .create_frame("text-extent-80", 80, 24, buf);
        {
            let frame_mut = eval.frame_manager_mut().get_mut(frame).unwrap();
            frame_mut.char_width = 1.0;
            frame_mut.char_height = 1.0;
            frame_mut.font_pixel_size = 1.0;
            frame_mut.set_window_system(window_system.map(Value::symbol));
        }
        let engine = Rc::new(RefCell::new(
            WindowLayoutQueryEngine::new_without_font_metrics(),
        ));
        let calls = Rc::new(std::cell::Cell::new(0usize));
        let handler_engine = Rc::clone(&engine);
        let handler_calls = Rc::clone(&calls);
        eval.install_window_layout_query(move |ctx, fid, wid, scope| {
            handler_calls.set(handler_calls.get() + 1);
            match handler_engine
                .borrow_mut()
                .query_window_layout(ctx, fid, wid, scope)
            {
                Ok(query) => neovm_core::window::WindowLayoutQueryOutcome::Ready(query),
                Err(err) => neovm_core::window::WindowLayoutQueryOutcome::Failed(err),
            }
        })
        .expect("install headless layout query handler");
        let to_arg = to.map_or_else(|| "nil".to_string(), |v| v.to_string());
        let xlim_arg = x_limit.map_or_else(|| "nil".to_string(), |v| v.to_string());
        let ylim_arg = y_limit.map_or_else(|| "nil".to_string(), |v| v.to_string());
        let expr = format!("(window-text-pixel-size nil {from} {to_arg} {xlim_arg} {ylim_arg})");
        let res = eval.eval_str(&expr).expect("window-text-pixel-size");
        if require_query {
            assert!(
                calls.get() > 0,
                "expected headless TextMeasurement::measure handler to be invoked"
            );
        }
        (
            res.cons_car().as_int().expect("width"),
            res.cons_cdr().as_int().expect("height"),
        )
    };
    let text_setup = |text: String, truncate: bool| {
        move |eval: &mut Context| {
            let buf = eval.buffer_manager().current_buffer().unwrap().id();
            eval.buffer_manager_mut()
                .get_mut(buf)
                .unwrap()
                .insert(&text);
            if truncate {
                eval.eval_str("(setq truncate-lines t)")
                    .expect("set buffer-local truncate-lines");
            }
        }
    };

    for ws in [None, Some("neomacs")] {
        let label = ws.unwrap_or("tty");
        // GNU: (-1 . 1) — truncate-lines prefix > X-LIMIT clamps before subtracting origin_x.
        assert_eq!(
            run_measured(
                ws,
                &text_setup("abcdefghij".into(), true),
                6,
                None,
                Some(4),
                None,
                true,
            ),
            (-1, 1),
            "GNU: (-1 . 1) on {label} TextMeasurement::measure for truncate-lines prefix > X-LIMIT"
        );
        // GNU: (4 . 2) — truncate-lines prefix > X-LIMIT followed by a second line.
        assert_eq!(
            run_measured(
                ws,
                &text_setup("abcdefghij\nxy".into(), true),
                6,
                None,
                Some(4),
                None,
                true,
            ),
            (4, 2),
            "GNU: (4 . 2) on {label} TextMeasurement::measure for multiline truncate-lines prefix > X-LIMIT"
        );
        // GNU: (4 . 3) and TO=7 (4 . 2) — plain prefix > X-LIMIT rewinds under body_edge first.
        assert_eq!(
            run_measured(
                ws,
                &text_setup("abcdefghij".into(), false),
                6,
                None,
                Some(4),
                None,
                true,
            ),
            (4, 3),
            "GNU: (4 . 3) on {label} TextMeasurement::measure for plain prefix > X-LIMIT"
        );
        assert_eq!(
            run_measured(
                ws,
                &text_setup("abcdefghij".into(), false),
                6,
                Some(7),
                Some(4),
                None,
                true,
            ),
            (4, 2),
            "GNU: (4 . 2) on {label} TextMeasurement::measure for plain prefix > X-LIMIT TO=7"
        );
        // N1: origin_is_newline && origin_x >= X-LIMIT with Y-LIMIT=1 and Y-LIMIT=2.
        assert_eq!(
            run_measured(
                ws,
                &text_setup("abcdefgh\nxy".into(), false),
                9,
                None,
                Some(4),
                Some(1),
                true,
            ),
            (2, 1),
            "GNU: (2 . 1) on {label} TextMeasurement::measure for N1 newline origin X-LIMIT=4 Y-LIMIT=1"
        );
        assert_eq!(
            run_measured(
                ws,
                &text_setup("abcdefgh\nxy".into(), false),
                9,
                None,
                Some(4),
                Some(2),
                true,
            ),
            (2, 2),
            "GNU: (2 . 2) on {label} TextMeasurement::measure for N1 newline origin X-LIMIT=4 Y-LIMIT=2"
        );
        // N2: mid-line single-line before-string overlay at FROM must not be double-counted.
        let ov_mid_setup = |eval: &mut Context| {
            let buf = eval.buffer_manager().current_buffer().unwrap().id();
            eval.buffer_manager_mut()
                .get_mut(buf)
                .unwrap()
                .insert("abhello");
            eval.eval_str(
                r#"(let ((o (make-overlay 3 3))) (overlay-put o 'before-string "ZZZZ"))"#,
            )
            .expect("install mid-line before-string overlay");
        };
        assert_eq!(
            run_measured(ws, &ov_mid_setup, 3, None, Some(4), None, false),
            (4, 3),
            "GNU: (4 . 3) on {label} TextMeasurement::measure for N2 plain-ov-mid"
        );
        assert_eq!(
            run_measured(ws, &ov_mid_setup, 3, Some(4), Some(4), None, false),
            (4, 2),
            "GNU: (4 . 2) on {label} TextMeasurement::measure for N2 plain-ov-mid-to4"
        );
        // N3: FROM == TO with origin_x >= X-LIMIT: wrapping -> (4 . 1), truncating -> (0 . 1) / (-1 . 1).
        assert_eq!(
            run_measured(
                ws,
                &text_setup("abcdefghij".into(), false),
                5,
                Some(5),
                Some(4),
                None,
                true,
            ),
            (4, 1),
            "GNU: (4 . 1) on {label} TextMeasurement::measure for N3 FROM==TO=5 wrapping X-LIMIT=4"
        );
        assert_eq!(
            run_measured(
                ws,
                &text_setup("abcdefghij".into(), false),
                6,
                Some(6),
                Some(4),
                None,
                true,
            ),
            (4, 1),
            "GNU: (4 . 1) on {label} TextMeasurement::measure for N3 FROM==TO=6 wrapping X-LIMIT=4"
        );
        assert_eq!(
            run_measured(
                ws,
                &text_setup("abcdefghij".into(), true),
                5,
                Some(5),
                Some(4),
                None,
                true,
            ),
            (0, 1),
            "GNU: (0 . 1) on {label} TextMeasurement::measure for N3 FROM==TO=5 truncating X-LIMIT=4"
        );
        assert_eq!(
            run_measured(
                ws,
                &text_setup("abcdefghij".into(), true),
                6,
                Some(6),
                Some(4),
                None,
                true,
            ),
            (-1, 1),
            "GNU: (-1 . 1) on {label} TextMeasurement::measure for N3 FROM==TO=6 truncating X-LIMIT=4"
        );
    }
    // TTY body_edge = 79 (80 - 1 continuation column): prefix wraps at 79 before X-LIMIT=4.
    let s82 = format!("{}bcdef", "a".repeat(82));
    assert_eq!(
        run_measured(
            None,
            &text_setup(s82.clone(), false),
            83,
            Some(85),
            Some(4),
            None,
            true,
        ),
        (4, 2),
        "GNU: (4 . 2) on tty TextMeasurement::measure for 82 a's wrap at body_edge=79 TO=85 X-LIMIT=4"
    );
    assert_eq!(
        run_measured(None, &text_setup(s82, false), 83, None, Some(4), None, true,),
        (4, 2),
        "GNU: (4 . 2) on tty TextMeasurement::measure for 82 a's wrap at body_edge=79 X-LIMIT=4"
    );
    let s85 = format!("{}bcdef", "a".repeat(85));
    assert_eq!(
        run_measured(None, &text_setup(s85, false), 86, None, Some(4), None, true,),
        (4, 3),
        "GNU: (4 . 3) on tty TextMeasurement::measure for 85 a's wrap at body_edge=79 X-LIMIT=4"
    );
}
