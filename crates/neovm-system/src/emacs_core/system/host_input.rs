//! Host-input publisher service (stage-6 S6-2 split).
//!
//! The receiving half (`HostInputReceiver`) stays in the evaluator because it
//! is a Context field; this system-layer service is what the frontend uses to
//! enqueue events and wake that evaluator.  It reaches eval's wait surface
//! (`Context`, `WaitNotifier`, `InputEvent`) and host_effect downward only.
//! `crate::emacs_core::host_input` re-exports this module and its receiver, so the facade
//! paths used by crates/neomacs stay valid.

use crate::emacs_core::process::SystemProcessExt;
use crossbeam_channel::{Receiver, Sender};
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::data::process_state::WaitNotifier;
use neovm_eval::emacs_core::eval::keyboard::InputEvent;
use neovm_eval::host_effect::wake::{
    ControlDestination, ControlNudge, ControlNudgeOutcome, HostSessionControl,
};
use std::sync::Arc;

pub(crate) use neovm_eval::emacs_core::eval::host_input::HostInputReceiver;

/// Test-only wake outcome the injected probe returns.
///
/// Compiled for this crate's tests and for upper test builds under
/// `test-support`; an upper test that boots a `Context` re-homes to core
/// (TEST-SUPPORT-DESIGN §1a option A) and reaches this seam through the
/// feature.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy)]
pub enum ProbeWake {
    Notified,
    Failed(std::io::ErrorKind),
}

/// Owns no sending capability: this observes only the wake side of publication.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone)]
pub struct WakeProbe {
    outcome: ProbeWake,
    notifications: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    observation: std::sync::Arc<std::sync::OnceLock<std::sync::Arc<dyn Fn() + Send + Sync>>>,
}

#[cfg(any(test, feature = "test-support"))]
impl WakeProbe {
    pub fn new(outcome: ProbeWake) -> Self {
        Self {
            outcome,
            notifications: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            observation: std::sync::Arc::new(std::sync::OnceLock::new()),
        }
    }

    pub fn observe(&self, observe: impl Fn() + Send + Sync + 'static) {
        assert!(
            self.observation.set(std::sync::Arc::new(observe)).is_ok(),
            "wake observation is installed once"
        );
    }

    pub fn count(&self) -> usize {
        self.notifications.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn notify(&self) -> HostInputWakeOutcome {
        self.notifications
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(observe) = self.observation.get() {
            observe();
        }
        match self.outcome {
            ProbeWake::Notified => HostInputWakeOutcome::Notified,
            ProbeWake::Failed(kind) => HostInputWakeOutcome::Failed(std::io::Error::new(
                kind,
                "injected native input wake failure",
            )),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
static_assertions::assert_impl_all!(WakeProbe: Send, Sync, Clone);

/// Build the injected-observation wake arm for a test publisher.
#[cfg(any(test, feature = "test-support"))]
pub fn observed_wake_for_test(probe: WakeProbe) -> InputWake {
    InputWake::Observed(probe)
}

/// Build the channel-fallback wake arm for a test publisher.
#[cfg(any(test, feature = "test-support"))]
pub fn channel_fallback_wake_for_test() -> InputWake {
    InputWake::ChannelFallback
}

/// Only a Context can pair a native wake capability with its input queue.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
enum InputWake {
    Native(WaitNotifier),
    ChannelFallback,
    Closed,
    #[cfg(any(test, feature = "test-support"))]
    Observed(WakeProbe),
}

impl InputWake {
    #[inline]
    pub fn notify(&self) -> HostInputWakeOutcome {
        match self {
            Self::Native(notifier) => match notifier.notify() {
                Ok(()) => HostInputWakeOutcome::Notified,
                Err(error) => HostInputWakeOutcome::Failed(error),
            },
            Self::ChannelFallback => HostInputWakeOutcome::ChannelFallback,
            Self::Closed => HostInputWakeOutcome::Closed,
            #[cfg(any(test, feature = "test-support"))]
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
        let publisher = Self::connect(context);
        publisher.announce_install();
        publisher
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    fn connect(context: &mut Context) -> Self {
        let session = context.processes().attach_host_control_session();
        let wake = match context.wait_notifier() {
            Some(notifier) => InputWake::Native(notifier),
            None => InputWake::ChannelFallback,
        };
        let (publisher, receiver) = Self::channel_with_session(wake, Some(session));
        context.attach_input_receiver(HostInputReceiver::from_receiver(receiver));
        publisher
    }

    fn announce_install(&self) {
        if let Some(session) = &self.connection.disconnect_wake.1 {
            for destination in [
                ControlDestination::NativeMain,
                ControlDestination::IdleScheduler,
            ] {
                if let ControlNudgeOutcome::WakeFailedAfterPublication(error) =
                    session.nudge(destination, ControlNudge::HostAttached)
                {
                    tracing::warn!(%error, "host attachment control wake failed after publication");
                }
            }
        }
    }

    /// A closed sink for batch auxiliary terminals, with no primary receiver.
    /// Creating it cannot attach input or start a Context's command loop.
    pub fn closed() -> Self {
        let (publisher, receiver) = Self::channel(InputWake::Closed);
        drop(receiver);
        publisher
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    fn channel(wake: InputWake) -> (Self, Receiver<InputEvent>) {
        Self::channel_with_session(wake, None)
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
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
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
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
