//! Physical native waits are the sole source of real-wait scheduling authority.
//!
//! All operations here borrow native I/O and synchronization data only. The
//! caller prepares and publishes its owner roots before supplying a handoff;
//! no evaluator borrow, Lisp callback or registered root reader enters a wait.

use super::{
    ControlWake, ControlWakeOutcome, HostClosure, Metadata, NativePollBusy, NativePollLease,
    NativePollOwner, ReadDeadline, WaitRegistration,
};
use std::io;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Condvar, MutexGuard};
use std::time::Duration;

/// Linear evidence minted immediately before an actual blocking native call.
#[derive(Debug)]
pub(crate) struct RealWaitPermit {
    _confined: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(RealWaitPermit: Send, Sync, Clone, Copy);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeWaitContinuation {
    RecheckPredicate,
    ReturnToDriver,
}

/// Static owner projection for release and restoration around physical waits.
///
/// `release` must not lock the paired control predicate: a condition wait still
/// holds that mutex until the OS wait atomically releases it. `reacquire` runs
/// after every returned guard is dropped, including poisoned-condition errors.
pub(crate) trait NativeWaitHandoff {
    fn release(&mut self, permit: RealWaitPermit);
    fn reacquire(&mut self);
    /// A routed pump must classify owned completions after every physical
    /// cycle. Ordinary static projections keep the canonical predicate loop.
    #[inline(always)]
    fn continuation(&self) -> NativeWaitContinuation {
        NativeWaitContinuation::RecheckPredicate
    }
    #[cfg(test)]
    fn physical_block_entered(&mut self) {}
    #[cfg(test)]
    fn physical_block_returned(&mut self) {}
}

/// Ordinary native waits retain their existing inline implementation.
pub(crate) struct InlineNativeWait;

impl NativeWaitHandoff for InlineNativeWait {
    #[inline(always)]
    fn release(&mut self, _permit: RealWaitPermit) {}

    #[inline(always)]
    fn reacquire(&mut self) {}
}

enum NativeOperation<'io, 'guard> {
    Poll {
        poller: &'io polling::Poller,
        events: &'io mut polling::Events,
        deadline: ReadDeadline,
    },
    Condition {
        changed: &'io Condvar,
        metadata: MutexGuard<'guard, Metadata>,
        deadline: ReadDeadline,
    },
}

struct BlockingNativeWait<'handoff, H> {
    handoff: &'handoff mut H,
}

impl NativeOperation<'_, '_> {
    fn deadline(&self) -> ReadDeadline {
        match self {
            Self::Poll { deadline, .. } | Self::Condition { deadline, .. } => *deadline,
        }
    }

    fn execute(self) -> io::Result<()> {
        // Recompute AFTER the release handoff, retaining the original absolute
        // deadline even if scheduling or an observation consumed its tail.
        let timeout = native_timeout(self.deadline());
        match self {
            Self::Poll { poller, events, .. } => poller.wait(events, timeout).map(|_| ()),
            Self::Condition {
                changed, metadata, ..
            } => {
                match timeout {
                    Some(remaining) => drop(
                        changed
                            .wait_timeout(metadata, remaining)
                            .unwrap_or_else(|error| error.into_inner()),
                    ),
                    None => drop(
                        changed
                            .wait(metadata)
                            .unwrap_or_else(|error| error.into_inner()),
                    ),
                }
                Ok(())
            }
        }
    }
}

fn native_timeout(deadline: ReadDeadline) -> Option<Duration> {
    match deadline {
        ReadDeadline::Immediate => Some(Duration::ZERO),
        ReadDeadline::At(end) => Some(end.saturating_duration_since(std::time::Instant::now())),
        ReadDeadline::Unbounded => None,
    }
}

impl<H: NativeWaitHandoff> BlockingNativeWait<'_, H> {
    /// Private executor; no duration, receive or callback API can mint a permit.
    fn execute(&mut self, operation: NativeOperation<'_, '_>) -> io::Result<()> {
        if native_timeout(operation.deadline()).is_some_and(|remaining| remaining.is_zero()) {
            // A ready probe neither releases nor fabricates scheduling authority.
            return match operation {
                NativeOperation::Poll { .. } => operation.execute(),
                NativeOperation::Condition { .. } => {
                    drop(operation);
                    Ok(())
                }
            };
        }
        self.handoff.release(RealWaitPermit {
            _confined: PhantomData,
        });
        #[cfg(test)]
        self.handoff.physical_block_entered();
        let result = operation.execute();
        #[cfg(test)]
        self.handoff.physical_block_returned();
        self.handoff.reacquire();
        result
    }
}

pub(super) fn poll<H: NativeWaitHandoff>(
    poller: &polling::Poller,
    events: &mut polling::Events,
    deadline: ReadDeadline,
    handoff: &mut H,
) -> io::Result<()> {
    BlockingNativeWait { handoff }.execute(NativeOperation::Poll {
        poller,
        events,
        deadline,
    })
}

pub(super) fn condition<H: NativeWaitHandoff>(
    changed: &Condvar,
    metadata: MutexGuard<'_, Metadata>,
    deadline: ReadDeadline,
    handoff: &mut H,
) {
    // Returned condition guards are dropped by the physical operation before
    // the scheduler reacquires. Poisoning retains that same obligation.
    let _ = BlockingNativeWait { handoff }.execute(NativeOperation::Condition {
        changed,
        metadata,
        deadline,
    });
}

/// Owned readiness plan: its exclusive lease and arm snapshot belong to the
/// same wake allocation and scheduling role by construction.
#[derive(Debug)]
pub(crate) struct PreparedNativeWait {
    lease: NativePollLease,
    registration: WaitRegistration,
    deadline: ReadDeadline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeWaitPreparationError {
    Closed(HostClosure),
    PollBusy(NativePollBusy),
}

impl PreparedNativeWait {
    /// Arming precedes the caller's durable input/scheduler predicate check.
    /// Failure never publishes another poller owner or resets its registration.
    pub(crate) fn new(
        wake: &Arc<ControlWake>,
        owner: NativePollOwner,
        deadline: ReadDeadline,
    ) -> Result<Self, NativeWaitPreparationError> {
        // Claim the exclusive lease before changing its destination's arm state.
        let lease = wake
            .try_native_poll(owner)
            .map_err(NativeWaitPreparationError::PollBusy)?;
        let registration = wake
            .arm(owner.destination())
            .map_err(NativeWaitPreparationError::Closed)?;
        Ok(Self {
            lease,
            registration,
            deadline,
        })
    }

    pub(crate) fn poll(&self) -> Option<ControlWakeOutcome> {
        self.registration.poll()
    }

    pub(crate) fn wait_with<H: NativeWaitHandoff>(
        &mut self,
        handoff: &mut H,
    ) -> io::Result<ControlWakeOutcome> {
        let mut events = polling::Events::new();
        self.lease
            .wait_with(&self.registration, &mut events, self.deadline, handoff)
    }
}

static_assertions::assert_not_impl_any!(PreparedNativeWait: Clone, Copy);

#[cfg(test)]
#[path = "tests/native_wait.rs"]
mod tests;
