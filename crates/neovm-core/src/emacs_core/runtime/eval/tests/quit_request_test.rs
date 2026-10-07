//! `QuitRequest`: the cross-thread C-g request the input bridges raise and
//! the evaluator drains (see `attention.rs`).

use super::QuitRequest;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[test]
fn quit_request_raises_takes_and_clears() {
    let request = QuitRequest::new();
    assert!(!request.is_requested(), "a new request is lowered");
    assert!(!request.take(), "taking a lowered request reports nothing");

    request.request();
    request.request();
    assert!(request.is_requested(), "a raise is visible to the poll");
    assert!(request.take(), "the first take consumes the raise");
    assert!(!request.is_requested());
    assert!(!request.take(), "a raise is consumed exactly once");

    request.request();
    request.clear();
    assert!(!request.is_requested(), "clear lowers a raised request");
}

#[test]
fn quit_request_clones_share_one_flag() {
    let request = QuitRequest::new();
    let bridge = request.clone();
    bridge.request();
    assert!(
        request.is_requested(),
        "the evaluator sees the bridge's raise"
    );
    assert!(request.take());
    assert!(
        !bridge.is_requested(),
        "the evaluator's take lowers the bridge's copy too"
    );
}

/// The process word counts raised requests, not raises: a second raise of a
/// raised request adds nothing, a take removes exactly what the raise added,
/// and a request dropped while raised gives its unit back.
#[test]
fn quit_request_counts_raised_cells_exactly() {
    use crate::emacs_core::eval::ASYNC_ATTENTION;
    let count = || ASYNC_ATTENTION.raised_quit_requests_for_test();
    let base = count();
    let a = QuitRequest::new();
    let b = QuitRequest::new();
    a.request();
    a.request();
    assert_eq!(count(), base + 1, "raising a raised request adds nothing");
    b.request();
    assert_eq!(count(), base + 2, "each raised cell counts once");
    assert!(a.take());
    assert_eq!(count(), base + 1);
    assert!(!a.take(), "a second take finds nothing");
    assert_eq!(count(), base + 1, "and removes nothing");
    let b_clone = b.clone();
    drop(b);
    assert_eq!(count(), base + 1, "a live clone keeps the raise");
    drop(b_clone);
    assert_eq!(
        count(),
        base,
        "the last handle of a raised request releases its unit"
    );
    let c = QuitRequest::new();
    c.request();
    c.clear();
    drop(c);
    assert_eq!(count(), base, "a cleared request owes nothing at drop");
}

/// A raised request is what the safe point's fast test sees: the word goes
/// nonzero, `maybe_quit_hot_ok` refuses, and the drain lowers both.
#[test]
fn a_raised_quit_request_sends_the_safe_point_to_its_slow_path() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    assert!(ctx.maybe_quit_hot_ok(), "nothing pending at start");
    ctx.quit_requested.request();
    assert_ne!(crate::emacs_core::eval::ASYNC_ATTENTION.load(), 0);
    assert!(
        !ctx.maybe_quit_hot_ok(),
        "a raised request is due at the next safe point"
    );
    let err = ctx.maybe_quit().expect_err("the drain signals quit");
    assert!(format!("{err:?}").contains("quit"), "{err:?}");
    assert!(!ctx.quit_requested.is_requested());
    ctx.set_quit_flag_value(Value::NIL);
    assert_eq!(crate::emacs_core::eval::ASYNC_ATTENTION.load(), 0);
    assert!(ctx.maybe_quit_hot_ok());
}

/// A raise from another thread -- the input bridge's shape -- reaches the
/// evaluator's next safe point as a `quit` signal, and the request is
/// drained so the following poll does not fire again.
#[test]
fn quit_request_from_another_thread_interrupts_an_evaluator_loop() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let bridge = ctx.quit_requested.clone();
    let raiser = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        bridge.request();
    });
    let result = ctx.eval_str("(let ((i 0)) (while t (setq i (1+ i))))");
    raiser.join().expect("raiser thread");
    match result {
        Err(e) => {
            let msg = format!("{e}");
            assert!(msg.contains("quit"), "expected quit, got: {msg}");
        }
        Ok(v) => panic!("expected a quit signal, got {v:?}"),
    }
    assert!(
        !ctx.quit_requested.is_requested(),
        "the safe point drained the request"
    );
    assert!(ctx.maybe_quit_hot_ok() || !ctx.quit_flag_value().is_nil());
    ctx.set_quit_flag_value(Value::NIL);
    assert!(
        ctx.maybe_quit_hot_ok(),
        "nothing is pending after the drain"
    );
}
