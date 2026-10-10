//! Resource-policy regressions using opaque payloads: no codegen or workers.

use std::sync::Arc;
use std::time::Instant;

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
        running: 0,
        workers: 0,
        spawn_failed: false,
        held: false,
    }
}

fn limits(jobs: usize, insts: u64) -> QueueLimits {
    QueueLimits::new(
        NonZeroUsize::new(jobs).expect("positive fixture"),
        NonZeroU64::new(insts).expect("positive fixture"),
    )
}

fn job(class: JobClass, seq: u64, insts: u64) -> BackendJob {
    BackendJob {
        payload: JobPayload {
            func: Function::new(),
            name: "queue-policy-only".into(),
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

#[test]
fn jit_bg_oversized_job_is_dropped_from_an_empty_queue() {
    let mut state = state();
    let oversized = job(JobClass::Entry, 1, 9);
    let cell = Arc::clone(&oversized.cell);
    assert!(state.push_bounded(oversized, limits(2, 8)).is_ok());
    assert!(state.jobs.is_empty());
    assert_eq!(state.insts, 0);
    assert!(cell.take_out().expect("dropped result").dropped);
}

#[test]
fn jit_bg_oversized_priority_job_preserves_all_queued_victims() {
    let mut state = state();
    let first = job(JobClass::Entry, 1, 2);
    let second = job(JobClass::Entry, 2, 3);
    let first_cell = Arc::clone(&first.cell);
    let second_cell = Arc::clone(&second.cell);
    assert!(state.push_bounded(first, limits(2, 8)).is_ok());
    assert!(state.push_bounded(second, limits(2, 8)).is_ok());
    let oversized = job(JobClass::FirstSight, 3, 9);
    let cell = Arc::clone(&oversized.cell);
    assert!(state.push_bounded(oversized, limits(2, 8)).is_ok());
    assert_eq!(state.insts, 5);
    assert_eq!(state.jobs.len(), 2);
    assert_eq!(
        state.jobs.pop().expect("first survives").key(),
        (JobClass::Entry, 1)
    );
    assert_eq!(
        state.jobs.pop().expect("second survives").key(),
        (JobClass::Entry, 2)
    );
    assert!(!first_cell.is_done());
    assert!(!second_cell.is_done());
    assert!(cell.take_out().expect("incoming drop").dropped);
}

#[test]
fn jit_bg_exact_instruction_cap_and_overflow_refusal_preserve_old_job() {
    let mut state = state();
    let full = job(JobClass::Entry, 1, u64::MAX);
    let cell = Arc::clone(&full.cell);
    assert!(state.push_bounded(full, limits(2, u64::MAX)).is_ok());
    let refused = state
        .push_bounded(job(JobClass::Entry, 2, 1), limits(2, u64::MAX))
        .err()
        .expect("newest lower priority is refused");
    assert_eq!(refused.seq, 2);
    assert!(!refused.cell.is_done());
    assert!(!cell.is_done());
    assert_eq!(state.insts, u64::MAX);
    assert_eq!(state.jobs.len(), 1);
}

#[test]
fn jit_bg_overflowing_priority_admission_replaces_only_the_old_job() {
    let mut state = state();
    let full = job(JobClass::Entry, 1, u64::MAX);
    let cell = Arc::clone(&full.cell);
    assert!(state.push_bounded(full, limits(2, u64::MAX)).is_ok());
    assert!(
        state
            .push_bounded(job(JobClass::FirstSight, 2, 1), limits(2, u64::MAX))
            .is_ok()
    );
    assert!(cell.take_out().expect("victim dropped").dropped);
    assert_eq!(state.insts, 1);
    assert_eq!(state.jobs.len(), 1);
    assert_eq!(
        state.jobs.pop().expect("priority job").key(),
        (JobClass::FirstSight, 2)
    );
}

#[test]
fn jit_bg_fitting_priority_job_drops_only_needed_newest_victim() {
    let mut state = state();
    let first = job(JobClass::Entry, 1, 2);
    let second = job(JobClass::Entry, 2, 3);
    let first_cell = Arc::clone(&first.cell);
    let second_cell = Arc::clone(&second.cell);
    assert!(state.push_bounded(first, limits(2, 8)).is_ok());
    assert!(state.push_bounded(second, limits(2, 8)).is_ok());
    assert!(
        state
            .push_bounded(job(JobClass::FirstSight, 3, 6), limits(2, 8))
            .is_ok()
    );
    assert_eq!(state.insts, 8);
    assert_eq!(state.jobs.len(), 2);
    assert_eq!(
        state.jobs.pop().expect("priority job").key(),
        (JobClass::FirstSight, 3)
    );
    assert_eq!(
        state.jobs.pop().expect("oldest survives").key(),
        (JobClass::Entry, 1)
    );
    assert!(!first_cell.is_done());
    assert!(second_cell.take_out().expect("newest victim").dropped);
}
