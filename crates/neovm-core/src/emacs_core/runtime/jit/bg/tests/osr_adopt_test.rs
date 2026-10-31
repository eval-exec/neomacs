//! An OSR request for a body whose entry compile is pending waits on that
//! job instead of compiling the body a second time; once the entry job has
//! settled, a loop still running gets its own OSR leaf.

use std::sync::atomic::Ordering;

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::vm::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{corpus, function};
use crate::emacs_core::jit::compile::force_deopt_for_test;
use crate::emacs_core::jit::{force_osr_for_test, try_run_compiled};
use crate::emacs_core::value::Value;

/// `(lambda (n) (let ((i 0)) (while (< i n) (setq i (1+ i))) i))`, hot.
fn counting_loop() -> ByteCodeFunction {
    let (ops, constants, arity) = corpus().swap_remove(3);
    let f = function(ops, constants, arity);
    f.jit_runtime().set_hot_for_test();
    f
}

/// An interpreted activation (its loop's hot wraps ask for OSR).
fn count(ctx: &mut Context, f: &ByteCodeFunction, n: i64) -> Value {
    Vm::from_context(ctx)
        .execute(f, vec![Value::make_int(n)])
        .expect("runs")
}

fn transfers() -> u64 {
    cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed)
}

fn kind(f: &ByteCodeFunction) -> &'static str {
    cache::cache_entry_kind_for_test(f.jit_runtime().compiled_id().expect("assigned"))
}

#[test]
fn jit_bg_osr_waits_on_the_pending_entry_compile_of_its_body() {
    force_mode_for_test(Some(BgMode::Sync));
    force_deferred_install_for_test(true);
    hold_publish_for_test(true);
    force_osr_for_test(true);
    force_deopt_for_test(false);
    let mut ctx = Context::new();
    let ptr = &mut ctx as *mut Context;
    let f = counting_loop();
    // The entry tier-up goes to the background and its call interprets.
    assert_eq!(
        // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
        unsafe { try_run_compiled(ptr, &f, Value::NIL, &[Value::make_int(3)]) }.expect("no signal"),
        None
    );
    assert_eq!(kind(&f), "pending");
    let (before, stats) = (transfers(), stats_snapshot());
    assert_eq!(count(&mut ctx, &f, 100_000), Value::make_int(100_000));
    let after = stats_snapshot();
    assert_eq!(
        after.enqueued[JobClass::Osr as usize],
        stats.enqueued[JobClass::Osr as usize],
        "no OSR compile of a body whose entry compile is pending"
    );
    assert!(
        after.osr_adopted - stats.osr_adopted > 100,
        "every hot wrap waited on the entry job: {}",
        after.osr_adopted - stats.osr_adopted
    );
    assert_eq!(transfers(), before);
    assert!(cache::osr_compile_in_flight(&f, 1));
    // The entry job publishes: the next activation's first hot wrap
    // installs it, and a loop still running at its next wrap gets its OSR
    // leaf after all.
    hold_publish_for_test(false);
    assert_eq!(publish_held_for_test(), 1);
    assert!(!cache::osr_compile_in_flight(&f, 1));
    assert_eq!(count(&mut ctx, &f, 100_000), Value::make_int(100_000));
    assert_eq!(kind(&f), "compiled");
    assert_eq!(
        stats_snapshot().enqueued[JobClass::Osr as usize],
        stats.enqueued[JobClass::Osr as usize] + 1
    );
    assert_eq!(
        transfers() - before,
        1,
        "the long loop moved to native code"
    );
    force_deferred_install_for_test(false);
    force_osr_for_test(false);
    force_mode_for_test(None);
}
