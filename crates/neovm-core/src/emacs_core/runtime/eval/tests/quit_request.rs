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

#[test]
fn quit_request_flag_offset_names_the_flag() {
    let offset = QuitRequest::raised_flag_offset().expect("the probe finds the flag");
    let request = QuitRequest::new();
    // SAFETY: a `QuitRequest` is one pointer word (asserted by the probe).
    let inner: usize = unsafe { std::mem::transmute_copy(&request) };
    let read = || unsafe { *((inner + offset) as *const u8) };
    assert_eq!(read(), 0, "a lowered request reads zero at the offset");
    request.request();
    assert_eq!(read(), 1, "a raised request reads one at the offset");
    request.clear();
    assert_eq!(read(), 0);
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
