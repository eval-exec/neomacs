//! A condition wait belongs to the parked owner, not to a native root table.
//!
//! The active subr's argument is retained by its existing backtrace/operand
//! window (and additionally rooted in a `Drop`-guarded VM root frame across
//! parks by `ParallelConditionSeat`). Parking publishes that window in the
//! owner image; the condition's canonical association then retains the mutex.
//! These handles add no GC edge.
//!
//! E5.4a D4: `ConditionSeat` abstracts the owner seat (`BatonConditionSeat`
//! backed by `GreenSeat::condition_wait` vs `ParallelConditionSeat` backed by
//! `OwnerSlab::parallel_condition_wait`) so `ValidatedWait` and
//! `ConditionWait::{Waiting, Reacquiring, Completed}` remain the single
//! typestate path for both thread kinds.

use super::{ConditionId, MutexId, ThreadKey, Wait};
use crate::emacs_core::eval::Context;
use crate::emacs_core::threads::{
    CondVarHandle, MutexAcquisition, MutexHandle, ThreadManager, WaitCondvar,
};
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::heap_types::threading::RecursionDepth;
use neovm_value::value::Value;

/// Only `unlock` can construct this association and saved recursive depth.
#[derive(Clone, Copy)]
pub(crate) struct ValidatedWait {
    condition: CondVarHandle,
    mutex: MutexHandle,
    depth: RecursionDepth,
}

impl ValidatedWait {
    pub(crate) fn unlock(
        manager: &mut ThreadManager,
        condition: CondVarHandle,
    ) -> Result<Self, Flow> {
        let mutex = manager.condition_variable_mutex(condition);
        let depth = manager.mutex_unlock_for_wait(mutex).ok_or_else(|| {
            signal(
                "error",
                vec![Value::string(
                    "Condition variable's mutex is not held by current thread",
                )],
            )
        })?;
        Ok(Self {
            condition,
            mutex,
            depth,
        })
    }
}

/// Stowed in the owner's seat across both kinds of park (`GreenSeat` for Baton
/// threads; `OwnerSlab::parallel_condition_wait` for parallel owners). In
/// particular, an unrelated mutex or an unchecked recursion count cannot enter
/// a wait.
pub(crate) enum ConditionWait {
    Waiting(ValidatedWait),
    Reacquiring(ValidatedWait),
    Completed,
}

static_assertions::assert_not_impl_any!(ConditionWait: Send, Sync, Clone, Copy);

pub(crate) fn invalid_wait_state() -> Flow {
    signal(
        "error",
        vec![Value::string("Invalid condition-wait owner state")],
    )
}

/// Seat abstraction shared by Baton (`BatonConditionSeat`) and parallel owners
/// (`ParallelConditionSeat`, E5.4a D4) so both share the exact same
/// `ValidatedWait` + `ConditionWait` state machine.
pub(crate) trait ConditionSeat {
    /// Validate that this seat is active on `ctx` and return its `ThreadKey`.
    fn key(&self, ctx: &mut Context) -> Result<ThreadKey, Flow>;

