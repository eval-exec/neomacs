use neomacs_display_protocol::types::Color;
use neomacs_layout_engine::engine::{FrameLayoutAttempt, LayoutEngine};
use neovm_core::emacs_core::{Context, Value};
use neovm_core::window::FrameParam;

#[test]
fn child_frame_keeps_border_properties_and_exact_pixel_placement() {
    let mut evaluator = Context::new();
    let buffer_id = evaluator
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let parent_id = evaluator
        .frame_manager_mut()
        .create_frame("parent", 800, 600, buffer_id);
    let child_id = evaluator
        .frame_manager_mut()
        .create_frame("child", 401, 121, buffer_id);

    {
        let child = evaluator
            .frame_manager_mut()
            .get_mut(child_id)
            .expect("child frame");
        child.set_window_system(Some(Value::symbol("neo")));
        child.parent_frame = Value::make_frame(parent_id.0);
        child.set_parameter(Value::symbol("child-frame-border-width"), Value::fixnum(3));
        child.set_parameter(Value::symbol("border-width"), Value::fixnum(1));
        child.set_parameter(Value::symbol("border-color"), Value::string("#654321"));
        child.left_pos = -1;
        child.top_pos = -2;
        child.z_order = 3;
    }
    evaluator.frame_manager_mut().select_frame(child_id);
    evaluator
        .eval_str(
            r##"(internal-set-lisp-face-attribute
                  'child-frame-border :background "#123456" (selected-frame))"##,
        )
        .expect("set child-frame border background");

    let mut engine = LayoutEngine::new();
    let state = match engine.redisplay_frame_attempt(&mut evaluator, child_id) {
        FrameLayoutAttempt::Prepared(state) => state,
        FrameLayoutAttempt::Aborted => panic!("child-frame fixture layout aborted"),
    };
    assert_eq!(state.border_width, 3.0);
    assert_eq!(state.border_color, Color::from_pixel(0x0012_3456));
    assert_eq!(state.outer_border_width, 1.0);
    assert_eq!(state.outer_border_color, Color::from_pixel(0x0065_4321));
    assert_eq!(state.frame_pixel_width, 401.0);
    assert_eq!(state.frame_pixel_height, 121.0);
    assert_eq!(state.frame_placement.parent().unwrap().get(), parent_id.0);
    assert_eq!(state.frame_placement.outer_in_parent().x(), -1.0);
    assert_eq!(state.frame_placement.outer_in_parent().y(), -2.0);
    assert_eq!(state.frame_placement.outer_in_parent().width(), 401.0);
    assert_eq!(state.frame_placement.outer_in_parent().height(), 121.0);
    assert_eq!(state.frame_placement.z_order(), 3);
}

#[test]
fn tty_frame_outer_border_width_is_zero_without_erasing_parameter() {
    let mut evaluator = Context::new();
    let buffer_id = evaluator
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = evaluator
        .frame_manager_mut()
        .create_frame("tty", 80, 25, buffer_id);

    {
        let frame = evaluator
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame");
        frame.set_parameter(Value::symbol("border-width"), Value::fixnum(7));
    }

    let frame = evaluator.frame_manager().get(frame_id).expect("frame");
    assert_eq!(frame.outer_border_width(), 0);
    assert_eq!(
        frame.known_parameter(FrameParam::BorderWidth),
        Some(Value::fixnum(7))
    );
}
