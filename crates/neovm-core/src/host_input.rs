//! Evaluator-bound input publication and its matching wait notification.
//!
//! Each Context owns the receiving queue and process wait backend. Publishers
//! may be cloned across producer threads, but cannot separate the queue from
//! that backend's wake capability. No Lisp state crosses this boundary.

use crate::emacs_core::Context;
use crate::emacs_core::process::WaitNotifier;
use crate::host_effect::wake::{
    ControlDestination, ControlNudge, ControlNudgeOutcome, HostSessionControl,
};
use crate::keyboard::InputEvent;
use crossbeam_channel::{Receiver, Sender};
use std::sync::Arc;

/// The receiving half is constructed only with its notifying publisher.
/// Context owns this optional host connection; absence still denotes batch input.
#[derive(Default, Debug)]
#[repr(transparent)]
pub(crate) struct HostInputReceiver(Option<Receiver<InputEvent>>);

impl HostInputReceiver {
    #[inline]
    pub(crate) fn as_ref(&self) -> Option<&Receiver<InputEvent>> {
        self.0.as_ref()
    }

    pub(crate) fn clear(&mut self) {
        self.0 = None;
    }
}

static_assertions::assert_eq_size!(HostInputReceiver, Option<Receiver<InputEvent>>);
static_assertions::assert_eq_align!(HostInputReceiver, Option<Receiver<InputEvent>>);

/// Only a Context can pair a native wake capability with its input queue.
enum InputWake {
    Native(WaitNotifier),
    ChannelFallback,
    Closed,
    #[cfg(test)]
    Observed(tests::WakeProbe),
}

impl InputWake {
    #[inline]
    fn notify(&self) -> HostInputWakeOutcome {
        match self {
            Self::Native(notifier) => match notifier.notify() {
                Ok(()) => HostInputWakeOutcome::Notified,
                Err(error) => HostInputWakeOutcome::Failed(error),
            },
            Self::ChannelFallback => HostInputWakeOutcome::ChannelFallback,
            Self::Closed => HostInputWakeOutcome::Closed,
            #[cfg(test)]
            Self::Observed(probe) => probe.notify(),
        }
    }
}

/// The result of notifying for input which has already been enqueued.
#[derive(Debug)]
pub enum HostInputWakeOutcome {
    Notified,
    ChannelFallback,
    Closed,
    Failed(std::io::Error),
}

impl HostInputWakeOutcome {
    #[inline]
    fn into_result(self) -> Result<(), std::io::Error> {
        match self {
            Self::Notified | Self::ChannelFallback | Self::Closed => Ok(()),
            Self::Failed(error) => Err(error),
        }
    }
}

