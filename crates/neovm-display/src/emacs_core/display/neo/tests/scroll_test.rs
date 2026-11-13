use super::*;

#[test]
fn resolved_scroll_observer_sees_complete_commit_and_current_command_only() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = eval.buffer_manager_mut().create_buffer("preview");
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("one\ntwo\nthree\n");
    let frame = eval
        .frame_manager_mut()
        .create_frame("preview", 800, 600, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = calls.clone();
    eval.scroll_preview_fn = Some(Box::new(move |eval, frame, window, inputs| {
        let state = eval
            .frame_manager()
            .get(frame)
            .unwrap()
            .find_window(window)
            .unwrap()
            .redisplay_state()
            .unwrap();
        assert_eq!(state.window_start.as_i64(), 5);
        assert_eq!(state.point.as_i64(), 5);
        assert_eq!(state.vscroll, -4);
        assert_eq!(inputs.len(), 1);
        observed.set(observed.get() + 1);
    }));
    let update = || crate::window::WindowScrollUpdate {
        frame,
        window,
        buffer,
        start: neovm_edit::buffer::LispCharPos1::new(5),
        point: neovm_edit::buffer::LispCharPos1::new(5),
        hidden_top_pixels: 4,
    };
    update().commit(&mut eval).unwrap();
    assert_eq!(calls.get(), 0);
    let stream = neomacs_display_protocol::input_progress::InputStream::default();
    let command = eval.input_progress.begin_command();
    eval.input_progress.consumed(stream.issue().unwrap());
    update().commit(&mut eval).unwrap();
    assert_eq!(calls.get(), 1);
    drop(command);
    update().commit(&mut eval).unwrap();
    assert_eq!(calls.get(), 1);
}
