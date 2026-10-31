//! Pending compiles without a worker (P2.4 B6): the backend runs in line
//! but the leaf waits in its cache entry for the next probe, which is the
//! life of a background compile minus the thread. T-L1-T-L3, T-L6, T-G1,
//! T-G2 and the removals that supersede a pending leaf.

use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::compile_pipeline_tests::function;
use crate::emacs_core::jit::compile::force_deopt_for_test;
use crate::emacs_core::jit::reopt::{LeafOrigin, Reprofile};
use crate::emacs_core::jit::{ReoptLevel, try_run_compiled};
use crate::emacs_core::value::Value;

/// Split compiles on this thread defer their install (no worker).
fn deferred_install() {
    force_mode_for_test(Some(BgMode::Sync));
    force_deferred_install_for_test(true);
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
    // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
    unsafe { try_run_compiled(ctx, f, Value::NIL, &[Value::make_int(arg)]) }.expect("no signal")
}

fn kind(f: &ByteCodeFunction) -> &'static str {
    cache::cache_entry_kind_for_test(f.jit_runtime().compiled_id().expect("assigned"))
}

/// T-L1: the tier-up compiles, leaves the leaf pending and interprets; the
/// next probe installs it and enters it.
#[test]
fn jit_bg_pending_leaf_installs_at_the_next_probe() {
    deferred_install();
    crate::emacs_core::jit::stats::force_observe_for_test(
        crate::emacs_core::jit::stats::ObserveOverride {
            stats: true,
            naming: false,
            entry_count: false,
        },
    );
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    let before = stats_snapshot();
    assert_eq!(run(ctx, &f, 41), None, "the first call interprets");
    assert_eq!(kind(&f), "pending");
    assert_eq!(pending_count(), 1);
    assert_eq!(
        stats_snapshot().enqueued[JobClass::Entry as usize],
        before.enqueued[JobClass::Entry as usize] + 1
    );
    let entries = crate::emacs_core::jit::stats::compile_stats_snapshot().native_entries;
    assert_eq!(run(ctx, &f, 41), Some(Value::make_int(42).bits()));
    assert_eq!(kind(&f), "compiled");
    assert_eq!(pending_count(), 0);
    assert!(crate::emacs_core::jit::stats::compile_stats_snapshot().native_entries > entries);
    assert_eq!(
        stats_snapshot().installed[JobClass::Entry as usize],
        before.installed[JobClass::Entry as usize] + 1
    );
    // Installed means installed: the hold is back where it was.
    assert_eq!(f.jit_runtime().deferred_heat(), 0);
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}

/// T-L2: a pending compile parks its function: the dispatcher interprets
/// every call without a probe, a probe that does run (the funcall seam,
/// another owner's expired hold) parks it again, and the backend's
/// publication releases the park so the next call installs.
#[test]
fn jit_bg_pending_function_is_parked_until_publication() {
    use crate::emacs_core::jit::Plan;
    deferred_install();
    hold_publish_for_test(true);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    let rt = f.jit_runtime();
    let len = f.executable_ops().len();
    assert_eq!(run(ctx, &f, 1), None);
    assert_eq!(kind(&f), "pending");
    assert!(rt.tier_up_parked());
    for _ in 0..5_000 {
        assert!(matches!(rt.dispatch_sized(len), Plan::Interpret));
    }
    let probes = stats_snapshot().pending_probes;
    assert_eq!(run(ctx, &f, 1), None, "still running");
    assert!(rt.tier_up_parked(), "a probe parks it again");
    assert_eq!(stats_snapshot().pending_probes, probes + 1);
    assert_eq!(publish_held_for_test(), 1);
    assert!(!rt.tier_up_parked(), "publication releases the park");
    assert!(matches!(rt.dispatch_sized(len), Plan::Compiled));
    assert_eq!(run(ctx, &f, 1), Some(Value::make_int(2).bits()));
    assert_eq!(rt.deferred_heat(), 0, "install restores the hold");
    hold_publish_for_test(false);
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}

