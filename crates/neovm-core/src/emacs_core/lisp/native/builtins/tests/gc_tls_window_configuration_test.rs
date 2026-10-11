use super::*;
use crate::emacs_core::eval::Context;

#[test]
fn gc_tls_ownership_window_snapshot_payload_is_rooted_by_its_configuration() {
    let mut ctx = Context::new();
    ctx.eval_str(r#"(progn (setq window-persistent-parameters '((gc-tls-payload . t))) (set-window-parameter nil 'gc-tls-payload (vector "saved payload")) (setq gc-tls-configuration (current-window-configuration)) (set-window-parameter nil 'gc-tls-payload nil))"#).unwrap();
    ctx.gc_collect_exact();
    ctx.eval_str("(set-window-configuration gc-tls-configuration)")
        .unwrap();
    assert_eq!(
        ctx.eval_str("(aref (window-parameter nil 'gc-tls-payload) 0)")
            .unwrap()
            .as_utf8_str(),
        Some("saved payload")
    );
}

#[test]
fn gc_tls_ownership_window_snapshot_reset_discards_a_dropped_heap() {
    {
        let mut first = Context::new();
        first.eval_str("(current-window-configuration)").unwrap();
        assert!(WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| !slot.borrow().is_empty()));
    }
    let mut next = Context::new();
    assert!(WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| slot.borrow().is_empty()));
    next.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_window_snapshots_follow_context_swaps() {
    let mut first = Context::new();
    first.eval_str("(progn (setq window-persistent-parameters '((gc-tls-payload . t))) (set-window-parameter nil 'gc-tls-payload (vector 1)) (setq gc-tls-configuration (current-window-configuration)) (set-window-parameter nil 'gc-tls-payload nil))").unwrap();
    let mut second = Context::new();
    second.eval_str("(progn (setq window-persistent-parameters '((gc-tls-payload . t))) (set-window-parameter nil 'gc-tls-payload (vector 2)) (setq gc-tls-configuration (current-window-configuration)) (set-window-parameter nil 'gc-tls-payload nil))").unwrap();
    first.setup_thread_locals();
    first.gc_collect_exact();
    first
        .eval_str("(set-window-configuration gc-tls-configuration)")
        .unwrap();
    assert_eq!(
        first
            .eval_str("(aref (window-parameter nil 'gc-tls-payload) 0)")
            .unwrap(),
        Value::fixnum(1)
    );
    second.setup_thread_locals();
    second.gc_collect_exact();
    second
        .eval_str("(set-window-configuration gc-tls-configuration)")
        .unwrap();
    assert_eq!(
        second
            .eval_str("(aref (window-parameter nil 'gc-tls-payload) 0)")
            .unwrap(),
        Value::fixnum(2)
    );
}
