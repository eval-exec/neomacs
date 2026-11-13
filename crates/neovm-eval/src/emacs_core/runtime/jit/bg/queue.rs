//! The background backend's job queue (P2.4 §3.4): one process-wide
//! priority queue that eval threads push into and the worker threads pop
//! from. The eval thread holds the lock only to push (never across Lisp, a
//! safepoint or a compile), and the GC never takes it.
//!
//! Service order: class first, then the shortest job first. A job's size is
//! its CLIF instruction count, which a backend's time follows; within a
//! class each job is tagged at push with its virtual finish, the
//! instructions the workers had taken by then plus its own, and the lowest
//! tag goes first. A small job thus overtakes a large one queued before it,
//! and a large one waits through at most its own size of later, smaller
//! work: the tags of later pushes grow with every job taken.
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

/// A queued job and its virtual finish (see the module docs).
struct Queued {
    job: BackendJob,
    finish: u64,
}

impl Queued {
    /// Tag `job` against `taken`, the instructions the workers have taken.
    #[doc(hidden)]
    pub fn new(job: BackendJob, taken: u64) -> Queued {
        let finish = taken.saturating_add(job.insts);
        Queued { job, finish }
    }

    /// Service order: the lowest class, then the earliest virtual finish,
    /// then the oldest.
    #[doc(hidden)]
    pub fn service_key(&self) -> (JobClass, u64, u64) {
        (self.job.class, self.finish, self.job.seq)
    }

    /// Drop order (`admission`): the highest class, then the newest, goes
    /// first when the queue is full.
    #[doc(hidden)]
    pub fn key(&self) -> (JobClass, u64) {
        (self.job.class, self.job.seq)
    }
}

impl PartialEq for Queued {
    fn eq(&self, other: &Self) -> bool {
        self.service_key() == other.service_key()
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
        // `BinaryHeap` pops the greatest: reverse the service order.
        other.service_key().cmp(&self.service_key())
    }
}

struct PoolState {
    jobs: BinaryHeap<Queued>,
    /// CLIF instructions of the queued jobs.
    insts: u64,
    /// CLIF instructions of every job a worker has taken: the virtual clock
    /// that tags pushes (see the module docs).
    taken: u64,
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

impl PoolState {
    /// Take the next job in service order if it has at most `insts` CLIF
    /// instructions, leasing it to the calling worker.
    #[doc(hidden)]
    pub fn take_next(&mut self, insts: u64) -> Option<BackendJob> {
        if self.jobs.peek()?.job.insts > insts {
            return None;
        }
        let Queued { job, .. } = self.jobs.pop()?;
        self.running += 1;
        self.insts -= job.insts;
        self.taken = self.taken.saturating_add(job.insts);
        Some(job)
    }
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
            taken: 0,
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
    #[doc(hidden)]
    pub fn lock(&self) -> MutexGuard<'_, PoolState> {
        // Poison-tolerant: a worker's backend panics inside `catch_unwind`,
        // never while holding this lock.
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Queue `job` for a worker, starting the workers on first use. `Err`
    /// hands the job back when no worker can run it (a spawn failed).
    #[doc(hidden)]
    pub fn push(&'static self, job: BackendJob) -> Result<(), BackendJob> {
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
    #[doc(hidden)]
    pub fn admits(&self, class: JobClass) -> bool {
        let state = self.lock();
        if state.jobs.len() < super::queue_cap() && state.insts < super::queue_insts_cap() {
            return true;
        }
        state.jobs.iter().any(|queued| queued.job.class > class)
    }

    /// The next job for a worker: blocks until there is one.
    #[doc(hidden)]
    pub fn pop(&self) -> BackendJob {
        let mut state = self.lock();
        loop {
            #[cfg(test)]
            let held = state.held;
            #[cfg(not(test))]
            let held = false;
            if !held && let Some(job) = state.take_next(u64::MAX) {
                return job;
            }
            state = self.work.wait(state).unwrap_or_else(|p| p.into_inner());
        }
    }

    /// Take an immediately ready job of any size without waiting (tests).
    #[cfg(test)]
    #[doc(hidden)]
    pub fn try_pop(&self) -> Option<BackendJob> {
        self.try_pop_within(u64::MAX)
    }

    /// Take an immediately ready job without waiting, if the next one has
    /// at most `insts` CLIF instructions. A worker with unsealed members
    /// must flush instead of blocking for more work, and grows a batch only
    /// while its first members' results are not held back long by the
    /// later ones'.
    #[doc(hidden)]
    pub fn try_pop_within(&self, insts: u64) -> Option<BackendJob> {
        let mut state = self.lock();
        #[cfg(test)]
        if state.held {
            return None;
        }
        state.take_next(insts)
    }

    /// A private preloaded queue for worker protocol tests; no process
    /// pool, knobs, thread startup or queue push path is involved.
    #[cfg(test)]
    #[doc(hidden)]
    pub fn from_jobs_for_worker_test(jobs: Vec<BackendJob>) -> Pool {
        let insts = jobs.iter().map(|job| job.insts).sum();
        Pool {
            state: Mutex::new(PoolState {
                jobs: jobs.into_iter().map(|job| Queued::new(job, 0)).collect(),
                insts,
                taken: 0,
                running: 0,
                workers: 0,
                spawn_failed: false,
                held: false,
            }),
            work: Condvar::new(),
            idle: Condvar::new(),
        }
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn running_for_worker_test(&self) -> usize {
        self.lock().running
    }

    /// Identity only: local protocol tests never consume global fault/log hooks.
    #[cfg(test)]
    #[doc(hidden)]
    pub fn is_global_for_worker_test(&self) -> bool {
        POOL.get().is_some_and(|pool| std::ptr::eq(self, pool))
    }

    /// Add ready work to an independent local pool without touching admission knobs.
    #[cfg(test)]
    #[doc(hidden)]
    pub fn push_ready_for_worker_test(&self, job: BackendJob) {
        assert!(
            !self.is_global_for_worker_test(),
            "only an owned local queue"
        );
        let mut state = self.lock();
        state.insts += job.insts;
        let queued = Queued::new(job, state.taken);
        state.jobs.push(queued);
    }

    /// A worker finished (or skipped) the job it popped.
    #[doc(hidden)]
    pub fn finish(&self) {
        let mut state = self.lock();
        state.running -= 1;
        let quiet = state.running == 0 && state.jobs.is_empty();
        drop(state);
        if quiet {
            self.idle.notify_all();
        }
    }

    /// Worker threads started.
    #[doc(hidden)]
    pub fn workers(&self) -> usize {
        self.lock().workers
    }

    /// Wait until no job is queued or running, up to `timeout`; whether it
    /// got there (tests).
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn quiesce(&self, timeout: std::time::Duration) -> bool {
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
    #[doc(hidden)]
    pub fn set_held(&self, held: bool) {
        self.lock().held = held;
        if !held {
            self.work.notify_all();
        }
    }
}

#[cfg(test)]
#[path = "queue/tests/order_test.rs"]
mod order_tests;
