//! Explicit redraws must repair physical terminal damage with either hook policy.

use crate::emacs_core::eval::{Context, RedisplayHookPolicyGuard};

fn assert_redraw_request(form: &str) {
    for hooks in [false, true] {
        let _policy = if hooks {
            RedisplayHookPolicyGuard::gnu()
        } else {
            RedisplayHookPolicyGuard::legacy()
        };
        let mut eval = Context::new();
        let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
        let target = eval.frames.get(frame).unwrap();
        let window = target.selected_window;
        let buffer = target.find_window(window).unwrap().buffer_id().unwrap();
        let body_before = eval.body_redisplay_revision(window, buffer);
        eval.note_chrome_generated(window);
        assert!(!eval.gnu_take_tty_frame_redraw(frame));
        eval.eval_str(form).expect("explicit terminal redraw");
        assert_ne!(eval.body_redisplay_revision(window, buffer), body_before);
        assert!(eval.chrome_dirty().is_dirty(window));
        assert!(
            eval.gnu_take_tty_frame_redraw(frame),
            "{form} must request physical repaint with GNU hooks={hooks}"
        );
        assert!(
            !eval.gnu_take_tty_frame_redraw(frame),
            "one prepared repaint consumes the request"
        );
    }
}

#[test]
fn redraw_frame_requests_physical_repaint_with_hooks_disabled() {
    assert_redraw_request("(redraw-frame)");
}

#[test]
fn redraw_display_requests_physical_repaint_with_hooks_disabled() {
    assert_redraw_request("(redraw-display)");
}

#[test]
fn full_frame_recenter_requests_physical_repaint_with_hooks_disabled() {
    assert_redraw_request("(let ((recenter-redisplay 'tty)) (recenter nil t))");
}
