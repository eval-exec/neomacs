//! Frame-resize tests that need the core upper layers.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::window::{DisplayFrameExt, DisplayFrameManagerExt};
use neovm_display::test_utils::window_cmds_host::{
    RecordingDisplayHost, remapped_mono_font_metrics,
};
use neovm_eval::emacs_core::eval::Context;
use neovm_value::value::Value;

#[test]
fn explicit_frame_minimum_overrides_inhibited_font_resize_on_only_that_axis() {
    let mut eval = crate::test_utils::new_context();
    let buffer = eval.edit.buffers.create_buffer("*font-minimum*");
    let fid = eval
        .frame_manager_mut()
        .create_frame("minimum", 800, 600, buffer);
    let frame = eval.frame_manager_mut().get_mut(fid).unwrap();
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.char_width = 8.0;
    frame.char_height = 16.0;
    let host = RecordingDisplayHost::with_resolved_frame_font(remapped_mono_font_metrics());
    let requests = host.resized.clone();
    eval.set_display_host(Box::new(host));
    eval.eval_str(
        r#"(progn
       (modify-frame-parameters nil '((min-width . 120)))
       (setq frame-inhibit-implied-resize t)
       (internal-set-lisp-face-attribute 'default :font "Remapped Mono-24" nil))"#,
    )
    .unwrap();
    let requests = requests.borrow();
    let request = requests
        .last()
        .expect("minimum must trigger a native resize");
    assert_eq!(request.width, 120 * 16);
    assert_eq!(request.height, 600);
}