/// A failed native wake never returns ownership of an enqueued event.
/// Batch callers must not replay a committed prefix after receiver closure.
#[derive(Debug, thiserror::Error)]
pub enum HostInputPublishError {
    #[error("input receiver closed before publication")]
    ReceiverClosed,
    #[error("input receiver closed after a published prefix (wake: {wake:?})")]
    ReceiverClosedAfterPrefix { wake: HostInputWakeOutcome },
    #[error("input was published but its native wake failed: {0}")]
    WakeAfterCommit(#[source] std::io::Error),
}

/// Declaration order is the disconnect protocol: retire the sender first,
/// then wake so a blocked reader can observe closure after its buffered tail.
#[repr(C)]
struct PublisherConnection {
    sender: Sender<InputEvent>,
    disconnect_wake: DisconnectWake,
}

#[repr(C)]
struct DisconnectWake(InputWake, Option<HostSessionControl>);

// Compare the original Rust layouts, before adding cold attachment storage.
// Field placement is part of the frozen publication-code contract.
struct FrozenPublisherConnection {
    sender: Sender<InputEvent>,
    disconnect_wake: FrozenDisconnectWake,
}
struct FrozenDisconnectWake(InputWake);

const _: () = {
    assert!(
        std::mem::offset_of!(PublisherConnection, sender)
            == std::mem::offset_of!(FrozenPublisherConnection, sender)
    );
    assert!(
        std::mem::offset_of!(PublisherConnection, disconnect_wake)
            == std::mem::offset_of!(FrozenPublisherConnection, disconnect_wake)
    );
    assert!(
        std::mem::offset_of!(DisconnectWake, 0) == std::mem::offset_of!(FrozenDisconnectWake, 0)
    );
};

impl Drop for DisconnectWake {
    fn drop(&mut self) {
        if let Some(session) = &self.1 {
            let outcomes = session.disconnect();
            // Replacement has already discarded this connection's receiver.
            // Its stale final drop must not notify the successor's backend.
            if outcomes
                .iter()
                .all(|outcome| matches!(outcome, ControlNudgeOutcome::RetiredSession))
            {
                return;
            }
            for outcome in outcomes {
                if let ControlNudgeOutcome::WakeFailedAfterPublication(error) = outcome {
                    tracing::warn!(%error, "control disconnect wake failed after sender retirement");
                }
            }
        }
        if let Err(error) = self.0.notify().into_result() {
            tracing::warn!(%error, "failed to wake evaluator for host input disconnect");
        }
    }
}

#[derive(Clone, Copy)]
enum PendingNotification {
    Pending,
    Discharged,
}

/// Borrows the connection: publishing a batch never clones/drops its owner.
/// An iterator unwind or partial send failure still wakes a committed prefix.
struct PublishedPrefix<'a> {
    wake: &'a InputWake,
    notification: PendingNotification,
}

impl<'a> PublishedPrefix<'a> {
    #[inline]
    fn new(wake: &'a InputWake) -> Self {
        Self {
            wake,
            notification: PendingNotification::Pending,
        }
    }

    #[inline]
    fn finish(mut self) -> HostInputWakeOutcome {
        // Disarm before notify: a failed wake is not retried by Drop.
        self.notification = PendingNotification::Discharged;
        self.wake.notify()
    }
}

impl Drop for PublishedPrefix<'_> {
    fn drop(&mut self) {
        match self.notification {
            PendingNotification::Pending => {
                if let Err(error) = self.wake.notify().into_result() {
                    tracing::warn!(%error, "failed to wake evaluator for published input prefix");
                }
            }
            PendingNotification::Discharged => {}
        }
    }
}

/// A session owns only its outstanding notification, never the connection.
/// Empty means no committed prefix remains to be notified by this owner.
enum PublicationState<'a> {
    Empty,
    Committed(PublishedPrefix<'a>),
}

/// One enqueue/notification authority shared by single and batch publication.
/// Its borrowed connection prevents per-event Arc ownership traffic.
struct PublicationSession<'a> {
    connection: &'a PublisherConnection,
    state: PublicationState<'a>,
}

impl<'a> PublicationSession<'a> {
    #[inline]
    fn new(connection: &'a PublisherConnection) -> Self {
        Self {
            connection,
            state: PublicationState::Empty,
        }
    }

    #[inline]
    fn enqueue(&mut self, event: InputEvent) -> Result<(), HostInputPublishError> {
        if self.connection.sender.send(event).is_err() {
            return Err(
                match std::mem::replace(&mut self.state, PublicationState::Empty) {
                    PublicationState::Empty => HostInputPublishError::ReceiverClosed,
                    PublicationState::Committed(prefix) => {
                        HostInputPublishError::ReceiverClosedAfterPrefix {
                            wake: prefix.finish(),
                        }
                    }
                },
            );
        }
        match &self.state {
            PublicationState::Empty => {
                self.state = PublicationState::Committed(PublishedPrefix::new(
                    &self.connection.disconnect_wake.0,
                ));
            }
            PublicationState::Committed(_) => {}
        }
        Ok(())
    }

    #[inline]
    fn finish(self) -> Result<(), HostInputPublishError> {
        match self.state {
            PublicationState::Empty => Ok(()),
            PublicationState::Committed(prefix) => prefix
                .finish()
                .into_result()
                .map_err(HostInputPublishError::WakeAfterCommit),
        }
    }
}

