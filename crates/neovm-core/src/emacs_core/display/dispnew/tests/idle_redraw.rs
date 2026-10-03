//! GNU `redraw_frame` (dispnew.c) invalidates all window display matrices,
//! even when Lisp-visible display state is unchanged.

use crate::emacs_core::eval::Context;
use std::cell::Cell;
use std::rc::Rc;

/// Restores a test-only override owned by the current test thread; it never
/// stores Lisp state or shares policy with another mutator.
struct IdleSkipOverride;

impl IdleSkipOverride {
    fn enable() -> Self {
        crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(Some(true));
        Self
    }
}

impl Drop for IdleSkipOverride {
    fn drop(&mut self) {
        crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(None);
    }
}

fn idle_context() -> (Context, Rc<Cell<usize>>) {
    let mut eval = Context::new();
    let buffer = eval.buffers.current_buffer_id().expect("current buffer");
    eval.frames.create_frame("idle-redraw", 640, 384, buffer);
    let layouts = Rc::new(Cell::new(0));
    let observed = Rc::clone(&layouts);
    eval.redisplay_fn = Some(Box::new(move |eval| {
        observed.set(observed.get() + 1);
        eval.buffers
            .current_buffer()
            .unwrap()
            .reset_unchanged_region();
    }));
    eval.redisplay_with_force(true).expect("redisplay");
    eval.redisplay_with_force(true).expect("redisplay");
    assert_eq!(layouts.get(), 1, "the fixture must have an idle signature");
    (eval, layouts)
}

#[test]
fn redraw_frame_invalidates_idle_redisplay() {
    let _idle_skip = IdleSkipOverride::enable();
    let (mut eval, layouts) = idle_context();
    for form in ["(redraw-frame)", "(redraw-frame (selected-frame))"] {
        assert!(eval.eval_str(form).unwrap().is_nil());
        let before = layouts.get();
        eval.redisplay_with_force(true).expect("redisplay");
        assert_eq!(layouts.get(), before + 1, "{form} must schedule layout");
        eval.redisplay_with_force(true).expect("redisplay");
        assert_eq!(
            layouts.get(),
            before + 1,
            "unchanged redisplay remains idle"
        );
    }
}

#[test]
fn redraw_display_invalidates_idle_redisplay() {
    let _idle_skip = IdleSkipOverride::enable();
    let (mut eval, layouts) = idle_context();
    assert!(eval.eval_str("(redraw-display)").unwrap().is_nil());
    eval.redisplay_with_force(true).expect("redisplay");
    assert_eq!(layouts.get(), 2, "redraw-display must schedule layout");
    eval.redisplay_with_force(true).expect("redisplay");
    assert_eq!(layouts.get(), 2, "unchanged redisplay remains idle");
}

#[test]
fn recenter_redraw_invalidates_unchanged_idle_redisplay() {
    let _idle_skip = IdleSkipOverride::enable();
    let (mut eval, layouts) = idle_context();
    // An empty buffer keeps point and window-start at the same positions.
    eval.eval_str("(let ((recenter-redisplay t)) (recenter nil t))")
        .unwrap();
    eval.redisplay_with_force(true).expect("redisplay");
    assert_eq!(layouts.get(), 2, "full-frame recenter must schedule layout");
    eval.redisplay_with_force(true).expect("redisplay");
    assert_eq!(layouts.get(), 2, "unchanged redisplay remains idle");
}

#[test]
fn force_window_update_invalidates_idle_redisplay() {
    let _idle_skip = IdleSkipOverride::enable();
    let (mut eval, layouts) = idle_context();
    for form in [
        "(force-window-update)",
        "(force-window-update (selected-window))",
    ] {
        assert!(eval.eval_str(form).unwrap().is_truthy());
        let before = layouts.get();
        eval.redisplay_with_force(true).expect("redisplay");
        assert_eq!(layouts.get(), before + 1, "{form} must schedule layout");
        eval.redisplay_with_force(true).expect("redisplay");
        assert_eq!(
            layouts.get(),
            before + 1,
            "unchanged redisplay remains idle"
        );
    }
    let before = layouts.get();
    assert!(eval.eval_str("(force-window-update t)").unwrap().is_nil());
    eval.redisplay_with_force(true).expect("redisplay");
    assert_eq!(
        layouts.get(),
        before,
        "an invalid designator must not repaint"
    );
}
