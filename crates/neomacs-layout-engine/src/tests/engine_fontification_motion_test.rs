//! A newly installed fontifier must run before retained rows answer motion.

use super::*;
use neovm_core::window::WindowLayoutQueryOutcome;

#[test]
fn installing_fontification_hook_remeasures_retained_motion_before_redisplay() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("abc\ndef\nghi\n");
    let frame =
        eval.frame_manager_mut()
            .create_frame("new-fontification-motion", 1200, 800, buffer);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .window_system = None;
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    // Prebuild the function and list before presentation, so installing the
    // hook changes neither the function epoch nor the source's properties.
    eval.eval_str(
        r#"(setq ttykey-fontify-calls 0 ttykey-fontify-done nil)
           (setq ttykey-fontify-hook
             (list (lambda (_pos)
               (setq ttykey-fontify-calls (1+ ttykey-fontify-calls))
               (if ttykey-fontify-done nil
                 (progn
                   (setq ttykey-fontify-done t)
                   (put-text-property 1 2 'display "X\nY")))
               (put-text-property (point-min) (point-max) 'fontified t))))
           (setq fontification-functions nil
                 mode-line-format nil header-line-format nil tab-line-format nil)
           (goto-char 1)
           (set-window-start nil 1 t)"#,
    )
    .expect("prebuilt but uninstalled fontifier");
    let mut display = LayoutEngine::new_without_font_metrics();
    display.layout_frame_rust(&mut eval, frame);
    activate_last_engine_presentation(&mut eval, &display, frame);
    assert!(
        eval.fresh_window_display_snapshot(frame, window, buffer)
            .is_some(),
        "motion must start with usable retained rows"
    );
    let mut query = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        match query.query_window_layout(eval, frame, window, scope) {
            Ok(query) => WindowLayoutQueryOutcome::Ready(query),
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    })
    .expect("install the layout-query handler");
    // No forced update/redisplay between installation and motion. GNU31.1's
    // interactive display iterator calls this hook once and sees its newline
    // replacement: (rows moved, target point, hook calls, hook completed).
    // Before the hook identity was captured, retained motion returned (1 5 0 nil).
    let actual = eval
        .eval_str(
            r#"(let ((noninteractive nil))
                (setq fontification-functions ttykey-fontify-hook)
                (let ((moved (vertical-motion 1)))
                  (list moved (point) ttykey-fontify-calls ttykey-fontify-done)))"#,
        )
        .expect("motion after hook installation");
    assert_eq!(
        neovm_core::emacs_core::print::print_value(&actual),
        "(1 2 1 t)"
    );
}
