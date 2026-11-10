//! GNU's cleared-echo branch cannot displace a selected active mini source.
//! These fixtures retain the real Context-owned echo history and reader owner;
//! only the frontend producer is replaced with a typed request observer.
use crate::emacs_core::eval::redisplay_hooks::{
    RedisplayMiniGeometryRequest, RedisplayMiniGeometrySource,
};
use crate::emacs_core::eval::{Context, RedisplayHookPolicyGuard};
use crate::window::DisplayFrameManagerExt;
use neovm_edit::buffer::BufferId;
use neovm_edit::emacs_core::display_metrics::WindowId;
use neovm_value::display_vocab::FrameId;
use neovm_value::value::Value;
use std::cell::RefCell;
use std::rc::Rc;

fn context_with_remembered_echo() -> (
    Context,
    FrameId,
    WindowId,
    BufferId,
    Rc<RefCell<Vec<RedisplayMiniGeometryRequest>>>,
) {
    let mut eval = crate::test_utils::new_context();
    let buffer = eval.edit.buffers.current_buffer_id().expect("root buffer");
    let frame = eval
        .frame_manager_mut()
        .create_frame("mini-source", 80, 25, buffer);
    let window = eval
        .frame_manager()
        .get(frame)
        .expect("frame")
        .minibuffer_window
        .expect("mini window");
    let requests = Rc::new(RefCell::new(Vec::new()));
    let observed = requests.clone();
    eval.redisplay_prepare_fn = Some(Box::new(move |_, request| {
        observed.borrow_mut().push(request);
        Ok(Value::NIL)
    }));
    eval.redisplay_fn = Some(Box::new(|_| {}));
    eval.set_current_message(Some(neovm_value::heap_types::LispString::from_utf8(
        "old echo",
    )));
    eval.redisplay_with_force_flow(true)
        .expect("remember echo preparation");
    let echo_buffer = eval.echo_area_display_buffer().expect("echo source");
    assert_eq!(
        neovm_eval::test_support::authority::echo_geometry_window_for_test(&eval),
        Some(window)
    );
    let request = requests.borrow_mut().pop().expect("first echo preparation");
    assert_eq!(request.source, RedisplayMiniGeometrySource::EchoArea);
    assert_eq!(request.buffer, echo_buffer);
    eval.set_current_message(None);
    assert!(!eval.has_current_message());
    assert_eq!(
        neovm_eval::test_support::authority::echo_geometry_window_for_test(&eval),
        Some(window)
    );
    (eval, frame, window, echo_buffer, requests)
}

#[test]
fn remembered_cleared_echo_does_not_displace_selected_active_mini_preparation() {
    crate::test_utils::init_test_tracing();
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, window, echo_buffer, requests) = context_with_remembered_echo();
    let active = eval.edit.buffers.create_buffer(" *Minibuf-1*");
    eval.edit
        .buffers
        .get_mut(active)
        .expect("active buffer")
        .set_buffer_local("resize-mini-windows", Value::T);
    let selected = eval
        .activate_minibuffer_window_for_buffer(
            active,
            neovm_value::heap_types::LispString::from_utf8("probe: "),
            None,
        )
        .expect("enter real reader ownership")
        .expect("active mini");
    assert_eq!(selected, window);
    assert_eq!(eval.gnu_selected_window(), Some(window));
    assert!(eval.minibuffer_is_active());
    assert!(!eval.has_current_message());
    assert_ne!(active, echo_buffer);
    eval.redisplay_with_force_flow(true)
        .expect("active mini preparation");
    let observed = requests.borrow();
    assert_eq!(observed.len(), 1);
    assert_eq!(
        observed[0].source,
        RedisplayMiniGeometrySource::ActiveMinibuffer
    );
    assert_eq!(
        (observed[0].frame, observed[0].window, observed[0].buffer),
        (frame, window, active)
    );
    assert!(!observed[0].exact);
    assert_eq!(eval.edit.buffers.current_buffer_id(), Some(active));
    assert_eq!(
        neovm_eval::test_support::authority::echo_geometry_window_for_test(&eval),
        None
    );
    assert!(eval.redisplay_prepare_fn.is_some());
    assert!(!eval.gnu_redisplay_hooks_active());
}

#[test]
fn remembered_cleared_echo_still_prepares_inactive_nonselected_mini() {
    crate::test_utils::init_test_tracing();
    let _policy = RedisplayHookPolicyGuard::gnu();
    let (mut eval, frame, window, echo_buffer, requests) = context_with_remembered_echo();
    assert!(!eval.minibuffer_is_active());
    assert_ne!(eval.gnu_selected_window(), Some(window));
    eval.redisplay_with_force_flow(true)
        .expect("cleared echo preparation");
    let observed = requests.borrow();
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].source, RedisplayMiniGeometrySource::EchoArea);
    assert_eq!(
        (observed[0].frame, observed[0].window, observed[0].buffer),
        (frame, window, echo_buffer)
    );
    assert_eq!(
        neovm_eval::test_support::authority::echo_geometry_window_for_test(&eval),
        None
    );
}
