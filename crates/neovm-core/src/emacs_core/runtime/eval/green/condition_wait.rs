//! A condition wait belongs to the parked owner, not to a native root table.
//!
//! The active subr's argument is retained by its existing backtrace/operand
//! window. Parking publishes that window in the owner image; the condition's
//! canonical association then retains the mutex. These handles add no GC edge.

use super::{ConditionId, MutexId, ThreadKey, Wait};
use crate::emacs_core::error::{EvalResult, Flow, signal};
use crate::emacs_core::eval::Context;
use crate::emacs_core::threads::{CondVarHandle, MutexAcquisition, MutexHandle, ThreadManager};
use crate::emacs_core::value::Value;
use crate::heap_types::threading::RecursionDepth;

/// Only `unlock` can construct this association and saved recursive depth.
#[derive(Clone, Copy)]
pub(super) struct ValidatedWait {
    condition: CondVarHandle,
    mutex: MutexHandle,
    depth: RecursionDepth,
}

impl ValidatedWait {
    fn unlock(manager: &mut ThreadManager, condition: CondVarHandle) -> Result<Self, Flow> {
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

/// Stowed in the owner's Green seat across both kinds of park. In particular,
/// an unrelated mutex or an unchecked recursion count cannot enter a wait.
pub(super) enum ConditionWait {
    Waiting(ValidatedWait),
    Reacquiring(ValidatedWait),
    Completed,
}

static_assertions::assert_not_impl_any!(ConditionWait: Send, Sync, Clone, Copy);

fn invalid_wait_state() -> Flow {
    signal(
        "error",
        vec![Value::string("Invalid condition-wait owner state")],
    )
}

/// Restores native continuation state and the existing event_object on every
/// return, including a refused park or process shutdown. It never runs Lisp or
/// waits in Drop; deferred errors remain owned by the Green seat.
struct WaitOwner<'a> {
    context: &'a mut Context,
    key: ThreadKey,
}

impl<'a> WaitOwner<'a> {
    fn enter(context: &'a mut Context, condition: CondVarHandle) -> Result<Self, Flow> {
        let key = context.green_key().ok_or_else(invalid_wait_state)?;
        if !context.threads.is_condition_variable(condition) {
            return Err(invalid_wait_state());
        }
        match context
            .green_seat()
            .ok_or_else(invalid_wait_state)?
            .condition_wait
        {
            ConditionWait::Completed => {}
            ConditionWait::Waiting(_) | ConditionWait::Reacquiring(_) => {
                return Err(invalid_wait_state());
            }
        }
        let owner = Self { context, key };
        // GNU thread.c:454-455 publishes the wait target before unlocking.
        owner
            .context
            .threads
            .set_thread_blocker(key.id(), condition.value());
        let wait = ValidatedWait::unlock(&mut owner.context.threads, condition)?;
        owner
            .context
            .green_seat()
            .ok_or_else(invalid_wait_state)?
            .condition_wait = ConditionWait::Waiting(wait);
        Ok(owner)
    }

    fn waiting(&mut self) -> Result<ValidatedWait, Flow> {
        match self
            .context
            .green_seat()
            .ok_or_else(invalid_wait_state)?
            .condition_wait
        {
            ConditionWait::Waiting(wait) => Ok(wait),
            ConditionWait::Reacquiring(_) | ConditionWait::Completed => Err(invalid_wait_state()),
        }
    }

    fn start_reacquiring(&mut self) -> Result<(), Flow> {
        let wait = self.waiting()?;
        self.context
            .green_seat()
            .ok_or_else(invalid_wait_state)?
            .condition_wait = ConditionWait::Reacquiring(wait);
        // GNU thread.c:466 clears the blocker before mutex reacquisition,
        // including when another owner must run before the mutex is available.
        self.context.threads.clear_thread_blocker(self.key.id());
        Ok(())
    }

    fn reacquiring(&mut self) -> Result<ValidatedWait, Flow> {
        match self
            .context
            .green_seat()
            .ok_or_else(invalid_wait_state)?
            .condition_wait
        {
            ConditionWait::Reacquiring(wait) => Ok(wait),
            ConditionWait::Waiting(_) | ConditionWait::Completed => Err(invalid_wait_state()),
        }
    }
}

impl Drop for WaitOwner<'_> {
    fn drop(&mut self) {
        self.context.threads.clear_thread_blocker(self.key.id());
        if let Some(seat) = self.context.green_seat() {
            seat.condition_wait = ConditionWait::Completed;
        }
    }
}

pub(super) fn wait(context: &mut Context, condition: CondVarHandle) -> EvalResult {
    let mut owner = WaitOwner::enter(context, condition)?;
    let waiting = owner.waiting()?;
    let control = std::sync::Arc::clone(
        &owner
            .context
            .green_seat()
            .ok_or_else(invalid_wait_state)?
            .control,
    );
    control.wake_waiting_for(Wait::Mutex(MutexId::new(waiting.mutex.native_key())));
    if owner
        .context
        .threads
        .pending_thread_signal(owner.key.id())
        .is_none()
    {
        owner.context.green_park(Wait::Condition(ConditionId::new(
            waiting.condition.native_key(),
        )))?;
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
            MutexAcquisition::Blocked => owner
                .context
                .green_park(Wait::Mutex(MutexId::new(waiting.mutex.native_key())))?,
            MutexAcquisition::DepthExhausted => return Err(super::api::mutex_depth_exhausted()),
        }
    }
    // Neither a pending thread-signal nor a deferred binding error is raised
    // until the saved recursive ownership has been restored (GNU thread.c:475).
    owner.context.green_raise_pending_signal(owner.key)?;
    Ok(Value::NIL)
}