    /// Inspect the seat's `ConditionWait` state.
    fn condition_wait<'a>(&self, ctx: &'a mut Context) -> Result<&'a ConditionWait, Flow>;

    /// Mutate the seat's `ConditionWait` state.
    fn condition_wait_mut<'a>(&self, ctx: &'a mut Context) -> Result<&'a mut ConditionWait, Flow>;

    /// Root `condition` (and its associated mutex) across the wait+relock span
    /// when the seat requires explicit VM root-frame coverage (parallel owners).
    fn push_wait_roots(&self, _ctx: &mut Context, _condition: CondVarHandle) {}

    /// Best-effort cleanup during `WaitOwner::drop`: clear `thread_blocker`,
    /// reset `ConditionWait::Completed`, and pop any VM root frame pushed by
    /// [`Self::push_wait_roots`].
    fn leave_wait_cleanup(&self, ctx: &mut Context, key: ThreadKey);

    /// Wake all waiters (both Baton and parallel) blocked on `mutex` after
    /// `ValidatedWait::unlock` releases it.
    fn wake_mutex_waiters(&self, ctx: &mut Context, mutex: MutexId) -> Result<(), Flow>;

    /// Park waiting on `condition` (setting/clearing `WaitCondvar::Condition`).
    fn park_condition(
        &self,
        ctx: &mut Context,
        key: ThreadKey,
        condition: ConditionId,
    ) -> Result<(), Flow>;

    /// Park reacquiring `mutex` at the saved recursive depth (setting/clearing
    /// `WaitCondvar::Mutex`; GNU `src/thread.c:232-234` ignores signals here).
    fn park_mutex_relock(
        &self,
        ctx: &mut Context,
        key: ThreadKey,
        mutex: MutexId,
    ) -> Result<(), Flow>;

    /// Deliver any pending `thread-signal` AFTER recursive mutex ownership has
    /// been restored (GNU `condition_wait_callback`, `src/thread.c:475`).
    fn deliver_signal(&self, ctx: &mut Context, key: ThreadKey) -> Result<(), Flow>;
}

/// Baton-thread seat backed by `GreenSeat::condition_wait`.
struct BatonConditionSeat;

impl ConditionSeat for BatonConditionSeat {
    fn key(&self, ctx: &mut Context) -> Result<ThreadKey, Flow> {
        ctx.green_key().ok_or_else(invalid_wait_state)
    }

    fn condition_wait<'a>(&self, ctx: &'a mut Context) -> Result<&'a ConditionWait, Flow> {
        Ok(&ctx
            .green_seat()
            .ok_or_else(invalid_wait_state)?
            .condition_wait)
    }

    fn condition_wait_mut<'a>(&self, ctx: &'a mut Context) -> Result<&'a mut ConditionWait, Flow> {
        Ok(&mut ctx
            .green_seat()
            .ok_or_else(invalid_wait_state)?
            .condition_wait)
    }

    fn leave_wait_cleanup(&self, ctx: &mut Context, key: ThreadKey) {
        ctx.threads.clear_thread_blocker(key.id());
        if let Some(seat) = ctx.green_seat() {
            seat.condition_wait = ConditionWait::Completed;
        }
    }

    fn wake_mutex_waiters(&self, ctx: &mut Context, mutex: MutexId) -> Result<(), Flow> {
        let control =
            std::sync::Arc::clone(&ctx.green_seat().ok_or_else(invalid_wait_state)?.control);
        control.wake_waiting_for(Wait::Mutex(mutex));
        Ok(())
    }

    fn park_condition(
        &self,
        ctx: &mut Context,
        _key: ThreadKey,
        condition: ConditionId,
    ) -> Result<(), Flow> {
        ctx.green_in_wait_condvar(WaitCondvar::Condition, |context| {
            context.green_park(Wait::Condition(condition))
        })
    }

    fn park_mutex_relock(
        &self,
        ctx: &mut Context,
        _key: ThreadKey,
        mutex: MutexId,
    ) -> Result<(), Flow> {
        ctx.green_in_wait_condvar(WaitCondvar::Mutex, |context| {
            context.green_park(Wait::Mutex(mutex))
        })
    }

    fn deliver_signal(&self, ctx: &mut Context, key: ThreadKey) -> Result<(), Flow> {
        ctx.green_raise_pending_signal(key)
    }
}

/// Restores native continuation state, the existing `event_object`, and any
/// seat-owned VM root frame on every return, including a refused park, Lisp
/// signal, or panic unwind. Never runs Lisp or waits in `Drop`; a deferred
/// binding-resume error stays in the Green seat.
struct WaitOwner<'a, S: ConditionSeat> {
    context: &'a mut Context,
    seat: S,
    key: ThreadKey,
}

