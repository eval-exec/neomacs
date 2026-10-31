//! Size-based backends under `NEOVM_JIT_BG=on`: a small function's OSR,
//! first-sight or entry backend runs in line, a larger one goes to a worker;
//! an upgrade always defers.

use std::sync::atomic::Ordering;

use super::*;
use crate::emacs_core::bytecode::vm::Vm;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{corpus, function};
use crate::emacs_core::jit::compile::force_deopt_for_test;
use crate::emacs_core::jit::{force_osr_for_test, try_run_compiled};
use crate::emacs_core::value::Value;

/// `(lambda (n) (let ((i 0)) (while (< i n) (setq i (1+ i))) i))`, already
/// hot, so its first 256-iteration wrap asks for an OSR leaf.
fn counting_loop() -> ByteCodeFunction {
    let (ops, constants, arity) = corpus().swap_remove(3);
    let f = function(ops, constants, arity);
    f.jit_runtime().set_hot_for_test();
    f
}

fn count(ctx: &mut Context, f: &ByteCodeFunction, n: i64) -> Value {
    Vm::from_context(ctx)
        .execute(f, vec![Value::make_int(n)])
        .expect("runs")
}

fn threaded(limit: u64) {
    force_mode_for_test(Some(BgMode::Threaded));
    force_osr_for_test(true);
    force_deopt_for_test(false);
    force_inline_insts_for_test(Some(limit));
}

fn done() {
    force_inline_insts_for_test(None);
    force_osr_for_test(false);
    force_mode_for_test(None);
}

#[test]
fn jit_bg_inline_size_policy_spares_upgrades_and_is_inclusive() {
    force_inline_insts_for_test(Some(100));
    for class in [JobClass::Osr, JobClass::FirstSight, JobClass::Entry] {
        assert!(backend_in_line(class, 100), "{class:?} at the limit");
        assert!(!backend_in_line(class, 101), "{class:?} over the limit");
    }
    assert!(
        !backend_in_line(JobClass::Upgrade, 1),
        "an upgrade's old leaf serves meanwhile"
    );
    force_inline_insts_for_test(Some(0));
    assert!(!backend_in_line(JobClass::Entry, 1));
    force_inline_insts_for_test(None);
    assert_eq!(in_line_insts(), 0, "unit tests defer unless they pin it");
    assert_eq!(in_line_insts_knob(), INLINE_INSTS);
}

/// `(lambda (x) (+ x 1))`.
fn add1() -> ByteCodeFunction {
    function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    )
}

/// Under the limit an entry tier-up compiles in line: the call that asked
/// for it already runs the leaf, and no entry job is enqueued.
#[test]
fn jit_bg_small_entry_backend_runs_in_line() {
    threaded(u64::MAX);
    let mut ctx = Context::new();
    let f = add1();
    let before = stats_snapshot();
    // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
    let got = unsafe { try_run_compiled(&mut ctx, &f, Value::NIL, &[Value::make_int(41)]) };
    assert_eq!(got.expect("no signal"), Some(Value::make_int(42).bits()));
    assert_eq!(
        stats_snapshot().enqueued[JobClass::Entry as usize],
        before.enqueued[JobClass::Entry as usize]
    );
    done();
}

/// Over the limit the same tier-up goes to the worker and the function
/// interprets until its leaf installs.
#[test]
fn jit_bg_large_entry_backend_goes_to_a_worker() {
    threaded(0);
    let mut ctx = Context::new();
    let f = add1();
    let before = stats_snapshot();
    // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
    let got = unsafe { try_run_compiled(&mut ctx, &f, Value::NIL, &[Value::make_int(41)]) };
    assert_eq!(got.expect("no signal"), None, "the tier-up interprets");
    assert_eq!(
        stats_snapshot().enqueued[JobClass::Entry as usize],
        before.enqueued[JobClass::Entry as usize] + 1
    );
    assert!(quiesce_for_test(std::time::Duration::from_secs(60)));
    done();
}

/// Under the limit the first hot wrap compiles and transfers at once: no
/// OSR job is enqueued and the loop never waits for a worker.
#[test]
fn jit_bg_small_osr_backend_runs_in_line() {
    threaded(u64::MAX);
    let mut ctx = Context::new();
    let f = counting_loop();
    let before = stats_snapshot();
    let transfers = cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed);
    assert_eq!(count(&mut ctx, &f, 100_000), Value::make_int(100_000));
    let after = stats_snapshot();
    assert_eq!(
        after.enqueued[JobClass::Osr as usize],
        before.enqueued[JobClass::Osr as usize]
    );
    assert_eq!(after.osr_waits, before.osr_waits);
    assert_eq!(
        cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed),
        transfers + 1
    );
    done();
}

/// Over the limit the same loop's backend goes to the worker.
#[test]
fn jit_bg_large_osr_backend_goes_to_a_worker() {
    threaded(0);
    let mut ctx = Context::new();
    let f = counting_loop();
    let before = stats_snapshot();
    assert_eq!(count(&mut ctx, &f, 100_000), Value::make_int(100_000));
    assert_eq!(
        stats_snapshot().enqueued[JobClass::Osr as usize],
        before.enqueued[JobClass::Osr as usize] + 1
    );
    assert!(quiesce_for_test(std::time::Duration::from_secs(60)));
    done();
}
