//! Measurement contracts through the public evaluator/frame APIs.
use neovm_core::emacs_core::{Context, Value};
#[test]
fn pixel_measurement_preserves_absolute_stretch_units() {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("pixel-stretch", 80, 24, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("x")));
    let result = eval.eval_str("(progn (erase-buffer) (insert (propertize \" \" 'display '(space :width (50)))) (list (car (window-text-pixel-size nil 1 2 t)) (car (buffer-text-pixel-size nil nil t))))").unwrap();
    assert_eq!(format!("{result}"), "(50 50)");
}