impl<'a, S: ConditionSeat> WaitOwner<'a, S> {
    fn enter(context: &'a mut Context, seat: S, condition: CondVarHandle) -> Result<Self, Flow> {
        let key = seat.key(context)?;
        if !context.threads.is_condition_variable(condition) {
            return Err(invalid_wait_state());
        }
        match seat.condition_wait(context)? {
            ConditionWait::Completed => {}
            ConditionWait::Waiting(_) | ConditionWait::Reacquiring(_) => {
                return Err(invalid_wait_state());
            }
        }
        seat.push_wait_roots(context, condition);
        let owner = Self { context, seat, key };
        // GNU thread.c:454-455 publishes the wait target before unlocking.
        owner
            .context
            .threads
            .set_thread_blocker(key.id(), condition.value());
        let wait = ValidatedWait::unlock(&mut owner.context.threads, condition)?;
        *owner.seat.condition_wait_mut(owner.context)? = ConditionWait::Waiting(wait);
        Ok(owner)
    }

    fn waiting(&mut self) -> Result<ValidatedWait, Flow> {
        match *self.seat.condition_wait(self.context)? {
            ConditionWait::Waiting(wait) => Ok(wait),
            ConditionWait::Reacquiring(_) | ConditionWait::Completed => Err(invalid_wait_state()),
        }
    }

    fn start_reacquiring(&mut self) -> Result<(), Flow> {
        let wait = self.waiting()?;
        *self.seat.condition_wait_mut(self.context)? = ConditionWait::Reacquiring(wait);
        // GNU thread.c:466 clears the blocker before mutex reacquisition,
        // including when another owner must run before the mutex is available.
        self.context.threads.clear_thread_blocker(self.key.id());
        Ok(())
    }

    fn reacquiring(&mut self) -> Result<ValidatedWait, Flow> {
        match *self.seat.condition_wait(self.context)? {
            ConditionWait::Reacquiring(wait) => Ok(wait),
            ConditionWait::Waiting(_) | ConditionWait::Completed => Err(invalid_wait_state()),
        }
    }
}

impl<S: ConditionSeat> Drop for WaitOwner<'_, S> {
    fn drop(&mut self) {
        self.seat.leave_wait_cleanup(self.context, self.key);
    }
}

/// Shared GNU `condition_wait_callback` (`src/thread.c:446-476`) over any
/// [`ConditionSeat`] (`BatonConditionSeat` or `ParallelConditionSeat`).
pub(crate) fn wait_with_seat<S: ConditionSeat>(
    context: &mut Context,
    seat: S,
    condition: CondVarHandle,
) -> EvalResult {
    let mut owner = WaitOwner::enter(context, seat, condition)?;
    let waiting = owner.waiting()?;
    owner
        .seat
        .wake_mutex_waiters(owner.context, MutexId::new(waiting.mutex.native_key()))?;
    if owner
        .context
        .threads
        .pending_thread_signal(owner.key.id())
        .is_none()
    {
        let cid = ConditionId::new(waiting.condition.native_key());
        owner.seat.park_condition(owner.context, owner.key, cid)?;
    }
    owner.start_reacquiring()?;
    loop {
        let waiting = owner.reacquiring()?;
        match owner
            .context
            .threads
            .mutex_relock_after_wait(waiting.mutex, waiting.depth)
        {
            MutexAcquisition::Acquired => break,
            // GNU's relock waits on the mutex's `wait_condvar` and ignores
            // pending signals until recursive ownership is restored
            // (`lisp_mutex_lock_for_thread`, src/thread.c:231-236).
            MutexAcquisition::Blocked => {
                let mid = MutexId::new(waiting.mutex.native_key());
                owner
                    .seat
                    .park_mutex_relock(owner.context, owner.key, mid)?;
            }
            MutexAcquisition::DepthExhausted => return Err(super::api::mutex_depth_exhausted()),
        }
    }
    // Neither a pending thread-signal nor a deferred binding error is raised
    // until the saved recursive ownership has been restored (GNU thread.c:475).
    owner.seat.deliver_signal(owner.context, owner.key)?;
    Ok(Value::NIL)
}

pub(super) fn wait(context: &mut Context, condition: CondVarHandle) -> EvalResult {
    wait_with_seat(context, BatonConditionSeat, condition)
}
