//! The background backend's job queue (P2.4 §3.4): one process-wide
//! priority queue (class first, then FIFO) that eval threads push into and
//! the worker threads pop from. The eval thread holds the lock only to push
//! (never across Lisp, a safepoint or a compile), and the GC never takes
//! it.
//!
//! Bounded (`NEOVM_JIT_BG_QUEUE` jobs, `NEOVM_JIT_BG_QUEUE_INSTS` CLIF
//! instructions): when a job does not fit, the lowest-class, newest job
//! goes -- a queued one is dropped (its cell reports it, and its function
//! asks again once its heat doubles), or the new one is refused before its
//! front runs ([`Pool::admits`]).

use std::cmp::Ordering as CmpOrdering;
use std::collections::BinaryHeap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

mod admission;

use super::{JobCell, JobClass};
use crate::emacs_core::jit::compile::shared::split::JobPayload;
use admission::QueueLimits;

/// One job for a backend thread: the payload and where its result goes.
/// Plain data (see `JobPayload`): nothing here reaches the Lisp heap. With
/// raw values `!Send`, the pin below also proves the job carries none.
pub(crate) struct BackendJob {
    pub(crate) payload: JobPayload,
    pub(crate) class: JobClass,
    pub(crate) seq: u64,
    pub(crate) enqueued_at: Instant,
    pub(crate) cell: Arc<JobCell>,
    /// The payload's CLIF instruction count (the queue's size unit).
    pub(crate) insts: u64,
}

static_assertions::assert_impl_all!(BackendJob: Send);

/// Heap order: the lowest class first, then the oldest.
struct Queued(BackendJob);

impl Queued {
    fn key(&self) -> (JobClass, u64) {
        (self.0.class, self.0.seq)
    }
}

impl PartialEq for Queued {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for Queued {}

impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for Queued {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // `BinaryHeap` pops the greatest: reverse the (class, seq) order.
        other.key().cmp(&self.key())
    }
}

struct PoolState {
    jobs: BinaryHeap<Queued>,
    /// CLIF instructions of the queued jobs.
    insts: u64,
    /// Jobs a worker has taken and not yet finished.
    running: usize,
    /// Worker threads started.
    workers: usize,
    /// A worker could not be started: every later compile runs in line.
    spawn_failed: bool,
    /// Test gate: workers take no job while it is set.
    #[cfg(test)]
    held: bool,
}

/// The queue and its two wake-ups: `work` for the workers, `idle` for a
/// test waiting for quiescence.
pub(crate) struct Pool {
    state: Mutex<PoolState>,
    work: Condvar,
    idle: Condvar,
}

static POOL: OnceLock<Pool> = OnceLock::new();

pub(crate) fn pool() -> &'static Pool {
    POOL.get_or_init(|| Pool {
        state: Mutex::new(PoolState {
            jobs: BinaryHeap::new(),
            insts: 0,
            running: 0,
            workers: 0,
            spawn_failed: false,
            #[cfg(test)]
            held: false,
        }),
        work: Condvar::new(),
        idle: Condvar::new(),
    })
}

impl Pool {
    fn lock(&self) -> MutexGuard<'_, PoolState> {
        // Poison-tolerant: a worker's backend panics inside `catch_unwind`,
        // never while holding this lock.
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Queue `job` for a worker, starting the workers on first use. `Err`
    /// hands the job back when no worker can run it (a spawn failed).
    pub(crate) fn push(&'static self, job: BackendJob) -> Result<(), BackendJob> {
        let limits = QueueLimits::configured();
        // An intrinsically oversized job must not evict useful work, spawn
        // a worker, or trigger the Err path's synchronous codegen fallback.
        if !limits.fits(job.insts) {
            job.cell.publish_dropped();
            return Ok(());
        }
        let mut state = self.lock();
        if state.spawn_failed {
            return Err(job);
        }
        let wanted = super::worker_threads();
        while state.workers < wanted {
            let index = state.workers;
            match super::spawn_worker(index, self) {
                Ok(()) => state.workers += 1,
                Err(err) => {
                    tracing::warn!(
                        target: "neovm_jit::bg",
                        %err,
                        "cannot start a JIT backend thread; compiles run in line"
                    );
                    if state.workers == 0 {
                        state.spawn_failed = true;
                        return Err(job);
                    }
                    break;
                }
            }
        }
        state.push_bounded(job, limits)?;
        drop(state);
        self.work.notify_one();
        Ok(())
    }

    /// Whether a job of `class` would be queued now rather than refused
    /// (checked before its front runs): there is room, or a queued job of a
    /// lower class would make way.
    pub(crate) fn admits(&self, class: JobClass) -> bool {
        let state = self.lock();
        if state.jobs.len() < super::queue_cap() && state.insts < super::queue_insts_cap() {
            return true;
        }
        state.jobs.iter().any(|queued| queued.0.class > class)
    }

    /// The next job for a worker: blocks until there is one.
    pub(crate) fn pop(&self) -> BackendJob {
        let mut state = self.lock();
        loop {
            #[cfg(test)]
            let held = state.held;
            #[cfg(not(test))]
            let held = false;
            if !held && let Some(Queued(job)) = state.jobs.pop() {
                state.running += 1;
                state.insts -= job.insts;
                return job;
            }
            state = self.work.wait(state).unwrap_or_else(|p| p.into_inner());
        }
    }

    /// A worker finished (or skipped) the job it popped.
    pub(crate) fn finish(&self) {
        let mut state = self.lock();
        state.running -= 1;
        let quiet = state.running == 0 && state.jobs.is_empty();
        drop(state);
        if quiet {
            self.idle.notify_all();
        }
    }

    /// Worker threads started.
    pub(crate) fn workers(&self) -> usize {
        self.lock().workers
    }

    /// Wait until no job is queued or running, up to `timeout`; whether it
    /// got there (tests).
    #[cfg(test)]
    pub(crate) fn quiesce(&self, timeout: std::time::Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.lock();
        loop {
            #[cfg(test)]
            let held = state.held;
            #[cfg(not(test))]
            let held = false;
            if state.running == 0 && (state.jobs.is_empty() || held) {
                return state.jobs.is_empty();
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            state = self
                .idle
                .wait_timeout(state, deadline - now)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }

    /// Stop or restart the workers taking jobs (tests).
    #[cfg(test)]
    pub(crate) fn set_held(&self, held: bool) {
        self.lock().held = held;
        if !held {
            self.work.notify_all();
        }
    }
}
