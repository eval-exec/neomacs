//! Service order: class first, then shortest first by virtual finish, with
//! a large job's wait bounded by its own size of later work.

use std::num::{NonZeroU64, NonZeroUsize};
use std::time::Instant;

use super::admission::QueueLimits;
use super::*;
use crate::emacs_core::jit::bg::{JobCell, JobClass};
use crate::emacs_core::jit::compile::lowering::RegallocChoice;
use crate::emacs_core::jit::compile::shared::split::JobPayload;
use cranelift_codegen::ir::Function;
use cranelift_module::Linkage;

fn state() -> PoolState {
    PoolState {
        jobs: BinaryHeap::new(),
        insts: 0,
        taken: 0,
        running: 0,
        workers: 0,
        spawn_failed: false,
        held: false,
    }
}

fn roomy() -> QueueLimits {
    QueueLimits::new(
        NonZeroUsize::new(64).expect("positive"),
        NonZeroU64::new(1 << 40).expect("positive"),
    )
}

fn job(class: JobClass, seq: u64, insts: u64) -> BackendJob {
    BackendJob {
        payload: JobPayload {
            func: Function::new(),
            name: "queue-order-only".into(),
            linkage: Linkage::Local,
            named: false,
            imports: Box::default(),
            portable: true,
            regalloc: RegallocChoice::Full,
            disasm: false,
        },
        class,
        seq,
        insts,
        enqueued_at: Instant::now(),
        cell: JobCell::new(),
    }
}

fn push(state: &mut PoolState, class: JobClass, seq: u64, insts: u64) {
    assert!(state.push_bounded(job(class, seq, insts), roomy()).is_ok());
}

fn take(state: &mut PoolState) -> u64 {
    let job = state.take_next(u64::MAX).expect("a queued job");
    state.running -= 1;
    job.seq
}

#[test]
fn jit_bg_queue_serves_the_shortest_job_of_a_class_first() {
    let mut state = state();
    push(&mut state, JobClass::Entry, 1, 9_000);
    push(&mut state, JobClass::Entry, 2, 1_500);
    push(&mut state, JobClass::Entry, 3, 3_000);
    push(&mut state, JobClass::Entry, 4, 1_500);
    assert_eq!(
        [take(&mut state), take(&mut state), take(&mut state)],
        [2, 4, 3],
        "shortest first; equal sizes oldest first"
    );
    assert_eq!(take(&mut state), 1);
    assert_eq!(state.insts, 0);
    assert_eq!(state.taken, 15_000);
}

#[test]
fn jit_bg_queue_class_still_comes_before_size() {
    let mut state = state();
    push(&mut state, JobClass::Upgrade, 1, 10);
    push(&mut state, JobClass::Entry, 2, 100);
    push(&mut state, JobClass::FirstSight, 3, 1_000);
    push(&mut state, JobClass::Osr, 4, 10_000);
    let served: Vec<u64> = (0..4).map(|_| take(&mut state)).collect();
    assert_eq!(served, [4, 3, 2, 1]);
}

#[test]
fn jit_bg_queue_a_large_job_waits_through_at_most_its_size_of_later_work() {
    let mut state = state();
    push(&mut state, JobClass::Entry, 1, 4_000);
    // A stream of smaller jobs, each pushed as the previous one is taken:
    // each overtakes the large job until the work taken since its push
    // reaches its size.
    let mut seq = 2;
    push(&mut state, JobClass::Entry, seq, 1_000);
    let mut overtaken = 0;
    loop {
        let served = take(&mut state);
        if served == 1 {
            break;
        }
        overtaken += 1;
        seq += 1;
        push(&mut state, JobClass::Entry, seq, 1_000);
    }
    assert_eq!(overtaken, 3, "4,000 instructions of later work at most");
}

#[test]
fn jit_bg_queue_take_within_leaves_a_larger_next_job_queued() {
    let mut state = state();
    push(&mut state, JobClass::Entry, 1, 600);
    assert!(state.take_next(599).is_none());
    assert_eq!(state.jobs.len(), 1);
    assert_eq!(state.running, 0);
    assert_eq!(state.take_next(600).map(|job| job.seq), Some(1));
    assert_eq!(state.running, 1);
}
