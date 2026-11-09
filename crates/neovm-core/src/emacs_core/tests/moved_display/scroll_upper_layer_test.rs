//! scroll_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::neo::scroll::ScrollContextExt;
use crate::emacs_core::neo::*;
use crate::window::DisplayFrameManagerExt;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::value::Value;

#[test]
fn compositor_pixel_scroll_policy_requires_definition_and_binding_witnesses() {
    let mut eval = crate::test_utils::new_context();
    let buffer = eval.buffer_manager_mut().create_buffer("scroll-policy");
    let frame = eval
        .frame_manager_mut()
        .create_frame("scroll-policy", 800, 600, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    assert!(!eval.permits_compositor_pixel_scroll(window));
    eval.eval_str("(use-global-map (make-sparse-keymap)) (setq neomacs-compositor-scrolling t pixel-scroll-precision-mode t)").unwrap();
    for name in [
        "pixel-scroll-precision",
        "pixel-scroll-precision-scroll-down",
        "pixel-scroll-precision-scroll-down-page",
        "pixel-scroll-precision-scroll-up",
        "pixel-scroll-precision-scroll-up-page",
        "pixel-scroll-precision-interpolate",
    ] {
        eval.eval_str(&format!("(fset '{name} (lambda () nil)) (put '{name} 'neomacs--scroll-definition (symbol-function '{name}))")).unwrap();
    }
    eval.eval_str("(define-key (current-global-map) [wheel-up] 'pixel-scroll-precision) (define-key (current-global-map) [wheel-down] 'pixel-scroll-precision)").unwrap();
    assert!(eval.permits_compositor_pixel_scroll(window));
    let other_buffer = eval
        .buffer_manager_mut()
        .create_buffer("other-scroll-policy");
    let other_window = eval
        .frame_manager_mut()
        .split_window(
            frame,
            window,
            crate::window::SplitDirection::Horizontal,
            other_buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .unwrap();
    assert!(eval.compositor_scrolling_enabled(other_window));
    assert!(!eval.permits_compositor_pixel_scroll(other_window));
    eval.buffer_manager_mut()
        .get_mut(other_buffer)
        .unwrap()
        .set_buffer_local("neomacs-compositor-scrolling", Value::NIL);
    assert!(!eval.compositor_scrolling_enabled(other_window));
    assert!(eval.compositor_scrolling_enabled(window));
    let publications = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let observed = publications.clone();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        observed
            .borrow_mut()
            .push(eval.permits_compositor_pixel_scroll(window));
        crate::test_utils::mock_redisplay::accept_all_frames(eval);
    }));
    eval.redisplay().expect("redisplay");
    eval.redisplay().expect("redisplay");
    assert_eq!(&*publications.borrow(), &[true]);
    eval.eval_str("(setq neomacs-compositor-scrolling nil)")
        .unwrap();
    eval.redisplay().expect("redisplay");
    assert_eq!(
        &*publications.borrow(),
        &[true, false],
        "policy-only changes must publish revocation"
    );
    eval.eval_str("(setq neomacs-compositor-scrolling t)")
        .unwrap();
    eval.redisplay().expect("redisplay");
    assert_eq!(&*publications.borrow(), &[true, false, true]);
    eval.eval_str("(define-key (current-global-map) [wheel-down] '(menu-item \"scroll\" pixel-scroll-precision :filter ignore))").unwrap();
    assert!(
        !eval.permits_compositor_pixel_scroll(window),
        "menu filters require actual dispatch"
    );
    eval.eval_str("(define-key (current-global-map) [wheel-down] 'ignore)")
        .unwrap();
    assert!(!eval.permits_compositor_pixel_scroll(window));
    eval.eval_str("(define-key (current-global-map) [wheel-down] 'pixel-scroll-precision) (setq pre-command-hook '(ignore))").unwrap();
    assert!(!eval.permits_compositor_pixel_scroll(window));
    eval.eval_str("(setq pre-command-hook nil)").unwrap();
    let local = |eval: &mut Context, name: &str, value| {
        eval.buffer_manager_mut()
            .get_mut(buffer)
            .unwrap()
            .set_buffer_local(name, value);
    };
    local(
        &mut eval,
        "pre-command-hook",
        Value::list(vec![Value::symbol("ignore")]),
    );
    assert!(
        !eval.permits_compositor_pixel_scroll(window),
        "displayed buffer hooks veto prediction"
    );
    local(&mut eval, "pre-command-hook", Value::NIL);
    eval.buffer_manager_mut()
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local(
            "pre-command-hook",
            Value::list(vec![Value::symbol("ignore")]),
        );
    assert!(
        eval.permits_compositor_pixel_scroll(window),
        "timer buffer hooks do not belong to the displayed buffer"
    );
    eval.eval_str("(fset 'eldoc-pre-command-refresh-echo-area (lambda () nil)) (put 'eldoc-pre-command-refresh-echo-area 'neomacs--scroll-definition (symbol-function 'eldoc-pre-command-refresh-echo-area))").unwrap();
    local(
        &mut eval,
        "pre-command-hook",
        Value::list(vec![
            Value::symbol("eldoc-pre-command-refresh-echo-area"),
            Value::T,
        ]),
    );
    assert!(eval.permits_compositor_pixel_scroll(window));
    local(
        &mut eval,
        "eldoc-last-message",
        Value::string("documentation"),
    );
    assert!(!eval.permits_compositor_pixel_scroll(window));
    local(&mut eval, "eldoc-last-message", Value::NIL);
    eval.eval_str("(fset 'eldoc-pre-command-refresh-echo-area (lambda () t))")
        .unwrap();
    assert!(!eval.permits_compositor_pixel_scroll(window));
    local(&mut eval, "pre-command-hook", Value::NIL);
    eval.eval_str("(setq pre-command-hook nil) (fset 'pixel-scroll-precision (lambda () t))")
        .unwrap();
    assert!(!eval.permits_compositor_pixel_scroll(window));
}
