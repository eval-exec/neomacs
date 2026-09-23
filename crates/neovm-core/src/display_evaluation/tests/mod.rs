use crate::emacs_core::Context;

#[test]
fn frame_display_context_selects_target_window_buffer_and_restores_caller() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let caller_buffer = eval.buffer_manager_mut().create_buffer("display-caller");
    let target_buffer = eval.buffer_manager_mut().create_buffer("display-target");
    let caller_frame =
        eval.frame_manager_mut()
            .create_frame("display-caller", 800, 600, caller_buffer);
    let target_frame =
        eval.frame_manager_mut()
            .create_frame("display-target", 800, 600, target_buffer);
    assert!(eval.frame_manager_mut().select_frame(caller_frame));
    assert!(
        eval.buffer_manager_mut()
            .switch_current_unrecorded(caller_buffer)
    );

    let observed = eval
        .with_frame_display_context(target_frame, |eval| {
            (
                eval.frame_manager().selected_frame().map(|frame| frame.id),
                eval.buffer_manager().current_buffer_id(),
            )
        })
        .expect("live target frame context");

    assert_eq!(observed, (Some(target_frame), Some(target_buffer)));
    assert_eq!(
        eval.frame_manager().selected_frame().map(|frame| frame.id),
        Some(caller_frame)
    );
    assert_eq!(
        eval.buffer_manager().current_buffer_id(),
        Some(caller_buffer)
    );
}

#[test]
fn frame_display_context_does_not_restore_a_deleted_caller_frame() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let caller_buffer = eval
        .buffer_manager_mut()
        .create_buffer("deleted-frame-caller");
    let target_buffer = eval
        .buffer_manager_mut()
        .create_buffer("deleted-frame-target");
    let caller_frame =
        eval.frame_manager_mut()
            .create_frame("deleted-frame-caller", 800, 600, caller_buffer);
    let target_frame =
        eval.frame_manager_mut()
            .create_frame("deleted-frame-target", 800, 600, target_buffer);
    assert!(eval.frame_manager_mut().select_frame(caller_frame));

    eval.with_frame_display_context(target_frame, |eval| {
        assert!(
            eval.frame_manager_mut()
                .delete_frame(caller_frame)
                .was_deleted()
        );
    })
    .expect("live target frame context");

    let selected = eval
        .frame_manager()
        .selected_frame()
        .expect("restoration must choose a live frame");
    assert_eq!(selected.id, target_frame);
}

#[test]
fn frame_display_context_does_not_restore_a_deleted_target_window() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let first_buffer = eval
        .buffer_manager_mut()
        .create_buffer("deleted-window-first");
    let second_buffer = eval
        .buffer_manager_mut()
        .create_buffer("deleted-window-second");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("deleted-window", 800, 600, first_buffer);
    let first_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let second_window = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            first_window,
            crate::window::SplitDirection::Horizontal,
            second_buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("split target window");
    assert!(eval.frame_manager_mut().select_frame(frame_id));

    eval.with_frame_display_context(frame_id, |eval| {
        assert!(
            eval.frame_manager_mut()
                .delete_window(frame_id, first_window)
        );
    })
    .expect("live target frame context");

    let frame = eval.frame_manager().get(frame_id).expect("live frame");
    assert_eq!(frame.selected_window, second_window);
    assert!(frame.find_window(frame.selected_window).is_some());
}
