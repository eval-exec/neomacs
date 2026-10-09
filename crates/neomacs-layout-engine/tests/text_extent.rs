//! Offscreen source projection through the public row-query API.
use neomacs_layout_engine::WindowLayoutQueryEngine;
use neovm_core::{
    buffer::LispCharPos1,
    emacs_core::{Context, Value},
    window::WindowLayoutQueryScope,
};

#[test]
fn text_extent_end_and_geometry_belong_to_projected_source() {
    let mut eval = Context::new();
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
