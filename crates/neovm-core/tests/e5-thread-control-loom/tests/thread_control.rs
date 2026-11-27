#![forbid(unsafe_code)]

//! E5.4b: Loom models (a)–(d) for E5.3 / E5.4 parallel thread control.
//!
//! `build.rs` copies four real product files read-only into `OUT_DIR`
//! (`green/scheduler/wait_core.rs`, `green/condition_wait.rs`,
//! `parallel_entry/driver_core.rs`, and `rendezvous/control.rs`) plus anchored
//! one-edge mutations for negative rows.

use std::sync::atomic::Ordering as StdOrdering;

/// The product key's shape, matching `keeper-driver-loom` for `control.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MutatorKey {
    pub heap: u64,
    pub world: u64,
    pub owner: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendezvousError {
    Poisoned,
    Identity,
    Loan,
    Publication,
    Generation,
    Busy,
    Readers,
    Capacity,
    Root,
}

mod model_abc;
mod model_d;
mod sync;

/// `Builder::new` reads `LOOM_MAX_PREEMPTIONS`; without it the bound is 2.
/// The env read is explicit so `scripts/ci/run-loom-models.sh` classes this
/// crate as `bounded` and exports its `BOUND` here.
pub(crate) fn model_with_bound(body: impl Fn() + Sync + Send + 'static) {
    let mut builder = loom::model::Builder::new();
    if std::env::var("LOOM_MAX_PREEMPTIONS").is_err() {
        builder.preemption_bound = Some(2);
    }
    builder.check(body);
}

// ===========================================================================
// Model (a): notify against wait registration (`wait_core.rs`)
// ===========================================================================

#[test]
fn model_a_notify_against_wait_registration() {
    let cov = model_abc::model_a_real::run(false);
    let exec = cov.executions.load(StdOrdering::Relaxed);
    let skip = cov.skipped_already_ready.load(StdOrdering::Relaxed);
    let before = cov.woke_before_park.load(StdOrdering::Relaxed);
    let parked = cov.woke_while_parked.load(StdOrdering::Relaxed);
    eprintln!(
        "E5.4b Model (a) coverage: executions={exec} skipped_already_ready={skip} \
         woke_before_park={before} woke_while_parked={parked}"
    );
    assert!(exec > 1, "Model (a) must explore multiple schedules");
    assert!(
        skip > 0,
        "Model (a) must explore B notifying before A registers"
    );
    assert!(
        before > 0,
        "Model (a) must explore wake-before-park on OwnerWake"
    );
    assert!(
        parked > 0,
        "Model (a) must explore wake-while-parked on OwnerWake"
    );
}

#[test]
#[should_panic(expected = "OwnerWake lost wake-before-park")]
fn negative_model_a_dropped_wake_flag_loses_wake_before_park() {
    // Mutated product `wait_core_drop_wake_flag.rs` drops `*woken = true` in
    // `OwnerWake::wake`, losing a wake-before-park notification.
    let _ = model_abc::model_a_drop_wake_flag::run(true);
}

// ===========================================================================
// Model (b): signal against wake and relock (`condition_wait.rs` + `wait_core.rs`)
// ===========================================================================

#[test]
fn model_b_signal_against_wake_and_relock() {
    let cov = model_abc::model_b_real::run();
    let exec = cov.executions.load(StdOrdering::Relaxed);
    let wait_calls = cov.entered_wait_with_seat.load(StdOrdering::Relaxed);
    let pcond = cov.entered_park_condition.load(StdOrdering::Relaxed);
    let prelock = cov.entered_park_mutex_relock.load(StdOrdering::Relaxed);
    let sig_after = cov
        .signal_delivered_after_exact_depth
        .load(StdOrdering::Relaxed);
    let nbs = cov.notify_before_signal.load(StdOrdering::Relaxed);
    let sw = cov.signal_woke_wait.load(StdOrdering::Relaxed);
    eprintln!(
        "E5.4b Model (b) coverage: executions={exec} wait_with_seat={wait_calls} \
         park_condition={pcond} park_mutex_relock={prelock} \
         signal_after_depth3={sig_after} notify_before_signal={nbs} signal_woke_wait={sw}"
    );
    assert!(exec > 1, "Model (b) must explore multiple schedules");
    assert_eq!(
        wait_calls, exec,
        "every execution must enter real condition_wait::wait_with_seat"
    );
    assert!(
        pcond > 0,
        "Model (b) must explore ConditionSeat::park_condition"
    );
    assert!(
        prelock > 0,
        "Model (b) must explore contended ConditionSeat::park_mutex_relock"
    );
    assert!(
        sig_after > 0,
        "Model (b) must explore thread-signal delivered after exact saved depth 3 relock"
    );
    assert!(
        nbs > 0,
        "Model (b) must explore notify winning before signal"
    );
    assert!(
        sw > 0,
        "Model (b) must explore signal waking condition-wait before notify"
    );
}

#[test]
#[should_panic(expected = "signal delivered before saved recursive mutex depth restored")]
fn negative_model_b_signal_before_relock_caught() {
    let _ = model_abc::model_b_signal_before_relock::run();
}

#[test]
fn model_b_condition_wait_wakes_mutex_waiter() {
    model_abc::model_b_real::run_mutex_wake_probe();
}