/// A park left by a job that published before its function was parked
/// (an in-line deferred backend) is released at once: no lost wake-up.
#[test]
fn jit_bg_park_after_publication_is_released_at_once() {
    deferred_install();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    assert_eq!(run(ctx, &f, 1), None);
    assert_eq!(kind(&f), "pending");
    assert!(!f.jit_runtime().tier_up_parked());
    assert_eq!(run(ctx, &f, 1), Some(Value::make_int(2).bits()));
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}

/// An idle drain installs without the caller's runtime: the job's own
/// parked runtime gets its hold back.
#[test]
fn jit_bg_drained_install_restores_the_hold() {
    deferred_install();
    hold_publish_for_test(true);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    assert_eq!(run(ctx, &f, 1), None);
    assert!(f.jit_runtime().tier_up_parked());
    assert_eq!(publish_held_for_test(), 1);
    cache::drain_ready_pending(None);
    assert_eq!(kind(&f), "compiled");
    assert_eq!(f.jit_runtime().deferred_heat(), 0);
    hold_publish_for_test(false);
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}

/// A pending entry dropped before its backend publishes (here a cache
/// clear) releases the park: the next call compiles again.
#[test]
fn jit_bg_superseded_pending_entry_releases_the_park() {
    use crate::emacs_core::jit::Plan;
    deferred_install();
    hold_publish_for_test(true);
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    assert_eq!(run(ctx, &f, 1), None);
    assert!(f.jit_runtime().tier_up_parked());
    cache::clear();
    assert_eq!(pending_count(), 0);
    assert!(!f.jit_runtime().tier_up_parked());
    let len = f.executable_ops().len();
    assert!(matches!(
        f.jit_runtime().dispatch_sized(len),
        Plan::Compiled
    ));
    // The dropped job's held result is no one's: publishing it releases
    // nothing.
    assert_eq!(publish_held_for_test(), 1);
    hold_publish_for_test(false);
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}

/// T-L3: a failed backend makes the body `NotCompilable`, as a failed
/// in-line compile does, and the dispatcher remembers the verdict.
#[test]
fn jit_bg_failed_backend_leaves_the_body_not_compilable() {
    deferred_install();
    fail_next_backend_for_test();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    assert_eq!(run(ctx, &f, 1), None);
    assert_eq!(kind(&f), "pending");
    let failed = stats_snapshot().discarded[DiscardReason::Failed as usize];
    assert_eq!(run(ctx, &f, 1), None, "the install finds the failure");
    assert_eq!(kind(&f), "not-compilable");
    assert_eq!(
        stats_snapshot().discarded[DiscardReason::Failed as usize],
        failed + 1
    );
    assert!(
        matches!(
            f.jit_runtime().dispatch_sized(4),
            crate::emacs_core::jit::Plan::Interpret
        ),
        "the rejection is remembered"
    );
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}

/// T-L6: the drains (before a miss's compile, at idle) install the
/// finished job of a function that is not called again.
#[test]
fn jit_bg_drain_installs_a_job_whose_function_is_not_called_again() {
    deferred_install();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    let g = add1();
    assert_eq!(run(ctx, &f, 1), None);
    assert_eq!(kind(&f), "pending");
    // The miss that compiles `g` drains first: `f`'s finished job installs.
    assert_eq!(run(ctx, &g, 1), None);
    assert_eq!(kind(&f), "compiled");
    assert_eq!(kind(&g), "pending");
    assert_eq!(pending_count(), 1);
    // And the idle drain installs `g`'s.
    cache::drain_ready_pending(None);
    assert_eq!(pending_count(), 0);
    assert_eq!(kind(&f), "compiled");
    assert_eq!(kind(&g), "compiled");
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}

/// A first-sight resolve installs a finished pending compile instead of
/// taking the strict path.
#[test]
fn jit_bg_first_sight_installs_a_finished_pending_compile() {
    deferred_install();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    assert_eq!(run(ctx, &f, 1), None);
    assert_eq!(kind(&f), "pending");
    assert!(cache::resolve_compiled_leaf_ptr(ctx, &f).is_some());
    assert_eq!(kind(&f), "compiled");
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}

