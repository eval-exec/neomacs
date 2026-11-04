//! Explicit retirement of the native root at the last completion boundary.
//!
//! GNU unlinks an exiting thread very late so GC cannot reclaim thread_state
//! during exit (thread.c:853-859). The manager, rather than the token, owns the
//! strong registration; abandoning a token leaks that edge instead of freeing
//! an object that the exiting carrier can still access.

use super::{RunState, ThreadId, ThreadManager, ThreadPhase};
use neovm_value::tagged::gc::HeapIdentity;
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Clone, Copy, Debug)]
pub(crate) enum CompletionExit {
    Executed,
    ClosingBeforeStart,
    Simulated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompletionConsumption {
    Outstanding,
    Consumed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompletionError {
    MissingRegistration,
    MainThread,
    AlreadyPrepared,
}

impl CompletionError {
    /// Completion invariants are checked while the owner still exists. No
    /// failure may authorize scheduler exit or root retirement.
    #[doc(hidden)]
    pub fn fail_stop(self) -> ! {
        match self {
            Self::MissingRegistration | Self::MainThread | Self::AlreadyPrepared => {
                std::process::abort()
            }
        }
    }
}

/// No field owns a root lease or can release registry state during Drop.
#[must_use = "explicitly retire the completion registration before publishing exit"]
#[derive(Debug)]
pub(crate) struct CompletingThread {
    thread: ThreadId,
    exit: CompletionExit,
    consumption: CompletionConsumption,
    _confined: PhantomData<Rc<()>>,
}

impl CompletingThread {
    #[doc(hidden)]
    pub fn heap_identity(&self) -> HeapIdentity {
        self.thread.heap_identity()
    }
}

impl Drop for CompletingThread {
    fn drop(&mut self) {
        debug_assert_eq!(
            self.consumption,
            CompletionConsumption::Consumed,
            "unconsumed completion registration on {:?}",
            self.exit,
        );
    }
}

/// Produced only by removing the matching World's native root registration.
#[must_use = "the scheduler must observe completed root retirement before exit"]
#[derive(Debug)]
pub(crate) struct RetiredCompletion {
    thread: ThreadId,
    _confined: PhantomData<Rc<()>>,
}

impl RetiredCompletion {
    #[doc(hidden)]
    pub fn into_thread_id(self) -> ThreadId {
        self.thread
    }
}

pub(crate) enum SimCompletion {
    Blocked,
    Completed(CompletingThread),
}

static_assertions::assert_not_impl_any!(CompletingThread: Clone, Copy, Send, Sync);
static_assertions::assert_not_impl_any!(RetiredCompletion: Clone, Copy, Send, Sync);

impl ThreadManager {
    /// Called under the live owner's installed heap, before owner retirement.
    #[doc(hidden)]
    pub fn prepare_thread_completion(
        &mut self,
        thread: ThreadId,
        exit: CompletionExit,
    ) -> Result<CompletingThread, CompletionError> {
        let payload = self
            .get_thread(thread.native_key())
            .ok_or(CompletionError::MissingRegistration)?;
        if payload.id() != thread {
            return Err(CompletionError::MissingRegistration);
        }
        if thread == self.ids.main_thread() {
            return Err(CompletionError::MainThread);
        }
        match payload.native().phase() {
            ThreadPhase::Terminal(_) => return Err(CompletionError::AlreadyPrepared),
            ThreadPhase::New | ThreadPhase::Active(_) | ThreadPhase::Completing(_) => {}
        }
        // Durable mailbox words are ordinary children of this object. They
        // remain reachable from a retained finished handle and disappear only
        // when that object becomes unreachable. No in-flight transport lease
        // can be hidden here or discarded by completion.
        // The late retirement below only removes the native live-map edge;
        // it decodes no heap word and performs no transport cleanup.
        self.mark_thread_exited(thread.native_key());
        Ok(CompletingThread {
            thread,
            exit,
            consumption: CompletionConsumption::Outstanding,
            _confined: PhantomData,
        })
    }

    /// A simulated suspension remains live; its next resume owns completion.
    #[doc(hidden)]
    pub fn prepare_sim_completion(
        &mut self,
        thread: ThreadId,
    ) -> Result<SimCompletion, CompletionError> {
        let payload = self
            .get_thread(thread.native_key())
            .ok_or(CompletionError::MissingRegistration)?;
        if payload.id() != thread {
            return Err(CompletionError::MissingRegistration);
        }
        match payload.native().phase() {
            ThreadPhase::Active(RunState::Blocked) => Ok(SimCompletion::Blocked),
            ThreadPhase::New
            | ThreadPhase::Active(RunState::Running)
            | ThreadPhase::Completing(_)
            | ThreadPhase::Terminal(_) => self
                .prepare_thread_completion(thread, CompletionExit::Simulated)
                .map(SimCompletion::Completed),
        }
    }

    /// Native-only removal: valid after carrier heap/TLS parking. The caller
    /// supplies serial owner authority or the still-held World's Baton.
    #[doc(hidden)]
    pub fn retire_thread_completion(
        &mut self,
        mut completion: CompletingThread,
    ) -> RetiredCompletion {
        if completion.thread.heap_identity() != self.ids.main_thread().heap_identity()
            || completion.thread == self.ids.main_thread()
        {
            // An impossible World/token mismatch cannot authorize exit or
            // release another registration. No Lisp/TLS error path is valid
            // after owner retirement; retain the roots and fail-stop.
            std::process::abort();
        }
        if self.threads.remove(&completion.thread).is_none() {
            std::process::abort();
        }
        completion.consumption = CompletionConsumption::Consumed;
        RetiredCompletion {
            thread: completion.thread,
            _confined: PhantomData,
        }
    }
}
