//! Deleted frames are reclaimed by an explicit `(garbage-collect)`, except
//! when a Lisp value still reaches the frame object or one of its
//! materialized window objects.
//!
//! `framep` of a held dead frame is the tombstone copied at deletion
//! (`DeletedFrameTombstone::lisp_value`). `unit_runtime` creates frames with
//! no window system, so the tombstone is `DeletedFrameOutput::Text` and
//! `framep` is `t`. That is the value
//! `framep_of_a_deleted_frame_keeps_its_output_method` locks. GNU on an X
//! build prints `x` for the same question (`frame.c:300-316`); this runtime
//! has no window system, so the assertion is `t`.

use crate::emacs_core::frame::builtin_framep;
use crate::window::DisplayFrameManagerExt;
use neovm_edit::buffer::BufferId;
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::keyboard::InputEvent;
use neovm_value::display_vocab::FrameId;
use neovm_value::value::Value;

fn fresh() -> (Context, BufferId) {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    // A live frame stays selected so deleting the frames under test does not
    // clear the only frame the runtime has.
    ev.frame_manager_mut().create_frame("keeper", 80, 24, buf);
    (ev, buf)
}

fn root_window(ev: &Context, id: FrameId) -> neovm_edit::emacs_core::display_metrics::WindowId {
    ev.frame_manager().get(id).expect("frame").selected_window
}

fn text_tombstone() -> Value {
    // `DeletedFrameOutput::Text` -> `Value::T`. See the module comment.
    Value::T
}

#[test]
fn deleted_frames_with_no_reference_return_to_baseline_after_garbage_collect() {
    let (mut ev, buf) = fresh();
    let base_windows = ev.frame_manager().deleted_windows.len();
    let base_params = ev.frame_manager().deleted_window_parameters.len();
    let base_frames = ev.frame_manager().deleted_frames.len();
    let mut doomed = Vec::new();
    for i in 0..4 {
        let id = ev
            .frame_manager_mut()
            .create_frame(&format!("doomed-{i}"), 80, 24, buf);
        let wid = root_window(&ev, id);
        ev.frame_manager_mut().set_window_parameter(
            wid,
            Value::symbol("d8-marker"),
            Value::cons(Value::fixnum(i), Value::fixnum(1)),
        );
        doomed.push(id);
    }
    for id in doomed {
        assert!(ev.frame_manager_mut().delete_frame(id).was_deleted());
    }
    ev.eval_str("(garbage-collect)").expect("garbage-collect");
    assert_eq!(ev.frame_manager().deleted_windows.len(), base_windows);
    assert_eq!(
        ev.frame_manager().deleted_window_parameters.len(),
        base_params
    );
    assert_eq!(ev.frame_manager().deleted_frames.len(), base_frames);
}

#[test]
fn held_deleted_frame_keeps_its_object_registry_slot_and_parameters() {
    let (mut ev, buf) = fresh();
    let id = ev.frame_manager_mut().create_frame("held", 80, 24, buf);
    let frame = Value::make_frame(id.0);
    let wid = root_window(&ev, id);
    let marker = Value::cons(Value::symbol("held-param"), Value::fixnum(9));
    ev.frame_manager_mut()
        .set_window_parameter(wid, Value::symbol("d8-marker"), marker);
    assert!(ev.frame_manager_mut().delete_frame(id).was_deleted());
    ev.set_variable("held-frame", frame);

    ev.eval_str("(garbage-collect)").expect("garbage-collect");

    assert_eq!(Value::make_frame(id.0), frame, "registry slot");
    assert_eq!(
        ev.frame_manager()
            .window_parameter(wid, &Value::symbol("d8-marker")),
        Some(marker)
    );
    assert!(ev.frame_manager().deleted_windows.contains_key(&wid));
    assert!(ev.frame_manager().deleted_frames.contains_key(&id));
    // Unit runtime: text tombstone, so `framep` is `t`. GNU X prints `x`.
    let framep = builtin_framep(&mut ev, vec![frame]).expect("framep");
    assert_eq!(framep, text_tombstone());
    assert!(
        ev.eval_str("(frame-live-p held-frame)")
            .expect("frame-live-p")
            .is_nil()
    );
}

