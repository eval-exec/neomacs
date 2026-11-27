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
#[doc(hidden)]
pub struct RealWaitPermit {
    _confined: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(RealWaitPermit: Send, Sync, Clone, Copy);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum NativeWaitContinuation {
    RecheckPredicate,
    ReturnToDriver,
}

/// Upper bound on a single physical native wait returned by
/// [`NativeWaitHandoff::release`].
///
/// Invariant (#877): a physical native wait must never outlive the earliest
/// peer select deadline it was released with. When `release` transfers the
/// Baton to a runnable peer, that peer's scheduler watches select deadlines
/// (`CallerDeadline`). When no peer is immediately runnable and Main retains
/// an idle or era-idle release while another Baton thread is parked in
/// `Wait::Select { wait_end }`, `EarlierPeer(wait_end)` caps this physical
/// wait so Main wakes no later than `wait_end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReleaseWaitBound {
    CallerDeadline,
    EarlierPeer(std::time::Instant),
}

/// How `NativePollLease::wait_with` / `WaitRegistration::wait_with` handles a
/// physical wait that ended with no events because an earlier peer select
/// deadline elapsed before the caller's own deadline (#877).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PeerDeadlineBehavior {
    /// Return `ControlWakeOutcome::NativeReady` so the caller's outer wait
    /// loop rechecks host/process readiness (including edge-triggered
    /// `Poller::notify()`) and re-enters release to dispatch the expired peer.
    ReturnToCaller,
    /// Stay inside `wait_with` and immediately loop to the next `release()`,
    /// which expires the peer's deadline and transfers the Baton to it while
    /// continuing to await the caller's control wake.
    RecheckInWait,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PhysicalWaitBound {
    CallerDeadline,
    ClampedToPeer(std::time::Instant),
}

/// Static owner projection for release and restoration around physical waits.
///
/// `release` must not lock the paired control predicate: a condition wait still
/// holds that mutex until the OS wait atomically releases it. `reacquire` runs
/// after every returned guard is dropped, including poisoned-condition errors.
pub(crate) trait NativeWaitHandoff {
    fn release(&mut self, permit: RealWaitPermit) -> ReleaseWaitBound;
    fn reacquire(&mut self);
    /// A routed pump must classify owned completions after every physical
    /// cycle. Ordinary static projections keep the canonical predicate loop.
    #[inline(always)]
    fn continuation(&self) -> NativeWaitContinuation {
        NativeWaitContinuation::RecheckPredicate
    }
    /// How `wait_with` handles an eventless wakeup whose physical wait was
    /// clamped to an earlier peer select deadline (#877).
    #[inline(always)]
    fn on_peer_deadline_elapsed(&self) -> PeerDeadlineBehavior {
        PeerDeadlineBehavior::ReturnToCaller
    }
    #[cfg(any(test, feature = "test-support"))]
    fn physical_block_entered(&mut self) {}
    #[cfg(any(test, feature = "test-support"))]
    fn physical_block_returned(&mut self) {}
}

/// Ordinary native waits retain their existing inline implementation.
pub(crate) struct InlineNativeWait;

impl NativeWaitHandoff for InlineNativeWait {
    #[inline(always)]
    fn release(&mut self, _permit: RealWaitPermit) -> ReleaseWaitBound {
        ReleaseWaitBound::CallerDeadline
    }

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
    #[doc(hidden)]
    pub fn deadline(&self) -> ReadDeadline {
        match self {
            Self::Poll { deadline, .. } | Self::Condition { deadline, .. } => *deadline,
        }
    }

    #[doc(hidden)]
    pub fn execute_with_deadline(self, effective_deadline: ReadDeadline) -> io::Result<()> {
        // Recompute AFTER the release handoff, retaining the original absolute
        // deadline (or an earlier sleeping peer's deadline, #877) even if
        // scheduling or an observation consumed its tail.
        let timeout = native_timeout(effective_deadline);
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
    #[doc(hidden)]
    pub fn execute(&mut self, operation: NativeOperation<'_, '_>) -> io::Result<PhysicalWaitBound> {
        let caller_deadline = operation.deadline();
        if native_timeout(caller_deadline).is_some_and(|remaining| remaining.is_zero()) {
            // A ready probe neither releases nor fabricates scheduling authority.
            let result = match operation {
                NativeOperation::Poll { .. } => {
                    operation.execute_with_deadline(ReadDeadline::Immediate)
                }
                NativeOperation::Condition { .. } => {
                    drop(operation);
                    Ok(())
                }
            };
            return result.map(|()| PhysicalWaitBound::CallerDeadline);
        }
        let release_bound = self.handoff.release(RealWaitPermit {
            _confined: PhantomData,
        });
        let (effective_deadline, bound_used) = match (caller_deadline, release_bound) {
            (ReadDeadline::At(caller), ReleaseWaitBound::EarlierPeer(peer)) if peer < caller => (
                ReadDeadline::At(peer),
                PhysicalWaitBound::ClampedToPeer(peer),
            ),
            (ReadDeadline::Unbounded, ReleaseWaitBound::EarlierPeer(peer)) => (
                ReadDeadline::At(peer),
                PhysicalWaitBound::ClampedToPeer(peer),
            ),
            (caller, _) => (caller, PhysicalWaitBound::CallerDeadline),
        };
        #[cfg(any(test, feature = "test-support"))]
        self.handoff.physical_block_entered();
        let result = operation.execute_with_deadline(effective_deadline);
        #[cfg(any(test, feature = "test-support"))]
        self.handoff.physical_block_returned();
        self.handoff.reacquire();
        result.map(|()| bound_used)
    }
}

pub(super) fn poll<H: NativeWaitHandoff>(
    poller: &polling::Poller,
    events: &mut polling::Events,
    deadline: ReadDeadline,
    handoff: &mut H,
) -> io::Result<PhysicalWaitBound> {
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
) -> PhysicalWaitBound {
    // Returned condition guards are dropped by the physical operation before
    // the scheduler reacquires. Poisoning retains that same obligation.
    BlockingNativeWait { handoff }
        .execute(NativeOperation::Condition {
            changed,
            metadata,
            deadline,
        })
        .unwrap_or(PhysicalWaitBound::CallerDeadline)
}

/// Owned readiness plan: its exclusive lease and arm snapshot belong to the
/// same wake allocation and scheduling role by construction.
#[derive(Debug)]
#[doc(hidden)]
pub struct PreparedNativeWait {
    lease: NativePollLease,
    registration: WaitRegistration,
    deadline: ReadDeadline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum NativeWaitPreparationError {
    Closed(HostClosure),
    PollBusy(NativePollBusy),
}

impl PreparedNativeWait {
    /// Arming precedes the caller's durable input/scheduler predicate check.
    /// Failure never publishes another poller owner or resets its registration.
    #[doc(hidden)]
    pub fn new(
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

    #[doc(hidden)]
    pub fn poll(&self) -> Option<ControlWakeOutcome> {
        self.registration.poll()
    }

    #[doc(hidden)]
    pub fn wait_with<H: NativeWaitHandoff>(
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