/// Thread-safe producer for one evaluator's host-input connection.
///
/// The only event operations enqueue AND notify. Cloning shares the connection;
/// only its final owner drop notifies disconnection. No per-event allocation or
/// owner clone is added by publication (the unbounded queue may allocate itself).
#[derive(Clone)]
pub struct HostInputPublisher {
    connection: Arc<PublisherConnection>,
}

impl std::fmt::Debug for HostInputPublisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostInputPublisher").finish_non_exhaustive()
    }
}

static_assertions::assert_impl_all!(HostInputPublisher: Send, Sync, Clone, std::fmt::Debug);
static_assertions::assert_eq_size!(HostInputPublisher, usize);

impl HostInputPublisher {
    /// Install a fresh connection without starting the command loop.
    ///
    /// Replacing a connection discards its unread channel tail; already-staged
    /// Context events remain queued. Old publishers stay on the retired queue.
    pub fn install(context: &mut Context) -> Self {
        let session = context.processes.attach_host_control_session();
        let wake = match context.wait_notifier() {
            Some(notifier) => InputWake::Native(notifier),
            None => InputWake::ChannelFallback,
        };
        let (publisher, receiver) = Self::channel_with_session(wake, Some(session));
        context.input_rx = HostInputReceiver(Some(receiver));
        publisher
    }

    /// A closed sink for batch auxiliary terminals, with no primary receiver.
    /// Creating it cannot attach input or start a Context's command loop.
    pub fn closed() -> Self {
        let (publisher, receiver) = Self::channel(InputWake::Closed);
        drop(receiver);
        publisher
    }

    fn channel(wake: InputWake) -> (Self, Receiver<InputEvent>) {
        Self::channel_with_session(wake, None)
    }

    fn channel_with_session(
        wake: InputWake,
        session: Option<HostSessionControl>,
    ) -> (Self, Receiver<InputEvent>) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let publisher = Self {
            connection: Arc::new(PublisherConnection {
                sender,
                disconnect_wake: DisconnectWake(wake, session),
            }),
        };
        (publisher, receiver)
    }

    /// Wake the main host's control observer without publishing keyboard input.
    /// The scheduler or target mailbox remains the sole durable predicate.
    pub(crate) fn nudge_control(&self, reason: ControlNudge) -> ControlNudgeOutcome {
        match &self.connection.disconnect_wake.1 {
            Some(session) => session.nudge(ControlDestination::NativeMain, reason),
            None => ControlNudgeOutcome::RetiredSession,
        }
    }

    /// Enqueue one event, then wake the receiving evaluator.
    #[inline]
    pub fn send(&self, event: InputEvent) -> Result<(), HostInputPublishError> {
        let mut publication = PublicationSession::new(&self.connection);
        publication.enqueue(event)?;
        publication.finish()
    }

    /// Publish a conversion batch, notifying once for its committed prefix.
    /// An empty batch does not wake. Other publishers may interleave events.
    #[inline]
    pub fn send_batch(
        &self,
        events: impl IntoIterator<Item = InputEvent>,
    ) -> Result<(), HostInputPublishError> {
        let mut events = events.into_iter();
        // Keep the iterator in this earlier binding. On unwind, the session
        // wakes its committed prefix before the iterator's destructor runs.
        let mut publication = PublicationSession::new(&self.connection);
        for event in &mut events {
            publication.enqueue(event)?;
        }
        publication.finish()
    }
}

#[cfg(any(test, feature = "redisplay-test-policy"))]
impl Context {
    /// Observe host input in dependent frontend fixtures without a raw receiver.
    #[doc(hidden)]
    pub fn receive_host_input_for_test(
        &self,
        timeout: std::time::Duration,
    ) -> Result<InputEvent, crossbeam_channel::RecvTimeoutError> {
        self.input_rx
            .as_ref()
            .ok_or(crossbeam_channel::RecvTimeoutError::Disconnected)?
            .recv_timeout(timeout)
    }
}

#[cfg(test)]
#[path = "host_input/tests/host_input_test.rs"]
mod tests;