#[test]
fn materialized_window_keeps_its_deleted_frame_after_garbage_collect() {
    let (mut ev, buf) = fresh();
    let id = ev
        .frame_manager_mut()
        .create_frame("window-held", 80, 24, buf);
    let frame = Value::make_frame(id.0);
    let wid = root_window(&ev, id);
    // GNU `window-parameter` uses `decode_any_window` (window.c:2398). The
    // attested form is `(4 . 1)` under `d8-marker`, and it survives
    // `delete-frame` plus `(garbage-collect)`.
    let marker = Value::cons(Value::fixnum(4), Value::fixnum(1));
    ev.frame_manager_mut()
        .set_window_parameter(wid, Value::symbol("d8-marker"), marker);
    let window = Value::make_window(wid.0);
    assert!(ev.frame_manager_mut().delete_frame(id).was_deleted());
    // The frame value is not stored. The window object is the only hold.
    ev.set_variable("held-window", window);

    ev.eval_str("(garbage-collect)").expect("garbage-collect");

    assert_eq!(Value::make_frame(id.0), frame, "registry slot");
    assert_eq!(
        ev.eval_str("(window-parameter held-window 'd8-marker)")
            .expect("window-parameter"),
        marker
    );
    assert!(
        ev.eval_str("(window-live-p held-window)")
            .expect("window-live-p")
            .is_nil()
    );
    // `Fwindow_frame` calls `decode_valid_window` (window.c:342-348). A
    // deleted frame's windows have nil contents, so this signals
    // `window-valid-p` and the datum is the window object.
    let signaled = ev
        .eval_str(
            "(condition-case e (window-frame held-window) \
              (error (list (car e) (car (cdr e)) (eq (car (cdr (cdr e))) held-window))))",
        )
        .expect("window-frame");
    assert_eq!(
        signaled,
        Value::list(vec![
            Value::symbol("wrong-type-argument"),
            Value::symbol("window-valid-p"),
            Value::T,
        ])
    );
    // The window object keeps the frame, so `framep` is still the tombstone.
    let framep = builtin_framep(&mut ev, vec![frame]).expect("framep");
    assert_eq!(framep, text_tombstone());
    assert!(ev.frame_manager().deleted_frames.contains_key(&id));
    assert!(ev.frame_manager().deleted_windows.contains_key(&wid));
}

#[test]
fn deleted_frame_with_no_window_record_is_reclaimed() {
    let (mut ev, buf) = fresh();
    let id = ev.frame_manager_mut().create_frame("empty", 80, 24, buf);
    assert!(ev.frame_manager_mut().delete_frame(id).was_deleted());
    assert!(ev.frame_manager().deleted_frames.contains_key(&id));
    // A deleted frame that never recorded a window must still be armed.
    // Grouping `deleted_windows` by owner would skip this tombstone.
    ev.frame_manager_mut().deleted_windows.clear();
    ev.frame_manager_mut().deleted_window_parameters.clear();

    ev.eval_str("(garbage-collect)").expect("garbage-collect");

    assert!(!ev.frame_manager().deleted_frames.contains_key(&id));
    assert!(Value::registered_frame(id.0).is_none());
}

#[test]
fn queued_toolbar_click_keeps_a_deleted_frame() {
    let (mut ev, buf) = fresh();
    let id = ev.frame_manager_mut().create_frame("queued", 80, 24, buf);
    let frame = Value::make_frame(id.0);
    ev.command_loop_mut()
        .keyboard
        .event_queue
        .push_back(InputEvent::ToolBarClick {
            index: 0,
            emacs_frame_id: id.0,
        });
    assert!(ev.frame_manager_mut().delete_frame(id).was_deleted());

    ev.eval_str("(garbage-collect)").expect("garbage-collect");

    assert_eq!(ev.event_frame_value(id.0), frame);
    assert_eq!(
        builtin_framep(&mut ev, vec![frame]).expect("framep"),
        text_tombstone()
    );
    assert!(ev.frame_manager().deleted_frames.contains_key(&id));
}

#[test]
fn mouse_pixel_position_keeps_a_deleted_frame() {
    let (mut ev, buf) = fresh();
    let id = ev.frame_manager_mut().create_frame("mouse", 80, 24, buf);
    let frame = Value::make_frame(id.0);
    ev.command_loop_mut()
        .keyboard
        .set_mouse_pixel_position(Some(id), 3, 5);
    assert!(ev.frame_manager_mut().delete_frame(id).was_deleted());

    ev.eval_str("(garbage-collect)").expect("garbage-collect");

    // `unit_runtime` binds `mouse-pixel-position` to the absent-layer slot
    // `core::mouse-pixel-position`. The real builtin
    // (`current_mouse_position_value`) reads this state and
    // `Value::registered_frame`.
    assert_eq!(
        ev.command_loop()
            .keyboard
            .mouse_pixel_position()
            .and_then(|state| state.frame_id),
        Some(id)
    );
    assert_eq!(Value::registered_frame(id.0), Some(frame));
    assert_eq!(
        builtin_framep(&mut ev, vec![frame]).expect("framep"),
        text_tombstone()
    );
    assert!(ev.frame_manager().deleted_frames.contains_key(&id));
}

#[test]
fn reclaimed_frame_id_does_not_reach_make_frame_from_an_event() {
    let (mut ev, buf) = fresh();
    let id = ev.frame_manager_mut().create_frame("unheld", 80, 24, buf);
    assert!(ev.frame_manager_mut().delete_frame(id).was_deleted());

    ev.eval_str("(garbage-collect)").expect("garbage-collect");

    assert!(Value::registered_frame(id.0).is_none());
    assert_eq!(ev.event_frame_value(id.0), Value::NIL);
    assert!(!ev.frame_manager().deleted_frames.contains_key(&id));
}
