//! Dormant, value-free control readiness for an owner-paired native wait.
//!
//! A notification is a hint to inspect independently durable scheduler state.
//! Callers arm first, recheck that state under its owning metadata lock, then
//! wait. Neither a reason nor an epoch completes a host operation. This module
//! owns no evaluator, root, callback, or authority to run Lisp.

use std::io;
use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, Weak};
use std::time::Instant;

use super::contracts::ReadDeadline;

/// Keep the ordinary input bit at the allocation's original address. Only cold
/// attachment/control operations consult the sidecar. Hot forwarding methods
/// must still pass the caller's frozen-code comparison; layout alone is no proof.
#[repr(C)]
#[derive(Debug)]
pub(crate) struct NotificationLatch {
    input_pending: AtomicBool,
    control: OnceLock<Arc<ControlAttachment>>,
}

impl NotificationLatch {
    pub(crate) const fn new() -> Self {
        Self {
            input_pending: AtomicBool::new(false),
            control: OnceLock::new(),
        }
    }

    pub(crate) fn control(&self) -> Option<&Arc<ControlWake>> {
        self.control.get().map(|attachment| &attachment.wake)
    }

    pub(crate) fn install_control(&self, wake: Arc<ControlWake>) -> Result<(), Arc<ControlWake>> {
        self.control
            .set(Arc::new(ControlAttachment {
                wake: Arc::clone(&wake),
                readiness: NativeReadiness::Unavailable(NativeReadinessCause::AbsentPoller),
                refusal_observation: OnceLock::new(),
            }))
            .map_err(|_| wake)
    }

    /// Cold, once-per-backend readiness admission. A failed native source is
    /// still a paired control owner, but never a process-readiness capability.
    pub(crate) fn attach_native(&self, poller: Option<Arc<polling::Poller>>) -> &ControlAttachment {
        self.control
            .get_or_init(|| Arc::new(ControlAttachment::open(poller, ControlWake::paired_native)))
    }

    #[cfg(test)]
    pub(crate) fn refuse_native_for_test(&self, cause: NativeReadinessCause) {
        let attachment = match cause {
            NativeReadinessCause::AbsentPoller => {
                ControlAttachment::open(None, ControlWake::paired_native)
            }
            NativeReadinessCause::RegistrationFailed(kind) => ControlAttachment::open(
                Some(Arc::new(
                    polling::Poller::new().expect("test native poller"),
                )),
                |_| Err(io::Error::from(kind)),
            ),
        };
        assert!(
            self.control.set(Arc::new(attachment)).is_ok(),
            "inject only before first attachment"
        );
    }

    pub(crate) fn nudge(
        &self,
        destination: ControlDestination,
        reason: ControlNudge,
    ) -> ControlNudgeOutcome {
        match self.control() {
            Some(wake) => wake.handle().nudge(destination, reason),
            None => ControlNudgeOutcome::RetiredSession,
        }
    }
}

/// Typed cause retained by cold admission, never surfaced as a Lisp condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeReadinessCause {
    AbsentPoller,
    RegistrationFailed(io::ErrorKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeReadiness {
    Available,
    Unavailable(NativeReadinessCause),
}

/// The only Available constructor creates and registers the owned native
/// source first. No caller can label a condition-only endpoint as native-ready.
#[derive(Debug)]
pub(crate) struct ControlAttachment {
    wake: Arc<ControlWake>,
    readiness: NativeReadiness,
    // The receipt retains the actual cause observed by the cold admission
    // trace. A healthy attachment never formats/emits or initializes it.
    refusal_observation: OnceLock<NativeReadinessCause>,
}

impl ControlAttachment {
    fn open(
        poller: Option<Arc<polling::Poller>>,
        create: impl FnOnce(
            Arc<polling::Poller>,
        ) -> io::Result<(Arc<ControlWake>, ControlNudgePublisher)>,
    ) -> Self {
        let cause = match poller {
            Some(poller) => match create(poller) {
                Ok((wake, _)) => {
                    return Self {
                        wake,
                        readiness: NativeReadiness::Available,
                        refusal_observation: OnceLock::new(),
                    };
                }
                Err(error) => NativeReadinessCause::RegistrationFailed(error.kind()),
            },
            None => NativeReadinessCause::AbsentPoller,
        };
        Self {
            wake: ControlWake::paired().0,
            readiness: NativeReadiness::Unavailable(cause),
            refusal_observation: OnceLock::new(),
        }
    }

    pub(crate) fn observe_admission_refusal(&self, observe: impl FnOnce(NativeReadinessCause)) {
        match self.readiness {
            NativeReadiness::Available => {}
            NativeReadiness::Unavailable(cause) => {
                self.refusal_observation.get_or_init(|| {
                    observe(cause);
                    cause
                });
            }
        }
    }

    pub(crate) fn require_native(&self) -> Result<&Arc<ControlWake>, NativeReadinessCause> {
        match self.readiness {
            NativeReadiness::Available => Ok(&self.wake),
            NativeReadiness::Unavailable(cause) => Err(cause),
        }
    }

    pub(crate) fn control(&self) -> &Arc<ControlWake> {
        &self.wake
    }
}

impl Deref for NotificationLatch {
    type Target = AtomicBool;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.input_pending
    }
}

