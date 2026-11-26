//! Exercise the evaluator's real special-event dispatcher with Core hooks.
use crate::keyboard::{InputEvent, KeyEvent};
use neomacs_display_protocol::{GraphicalBackend, GraphicalDisplayIdentity};
use neovm_value::value::Value;
use std::time::Duration;

fn context(bound: bool) -> crate::emacs_core::Context {
    let mut eval = crate::emacs_core::load_boot::create_bootstrap_evaluator_cached()
        .expect("font-render event dispatch requires the bootstrapped Lisp command machinery");
    eval.eval_str(
        r#"(progn
             (setq special-event-map (make-sparse-keymap)
                   raster-test-events nil)
             (defun raster-test-config-changed (event)
               (interactive "e")
               (setq raster-test-events (cons event raster-test-events))))"#,
    )
    .unwrap();
    if bound {
        eval.eval_str(
            "(define-key special-event-map [config-changed-event] #'raster-test-config-changed)",
        )
        .unwrap();
    }
    eval
}

fn enqueue(eval: &mut crate::emacs_core::Context, backend: GraphicalBackend, name: &str) {
    let queue = &mut eval.command_loop_mut().keyboard.pending_input_events;
    queue.push_back(InputEvent::SystemFontRenderingChanged {
        display: GraphicalDisplayIdentity::named(backend, name).unwrap(),
    });
    queue.push_back(InputEvent::key_press(KeyEvent::char('k')));
}

fn assert_key_retained(eval: &mut crate::emacs_core::Context) {
    assert_eq!(
        eval.read_char_with_timeout(Some(Duration::ZERO)).unwrap(),
        Some(Value::fixnum('k' as i64)),
        "servicing a settings notification must retain the following command input"
    );
}

#[test]
fn wayland_font_render_wait_service_runs_real_lisp_handler_and_invalidates_redisplay() {
    let mut eval = context(true);
    enqueue(&mut eval, GraphicalBackend::Wayland, "wayland-raster-test");
    let before = eval.redisplay_generation();
    let outcome = eval.service_wait_request_special_input_events().unwrap();
    assert!(outcome.redisplay_needed());
    assert!(eval.redisplay_generation() > before);
    assert!(eval.last_redisplay_signature_is_none_for_test());
    let events = eval.eval_str("raster-test-events").unwrap();
    assert_eq!(
        crate::emacs_core::print_value(&events),
        "((config-changed-event font-render \"wayland-raster-test\"))"
    );
    assert_key_retained(&mut eval);
}

#[test]
fn unbound_wayland_font_render_wait_service_still_requests_redisplay_and_retains_keys() {
    let mut eval = context(false);
    enqueue(
        &mut eval,
        GraphicalBackend::Wayland,
        "wayland-raster-unbound",
    );
    let before = eval.redisplay_generation();
    let outcome = eval.service_wait_request_special_input_events().unwrap();
    assert!(outcome.redisplay_needed());
    assert!(eval.redisplay_generation() > before);
    assert!(eval.last_redisplay_signature_is_none_for_test());
    assert!(eval.eval_str("raster-test-events").unwrap().is_nil());
    assert_key_retained(&mut eval);
}

#[test]
fn x11_font_render_wait_service_does_not_dispatch_or_invalidate() {
    let mut eval = context(true);
    enqueue(&mut eval, GraphicalBackend::X11, ":359");
    let before = eval.redisplay_generation();
    let outcome = eval.service_wait_request_special_input_events().unwrap();
    assert!(!outcome.redisplay_needed());
    assert_eq!(eval.redisplay_generation(), before);
    assert!(eval.eval_str("raster-test-events").unwrap().is_nil());
    assert_key_retained(&mut eval);
}
