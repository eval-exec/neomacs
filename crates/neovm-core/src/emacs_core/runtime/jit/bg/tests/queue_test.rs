//! The worker queue (P2.4 B12): class order (T-Q1) and the caps with their
//! drop policy (T-Q2).

use std::time::Duration;

use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::{corpus, function};
use crate::emacs_core::jit::compile::force_deopt_for_test;
use crate::emacs_core::jit::try_run_compiled;
use crate::emacs_core::value::Value;

const QUIET: Duration = Duration::from_secs(60);

/// `(lambda (x) (+ x 1))`, a fresh object each time.
fn add1() -> ByteCodeFunction {
    function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    )
}

fn entry(ctx: *mut Context, f: &ByteCodeFunction) -> Option<usize> {
    try_run_compiled(ctx, f, Value::NIL, &[Value::make_int(1)]).expect("no signal")
}

fn kind(f: &ByteCodeFunction) -> &'static str {
    cache::cache_entry_kind_for_test(f.jit_runtime().compiled_id().expect("assigned"))
}

/// T-Q1: held workers see OSR jobs first, then first-sight, then entry
/// tier-ups, each class in request order.
#[test]
fn jit_bg_queue_serves_classes_in_priority_order() {
    force_mode_for_test(Some(BgMode::Threaded));
    force_deopt_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let _ = take_served_for_test();
    let (e1, s1, e2, s2) = (add1(), add1(), add1(), add1());
    let (ops, constants, arity) = corpus().swap_remove(3);
    let looping = function(ops, constants, arity);
    {
        let _hold = hold_workers_for_test();
        assert_eq!(entry(ctx, &e1), None);
        assert!(cache::resolve_compiled_leaf_ptr(ctx, &s1).is_none());
        assert_eq!(entry(ctx, &e2), None);
        assert!(matches!(
            cache::try_run_osr_probe(
                ctx,
                &looping,
                1,
                &[Value::make_int(10), Value::make_int(0)],
                &[]
            ),
            cache::OsrProbe::Pending
        ));
        assert!(cache::resolve_compiled_leaf_ptr(ctx, &s2).is_none());
    }
    assert!(quiesce_for_test(QUIET));
    let served = take_served_for_test();
    let classes: Vec<JobClass> = served.iter().map(|&(class, _, _)| class).collect();
    assert_eq!(
        classes,
        [
            JobClass::Osr,
            JobClass::FirstSight,
            JobClass::FirstSight,
            JobClass::Entry,
            JobClass::Entry
        ]
    );
    assert!(
        served
            .windows(2)
            .all(|w| w[0].0 != w[1].0 || w[0].1 < w[1].1),
        "FIFO within a class: {served:?}"
    );
    force_mode_for_test(None);
}

/// T-Q2: a full queue refuses a job of the lowest class before its front
/// (its function asks again once its heat doubled), and makes way for a
/// higher class by dropping its newest lowest-class job, whose function
/// is told so at its next probe.
#[test]
fn jit_bg_full_queue_refuses_and_drops_the_lowest_class_newest() {
    force_mode_for_test(Some(BgMode::Threaded));
    force_queue_cap_for_test(Some(2));
    force_deopt_for_test(false);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let _ = take_served_for_test();
    let (e1, e2, e3, s1) = (add1(), add1(), add1(), add1());
    let before = stats_snapshot();
    {
        let _hold = hold_workers_for_test();
        assert_eq!(entry(ctx, &e1), None);
        assert_eq!(entry(ctx, &e2), None);
        assert_eq!((kind(&e1), kind(&e2)), ("pending", "pending"));
        // Full, and nothing queued below an entry tier-up: refused.
        let heat = e3.jit_runtime().heat();
        assert_eq!(entry(ctx, &e3), None);
        assert_eq!(kind(&e3), "deferred-backlog");
        assert_eq!(stats_snapshot().refused, before.refused + 1);
        assert!(e3.jit_runtime().deferred_heat() >= heat.saturating_mul(2));
        // A first-sight job makes way: the newest entry job is dropped.
        assert!(cache::resolve_compiled_leaf_ptr(ctx, &s1).is_none());
        assert_eq!(kind(&s1), "pending");
        assert_eq!(entry(ctx, &e2), None, "told it was dropped");
        assert_eq!(kind(&e2), "none");
        assert_eq!(
            stats_snapshot().discarded[DiscardReason::Dropped as usize],
            before.discarded[DiscardReason::Dropped as usize] + 1
        );
        let heat = e2.jit_runtime().heat();
        assert!(e2.jit_runtime().deferred_heat() >= heat.saturating_mul(2));
    }
    assert!(quiesce_for_test(QUIET));
    let served: Vec<JobClass> = take_served_for_test()
        .into_iter()
        .map(|(class, _, _)| class)
        .collect();
    assert_eq!(served, [JobClass::FirstSight, JobClass::Entry]);
    assert_eq!(entry(ctx, &e1), Some(Value::make_int(2).bits()));
    force_queue_cap_for_test(None);
    force_mode_for_test(None);
}

#[test]
fn jit_bg_mode_auto_needs_two_cpus() {
    let auto = parse_mode(Some("auto"));
    if available_cpus() >= 2 && workers_supported() {
        assert_eq!(auto, BgMode::Threaded);
    } else {
        assert_eq!(auto, BgMode::Sync);
    }
    assert_eq!(
        parse_mode(Some("on")) == BgMode::Threaded,
        workers_supported()
    );
}
