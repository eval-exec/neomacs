//! Size-based OSR backends under `NEOVM_JIT_BG=on`: a small OSR function's
//! backend runs in line at the hot back-edge, a larger one goes to a worker.

use std::sync::atomic::Ordering;

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::vm::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{corpus, function};
use crate::emacs_core::jit::compile::force_deopt_for_test;
use crate::emacs_core::jit::force_osr_for_test;
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
    force_osr_inline_insts_for_test(Some(limit));
}

fn done() {
    force_osr_inline_insts_for_test(None);
    force_osr_for_test(false);
    force_mode_for_test(None);
}

#[test]
fn jit_bg_osr_size_policy_is_osr_only_and_inclusive() {
    force_osr_inline_insts_for_test(Some(100));
    assert!(osr_backend_in_line(JobClass::Osr, 100));
    assert!(!osr_backend_in_line(JobClass::Osr, 101));
    for class in [JobClass::Entry, JobClass::FirstSight, JobClass::Upgrade] {
        assert!(!osr_backend_in_line(class, 1), "{class:?} keeps its route");
    }
    force_osr_inline_insts_for_test(Some(0));
    assert!(!osr_backend_in_line(JobClass::Osr, 1));
    force_osr_inline_insts_for_test(None);
    assert_eq!(osr_in_line_insts(), OSR_INLINE_INSTS);
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
