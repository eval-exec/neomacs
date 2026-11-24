//! `QuitRequest`: the cross-thread C-g request the input bridges raise and
//! the evaluator drains (see `attention.rs`).

use super::QuitRequest;
use crate::emacs_core::eval::Context;
use neovm_value::value::Value;

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
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
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
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
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

/// GNU `handle_interrupt` (`src/keyboard.c:12714-12720`) tracks
/// `force_quit_count`: pressing C-g three times while `quit-flag` stays
/// non-nil clears `inhibit-quit` so stuck `inhibit-quit` loops remain
/// interruptible.
#[test]
fn three_quit_requests_clear_inhibit_quit_and_interrupt_loop() {
    use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};
    use std::sync::atomic::{AtomicBool, Ordering};

    static LOOP_ENTERED: AtomicBool = AtomicBool::new(false);
    fn mark_loop_entered(
        _ctx: &mut Context,
        _args: Vec<Value>,
    ) -> Result<Value, neovm_value::error::Flow> {
        LOOP_ENTERED.store(true, Ordering::Release);
        Ok(Value::T)
    }

    crate::test_utils::init_test_tracing();
    LOOP_ENTERED.store(false, Ordering::Release);
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    ctx.register_subr(SubrSpec::new(
        "qtest-mark-inhibit-loop-entered",
        NativeFn::ContextVec(mark_loop_entered),
        SubrArity::new(0, Some(0)),
    ));
    let bridge = ctx.quit_requested.clone();
    let raiser = std::thread::spawn(move || {
        while !LOOP_ENTERED.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        // Wait for each of the first two raises to be drained while the loop
        // keeps running under `inhibit-quit`, then send the third to force quit.
        for _ in 0..2 {
            bridge.request();
            while bridge.is_requested() {
                std::thread::yield_now();
            }
        }
        bridge.request();
    });
    let result = ctx.eval_str(
        "(let ((inhibit-quit t))
           (setq qtest-inhibit-iters 0)
           (qtest-mark-inhibit-loop-entered)
           (while t (setq qtest-inhibit-iters (1+ qtest-inhibit-iters))))",
    );
    raiser.join().expect("raiser thread");
    match result {
        Err(e) => {
            let msg = format!("{e}");
            assert!(msg.contains("quit"), "expected quit, got: {msg}");
        }
        Ok(v) => panic!("expected forced quit signal after 3 C-g presses, got {v:?}"),
    }
    assert!(
        LOOP_ENTERED.load(Ordering::Acquire),
        "loop must have entered inhibit-quit body before C-g presses"
    );
    assert!(
        ctx.eval_str("qtest-inhibit-iters")
            .unwrap()
            .as_fixnum()
            .is_some_and(|n| n > 0),
        "loop must have executed iterations under inhibit-quit before forced quit"
    );
    assert!(
        ctx.inhibit_quit.is_nil(),
        "inhibit-quit binding must unwind cleanly after forced quit"
    );
    assert!(!ctx.quit_requested.is_requested());
}

/// Three rapid C-g raises before a single `maybe_quit` poll must still count
/// as three interrupts rather than collapsing into a single boolean flag.
#[test]
fn three_batched_quit_requests_clear_inhibit_quit() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    ctx.set_variable("inhibit-quit", Value::T);

    // Two raises while inhibit-quit is non-nil must NOT clear inhibit-quit.
    ctx.quit_requested.request();
    ctx.quit_requested.request();
    ctx.maybe_quit()
        .expect("two C-g presses must still respect inhibit-quit");
    assert!(ctx.quit_flag_value().is_truthy());
    assert!(ctx.inhibit_quit.is_truthy());

    // The third C-g clears inhibit-quit and signals quit.
    ctx.quit_requested.request();
    let err = ctx
        .maybe_quit()
        .expect_err("third C-g must clear inhibit-quit and signal quit");
    assert!(format!("{err:?}").contains("quit"), "{err:?}");
    assert!(ctx.inhibit_quit.is_nil());

    // After quit-flag is cleared, the count resets to 1 on the next C-g.
    ctx.set_variable("inhibit-quit", Value::T);
    ctx.quit_requested.request();
    ctx.maybe_quit()
        .expect("first C-g after reset must respect inhibit-quit again");
    assert!(ctx.inhibit_quit.is_truthy());

    // Three raises delivered before a single poll also force quit.
    ctx.set_quit_flag_value(Value::NIL);
    ctx.quit_requested.request();
    ctx.quit_requested.request();
    ctx.quit_requested.request();
    let err = ctx
        .maybe_quit()
        .expect_err("three batched C-g presses must force quit");
    assert!(format!("{err:?}").contains("quit"), "{err:?}");
    assert!(ctx.inhibit_quit.is_nil());
}

/// Direct quit-interrupt recording follows the GNU `force_quit_count == 3`
/// rule (`src/keyboard.c:12714-12720`).
#[test]
fn three_record_quit_interrupts_clear_inhibit_quit() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    ctx.set_variable("inhibit-quit", Value::T);

    ctx.record_quit_interrupts(1);
    ctx.maybe_quit().expect("1st C-g respects inhibit-quit");
    assert!(ctx.inhibit_quit.is_truthy());

    ctx.record_quit_interrupts(1);
    ctx.maybe_quit().expect("2nd C-g respects inhibit-quit");
    assert!(ctx.inhibit_quit.is_truthy());

    ctx.record_quit_interrupts(1);
    let err = ctx
        .maybe_quit()
        .expect_err("3rd C-g clears inhibit-quit and signals quit");
    assert!(format!("{err:?}").contains("quit"), "{err:?}");
    assert!(ctx.inhibit_quit.is_nil());
}

