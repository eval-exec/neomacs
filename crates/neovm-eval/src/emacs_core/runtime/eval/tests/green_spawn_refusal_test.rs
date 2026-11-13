//! Deterministic carrier-spawn refusal (P11 SOAK finding, GNU-exact).
//!
//! GNU 31.1 `Fmake_thread` (src/thread.c:933-938) unlinks the never-started
//! thread from `all_threads` and signals `(error "Could not start a new
//! thread")` when `sys_thread_create` fails. Neomacs must do the same and
//! must also drop the scheduler registration of the worker, or a stale
//! Runnable turn would be picked and block the scheduler on a carrier that
//! does not exist.
//!
//! These tests inject the spawn failure through the pool's `cfg(test)` hook
//! (per pool, no global or env knob), so the undo path is deterministic on
//! every host. The real resource-failure end-to-end row lives in
//! `green_spawn_failure_test.rs`.

use std::sync::Arc;

use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn green() -> Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    Context::new_with_threading(ThreadingMode::Green)
}

#[test]
fn a_refused_carrier_unregisters_the_worker_and_signals() {
    let mut main = green();
    // Seat main and create the pool without spawning a carrier, then arm the
    // one-shot failure for the first fresh spawn.
    main.start_green_threads().expect("start the Green runtime");
    let control = main.with_green_runtime(|runtime| Arc::clone(&runtime.control));
    let threads_before = eval(&mut main, "(length (all-threads))");
    main.with_green_runtime(|runtime| runtime.carriers.inject_spawn_failure());

    let caught = eval(
        &mut main,
        "(condition-case make-thread-error \
             (make-thread (lambda () 1) \"refused-carrier\") \
           (error (format \"%S\" make-thread-error)))",
    );
    assert!(
        caught.contains("Could not start a new thread"),
        "the refusal must signal GNU's error, got {caught}"
    );
    assert_eq!(
        eval(&mut main, "(length (all-threads))"),
        threads_before,
        "the never-started thread must not be listed by all-threads"
    );

    let refused = main
        .with_green_runtime(|runtime| runtime.carriers.last_refusal_for_test())
        .expect("the pool records the refused worker key");
    assert!(
        !control.worker_registered_for_test(refused),
        "no scheduler record may survive the refused spawn"
    );

    // A second refusal before any carrier exists again (an idle carrier would
    // absorb the next job and no spawn would happen): the pool stays open and
    // the refusal stays typed.
    main.with_green_runtime(|runtime| runtime.carriers.inject_spawn_failure());
    let caught_again = eval(
        &mut main,
        "(condition-case second-error \
             (make-thread (lambda () 2) \"refused-again\") \
           (error (format \"%S\" second-error)))",
    );
    assert!(
        caught_again.contains("Could not start a new thread"),
        "a second refusal must signal the same error, got {caught_again}"
    );
    assert_eq!(
        eval(&mut main, "(length (all-threads))"),
        threads_before,
        "repeated refusals must not leave thread objects behind"
    );

    // A stale Runnable turn would be picked at this join and block forever on
    // the carrier that never existed; the successful spawn+join proves every
    // refused turn is gone. (The named mutation removes the unregister call
    // and this form deadlocks; the mutation run bounds it externally.)
    assert_eq!(
        eval(
            &mut main,
            "(progn (setq sr-thread (make-thread (lambda () 7) \"after-refusals\")) \
                    (thread-join sr-thread))"
        ),
        "7",
        "the scheduler must keep running after refused spawns"
    );
}