/// T-G1: the heap constants of a pending leaf are GC roots. Here nothing
/// else roots them (the function and the list live only in Rust locals), so
/// after three collections and a heap churn the installed leaf must still
/// load the same, intact list.
#[test]
fn jit_bg_pending_leaf_roots_its_constants_across_gc() {
    deferred_install();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let list = ev
        .eval_str("(list 'jit-bg-a (make-string 3 ?x) (cons 1 2))")
        .expect("list");
    let f = function(vec![Op::Constant(0), Op::Return], vec![list], 0);
    assert_eq!(
        // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
        unsafe { try_run_compiled(ctx, &f, Value::NIL, &[]) }.expect("no signal"),
        None
    );
    assert_eq!(kind(&f), "pending");
    for _ in 0..3 {
        ev.gc_collect();
        ev.eval_str("(make-list 20000 (cons 7 (make-string 3 ?y)))")
            .expect("churn");
    }
    // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
    let bits = unsafe { try_run_compiled(ctx, &f, Value::NIL, &[]) }
        .expect("no signal")
        .expect("installed and run");
    assert_eq!(bits, list.bits(), "the same object");
    let got = Value::from_bits(bits);
    assert_eq!(
        crate::emacs_core::print::print_value(&got),
        "(jit-bg-a \"xxx\" (1 . 2))",
        "its contents intact"
    );
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}

/// T-G2: the root walk and the handshake's size probe include a pending
/// leaf's reloc constants.
#[test]
fn jit_bg_root_walk_includes_pending_constants() {
    deferred_install();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let list = ev.eval_str("(list 'jit-bg-root)").expect("list");
    let f = function(vec![Op::Constant(0), Op::Return], vec![list], 0);
    let (_, slots_before) = cache::compiled_cache_probe();
    assert_eq!(
        // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
        unsafe { try_run_compiled(ctx, &f, Value::NIL, &[]) }.expect("no signal"),
        None
    );
    assert_eq!(kind(&f), "pending");
    let mut roots = Vec::new();
    cache::collect_jit_reloc_gc_roots(&mut roots);
    assert!(roots.iter().any(|v| v.bits() == list.bits()));
    let (_, slots) = cache::compiled_cache_probe();
    assert_eq!(slots, slots_before + 1);
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}

/// Removing a pending entry (here a `make-closure` prefix widening's
/// eviction) cancels its job and counts it superseded; the next hot call
/// compiles again.
#[test]
fn jit_bg_evicted_pending_compile_is_superseded() {
    deferred_install();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    assert_eq!(run(ctx, &f, 1), None);
    let superseded = stats_snapshot().discarded[DiscardReason::Superseded as usize];
    cache::evict_compiled(f.jit_runtime().compiled_id().expect("id"));
    assert_eq!(kind(&f), "none");
    assert_eq!(pending_count(), 0);
    assert_eq!(
        stats_snapshot().discarded[DiscardReason::Superseded as usize],
        superseded + 1
    );
    assert_eq!(run(ctx, &f, 1), None, "compiles again, pending again");
    assert_eq!(kind(&f), "pending");
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}

/// A deopt invalidation of the source drops its pending compile as it
/// would retire the entry leaf: the same re-profile window follows.
#[test]
fn jit_bg_reopt_invalidation_drops_a_pending_compile() {
    deferred_install();
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context;
    let f = add1();
    assert_eq!(run(ctx, &f, 1), None);
    assert_eq!(kind(&f), "pending");
    cache::invalidate_for_reopt(
        &f,
        LeafOrigin::Entry,
        ReoptLevel::Speculative,
        Reprofile::Window,
    );
    assert_eq!(kind(&f), "deferred-reopt");
    assert_eq!(pending_count(), 0);
    let rt = f.jit_runtime();
    assert!(
        rt.deferred_heat() > rt.heat(),
        "a re-profile window holds it"
    );
    force_deferred_install_for_test(false);
    force_mode_for_test(None);
}
