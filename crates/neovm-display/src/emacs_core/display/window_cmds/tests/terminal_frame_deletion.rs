use crate::window::DisplayFrameManagerExt;
use crate::window::FrameVisibility;
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::print::format_eval_result;
use neovm_value::value::Value;

fn context_with_hidden_gui_peer(primary_window_system: Option<&str>) -> Context {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let scratch = eval.edit.buffers.create_buffer("*scratch*");
    eval.edit.buffers.set_current(scratch);
    let primary = eval
        .frame_manager_mut()
        .create_frame("target", 800, 600, scratch);
    let frame = eval
        .frame_manager_mut()
        .get_mut(primary)
        .expect("target frame");
    frame.set_window_system(primary_window_system.map(Value::symbol));
    frame.visibility = FrameVisibility::Visible;
    let peer = eval
        .frame_manager_mut()
        .create_frame("hidden GUI", 800, 600, scratch);
    let frame = eval
        .frame_manager_mut()
        .get_mut(peer)
        .expect("hidden GUI frame");
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.visibility = FrameVisibility::Invisible;
    eval.set_variable("tested-frame", Value::make_frame(primary.0));
    eval.set_variable("hidden-peer", Value::make_frame(peer.0));
    eval
}

#[test]
fn terminal_frame_can_be_deleted_when_only_an_invisible_gui_frame_remains() {
    crate::test_utils::init_test_tracing();
    let mut eval = context_with_hidden_gui_peer(None);
    // GNU frame-initialize retires its terminal even when initial-frame-alist
    // requests an invisible graphical frame. No force argument is required.
    let result = eval.eval_str(
        "(progn
           (delete-frame tested-frame)
           (list (frame-live-p tested-frame)
                 (and (frame-live-p hidden-peer) t)
                 (frame-visible-p hidden-peer)
                 (length (frame-list))))",
    );
    assert_eq!(format_eval_result(&result), "OK (nil t nil 1)");
}

#[test]
fn terminal_frame_cannot_be_hidden_when_only_an_invisible_gui_frame_remains() {
    crate::test_utils::init_test_tracing();
    let mut eval = context_with_hidden_gui_peer(None);
    let result = eval.eval_str(
        "(condition-case err
             (make-frame-invisible tested-frame)
           (error err))",
    );
    assert_eq!(
        format_eval_result(&result),
        "OK (error \"Attempt to make invisible the sole visible or iconified frame\")",
    );
    let state = eval.eval_str("(list (frame-visible-p tested-frame) (length (frame-list)))");
    assert_eq!(format_eval_result(&state), "OK (t 2)");
}

#[test]
fn graphical_frame_cannot_be_deleted_when_only_an_invisible_gui_frame_remains() {
    crate::test_utils::init_test_tracing();
    let mut eval = context_with_hidden_gui_peer(Some("neo"));
    let result = eval.eval_str(
        "(condition-case err
             (delete-frame tested-frame)
           (error err))",
    );
    assert_eq!(
        format_eval_result(&result),
        "OK (error \"Attempt to delete the sole visible or iconified frame\")",
    );
    let state = eval.eval_str("(list (frame-visible-p tested-frame) (length (frame-list)))");
    assert_eq!(format_eval_result(&state), "OK (t 2)");
}