#[test]
#[should_panic(expected = "condition-wait failed to wake mutex waiter")]
fn negative_model_b_missing_mutex_wake_caught() {
    model_abc::model_b_drop_wake_mutex::run_mutex_wake_probe();
}

// ===========================================================================
// Model (c): join against exit + pre-wait signal snapshot (`wait_core.rs`)
// ===========================================================================

#[test]
fn model_c_join_against_exit() {
    let cov = model_abc::model_c::run_join_against_exit(false);
    let exec = cov.executions.load(StdOrdering::Relaxed);
    let after = cov.joined_after_exit.load(StdOrdering::Relaxed);
    let parked = cov.joined_while_parked.load(StdOrdering::Relaxed);
    let wbp = cov.wake_before_park_join.load(StdOrdering::Relaxed);
    eprintln!(
        "E5.4b Model (c) coverage: executions={exec} joined_after_exit={after} \
         joined_while_parked={parked} wake_before_park_join={wbp}"
    );
    assert!(exec > 1, "Model (c) must explore multiple schedules");
    assert!(after > 0, "Model (c) must explore join after Wait::Exited");
    assert!(
        parked > 0,
        "Model (c) must explore join parked until exit wakes it"
    );
    assert!(wbp > 0, "Model (c) must explore wake-before-park on join");
}

#[test]
fn model_c_join_pre_wait_signal_snapshot() {
    let cov = model_abc::model_c::run_join_pre_wait_signal_snapshot();
    let exec = cov.executions.load(StdOrdering::Relaxed);
    let pre = cov.snapshot_saw_pre_wait_signal.load(StdOrdering::Relaxed);
    let post = cov
        .snapshot_ignored_post_wait_signal
        .load(StdOrdering::Relaxed);
    eprintln!(
        "E5.4b Model (c) snapshot coverage: executions={exec} \
         saw_pre_wait_signal={pre} ignored_post_wait_signal={post}"
    );
    assert!(
        exec > 1,
        "Model (c) snapshot must explore multiple schedules"
    );
    assert!(
        pre > 0,
        "Model (c) snapshot must explore signal present before join"
    );
    assert!(
        post > 0,
        "Model (c) snapshot must explore signal sent after snapshot"
    );
}

#[test]
#[should_panic(expected = "exit failed to wake parked joiner")]
fn negative_model_c_exit_without_join_wake_caught() {
    let _ = model_abc::model_c::run_join_against_exit(true);
}

// ===========================================================================
// Model (d): cross-kind wake into idle Baton group (`driver_core.rs` + `control.rs`)
// ===========================================================================

#[test]
fn model_d_cross_kind_wake_into_idle_baton_group() {
    let cov = model_d::run_real();
    let exec = cov.executions.load(StdOrdering::Relaxed);
    let pturn = cov.parallel_turn_granted.load(StdOrdering::Relaxed);
    let gturn = cov.group_turn_granted.load(StdOrdering::Relaxed);
    let idle = cov.coalesced_idle_to_requested.load(StdOrdering::Relaxed);
    let dup = cov
        .coalesced_suppressed_duplicate
        .load(StdOrdering::Relaxed);
    let done = cov.baton_threads_completed.load(StdOrdering::Relaxed);
    eprintln!(
        "E5.4b Model (d) coverage: executions={exec} parallel_turns={pturn} \
         group_turns={gturn} coalesced_idle_to_requested={idle} \
         coalesced_suppressed_duplicate={dup} baton_threads_completed={done}"
    );
    assert!(exec > 1, "Model (d) must explore multiple schedules");
    assert_eq!(
        pturn, exec,
        "every execution must grant the parallel owner turn via driver_core"
    );
    assert_eq!(
        gturn, exec,
        "every execution must grant the Baton group turn via driver_core"
    );
    assert_eq!(
        idle, exec,
        "every execution must transition GroupTurn::Idle -> Requested"
    );
    assert_eq!(
        dup, exec,
        "every execution must coalesce the second wake without duplicate request"
    );
    assert_eq!(
        done,
        exec * 2,
        "every execution must run both woken Baton threads"
    );
}

#[test]
fn model_d_concurrent_wakers_coalesce_single_baton_request() {
    let cov = model_d::run_concurrent_wakers_coalesce();
    let exec = cov.executions.load(StdOrdering::Relaxed);
    let p1 = cov.p1_won_idle.load(StdOrdering::Relaxed);
    let p2 = cov.p2_won_idle.load(StdOrdering::Relaxed);
    eprintln!(
        "E5.4b Model (d) concurrent wakers coverage: executions={exec} p1_won_idle={p1} p2_won_idle={p2}"
    );
    assert!(
        exec > 1,
        "Model (d) concurrent must explore multiple schedules"
    );
    assert!(
        p1 > 0 && p2 > 0,
        "both parallel wakers must win Idle -> Requested across schedules"
    );
}

#[test]
#[should_panic(expected = "duplicate BatonRequest sent")]
fn negative_model_d_missing_idle_guard_duplicates_group_entry() {
    let _ = model_d::run_missing_idle_guard();
}

#[test]
#[should_panic(expected = "woken Baton thread never ran")]
fn negative_model_d_dropped_coalesce_loses_baton_wake() {
    let _ = model_d::run_dropped_coalesce();
}
