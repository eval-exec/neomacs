//! Owned keyed native completion, without poll leases or evaluator state.
//!
//! This is the Baton-serial instance of E5's released native plan protocol.
//! A pump borrows one physical wait authority for its next cycle; each pending
//! record retains only its operation's completion and cancellation obligations.

use super::contracts::{HostRequestKey, ReadDeadline};
use super::native_outcome::{CancelReason, NativeOutcome};
use super::wake::{ControlDestination, ControlNudge, ControlNudgeOutcome, ControlNudgePublisher};
use crossbeam_channel::{Receiver, Sender, TryRecvError, unbounded};
use std::convert::Infallible;
use std::time::Instant;

/// A pending native operation is returned, never a borrowed scope continuation.
pub(crate) enum NativeAdapterStep<Owner = Infallible> {
    Complete(NativeOutcome),
    Pending(PendingNativeOperation),
    RequiresOwner(Owner),
}

/// Cancellation asks the native owner to settle; it does not claim rollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeCancellationRequest {
    Owner(CancelReason),
    DeadlineElapsed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeCancellationObservation {
    NoRequest,
    Requested(NativeCancellationRequest),
    OwnerClosed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeCancelProgress {
    Delivered,
    ProducerClosed,
    AlreadyAwaitingSettlement,
    TerminalAlreadyObserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeDeadlineProgress {
    NotElapsed,
    Cancellation(NativeCancelProgress),
}

/// The private key is bound by the paired publisher; callers cannot relabel it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct NativeCompletion {
    key: HostRequestKey,
    outcome: NativeOutcome,
}

impl NativeCompletion {
    pub(crate) fn key(&self) -> HostRequestKey {
        self.key
    }

    pub(crate) fn into_parts(self) -> (HostRequestKey, NativeOutcome) {
        (self.key, self.outcome)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeProducerClosed {
    key: HostRequestKey,
}

impl NativeProducerClosed {
    pub(crate) fn key(self) -> HostRequestKey {
        self.key
    }
}

#[derive(Debug)]
pub(crate) enum NativePendingObservation {
    Pending,
    Complete(NativeCompletion),
    Closed(NativeProducerClosed),
    AlreadyObserved,
}

#[derive(Debug)]
enum PendingPhase {
    Waiting(Sender<NativeCancellationRequest>),
    AwaitingSettlement(NativeCancellationRequest),
    Observed,
}

/// The absolute deadline never changes when another request is served.
#[derive(Debug)]
pub(crate) struct PendingNativeOperation {
    key: HostRequestKey,
    deadline: ReadDeadline,
    completion: Receiver<NativeCompletion>,
    phase: PendingPhase,
}

impl PendingNativeOperation {
    pub(crate) fn key(&self) -> HostRequestKey {
        self.key
    }

    pub(crate) fn deadline(&self) -> ReadDeadline {
        self.deadline
    }

    /// An expired deadline requests cancellation only once. Native settlement
    /// thereafter waits on actual readiness, rather than spinning on the past.
    pub(crate) fn next_wait_deadline(&self) -> ReadDeadline {
        match self.phase {
            PendingPhase::Waiting(_) => self.deadline,
            PendingPhase::AwaitingSettlement(_) | PendingPhase::Observed => ReadDeadline::Unbounded,
        }
    }

    pub(crate) fn cancellation_requested(&self) -> Option<NativeCancellationRequest> {
        match self.phase {
            PendingPhase::AwaitingSettlement(request) => Some(request),
            PendingPhase::Waiting(_) | PendingPhase::Observed => None,
        }
    }

    pub(crate) fn request_cancel(&mut self, reason: CancelReason) -> NativeCancelProgress {
        self.request(NativeCancellationRequest::Owner(reason))
    }

    pub(crate) fn expire_deadline(&mut self, now: Instant) -> NativeDeadlineProgress {
        let elapsed = match self.deadline {
            ReadDeadline::Immediate => true,
            ReadDeadline::At(end) => now >= end,
            ReadDeadline::Unbounded => false,
        };
        if !elapsed {
            return NativeDeadlineProgress::NotElapsed;
        }
        NativeDeadlineProgress::Cancellation(
            self.request(NativeCancellationRequest::DeadlineElapsed),
        )
    }

    fn request(&mut self, request: NativeCancellationRequest) -> NativeCancelProgress {
        let previous = std::mem::replace(&mut self.phase, PendingPhase::Observed);
        match previous {
            PendingPhase::Waiting(sender) => {
                self.phase = PendingPhase::AwaitingSettlement(request);
                match sender.send(request) {
                    Ok(()) => NativeCancelProgress::Delivered,
                    Err(_) => NativeCancelProgress::ProducerClosed,
                }
            }
            PendingPhase::AwaitingSettlement(original) => {
                self.phase = PendingPhase::AwaitingSettlement(original);
                NativeCancelProgress::AlreadyAwaitingSettlement
            }
            PendingPhase::Observed => NativeCancelProgress::TerminalAlreadyObserved,
        }
    }

    pub(crate) fn observe(&mut self) -> NativePendingObservation {
        if matches!(self.phase, PendingPhase::Observed) {
            return NativePendingObservation::AlreadyObserved;
        }
        match self.completion.try_recv() {
            Ok(completion) => {
                self.phase = PendingPhase::Observed;
                NativePendingObservation::Complete(completion)
            }
            Err(TryRecvError::Empty) => NativePendingObservation::Pending,
            Err(TryRecvError::Disconnected) => {
                self.phase = PendingPhase::Observed;
                NativePendingObservation::Closed(NativeProducerClosed { key: self.key })
            }
        }
    }
}

/// Immutable primary plus settlement wake ownership. Neither route can expose
/// a bare completion sender or redirect a publication to another destination.
#[derive(Clone, Debug)]
pub(crate) struct NativeCompletionRoute {
    primary: ControlNudgePublisher,
    settlement: ControlNudgePublisher,
}

impl NativeCompletionRoute {
    pub(crate) fn new(primary: ControlNudgePublisher, settlement: ControlNudgePublisher) -> Self {
        Self {
            primary,
            settlement,
        }
    }

    fn notify(&self) -> NativeCompletionNotification {
        let primary = self.primary.nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady,
        );
        // Always publish settlement readiness, including after the primary
        // native source has closed. Its independent owner remains wakeable.
        let settlement = self.settlement.nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady,
        );
        NativeCompletionNotification::Published {
            primary,
            settlement,
        }
    }
}

#[derive(Debug)]
pub(crate) enum NativeCompletionNotification {
    Published {
        primary: ControlNudgeOutcome,
        settlement: ControlNudgeOutcome,
    },
    AlreadyPublished,
}

#[derive(Debug)]
enum CompletionWakeState {
    Pending(NativeCompletionRoute),
    Published,
}

/// Only this wrapper has Drop. Moving the state enum never recursively runs
/// the wake destructor, so one publication has one effective route notification.
#[derive(Debug)]
struct CompletionWake {
    state: CompletionWakeState,
}

impl CompletionWake {
    fn notify(&mut self) -> NativeCompletionNotification {
        match std::mem::replace(&mut self.state, CompletionWakeState::Published) {
            CompletionWakeState::Pending(route) => route.notify(),
            CompletionWakeState::Published => NativeCompletionNotification::AlreadyPublished,
        }
    }
}

impl Drop for CompletionWake {
    fn drop(&mut self) {
        let _ = self.notify();
    }
}

/// One linear producer. Field drop order commits sender disconnect before
/// its RAII wake; completion consumes it and explicitly publishes in that order.
#[derive(Debug)]
pub(crate) struct NativeCompletionPublisher {
    key: HostRequestKey,
    sender: Sender<NativeCompletion>,
    cancellations: Receiver<NativeCancellationRequest>,
    wake: CompletionWake,
}

#[derive(Debug)]
pub(crate) enum NativePublication {
    Committed(NativeCompletionNotification),
    ReceiverClosed {
        completion: NativeCompletion,
        wake: NativeCompletionNotification,
    },
}

impl NativeCompletionPublisher {
    pub(crate) fn cancellation(&self) -> NativeCancellationObservation {
        match self.cancellations.try_recv() {
            Ok(request) => NativeCancellationObservation::Requested(request),
            Err(TryRecvError::Empty) => NativeCancellationObservation::NoRequest,
            Err(TryRecvError::Disconnected) => NativeCancellationObservation::OwnerClosed,
        }
    }

    /// A native producer without the Lisp Baton can wait for the owned
    /// cancellation request. This does not mint a Green real-wait permit.
    pub(crate) fn wait_cancellation(&self) -> NativeCancellationObservation {
        match self.cancellations.recv() {
            Ok(request) => NativeCancellationObservation::Requested(request),
            Err(_) => NativeCancellationObservation::OwnerClosed,
        }
    }

    pub(crate) fn complete(self, outcome: NativeOutcome) -> NativePublication {
        let Self {
            key,
            sender,
            cancellations: _,
            mut wake,
        } = self;
        let sent = sender.send(NativeCompletion { key, outcome });
        drop(sender);
        let wake = wake.notify();
        match sent {
            Ok(()) => NativePublication::Committed(wake),
            Err(error) => NativePublication::ReceiverClosed {
                completion: error.0,
                wake,
            },
        }
    }
}

pub(crate) fn pair(
    key: HostRequestKey,
    deadline: ReadDeadline,
    route: NativeCompletionRoute,
) -> (PendingNativeOperation, NativeCompletionPublisher) {
    let (sender, completion) = unbounded();
    let (cancel, cancellations) = unbounded();
    (
        PendingNativeOperation {
            key,
            deadline,
            completion,
            phase: PendingPhase::Waiting(cancel),
        },
        NativeCompletionPublisher {
            key,
            sender,
            cancellations,
            wake: CompletionWake {
                state: CompletionWakeState::Pending(route),
            },
        },
    )
}

static_assertions::assert_impl_all!(NativeCompletionRoute: Send, Sync, Clone);
static_assertions::assert_impl_all!(PendingNativeOperation: Send);
static_assertions::assert_not_impl_any!(PendingNativeOperation: Clone, Copy);
static_assertions::assert_impl_all!(NativeCompletionPublisher: Send);
static_assertions::assert_not_impl_any!(NativeCompletionPublisher: Clone, Copy);

#[cfg(test)]
#[path = "tests/native_pending_test.rs"]
mod tests;
