//! GNU's global-lock release points in the simulated threading backend.
//!
//! GNU Emacs runs Lisp threads one at a time under `global_lock`; every
//! operation that allows another Lisp thread to run first releases that lock.
//! The pinned GNU 31.1 release points are:
//!
//! - `thread_select` (`src/thread.c:613-643`, `really_call_select`
//!   `:613-643`): `sleep-for`, `accept-process-output`, `sit-for` and the
//!   command-input wait all reach it through `wait_reading_process_output`.
//! - `Fthread_yield` (`:725-740`, `yield_callback`): release, yield the CPU,
//!   reacquire.
//! - a contended `lisp_mutex_lock_for_thread` (`:212-246`): the caller waits
//!   on the mutex's own condition variable while the global lock is released.
//! - `Fcondition_wait` (`:446-501`, `condition_wait_callback`): the mutex is
//!   unlocked and the thread waits on the condition variable, then the mutex
//!   is reacquired with the saved count.
//! - `Fthread_join` (`:1095-1144`, `thread_join_callback`): wait on the
//!   target's exit condition.
//!
//! The simulation has a single OS thread, so a release point is the only
//! place another simulated thread can run. [`SimReleasePoint`] names the GNU
//! operation that released the lock; [`Context::sim_release_point`] is the one
//! implementation of the scheduling pass every one of them calls. The pass is
//! the same for all five classes because GNU's release itself has one meaning:
//! any other live thread may run until it blocks again.

use super::*;
use crate::emacs_core::eval::Context;

/// The GNU operation class at which the simulated backend releases the
/// global lock. Callers must name the operation they implement; the
/// scheduling pass is shared (see the module documentation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum SimReleasePoint {
    /// `thread_select` (src/thread.c:613-643).
    Select,
    /// `thread-yield` (src/thread.c:725-740).
    Yield,
    /// A contended `mutex-lock` (src/thread.c:212-246).
    MutexLock,
    /// `condition-wait` (src/thread.c:446-501).
    ConditionWait,
    /// `thread-join` (src/thread.c:1095-1144).
    Join,
}

impl Context {
    /// Run the other threads this release point unblocks.
    ///
    /// Returns whether any other thread ran and reached a new state. GNU
    /// releases the global lock at every one of these operations, even when
    /// no peer is runnable; the simulation runs the pass to quiescence and
    /// then lets the caller block in its own wait. With no worker ever
    /// created the pass is a single flag test, so hot wait paths that never
    /// use Elisp threads keep their pre-threading cost.
    /// Raise a pending `thread-signal` in the current simulated worker.
    ///
    /// GNU does this in `post_acquire_global_lock` (src/thread.c:141-154),
    /// after the lock is held again, and only when `handlerlist` is set.
    /// The first acquire (`run_thread`, :805) happens before
    /// `handlerlist_sentinel` (:810-815), so this is a no-op until
    /// [`ThreadManager::mark_handlerlist_installed`] runs. Callers invoke
    /// this when their wait returns, not inside the release itself: a
    /// `sleep-for` must finish sleeping before the signal is raised.
    #[doc(hidden)]
    pub fn deliver_pending_sim_signal(&mut self) -> Result<(), Flow> {
        if self.green_threads_running() {
            return Ok(());
        }
        let id = self.threads.current_thread_id();
        if id == 0 || !self.threads.handlerlist_installed(id) {
            return Ok(());
        }
        if let Some(flow) = self.threads.take_pending_thread_signal(id) {
            return Err(flow);
        }
        Ok(())
    }

    #[doc(hidden)]
    pub fn sim_release_point(&mut self, _point: SimReleasePoint) -> Result<bool, Flow> {
        if self.green_threads_running() || !self.threads.any_thread_created() {
            return Ok(false);
        }
        let mut ran_thread = false;
        while self.sim_release_pass()? {
            ran_thread = true;
        }
        Ok(ran_thread)
    }

    /// GNU `lisp_mutex_lock_for_thread(mutex, self, new_count)` after
    /// `lisp_mutex_unlock_for_wait` (`src/thread.c:212-246`, `:276-288`):
    /// reacquire the mutex at the recursion count saved before the wait,
    /// releasing the global lock at each contention. Returns whether the
    /// mutex was acquired; `false` means no other thread could release it.
    pub(crate) fn sim_relock_after_wait(
        &mut self,
        mutex: MutexHandle,
        depth: RecursionDepth,
        point: SimReleasePoint,
    ) -> Result<bool, Flow> {
        loop {
            match self.threads.mutex_relock_after_wait(mutex, depth) {
                MutexAcquisition::Acquired => return Ok(true),
                MutexAcquisition::Blocked => {
                    if !self.sim_release_point(point)? {
                        return Ok(false);
                    }
                }
                MutexAcquisition::DepthExhausted => {
                    return Err(signal(
                        LispCondition::Error,
                        vec![Value::string("Mutex recursion depth limit exceeded")],
                    ));
                }
            }
        }
    }

    /// One scheduling pass: created workers run, then suspended waiters whose
    /// wait fired resume. Repeats until neither list produces work.
    fn sim_release_pass(&mut self) -> Result<bool, Flow> {
        let current = self.threads.current_thread_id();
        let mut progressed = false;

        // GNU run_thread (src/thread.c:760-780) executes a created worker
        // only after acquire_global_lock, i.e. at the creator's first release
        // point. Registration order keeps the simulation deterministic.
        let mut pending: Vec<u64> = self
            .threads
            .all_thread_ids()
            .into_iter()
            .filter(|id| *id != current && *id != 0)
            .filter(|id| self.threads.thread_is_pending_sim_run(*id))
            .collect();
        pending.sort_unstable();
        for id in pending {
            run_pending_sim_thread(self, id)?;
            progressed = true;
        }

        // A wait that fired resumes before the releasing thread reacquires
        // the global lock (GNU post_acquire_global_lock, thread.c:105-155).
        let mut ready: Vec<u64> = self
            .threads
            .all_thread_ids()
            .into_iter()
            .filter(|id| *id != current && *id != 0)
            .filter(|id| self.threads.thread_ready_to_resume(*id))
            .collect();
        ready.sort_unstable();
        for id in ready {
            resume_blocked_thread(self, id)?;
            progressed = true;
        }

        Ok(progressed)
    }
}
