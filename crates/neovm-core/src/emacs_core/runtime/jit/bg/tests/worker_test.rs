//! The worker thread (P2.4 B7): entry tier-ups compiled off the eval
//! thread. T-L1, T-L4, T-L5 with a real worker, T-S1 for `on`, and T-S5.

use std::time::Duration;

use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{
    captured_clif, captured_code, corpus, function, mask_code_text,
};
use crate::emacs_core::jit::compile::force_deopt_for_test;
use crate::emacs_core::jit::try_run_compiled;
use crate::emacs_core::value::Value;

const QUIET: Duration = Duration::from_secs(60);

fn threaded() {
    force_mode_for_test(Some(BgMode::Threaded));
    force_deopt_for_test(false);
}

/// `(lambda (x) (+ x 1))`.
fn add1() -> ByteCodeFunction {
    function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    )
}

fn run(ctx: *mut Context, f: &ByteCodeFunction, arg: i64) -> Option<usize> {
    try_run_compiled(ctx, f, Value::NIL, &[Value::make_int(arg)]).expect("no signal")
}

fn kind(f: &ByteCodeFunction) -> &'static str {
    cache::cache_entry_kind_for_test(f.jit_runtime().compiled_id().expect("assigned"))
}

/// T-L1: a tier-up's backend runs on the worker; the function interprets
/// until a probe after the job finished installs the leaf and enters it.
#[test]
fn jit_bg_worker_compiles_an_entry_tier_up() {
    threaded();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    let jobs = WORKER_STATS.jobs.load(Ordering::Relaxed);
    assert_eq!(run(ctx, &f, 41), None, "the tier-up interprets");
    assert_eq!(kind(&f), "pending");
    assert!(quiesce_for_test(QUIET), "the worker finishes");
    assert_eq!(WORKER_STATS.jobs.load(Ordering::Relaxed), jobs + 1);
    assert_eq!(queue::pool().workers(), 1);
    assert_eq!(run(ctx, &f, 41), Some(Value::make_int(42).bits()));
    assert_eq!(kind(&f), "compiled");
    let stats = stats_snapshot();
    assert_eq!(stats.installed[JobClass::Entry as usize], 1);
    assert!(stats.latency_us.iter().sum::<u64>() >= 1);
    force_mode_for_test(None);
}

/// T-L4: a backend panic on the worker is contained: the body becomes
/// `NotCompilable`, and the worker (on a fresh backend) compiles the next
/// job.
#[test]
fn jit_bg_worker_panic_is_contained() {
    threaded();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    let panics = WORKER_STATS.panics.load(Ordering::Relaxed);
    force_backend_panic_for_test();
    assert_eq!(run(ctx, &f, 1), None);
    assert!(quiesce_for_test(QUIET));
    assert_eq!(WORKER_STATS.panics.load(Ordering::Relaxed), panics + 1);
    assert_eq!(
        run(ctx, &f, 1),
        None,
        "the failed job leaves it interpreted"
    );
    assert_eq!(kind(&f), "not-compilable");
    let g = add1();
    assert_eq!(run(ctx, &g, 1), None);
    assert!(quiesce_for_test(QUIET));
    assert_eq!(run(ctx, &g, 1), Some(Value::make_int(2).bits()));
    force_mode_for_test(None);
}

/// T-L5: a job whose pending entry is dropped before a worker takes it is
/// skipped, not compiled.
#[test]
fn jit_bg_cancelled_job_is_skipped() {
    threaded();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    let (jobs, skipped) = (
        WORKER_STATS.jobs.load(Ordering::Relaxed),
        WORKER_STATS.skipped.load(Ordering::Relaxed),
    );
    {
        let _hold = hold_workers_for_test();
        assert_eq!(run(ctx, &f, 1), None);
        assert_eq!(kind(&f), "pending");
        cache::evict_compiled(f.jit_runtime().compiled_id().expect("id"));
        assert_eq!(kind(&f), "none");
    }
    assert!(quiesce_for_test(QUIET));
    assert_eq!(WORKER_STATS.skipped.load(Ordering::Relaxed), skipped + 1);
    assert_eq!(WORKER_STATS.jobs.load(Ordering::Relaxed), jobs);
    force_mode_for_test(None);
}

/// Compile the pipeline corpus through the dispatch seam under `mode`
/// (with an argument count no body accepts, so nothing runs), installing
/// any pending leaf, and return its machine code and CLIF.
fn corpus_by_dispatch(mode: BgMode) -> (Vec<String>, Vec<String>) {
    force_mode_for_test(Some(mode));
    force_deopt_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let wrong = [Value::NIL, Value::NIL, Value::NIL];
    let mut code = Vec::new();
    let clif = captured_clif(|| {
        code = captured_code(|| {
            for (ops, constants, arity) in corpus() {
                assert!(arity < wrong.len());
                let f = function(ops, constants, arity);
                let _ = try_run_compiled(ctx, &f, Value::NIL, &wrong);
            }
            assert!(quiesce_for_test(QUIET));
            cache::drain_ready_pending();
            assert_eq!(pending_count(), 0);
        })
    });
    force_mode_for_test(None);
    let mask = |text: &str| {
        // CLIF prints immediates digit-grouped; drop the grouping first.
        mask_code_text(&text.replace('_', ""))
    };
    (code, clif.iter().map(|c| mask(c)).collect())
}

/// T-S1 for `on`: leaves compiled on the worker have the machine code and
/// the CLIF of the legacy path.
#[test]
fn jit_bg_worker_code_and_clif_match_legacy() {
    let legacy = corpus_by_dispatch(BgMode::Legacy);
    let threaded = corpus_by_dispatch(BgMode::Threaded);
    assert_eq!(legacy.0.len(), corpus().len(), "{:?}", legacy.0);
    assert_eq!(legacy.0, threaded.0, "machine code");
    assert_eq!(legacy.1, threaded.1, "CLIF");
    assert_eq!(
        stats_snapshot().installed[JobClass::Entry as usize],
        corpus().len() as u64
    );
}

/// T-S5: the worker is not a mutator: its source names nothing of the Lisp
/// heap (the structural fence of P2.4 §4).
#[test]
fn jit_bg_worker_source_names_nothing_of_the_lisp_heap() {
    let source = include_str!("../worker.rs");
    for forbidden in ["Value", "with_tagged_heap", "tagged::", "Context"] {
        assert!(
            !source.contains(forbidden),
            "bg/worker.rs mentions `{forbidden}`"
        );
    }
}