/// Regression test for review B1: one physical C-g delivered through both the
/// input bridge (`quit_requested.request()`) and keyboard input handling
/// (`request_quit_from_keyboard_input`) must never be counted twice, and
/// `clear_quit_flag_after_read_char_under_inhibit_quit` clears `quit-flag`
/// (GNU `src/keyboard.c:2809-2812,3834`).
#[test]
fn request_quit_from_keyboard_input_reconciles_bridge_and_clears_quit_flag_under_inhibit_quit() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    ctx.set_variable("inhibit-quit", Value::T);

    // Ordering A: keyboard input consumes the queued bridge unit before maybe_quit.
    ctx.quit_requested.request();
    ctx.request_quit_from_keyboard_input();
    assert!(ctx.quit_flag_value().is_truthy());
    assert!(!ctx.quit_requested.is_requested());
    assert_eq!(ctx.quit_requested.force_quit_count_for_test(), 1);
    ctx.clear_quit_flag_after_read_char_under_inhibit_quit();
    assert!(ctx.quit_flag_value().is_nil());
    ctx.maybe_quit()
        .expect("no leftover quit after keyboard C-g under inhibit-quit");

    // Ordering B: maybe_quit drains the bridge before keyboard input runs.
    ctx.quit_requested.request();
    ctx.maybe_quit()
        .expect("maybe_quit respects inhibit-quit on first C-g");
    assert_eq!(ctx.quit_requested.force_quit_count_for_test(), 1);
    ctx.request_quit_from_keyboard_input();
    assert_eq!(
        ctx.quit_requested.force_quit_count_for_test(),
        1,
        "keyboard input path must not double-count the bridge press"
    );
    ctx.clear_quit_flag_after_read_char_under_inhibit_quit();
    assert!(
        ctx.quit_flag_value().is_nil(),
        "GNU keyboard.c:2811-2812 clears quit-flag when returning C-g under inhibit-quit"
    );

    // Three sequential bridge+keyboard C-g pairs under inhibit-quit never force quit.
    for _ in 0..3 {
        ctx.quit_requested.request();
        ctx.maybe_quit().expect("inhibit-quit must remain active");
        ctx.request_quit_from_keyboard_input();
        ctx.clear_quit_flag_after_read_char_under_inhibit_quit();
        ctx.maybe_quit()
            .expect("quit-flag was cleared on return so inhibit-quit stays active");
    }
    assert!(ctx.quit_flag_value().is_nil());
    assert!(ctx.inhibit_quit.is_truthy());

    // Without inhibit-quit, one C-g delivered through both bridge and queue counts once.
    ctx.set_variable("inhibit-quit", Value::NIL);
    ctx.quit_requested.clear();
    ctx.quit_requested.request();
    ctx.request_quit_from_keyboard_input();
    assert!(!ctx.quit_requested.is_requested());
    assert_eq!(ctx.quit_requested.force_quit_count_for_test(), 1);
    assert!(ctx.quit_flag_value().is_truthy());
    let err = ctx
        .maybe_quit()
        .expect_err("without inhibit-quit, maybe_quit signals quit");
    assert!(format!("{err:?}").contains("quit"), "{err:?}");
}

/// Coalesced `RawTtyBytes` chunks count every embedded 0x07 quit byte and
/// `QuitRequest::request_n` / `consume_one` maintain exact press accounting.
#[test]
fn raw_tty_bytes_counts_each_quit_byte() {
    use crate::emacs_core::eval::keyboard::InputEvent;

    let chunk = InputEvent::raw_tty_bytes_for_terminal(vec![0x07, b'x', 0x07, 0x07], 0);
    assert_eq!(chunk.default_quit_count(), 3);
    assert!(chunk.requests_default_quit());

    let req = QuitRequest::new();
    req.request_n(3);
    assert!(req.consume_one());
    assert!(req.is_requested());
    assert_eq!(req.take_count(), 2);
    assert!(!req.is_requested());
    assert!(!req.consume_one());
}

/// Default quit-char detection matches GNU `src/keyboard.c:2178-2203,3776-3788`:
/// plain `C-g` and raw `\x07` request quit, while modified chords like
/// `A-C-g`, `M-C-g`, and `C-S-g` do not.
#[test]
fn is_default_quit_char_matches_gnu_modifiers() {
    use crate::emacs_core::eval::keyboard::{KeyEvent, Modifiers};

    assert!(KeyEvent::char_with_mods('g', Modifiers::ctrl()).is_default_quit_char());
    assert!(KeyEvent::char('\x07').is_default_quit_char());

    let mut alt_ctrl = Modifiers::ctrl();
    alt_ctrl.alt = true;
    assert!(!KeyEvent::char_with_mods('g', alt_ctrl).is_default_quit_char());

    let mut shift_ctrl = Modifiers::ctrl();
    shift_ctrl.shift = true;
    assert!(!KeyEvent::char_with_mods('g', shift_ctrl).is_default_quit_char());

    let mut meta_ctrl = Modifiers::ctrl();
    meta_ctrl.meta = true;
    assert!(!KeyEvent::char_with_mods('g', meta_ctrl).is_default_quit_char());

    assert!(!KeyEvent::char('g').is_default_quit_char());
}
