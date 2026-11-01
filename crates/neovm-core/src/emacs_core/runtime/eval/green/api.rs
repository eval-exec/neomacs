//! GNU's thread primitives in a Green World once its runtime started
//! (ported from F4's `facade_thread_operation`, with GNU's rules from
//! `src/thread.c`). Each runs on the thread holding the Baton, whose
//! `Context::threads` is the World's `ThreadManager`.

use super::{ConditionId, MutexId, ThreadKey, Wait};
use crate::emacs_core::error::{EvalResult, expect_args, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::threads::{
    CondVarHandle, MutexAcquisition, MutexHandle, MutexRelease, NotificationScope,
};
use crate::emacs_core::value::Value;
use crate::heap_types::threading::RecursionDepth;

fn mutex_depth_exhausted() -> crate::emacs_core::error::Flow {
    signal(
        "error",
        vec![Value::string("Mutex recursion depth exhausted")],
    )
}

impl Context {
    /// Whether thread primitives take the Green path: this owner runs in a
    /// started Green runtime.
    pub(crate) fn green_threads_running(&mut self) -> bool {
        self.green_seat().is_some()
    }

    /// The running thread's id in the World's `ThreadManager`.
    fn green_current_id(&mut self) -> u64 {
        self.green_key().map_or(0, ThreadKey::id)
    }

    /// `(thread-yield)`: let every other ready thread run first.
    pub(crate) fn green_thread_yield(&mut self, args: &[Value]) -> EvalResult {
        expect_args("thread-yield", args, 0)?;
        self.green_yield()?;
        Ok(Value::NIL)
    }

    /// `(thread-join THREAD)` (GNU `Fthread_join`, thread.c:1118-1144):
    /// wait for THREAD to end and return its result. An error another
    /// thread injected into THREAD with `thread-signal` is re-raised here,
    /// as snapshotted before the wait; an error THREAD signalled itself is
    /// only `thread-last-error`. An error signalled into this thread ends
    /// the wait.
    pub(crate) fn green_thread_join(&mut self, args: &[Value]) -> EvalResult {
        expect_args("thread-join", args, 1)?;
        let id = crate::emacs_core::threads::expect_thread_id(&self.threads, &args[0])?;
        if id == self.green_current_id() {
            return Err(signal(
                "error",
                vec![Value::string("Cannot join current thread")],
            ));
        }
        let pending = self.threads.pending_thread_signal_for_value(args[0]);
        let key = ThreadKey::from_id(id);
        let control = self.green_control();
        if !control.exited(key) {
            self.green_blocked_on(args[0], |ctx| {
                while !control.exited(key) {
                    ctx.green_wait(Wait::Join(key))?;
                }
                Ok(())
            })?;
        }
        self.threads.join_thread_for_value(args[0]);
        // Join is about the Lisp thread, not the OS carrier: the carrier stays
        // in the pool for the next Lisp thread. `exited(key)` is published
        // after owner retirement; the TLS reset is deferred to that next job.
        if let Some(flow) = pending {
            return Err(flow);
        }
        Ok(self.threads.thread_result_for_value(args[0]))
    }

    /// `(thread-signal THREAD ERROR-SYMBOL DATA)` for a worker thread (GNU
    /// `Fthread_signal`): THREAD raises the error when it next runs, and a
    /// `mutex-lock`, `condition-wait` or `thread-join` it waits in ends.
    ///
    /// A main-thread target never reaches here: `builtin_thread_signal`
    /// queues GNU's `THREAD_EVENT` for the command loop first
    /// (`src/thread.c:1002-1021`), exactly as GNU does.
    pub(crate) fn green_thread_signal(
        &mut self,
        thread: Value,
        symbol: Value,
        data: Value,
    ) -> EvalResult {
        let id = crate::emacs_core::threads::expect_thread_id(&self.threads, &thread)?;
        debug_assert_ne!(id, 0, "the main thread receives a THREAD_EVENT");
        self.threads
            .set_thread_error_for_value(thread, symbol, data);
        let key = ThreadKey::from_id(id);
        let control = self.green_control();
        if matches!(
            control.wait_of(key),
            Some(Wait::Join(_) | Wait::Mutex(_) | Wait::Condition(_) | Wait::Select { .. })
        ) {
            control.wake(key);
        }
        Ok(Value::NIL)
    }

    /// Wake the main thread's `thread_select` wait after a `THREAD_EVENT` was
    /// queued for it.  The event travels through the World-shared manager;
    /// this only makes a main owner parked in `read-event` re-check its input
    /// queue.  No other wait is interrupted: GNU's input event does not end a
    /// join, mutex or condition wait.
    pub(crate) fn green_wake_main_for_thread_event(&mut self) {
        let control = self.green_control();
        let main = ThreadKey::from_id(0);
        if matches!(control.wait_of(main), Some(Wait::Select { .. })) {
            control.wake(main);
        }
        control.nudge_main_mailbox();
    }

    /// `(mutex-lock MUTEX)` (GNU `Fmutex_lock`): take MUTEX, waiting
    /// while another thread holds it, until this thread is signalled.
    pub(crate) fn green_mutex_lock(&mut self, mutex: MutexHandle) -> EvalResult {
        match self.threads.mutex_lock(mutex) {
            MutexAcquisition::Acquired => return Ok(Value::NIL),
            MutexAcquisition::Blocked => {}
            MutexAcquisition::DepthExhausted => return Err(mutex_depth_exhausted()),
        }
        let key = self.green_key().expect("a Green thread");
        self.green_blocked_on(mutex.value(), |ctx| {
            loop {
                // GNU `lisp_mutex_lock_for_thread`: a signal ends the wait;
                // `post_acquire_global_lock` raises it.
                ctx.green_raise_pending_signal(key)?;
                ctx.green_wait(Wait::Mutex(MutexId::new(mutex.native_key())))?;
                match ctx.threads.mutex_lock(mutex) {
                    MutexAcquisition::Acquired => return Ok(()),
                    MutexAcquisition::Blocked => {}
                    MutexAcquisition::DepthExhausted => return Err(mutex_depth_exhausted()),
                }
            }
        })?;
        Ok(Value::NIL)
    }

    /// `(mutex-unlock MUTEX)` once its owner is checked: a release wakes
    /// every thread waiting for the mutex. This thread keeps running.
    pub(crate) fn green_mutex_unlock(&mut self, mutex: MutexHandle) -> EvalResult {
        match self.threads.mutex_unlock(mutex) {
            MutexRelease::StillLocked => {}
            MutexRelease::Released => {
                self.green_control()
                    .wake_waiting_for(Wait::Mutex(MutexId::new(mutex.native_key())));
            }
            MutexRelease::NotOwner => {
                return Err(signal(
                    "error",
                    vec![Value::string("Cannot unlock mutex owned by another thread")],
                ));
            }
        }
        Ok(Value::NIL)
    }

    /// `(condition-wait COND)` once this thread is checked to hold COND's
    /// mutex MUTEX (GNU `condition_wait_callback`): release the mutex
    /// whatever its count, wait for a notification unless already
    /// signalled, take the mutex back with its count (even when signalled),
    /// then raise a pending signal.
    pub(crate) fn green_condition_wait(
        &mut self,
        condition: CondVarHandle,
        mutex: MutexHandle,
    ) -> EvalResult {
        let key = self.green_key().expect("a Green thread");
        let control = self.green_control();
        let count: RecursionDepth = self.threads.mutex_unlock_for_wait(mutex).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string(
                    "Condition variable's mutex is not held by current thread",
                )],
            )
        })?;
        control.wake_waiting_for(Wait::Mutex(MutexId::new(mutex.native_key())));
        if self.threads.pending_thread_signal(key.id()).is_none() {
            self.green_blocked_on(condition.value(), |ctx| {
                ctx.green_park(Wait::Condition(ConditionId::new(condition.native_key())))
            })?;
        }
        loop {
            match self.threads.mutex_relock_after_wait(mutex, count) {
                MutexAcquisition::Acquired => break,
                MutexAcquisition::Blocked => {
                    self.green_park(Wait::Mutex(MutexId::new(mutex.native_key())))?
                }
                MutexAcquisition::DepthExhausted => return Err(mutex_depth_exhausted()),
            }
        }
        self.green_raise_pending_signal(key)?;
        Ok(Value::NIL)
    }

    /// `(condition-notify COND ALL)` once this thread is checked to hold
    /// COND's mutex: wake one waiting thread, or all of them. GNU releases
    /// and retakes the mutex around the notification without letting
    /// another thread run, so nothing else changes.
    pub(crate) fn green_condition_notify(
        &mut self,
        condition: CondVarHandle,
        scope: NotificationScope,
    ) -> EvalResult {
        let control = self.green_control();
        match scope {
            NotificationScope::All => {
                control.wake_waiting_for(Wait::Condition(ConditionId::new(condition.native_key())));
            }
            NotificationScope::One => {
                control.wake_one_waiting_for(Wait::Condition(ConditionId::new(
                    condition.native_key(),
                )));
            }
        }
        Ok(Value::NIL)
    }

    /// `set-standard-case-table` set TABLE: every thread uses it from its
    /// next resume on.
    pub(crate) fn green_set_standard_case_table(&mut self, table: Value) {
        if self.green_threads_running() {
            self.with_green_runtime(|runtime| runtime.standard_case_table.set(Some(table)));
        }
    }

    /// GNU `thread_check_current_buffer`, asked by `kill-buffer`: whether
    /// another thread's current buffer is BUFFER and keeps it alive.
    pub(crate) fn green_buffer_held(&mut self, buffer: crate::buffer::BufferId) -> bool {
        self.green_threads_running() && self.threads.buffer_held_by_other_thread(buffer)
    }

    /// GNU `thread_all_before_buffer_killed`, before `kill-buffer` kills
    /// BUFFER: move every other thread in it to `(other-buffer BUFFER)`.
    pub(crate) fn green_before_buffer_killed(
        &mut self,
        buffer: crate::buffer::BufferId,
    ) -> Result<(), crate::emacs_core::error::Flow> {
        if !self.green_threads_running() || !self.threads.buffer_current_in_other_thread(buffer) {
            return Ok(());
        }
        let other = crate::emacs_core::buffer::other_buffer_impl_in_state(
            &mut self.frames,
            &mut self.buffers,
            vec![Value::make_buffer(buffer)],
        )?
        .as_buffer_id();
        // GNU's `other-buffer` always finds one, making `*scratch*` if
        // need be; without one the threads keep their buffer.
        if let Some(other) = other {
            self.threads.move_threads_off_killed_buffer(buffer, other);
        }
        Ok(())
    }

    /// Run WAIT with this thread's `thread--blocker` set to OBJECT (GNU's
    /// `event_object`), cleared however it ends.
    fn green_blocked_on(
        &mut self,
        object: Value,
        wait: impl FnOnce(&mut Self) -> Result<(), crate::emacs_core::error::Flow>,
    ) -> Result<(), crate::emacs_core::error::Flow> {
        let id = self.green_current_id();
        self.threads.set_thread_blocker(id, object);
        let result = wait(self);
        self.threads.clear_thread_blocker(id);
        result
    }

    /// The runtime's scheduler.
    fn green_control(&mut self) -> std::sync::Arc<super::scheduler::Control> {
        std::sync::Arc::clone(&self.green_seat().expect("a Green thread").control)
    }
}

/// `make-thread` in a Green World.
pub(crate) fn green_make_thread(ctx: &mut Context, args: &[Value]) -> EvalResult {
    ctx.green_spawn(args)
}

/// Whether `make-thread` takes the Green path: the World is Green.
pub(crate) fn green_world(ctx: &Context) -> bool {
    ctx.threading_mode() == super::super::ThreadingMode::Green
        && ctx.green_admission_readiness().is_ok()
}