const _: () = assert!(std::mem::offset_of!(NotificationLatch, input_pending) == 0);
static_assertions::assert_eq_size!(Arc<NotificationLatch>, Arc<AtomicBool>);
static_assertions::assert_eq_align!(Arc<NotificationLatch>, Arc<AtomicBool>);
static_assertions::assert_impl_all!(NotificationLatch: Send, Sync);
#[cfg(target_pointer_width = "64")]
static_assertions::const_assert_eq!(std::mem::size_of::<NotificationLatch>(), 24);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlNudge {
    HostServiceReady,
    MainMailboxReady,
    Cancellation,
    HostClosing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlDestination {
    NativeMain,
    IdleScheduler,
}

impl ControlDestination {
    fn index(self) -> usize {
        match self {
            Self::NativeMain => 0,
            Self::IdleScheduler => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ControlWakeEpoch(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostClosure {
    RetiredSession,
    NativeWakeFailed,
    EpochExhausted,
}

/// Every variant describes work that was already published by the caller.
/// Even `WakeFailedAfterPublication` does not authorize replay.
#[derive(Debug)]
#[must_use]
pub(crate) enum ControlNudgeOutcome {
    Notified,
    AlreadyPending,
    RetiredSession,
    WakeFailedAfterPublication(io::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlWakeOutcome {
    ControlReady(ControlNudge),
    /// A cancellation hint, requiring the caller's durable-state recheck.
    CancellationRequested,
    NativeReady,
    DeadlineElapsed,
    HostClosed(HostClosure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativePollOwner {
    NativeMain,
    IdleScheduler,
}

impl NativePollOwner {
    fn destination(self) -> ControlDestination {
        match self {
            Self::NativeMain => ControlDestination::NativeMain,
            Self::IdleScheduler => ControlDestination::IdleScheduler,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativePollBusy {
    pub(crate) owner: NativePollOwner,
}

#[derive(Clone, Copy, Debug, Default)]
struct DestinationState {
    publication: ControlPublication,
    armed_epoch: ControlWakeEpoch,
}

#[derive(Clone, Copy, Debug, Default)]
enum ControlPublication {
    #[default]
    Empty,
    Published {
        epoch: ControlWakeEpoch,
        reason: ControlNudge,
    },
}

impl ControlPublication {
    fn epoch(self) -> ControlWakeEpoch {
        match self {
            Self::Empty => ControlWakeEpoch::default(),
            Self::Published { epoch, .. } => epoch,
        }
    }
}

#[derive(Debug, Default)]
struct Metadata {
    destinations: [DestinationState; 2],
    closed: Option<HostClosure>,
    poll_owner: Option<NativePollOwner>,
    host_session: Option<Arc<HostSessionIdentity>>,
}

#[derive(Debug)]
struct HostSessionIdentity;

/// A host connection's exact attachment identity, separate from its backend's
/// lifetime. The weak backend reference prevents an owner/session reference
/// cycle. Replacing or revoking the identity makes every retained clone stale.
#[derive(Clone, Debug)]
pub(crate) struct HostSessionControl {
    wake: Weak<ControlWake>,
    identity: Arc<HostSessionIdentity>,
}

enum NudgeScope<'a> {
    Backend,
    HostSession(&'a Arc<HostSessionIdentity>),
}

/// The session is its allocation identity: publishers retain this exact owner,
/// never a replaceable slot or a numeric generation that could target a successor.
#[derive(Debug)]
pub(crate) struct ControlWake {
    metadata: Mutex<Metadata>,
    changed: Condvar,
    native: Option<NativeEndpoint>,
    #[cfg(test)]
    wait_stage: Mutex<Option<std::sync::mpsc::Sender<()>>>,
    #[cfg(test)]
    fallback_fault: Mutex<Option<FallbackFault>>,
    #[cfg(test)]
    clock: Mutex<Option<Instant>>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
enum FallbackFault {
    BlindIdleSleep,
    BlindNoReceiverSleep,
}

/// Constructed only with its receiving owner. It carries no input sender and
/// cannot be redirected to another session or expose a native write handle.
#[derive(Clone, Debug)]
pub(crate) struct ControlNudgePublisher {
    wake: Arc<ControlWake>,
}

impl ControlWake {
    pub(crate) fn paired() -> (Arc<Self>, ControlNudgePublisher) {
        Self::pair(Self {
            metadata: Mutex::new(Metadata::default()),
            changed: Condvar::new(),
            native: None,
            #[cfg(test)]
            wait_stage: Mutex::new(None),
            #[cfg(test)]
            fallback_fault: Mutex::new(None),
            #[cfg(test)]
            clock: Mutex::new(None),
        })
    }

    /// The process backend reserves MAX-1 for signals; this source reserves
    /// MAX-2, outside ordinary process descriptor keys.
    ///
    /// A private nonblocking socket, rather than Poller::notify alone, guarantees
    /// that closing the unique write side wakes a registered native reader after
    /// publication failure. Unix uses a socket pair; other platforms use a
    /// privately connected loopback TCP pair with exactly the same ownership.
    pub(crate) fn paired_native(
        poller: Arc<polling::Poller>,
    ) -> io::Result<(Arc<Self>, ControlNudgePublisher)> {
        let native = NativeEndpoint::new(poller)?;
        Ok(Self::pair(Self {
            metadata: Mutex::new(Metadata::default()),
            changed: Condvar::new(),
            native: Some(native),
            #[cfg(test)]
            wait_stage: Mutex::new(None),
            #[cfg(test)]
            fallback_fault: Mutex::new(None),
            #[cfg(test)]
            clock: Mutex::new(None),
        }))
    }

    fn pair(wake: Self) -> (Arc<Self>, ControlNudgePublisher) {
        let wake = Arc::new(wake);
        let publisher = ControlNudgePublisher {
            wake: Arc::clone(&wake),
        };
        (wake, publisher)
    }

    pub(crate) fn handle(self: &Arc<Self>) -> ControlNudgePublisher {
        ControlNudgePublisher {
            wake: Arc::clone(self),
        }
    }

    /// Cold owner attachment. Installation always creates a fresh session;
    /// physical input publication is independent of Green admission.
    pub(crate) fn attach_host_session(self: &Arc<Self>) -> HostSessionControl {
        let identity = Arc::new(HostSessionIdentity);
        self.lock().host_session = Some(Arc::clone(&identity));
        HostSessionControl {
            wake: Arc::downgrade(self),
            identity,
        }
    }

    /// Revoke host attachment without retiring the backend's scheduler source.
    pub(crate) fn revoke_host_session(&self) {
        self.lock().host_session = None;
    }

    fn revoke_exact_host_session(&self, identity: &Arc<HostSessionIdentity>) {
        let mut metadata = self.lock();
        if metadata
            .host_session
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, identity))
        {
            metadata.host_session = None;
        }
    }

    fn lock(&self) -> MutexGuard<'_, Metadata> {
        self.metadata.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn now(&self) -> Instant {
        #[cfg(test)]
        if let Some(now) = *self.clock.lock().unwrap() {
            return now;
        }
        Instant::now()
    }

    #[cfg(test)]
    fn report_wait_stage(&self) {
        if let Some(sender) = &*self.wait_stage.lock().unwrap() {
            // Unbounded send never waits for the observing test. In fallback
            // this happens while holding the predicate lock, directly before
            // Condvar atomically unlocks; in native wait the source is armed.
            let _ = sender.send(());
        }
    }

    pub(crate) fn arm(
        self: &Arc<Self>,
        destination: ControlDestination,
    ) -> Result<WaitRegistration, HostClosure> {
        let mut metadata = self.lock();
        if let Some(closed) = metadata.closed {
            return Err(closed);
        }
        let state = &mut metadata.destinations[destination.index()];
        state.armed_epoch = state.publication.epoch();
        Ok(WaitRegistration {
            wake: Arc::clone(self),
            destination,
            epoch: state.publication.epoch(),
        })
    }

    pub(crate) fn try_native_poll(
        self: &Arc<Self>,
        owner: NativePollOwner,
    ) -> Result<NativePollLease, NativePollBusy> {
        let mut metadata = self.lock();
        if let Some(owner) = metadata.poll_owner {
            return Err(NativePollBusy { owner });
        }
        metadata.poll_owner = Some(owner);
        Ok(NativePollLease {
            wake: Arc::clone(self),
            owner,
        })
    }

    /// Retirement is durable before closing the owned wake source. An old
    /// publisher can only observe this allocation's retired session.
    pub(crate) fn retire(&self) {
        self.close(HostClosure::RetiredSession);
    }

    fn close(&self, closure: HostClosure) {
        {
            let mut metadata = self.lock();
            metadata.closed.get_or_insert(closure);
        }
        self.changed.notify_all();
        if let Some(native) = &self.native {
            native.close_writer();
        }
    }
}

impl ControlNudgePublisher {
    pub(crate) fn nudge(
        &self,
        destination: ControlDestination,
        reason: ControlNudge,
    ) -> ControlNudgeOutcome {
        self.wake
            .publish_nudge(destination, reason, NudgeScope::Backend)
    }
}

impl HostSessionControl {
    pub(crate) fn nudge(
        &self,
        destination: ControlDestination,
        reason: ControlNudge,
    ) -> ControlNudgeOutcome {
        match self.wake.upgrade() {
            Some(wake) => {
                wake.publish_nudge(destination, reason, NudgeScope::HostSession(&self.identity))
            }
            None => ControlNudgeOutcome::RetiredSession,
        }
    }

    /// The sender has already been dropped. Both hints use this exact session,
    /// and revocation cannot affect a replacement installed during disconnect.
    pub(crate) fn disconnect(&self) -> [ControlNudgeOutcome; 2] {
        let outcomes = [
            self.nudge(ControlDestination::NativeMain, ControlNudge::HostClosing),
            self.nudge(ControlDestination::IdleScheduler, ControlNudge::HostClosing),
        ];
        if let Some(wake) = self.wake.upgrade() {
            wake.revoke_exact_host_session(&self.identity);
        }
        outcomes
    }
}

impl ControlWake {
    fn publish_nudge(
        &self,
        destination: ControlDestination,
        reason: ControlNudge,
        scope: NudgeScope<'_>,
    ) -> ControlNudgeOutcome {
        let already_pending = {
            let mut metadata = self.lock();
            if metadata.closed.is_some() {
                return ControlNudgeOutcome::RetiredSession;
            }
            if let NudgeScope::HostSession(identity) = scope
                && !metadata
                    .host_session
                    .as_ref()
                    .is_some_and(|active| Arc::ptr_eq(active, identity))
            {
                return ControlNudgeOutcome::RetiredSession;
            }
            let state = &mut metadata.destinations[destination.index()];
            let Some(next) = state.publication.epoch().0.checked_add(1) else {
                drop(metadata);
                self.close(HostClosure::EpochExhausted);
                return ControlNudgeOutcome::WakeFailedAfterPublication(io::Error::other(
                    "control wake epoch exhausted; session closed",
                ));
            };
            let pending = state.publication.epoch() != state.armed_epoch;
            state.publication = ControlPublication::Published {
                epoch: ControlWakeEpoch(next),
                reason,
            };
            pending
        };
        self.changed.notify_all();
        if let Some(native) = &self.native {
            match native.notify() {
                Ok(NativeNotification::AlreadyPending) => {
                    return ControlNudgeOutcome::AlreadyPending;
                }
                Ok(NativeNotification::Written) => {}
                Err(error) => {
                    self.close(HostClosure::NativeWakeFailed);
                    return ControlNudgeOutcome::WakeFailedAfterPublication(error);
                }
            }
        }
        if already_pending {
            ControlNudgeOutcome::AlreadyPending
        } else {
            ControlNudgeOutcome::Notified
        }
    }
}

/// Linear arm snapshot, independent of the physical poller's notification bit.
#[derive(Debug)]
pub(crate) struct WaitRegistration {
    wake: Arc<ControlWake>,
    destination: ControlDestination,
    epoch: ControlWakeEpoch,
}

impl WaitRegistration {
    #[cfg(test)]
    fn report_fallback_stage(&self, metadata: &Metadata, reported: &mut Option<ControlWakeEpoch>) {
        let epoch = metadata.destinations[self.destination.index()]
            .publication
            .epoch();
        if *reported != Some(epoch) {
            *reported = Some(epoch);
            self.wake.report_wait_stage();
        }
    }

    fn observe(&self, metadata: &Metadata) -> Option<ControlWakeOutcome> {
        if let Some(closed) = metadata.closed {
            return Some(ControlWakeOutcome::HostClosed(closed));
        }
        match metadata.destinations[self.destination.index()].publication {
            ControlPublication::Empty => None,
            ControlPublication::Published { epoch, .. } if epoch == self.epoch => None,
            ControlPublication::Published {
                reason: ControlNudge::Cancellation,
                ..
            } => Some(ControlWakeOutcome::CancellationRequested),
            ControlPublication::Published { reason, .. } => {
                Some(ControlWakeOutcome::ControlReady(reason))
            }
        }
    }

    pub(crate) fn poll(&self) -> Option<ControlWakeOutcome> {
        self.observe(&self.wake.lock())
    }

    /// The same absolute deadline survives every spurious or unrelated wake.
    /// Condvar atomically releases the predicate lock on entry to the OS wait.
    pub(crate) fn wait(&self, deadline: ReadDeadline) -> ControlWakeOutcome {
        #[cfg(test)]
        let blind = self.wake.fallback_fault.lock().unwrap().take().is_some();
        #[cfg(not(test))]
        let blind = false;
        #[cfg(test)]
        let mut reported_epoch = None;
        let mut metadata = self.wake.lock();
        loop {
            if let Some(outcome) = self.observe(&metadata) {
                if !blind || matches!(outcome, ControlWakeOutcome::HostClosed(_)) {
                    return outcome;
                }
            }
            match deadline {
                ReadDeadline::Immediate => return ControlWakeOutcome::DeadlineElapsed,
                ReadDeadline::At(deadline) => {
                    let remaining = deadline.saturating_duration_since(self.wake.now());
                    if remaining.is_zero() {
                        return ControlWakeOutcome::DeadlineElapsed;
                    }
                    #[cfg(test)]
                    self.report_fallback_stage(&metadata, &mut reported_epoch);
                    let (next, _) = self
                        .wake
                        .changed
                        .wait_timeout(metadata, remaining)
                        .unwrap_or_else(|e| e.into_inner());
                    metadata = next;
                }
                ReadDeadline::Unbounded => {
                    #[cfg(test)]
                    self.report_fallback_stage(&metadata, &mut reported_epoch);
                    metadata = self
                        .wake
                        .changed
                        .wait(metadata)
                        .unwrap_or_else(|e| e.into_inner());
                }
            }
        }
    }
}

/// Sole hosted physical poll authority. No metadata or native writer guard is
/// retained across Poller::wait. Ordinary unhosted waits keep their old path.
///
/// This linear lease is automatically Send: it owns only I/O and synchronization
/// metadata, and waiting requires exclusive mutable access. Its owner tag names
/// a scheduling role, not evaluator thread affinity. A later native adapter must
/// supply that separate affinity authority; no unsafe Send implementation or
/// evaluator state is carried here.
#[derive(Debug)]
pub(crate) struct NativePollLease {
    wake: Arc<ControlWake>,
    owner: NativePollOwner,
}

impl NativePollLease {
    pub(crate) fn wait(
        &mut self,
        registration: &WaitRegistration,
        events: &mut polling::Events,
        deadline: ReadDeadline,
    ) -> io::Result<ControlWakeOutcome> {
        if !Arc::ptr_eq(&self.wake, &registration.wake)
            || self.owner.destination() != registration.destination
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "native lease and registration belong to different owners",
            ));
        }
        events.clear();
        if let Some(native) = &self.wake.native {
            loop {
                if let Some(outcome) = registration.poll() {
                    return Ok(outcome);
                }
                if matches!(deadline, ReadDeadline::At(value) if Instant::now() >= value) {
                    return Ok(ControlWakeOutcome::DeadlineElapsed);
                }
                events.clear();
                // Rearm BEFORE the final epoch check. Publication between that
                // check and wait remains durable in this registered socket.
                if native.health.load() == NativeHealth::Failed {
                    return Ok(registration.wait(deadline));
                }
                if let Err(error) = native.rearm() {
                    native.health.fail();
                    return Err(error);
                }
                if let Some(outcome) = registration.poll() {
                    return Ok(outcome);
                }
                #[cfg(test)]
                self.wake.report_wait_stage();
                // Keep the existing Poller::wait entry shared with ordinary
                // input. Introducing wait_deadline creates a second caller of
                // polling's private wait_impl and changes its fat-LTO inlining
                // into the ordinary process wait. Derive each relative wait
                // from the same original deadline, after rearm/final recheck;
                // unrelated wakes never restart the requested duration.
                let timeout = match deadline {
                    ReadDeadline::Immediate => Some(std::time::Duration::ZERO),
                    ReadDeadline::At(value) => {
                        Some(value.saturating_duration_since(Instant::now()))
                    }
                    ReadDeadline::Unbounded => None,
                };
                let result = native.poller.wait(events, timeout);
                if let Err(error) = result {
                    native.health.fail();
                    return Err(error);
                }
                if events.iter().any(|event| event.key == native.event_key.0) {
                    native.drain()?;
                }
                if let Some(outcome) = registration.poll() {
                    return Ok(outcome);
                }
                if events.iter().any(|event| event.key != native.event_key.0) {
                    return Ok(ControlWakeOutcome::NativeReady);
                }
                if matches!(deadline, ReadDeadline::Immediate) {
                    return Ok(ControlWakeOutcome::DeadlineElapsed);
                }
                // An eventless Poller::notify is ordinary input readiness;
                // native owners forward it without pretending it is a timeout.
                if events.is_empty() {
                    return Ok(
                        if matches!(deadline, ReadDeadline::At(value) if Instant::now() >= value) {
                            ControlWakeOutcome::DeadlineElapsed
                        } else {
                            ControlWakeOutcome::NativeReady
                        },
                    );
                }
            }
        }
        Ok(registration.wait(deadline))
    }
}

impl Drop for NativePollLease {
    fn drop(&mut self) {
        {
            let mut metadata = self.wake.lock();
            debug_assert_eq!(metadata.poll_owner, Some(self.owner));
            metadata.poll_owner = None;
        }
        self.wake.changed.notify_all();
    }
}

#[derive(Debug)]
struct NativeEndpoint {
    poller: Arc<polling::Poller>,
    reader: WakeStream,
    writer: Mutex<Option<WakeStream>>,
    event_key: ControlEventKey,
    health: NativeHealthCell,
    #[cfg(test)]
    fail_next_notification: AtomicBool,
    #[cfg(test)]
    suppress_close: AtomicBool,
}

#[derive(Debug)]
struct ControlEventKey(usize);

enum NativeNotification {
    Written,
    AlreadyPending,
}

impl ControlEventKey {
    const fn reserved() -> Self {
        Self(usize::MAX - 2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeHealth {
    Ready,
    Failed,
}

#[derive(Debug)]
struct NativeHealthCell(AtomicBool);

impl NativeHealthCell {
    fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    fn load(&self) -> NativeHealth {
        if self.0.load(Ordering::Acquire) {
            NativeHealth::Failed
        } else {
            NativeHealth::Ready
        }
    }

    fn fail(&self) {
        self.0.store(true, Ordering::Release);
    }
}

#[cfg(unix)]
type WakeStream = std::os::unix::net::UnixStream;
#[cfg(not(unix))]
type WakeStream = std::net::TcpStream;

fn wake_stream_pair() -> io::Result<(WakeStream, WakeStream)> {
    #[cfg(unix)]
    {
        WakeStream::pair()
    }
    #[cfg(not(unix))]
    {
        loopback_wake_stream_pair()
    }
}

#[cfg(any(not(unix), test))]
const LOOPBACK_WAKE_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Construction owns the listener until its single accept. It cannot be kept
/// alive by an endpoint returned to a caller or by the registered wake source.
#[cfg(any(not(unix), test))]
struct LoopbackWakeListener(std::net::TcpListener);

#[cfg(any(not(unix), test))]
impl LoopbackWakeListener {
    fn bind() -> io::Result<Self> {
        std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map(Self)
    }

    fn into_pair(self) -> io::Result<LoopbackWakePair> {
        self.into_pair_with(LoopbackWakePair::validate)
    }

    /// One construction path; the test-only caller can omit just validation
    /// to prove the foreign-first connection is an actual invariant violation.
    fn into_pair_with(
        self,
        validate: impl FnOnce(
            std::net::TcpStream,
            std::net::SocketAddr,
            std::net::TcpStream,
        ) -> io::Result<LoopbackWakePair>,
    ) -> io::Result<LoopbackWakePair> {
        let address = self.0.local_addr()?;
        let writer = std::net::TcpStream::connect_timeout(&address, LOOPBACK_WAKE_CONNECT_TIMEOUT)?;
        let accepted = self.0.accept();
        // Close immediately after the one accept, including failed accept or
        // subsequent identity rejection. Never read data from a foreign peer.
        drop(self);
        let (reader, peer) = accepted?;
        let pair = validate(reader, peer, writer)?;
        pair.writer.set_nodelay(true)?;
        Ok(pair)
    }
}

/// Its validating constructor keeps both connected sockets alive while the
/// OS-owned peer address is compared. Only this paired reader may be returned
/// for registration; no token, client data read, retry or listener escapes.
#[cfg(any(not(unix), test))]
struct LoopbackWakePair {
    reader: std::net::TcpStream,
    writer: std::net::TcpStream,
}

#[cfg(any(not(unix), test))]
impl LoopbackWakePair {
    fn validate(
        reader: std::net::TcpStream,
        peer: std::net::SocketAddr,
        writer: std::net::TcpStream,
    ) -> io::Result<Self> {
        if peer != writer.local_addr()? {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "loopback wake accepted an unrelated connection",
            ));
        }
        Ok(Self { reader, writer })
    }

    fn into_streams(self) -> (std::net::TcpStream, std::net::TcpStream) {
        (self.reader, self.writer)
    }
}

#[cfg(any(not(unix), test))]
fn loopback_wake_stream_pair() -> io::Result<(std::net::TcpStream, std::net::TcpStream)> {
    LoopbackWakeListener::bind()?
        .into_pair()
        .map(LoopbackWakePair::into_streams)
}

impl NativeEndpoint {
    fn new(poller: Arc<polling::Poller>) -> io::Result<Self> {
        let event_key = ControlEventKey::reserved();
        let (reader, writer) = wake_stream_pair()?;
        reader.set_nonblocking(true)?;
        writer.set_nonblocking(true)?;
        // SAFETY: reader remains owned by this endpoint and is deregistered in
        // Drop before it is closed; the poller is retained by the same owner.
        unsafe { poller.add(&reader, polling::Event::readable(event_key.0))? };
        Ok(Self {
            poller,
            reader,
            writer: Mutex::new(Some(writer)),
            event_key,
            health: NativeHealthCell::new(),
            #[cfg(test)]
            fail_next_notification: AtomicBool::new(false),
            #[cfg(test)]
            suppress_close: AtomicBool::new(false),
        })
    }

    fn notify(&self) -> io::Result<NativeNotification> {
        use std::io::Write;
        #[cfg(test)]
        if self.fail_next_notification.swap(false, Ordering::AcqRel) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "injected native source failure",
            ));
        }
        let writer = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        let mut socket = writer.as_ref().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "control wake source closed")
        })?;
        match socket.write(&[1]) {
            Ok(1) => Ok(NativeNotification::Written),
            Ok(_) => Err(io::Error::new(io::ErrorKind::WriteZero, "control wake")),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                Ok(NativeNotification::AlreadyPending)
            }
            Err(error) => Err(error),
        }
    }

    fn close_writer(&self) {
        #[cfg(test)]
        if self.suppress_close.load(Ordering::Acquire) {
            return;
        }
        self.writer.lock().unwrap_or_else(|e| e.into_inner()).take();
    }

    fn rearm(&self) -> io::Result<()> {
        self.poller
            .modify(&self.reader, polling::Event::readable(self.event_key.0))
    }

    fn drain(&self) -> io::Result<()> {
        use std::io::Read;
        let mut reader = &self.reader;
        let mut bytes = [0; 256];
        loop {
            match reader.read(&mut bytes) {
                Ok(0) => return Ok(()),
                Ok(_) => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for NativeEndpoint {
    fn drop(&mut self) {
        // No lease can exist: it would retain the containing ControlWake Arc.
        let _ = self.poller.delete(&self.reader);
    }
}

#[cfg(test)]
#[path = "wake/tests/mod.rs"]
mod tests;

/// Existing Baton-serial select outcome, located beside the lower wait vocabulary.
/// No evaluator or native registration is carried across this boundary.
pub(crate) enum SelectTurn {
    Switched,
    Block(std::time::Duration),
}
