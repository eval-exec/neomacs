//! Who holds the Baton next (ported from F4's `Control`, research branch
//! `facade_threads.rs`).
//!
//! Each Lisp thread has a key and a channel; a parked thread blocks in its
//! channel's receive until the Baton arrives. The holder hands the Baton on
//! only at a switch point: it records what it waits for, picks the first
//! ready thread (FIFO, as GNU's runnable queue) and sends it the Baton.
//! A thread waiting for a mutex, a condition, another thread's end or a
//! deadline is not ready until another thread makes it so.

use super::super::world::Baton;
use super::host_service::{HostParkRegistration, HostRequestKey, RealWaitPermit, SettledHostCall};
use crate::emacs_core::process::GreenSelectHandle;
use std::collections::{BTreeMap, VecDeque};
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Instant;

/// A Lisp thread's identity in the scheduler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ThreadKey {
    /// The thread the World started on (GNU's `main-thread`).
    Main,
    Worker(NonZeroU64),
}

impl ThreadKey {
    /// The thread's id in the World's `ThreadManager` (0 for main).
    pub(crate) fn id(self) -> u64 {
        match self {
            Self::Main => 0,
            Self::Worker(id) => id.get(),
        }
    }

    /// The key of the thread with ID in the World's `ThreadManager`.
    pub(crate) fn from_id(id: u64) -> Self {
        NonZeroU64::new(id).map_or(Self::Main, Self::Worker)
    }
}

/// What a thread that gave the Baton up waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wait {
    /// Nothing: it runs again in its turn.
    Runnable,
    /// Another thread's end (`thread-join`).
    Join(ThreadKey),
    /// A mutex another thread holds (`mutex-lock`).
    Mutex(u64),
    /// A condition variable's notification (`condition-wait`).
    Condition(u64),
    /// A `thread_select` wait, ready on input, a scheduler event, or its
    /// absolute deadline. The scheduler watches I/O while no owner runs.
    Select { wait_end: Instant },
    /// A rooted host request; only the service lifecycle can complete it.
    HostRequest(HostRequestKey),
    /// It ended.
    Exited,
}

/// What the other threads are doing, seen by a thread about to wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Peers {
    /// One can run now.
    Ready,
    /// None can run before this deadline, the earliest one sleeps to.
    Sleeping(Instant),
    /// None can run until a running thread wakes it.
    Blocked,
}

/// Whether the runtime still runs threads, or is shutting down because its
/// main thread ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lifecycle {
    Running,
    Closing,
}

/// Why a hand-off did not happen.
#[derive(Debug)]
pub(crate) enum Handoff {
    /// No other thread can run and the caller waits for something only a
    /// running thread could give it: GNU would hang (neomacs signals).
    NoRunnable(Baton),
}

/// A native service turn does not resume main's suspended Lisp continuation.
#[derive(Debug)]
#[must_use]
pub(super) enum Wake {
    Resume(Baton),
    ServiceTurn { key: HostRequestKey, baton: Baton },
}

impl Wake {
    fn baton(&self) -> &Baton {
        match self {
            Self::Resume(baton) | Self::ServiceTurn { baton, .. } => baton,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OwnerLocation {
    Running,
    Receiving,
    NativeReleased,
    Serving(HostRequestKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServicePhase {
    Reserved,
    Serving,
    RealWait,
    Reacquiring,
    ServingAfterWait,
}

#[derive(Clone, Copy, Debug)]
struct ServiceMetadata {
    requester: ThreadKey,
    phase: ServicePhase,
}

/// Linear reservation held beside the World-owned host root record.
#[derive(Debug)]
#[must_use]
pub(super) struct ReservedServiceSlice {
    key: HostRequestKey,
    requester: ThreadKey,
    _confined: PhantomData<Rc<()>>,
}

impl ReservedServiceSlice {
    pub(super) fn key(&self) -> HostRequestKey {
        self.key
    }
}

static_assertions::assert_not_impl_any!(ReservedServiceSlice: Send, Sync, Clone, Copy);

/// Prepared under the registry borrow, consumed only after that borrow ends.
#[derive(Debug)]
#[must_use]
pub(super) struct ServiceTransferPlan {
    key: HostRequestKey,
    requester: ThreadKey,
    _confined: PhantomData<Rc<()>>,
}
static_assertions::assert_not_impl_any!(ServiceTransferPlan: Send, Sync, Clone, Copy);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ServiceRefusal {
    Closing,
    MainUnavailable,
    MainIsRequester,
    RequesterUnavailable,
    DuplicateRequest,
    WrongServiceState,
    WrongMainThread,
}

#[derive(Debug)]
#[must_use]
pub(super) enum ServiceTransfer {
    Dispatched,
    /// Main is inside a native release. The requester keeps the Baton in a
    /// metadata-only queued wait until main's native acquire is announced.
    Queued(Baton),
}

#[derive(Debug)]
#[must_use]
pub(super) struct ServiceTransferRefusal {
    pub(super) reason: ServiceRefusal,
    pub(super) baton: Baton,
}

#[derive(Debug)]
#[must_use]
#[allow(
    dead_code,
    reason = "rejection retains linear obligations for recovery or fail-stop"
)]
pub(super) struct ServiceReturnRefusal {
    pub(super) reason: ServiceRefusal,
    pub(super) slice: ReservedServiceSlice,
    pub(super) baton: Baton,
}

#[derive(Debug)]
#[must_use]
#[allow(
    dead_code,
    reason = "rejection retains linear obligations for recovery or fail-stop"
)]
pub(super) struct ServiceCancelRefusal {
    pub(super) reason: ServiceRefusal,
    pub(super) slice: ReservedServiceSlice,
    pub(super) settled: SettledHostCall,
}

/// The actual native wait holds either the idle Baton, inaccessible to Lisp,
/// or an obligation to reacquire it after another eligible owner ran.
#[derive(Debug)]
#[must_use]
pub(super) enum NativeRelease {
    Transferred,
    Idle(Baton),
}

#[derive(Debug)]
#[must_use]
#[allow(
    dead_code,
    reason = "rejection retains linear obligations for recovery or fail-stop"
)]
pub(super) struct ServiceAcquireRefusal {
    pub(super) reason: ServiceRefusal,
    pub(super) release: NativeRelease,
}

#[derive(Debug)]
#[must_use]
pub(super) enum ServiceReturn {
    Sent,
    ResumeMain(Baton),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NextOwner {
    Resume(ThreadKey),
    Service(HostRequestKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "ordinary completion is reserved for later host cancellation delivery"
)]
enum CancelPlacement {
    CurrentBatonOwner,
    OrdinaryCompletion,
}

#[derive(Default)]
struct ControlState {
    // There is only one Baton in the World, so a sender holding it proves
    // every recipient queue empty. A single reusable slot also allows a
    // newborn or parked recipient to receive after the sender has finished.
    senders: BTreeMap<ThreadKey, mpsc::SyncSender<Wake>>,
    waits: BTreeMap<ThreadKey, Wait>,
    ready: VecDeque<ThreadKey>,
    closing: bool,
    physical: BTreeMap<ThreadKey, OwnerLocation>,
    services: BTreeMap<HostRequestKey, ServiceMetadata>,
    pending_services: VecDeque<HostRequestKey>,
    pending_acquires: VecDeque<HostRequestKey>,
    main_os_thread: Option<std::thread::ThreadId>,
}

#[cfg(test)]
#[derive(Debug, thiserror::Error)]
pub(crate) enum SelectParkObservationError {
    #[error("select park gate lock poisoned")]
    GatePoisoned,
    #[error("MainConnectPending requires the Main owner")]
    NotMain,
    #[error("select park gate prepared twice")]
    AlreadyPrepared,
    #[error(transparent)]
    PendingConnect(#[from] crate::emacs_core::process::PendingConnectObservationError),
}
#[cfg(test)]
static_assertions::assert_impl_all!(SelectParkObservationError: std::error::Error, Send, Sync);

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectParkExpectation {
    Ordinary,
    MainConnectPending(crate::emacs_core::process::ProcessId),
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) enum SelectParkFacts {
    Ordinary,
    MainConnectPending(crate::emacs_core::process::MainConnectPending),
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct SelectParked {
    owner: ThreadKey,
    wait_end: Instant,
    facts: SelectParkFacts,
}

#[cfg(test)]
impl SelectParked {
    pub(crate) fn owner(&self) -> ThreadKey {
        self.owner
    }
    pub(crate) fn wait_end(&self) -> Instant {
        self.wait_end
    }
    pub(crate) fn into_facts(self) -> SelectParkFacts {
        self.facts
    }
}

#[cfg(test)]
static_assertions::assert_impl_all!(SelectParked: Send, std::fmt::Debug);
#[cfg(test)]
static_assertions::assert_not_impl_any!(SelectParked: Clone, Copy);

#[cfg(test)]
type SelectParkCallback = Box<dyn FnOnce(SelectParked) + Send>;

#[cfg(test)]
enum SelectParkGate {
    Armed {
        owner: ThreadKey,
        expectation: SelectParkExpectation,
        callback: SelectParkCallback,
    },
    Prepared {
        owner: ThreadKey,
        facts: SelectParkFacts,
        callback: SelectParkCallback,
    },
}

#[cfg(test)]
impl std::fmt::Debug for SelectParkGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Armed {
                owner,
                expectation,
                callback: _,
            } => f
                .debug_struct("ArmedSelectParkGate")
                .field("owner", owner)
                .field("expectation", expectation)
                .finish_non_exhaustive(),
            Self::Prepared {
                owner,
                facts,
                callback: _,
            } => f
                .debug_struct("PreparedSelectParkGate")
                .field("owner", owner)
                .field("facts", facts)
                .finish_non_exhaustive(),
        }
    }
}

#[cfg(test)]
static_assertions::assert_impl_all!(SelectParkGate: Send, std::fmt::Debug);

/// The scheduler, shared by every thread's seat.
#[derive(Default)]
pub(crate) struct Control {
    state: Mutex<ControlState>,
    metadata_changed: Condvar,
    #[cfg(test)]
    after_next_select_park: Mutex<Option<SelectParkGate>>,
    /// An owned I/O handle: the idle scheduler never reads World storage.
    select: OnceLock<Option<GreenSelectHandle>>,
}

impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Control").finish_non_exhaustive()
    }
}

impl Control {
    pub(crate) fn install_select(&self, select: Option<GreenSelectHandle>) {
        assert!(
            self.select.set(select).is_ok(),
            "select handle installed twice"
        );
    }

    fn notify_select(&self) {
        self.metadata_changed.notify_all();
        if let Some(Some(select)) = self.select.get() {
            select.notify();
        }
    }

    pub(super) fn nudge_main_mailbox(&self) {
        self.nudge_main(crate::host_effect::wake::ControlNudge::MainMailboxReady);
    }

    fn nudge_main(&self, reason: crate::host_effect::wake::ControlNudge) {
        self.metadata_changed.notify_all();
        if let Some(Some(select)) = self.select.get() {
            select.nudge(
                crate::host_effect::wake::ControlDestination::NativeMain,
                reason,
            );
        }
    }

    fn state(&self) -> MutexGuard<'_, ControlState> {
        // Every update is a few map/queue operations that leave the state
        // consistent; a panic while holding the lock cannot tear it.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn reserve_service(
        &self,
        registration: &HostParkRegistration,
    ) -> Result<ReservedServiceSlice, ServiceRefusal> {
        let mut state = self.state();
        if state.closing {
            return Err(ServiceRefusal::Closing);
        }
        if registration.owner() == ThreadKey::Main {
            return Err(ServiceRefusal::MainIsRequester);
        }
        if !state.senders.contains_key(&ThreadKey::Main)
            || matches!(state.waits.get(&ThreadKey::Main), Some(Wait::Exited) | None)
        {
            return Err(ServiceRefusal::MainUnavailable);
        }
        if matches!(
            state.waits.get(&registration.owner()),
            Some(Wait::Exited) | None
        ) {
            return Err(ServiceRefusal::RequesterUnavailable);
        }
        if state.services.contains_key(&registration.key()) {
            return Err(ServiceRefusal::DuplicateRequest);
        }
        state.services.insert(
            registration.key(),
            ServiceMetadata {
                requester: registration.owner(),
                phase: ServicePhase::Reserved,
            },
        );
        Ok(ReservedServiceSlice {
            key: registration.key(),
            requester: registration.owner(),
            _confined: PhantomData,
        })
    }

    /// Only the rooted registration, never a generic wake, starts a service.
    /// Main's original wait and its ordinary FIFO position remain untouched.
    #[allow(
        dead_code,
        reason = "dormant service API; hosted activation remains refused"
    )]
    pub(super) fn transfer_to_service(
        &self,
        slice: &ReservedServiceSlice,
        baton: Baton,
    ) -> Result<ServiceTransfer, ServiceTransferRefusal> {
        let plan = match self.prepare_service_transfer(slice) {
            Ok(plan) => plan,
            Err(reason) => return Err(ServiceTransferRefusal { reason, baton }),
        };
        self.transfer_prepared_service(plan, baton)
    }

    pub(super) fn prepare_service_transfer(
        &self,
        slice: &ReservedServiceSlice,
    ) -> Result<ServiceTransferPlan, ServiceRefusal> {
        let state = self.state();
        if state.closing {
            return Err(ServiceRefusal::Closing);
        }
        if !state.services.get(&slice.key).is_some_and(|metadata| {
            metadata.requester == slice.requester && metadata.phase == ServicePhase::Reserved
        }) {
            return Err(ServiceRefusal::WrongServiceState);
        }
        Ok(ServiceTransferPlan {
            key: slice.key,
            requester: slice.requester,
            _confined: PhantomData,
        })
    }

    pub(super) fn transfer_prepared_service(
        &self,
        slice: ServiceTransferPlan,
        baton: Baton,
    ) -> Result<ServiceTransfer, ServiceTransferRefusal> {
        let mut state = self.state();
        let valid = state.services.get(&slice.key).is_some_and(|metadata| {
            metadata.requester == slice.requester && metadata.phase == ServicePhase::Reserved
        });
        if !valid || state.closing {
            return Err(ServiceTransferRefusal {
                reason: if state.closing {
                    ServiceRefusal::Closing
                } else {
                    ServiceRefusal::WrongServiceState
                },
                baton,
            });
        }
        let main_location = state.physical.get(&ThreadKey::Main).copied();
        if !matches!(
            main_location,
            Some(OwnerLocation::Receiving | OwnerLocation::NativeReleased)
        ) {
            return Err(ServiceTransferRefusal {
                reason: ServiceRefusal::MainUnavailable,
                baton,
            });
        }
        state
            .waits
            .insert(slice.requester, Wait::HostRequest(slice.key));
        state.ready.retain(|key| *key != slice.requester);
        state
            .physical
            .insert(slice.requester, OwnerLocation::Receiving);
        match main_location {
            Some(OwnerLocation::Receiving) => {
                if !state.pending_services.contains(&slice.key) {
                    state.pending_services.push_back(slice.key);
                }
                let Some(key) = Self::pick_service(&mut state) else {
                    return Err(ServiceTransferRefusal {
                        reason: ServiceRefusal::WrongServiceState,
                        baton,
                    });
                };
                drop(state);
                self.send_wake(ThreadKey::Main, Wake::ServiceTurn { key, baton });
                Ok(ServiceTransfer::Dispatched)
            }
            Some(OwnerLocation::NativeReleased) => {
                state.pending_services.push_back(slice.key);
                drop(state);
                self.nudge_main(crate::host_effect::wake::ControlNudge::HostServiceReady);
                Ok(ServiceTransfer::Queued(baton))
            }
            Some(OwnerLocation::Running | OwnerLocation::Serving(_)) | None => {
                // The sole-Baton protocol makes a simultaneously running main
                // impossible. Preserve the Baton and reservation for diagnosis.
                Err(ServiceTransferRefusal {
                    reason: ServiceRefusal::MainUnavailable,
                    baton,
                })
            }
        }
    }

    /// Called on main's acquire path before its Lisp continuation is restored.
    #[allow(
        dead_code,
        reason = "dormant service API; hosted activation remains refused"
    )]
    pub(super) fn take_queued_service(&self, baton: Baton) -> Result<Wake, ServiceTransferRefusal> {
        let mut state = self.state();
        if state.main_os_thread != Some(std::thread::current().id()) {
            return Err(ServiceTransferRefusal {
                reason: ServiceRefusal::WrongMainThread,
                baton,
            });
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        if let Some(key) = Self::pick_service(&mut state) {
            return Ok(Wake::ServiceTurn { key, baton });
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Running);
        Ok(Wake::Resume(baton))
    }

    /// The service result was committed to the registry first. An immediate
    /// service returns its reserved slice; a real release completes through
    /// ordinary FIFO eligibility. Main's original Wait stays intact.
    pub(super) fn return_from_service(
        &self,
        slice: ReservedServiceSlice,
        baton: Baton,
    ) -> Result<ServiceReturn, ServiceReturnRefusal> {
        let mut state = self.state();
        let reason = if state.main_os_thread != Some(std::thread::current().id()) {
            Some(ServiceRefusal::WrongMainThread)
        } else if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Serving(slice.key))
            || state.waits.get(&slice.requester) != Some(&Wait::HostRequest(slice.key))
            || !state.services.get(&slice.key).is_some_and(|metadata| {
                metadata.requester == slice.requester
                    && matches!(
                        metadata.phase,
                        ServicePhase::Serving | ServicePhase::ServingAfterWait
                    )
            })
        {
            Some(ServiceRefusal::WrongServiceState)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(ServiceReturnRefusal {
                reason,
                slice,
                baton,
            });
        }
        let completion_phase = state
            .services
            .remove(&slice.key)
            .map(|metadata| metadata.phase);
        state.waits.insert(slice.requester, Wait::Runnable);
        // An acquire may have selected main's ordinary turn before noticing
        // the queued service. Preserve that eligibility, but the reserved
        // requester still receives the Baton first.
        if state.waits.get(&ThreadKey::Main) == Some(&Wait::Runnable)
            && !state.ready.contains(&ThreadKey::Main)
        {
            state.ready.push_front(ThreadKey::Main);
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        match completion_phase {
            Some(ServicePhase::Serving) => {
                state
                    .physical
                    .insert(slice.requester, OwnerLocation::Running);
                drop(state);
                self.send_wake(slice.requester, Wake::Resume(baton));
                Ok(ServiceReturn::Sent)
            }
            Some(ServicePhase::ServingAfterWait) => {
                state.ready.push_back(slice.requester);
                drop(state);
                let Some(next) = self.pick_next() else {
                    return Err(ServiceReturnRefusal {
                        reason: ServiceRefusal::WrongServiceState,
                        slice,
                        baton,
                    });
                };
                let (target, wake) = Self::wake_for(next, baton);
                match wake {
                    Wake::Resume(baton) if target == ThreadKey::Main => {
                        self.state()
                            .physical
                            .insert(ThreadKey::Main, OwnerLocation::Running);
                        Ok(ServiceReturn::ResumeMain(baton))
                    }
                    Wake::Resume(baton) => {
                        self.send_wake(target, Wake::Resume(baton));
                        Ok(ServiceReturn::Sent)
                    }
                    Wake::ServiceTurn { key, baton } => {
                        self.send_wake(target, Wake::ServiceTurn { key, baton });
                        Ok(ServiceReturn::Sent)
                    }
                }
            }
            Some(ServicePhase::Reserved | ServicePhase::RealWait | ServicePhase::Reacquiring)
            | None => std::process::abort(),
        }
    }

    /// The owned native wrapper has reached a genuine GNU release point.
    /// The requester remains parked; only this permit admits ordinary peers.
    pub(super) fn release_service(
        &self,
        slice: &ReservedServiceSlice,
        _permit: RealWaitPermit,
        baton: Baton,
    ) -> Result<NativeRelease, ServiceTransferRefusal> {
        let mut state = self.state();
        let valid = state.main_os_thread == Some(std::thread::current().id())
            && state.physical.get(&ThreadKey::Main) == Some(&OwnerLocation::Serving(slice.key))
            && state.services.get(&slice.key).is_some_and(|metadata| {
                metadata.requester == slice.requester
                    && matches!(
                        metadata.phase,
                        ServicePhase::Serving | ServicePhase::ServingAfterWait
                    )
            });
        if !valid {
            return Err(ServiceTransferRefusal {
                reason: ServiceRefusal::WrongServiceState,
                baton,
            });
        }
        if let Some(metadata) = state.services.get_mut(&slice.key) {
            metadata.phase = ServicePhase::RealWait;
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        let next = match Self::pick(&mut state) {
            Some((key, None)) => Some(key),
            Some((_, Some(_))) | None => None,
        };
        drop(state);
        match next {
            Some(key) => {
                self.send_wake(key, Wake::Resume(baton));
                Ok(NativeRelease::Transferred)
            }
            None => Ok(NativeRelease::Idle(baton)),
        }
    }

    /// Called after the native wrapper returns. Main reacquires for native
    /// completion only; its suspended Lisp Wait is neither consumed nor read.
    pub(super) fn reacquire_service(
        &self,
        slice: &ReservedServiceSlice,
        release: NativeRelease,
        receiver: &mpsc::Receiver<Wake>,
    ) -> Result<Baton, ServiceAcquireRefusal> {
        let mut state = self.state();
        if state.main_os_thread != Some(std::thread::current().id())
            || !state.services.get(&slice.key).is_some_and(|metadata| {
                metadata.requester == slice.requester && metadata.phase == ServicePhase::RealWait
            })
        {
            return Err(ServiceAcquireRefusal {
                reason: ServiceRefusal::WrongServiceState,
                release,
            });
        }
        match release {
            NativeRelease::Idle(baton) => {
                if let Some(metadata) = state.services.get_mut(&slice.key) {
                    metadata.phase = ServicePhase::ServingAfterWait;
                }
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Serving(slice.key));
                drop(state);
                baton.enter();
                Ok(baton)
            }
            NativeRelease::Transferred => {
                if let Some(metadata) = state.services.get_mut(&slice.key) {
                    metadata.phase = ServicePhase::Reacquiring;
                }
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Receiving);
                state.pending_acquires.push_back(slice.key);
                drop(state);
                self.metadata_changed.notify_all();
                self.notify_select();
                match receiver.recv() {
                    Ok(Wake::ServiceTurn { key, baton }) if key == slice.key => {
                        baton.enter();
                        Ok(baton)
                    }
                    Ok(Wake::Resume(baton) | Wake::ServiceTurn { baton, .. }) => {
                        Err(ServiceAcquireRefusal {
                            reason: ServiceRefusal::WrongServiceState,
                            release: NativeRelease::Idle(baton),
                        })
                    }
                    // Losing the sole Baton is an ownership failure. Native
                    // request cancellation itself never closes this channel.
                    Err(_) => std::process::abort(),
                }
            }
        }
    }

    /// The requester keeps the Baton while main finishes an earlier native
    /// release. This metadata-only wait cannot run unrelated Lisp.
    pub(super) fn wait_queued_prepared(
        &self,
        slice: ServiceTransferPlan,
        baton: Baton,
    ) -> Result<ServiceTransfer, ServiceTransferRefusal> {
        let mut state = self.state();
        loop {
            if state.closing {
                return Err(ServiceTransferRefusal {
                    reason: ServiceRefusal::Closing,
                    baton,
                });
            }
            if !state.services.get(&slice.key).is_some_and(|metadata| {
                metadata.requester == slice.requester && metadata.phase == ServicePhase::Reserved
            }) {
                return Err(ServiceTransferRefusal {
                    reason: ServiceRefusal::WrongServiceState,
                    baton,
                });
            }
            if let Some(key) = Self::pick_service(&mut state) {
                drop(state);
                self.send_wake(ThreadKey::Main, Wake::ServiceTurn { key, baton });
                return Ok(ServiceTransfer::Dispatched);
            }
            state = self
                .metadata_changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    #[allow(
        dead_code,
        reason = "dormant service API; hosted activation remains refused"
    )]
    pub(super) fn wait_queued_service(
        &self,
        slice: &ReservedServiceSlice,
        baton: Baton,
    ) -> Result<ServiceTransfer, ServiceTransferRefusal> {
        let plan = match self.prepare_service_transfer(slice) {
            Ok(plan) => plan,
            Err(reason) => return Err(ServiceTransferRefusal { reason, baton }),
        };
        self.wait_queued_prepared(plan, baton)
    }

    /// A main native wait returning without the Baton announces that its
    /// receive loop can now accept service turns, preserving the original Wait.
    #[allow(
        dead_code,
        reason = "dormant service API; hosted activation remains refused"
    )]
    pub(super) fn main_native_acquire(&self, receiver: &mpsc::Receiver<Wake>) -> Wake {
        {
            let mut state = self.state();
            if state.main_os_thread != Some(std::thread::current().id())
                || state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::NativeReleased)
            {
                std::process::abort()
            }
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::Receiving);
        }
        self.metadata_changed.notify_all();
        self.notify_select();
        match receiver.recv() {
            Ok(wake) => {
                wake.baton().enter();
                wake
            }
            Err(_) => std::process::abort(),
        }
    }

    /// The existing native blocking wrapper announces its physical release;
    /// this does not alter the owner's semantic Wait or make a peer runnable.
    #[allow(
        dead_code,
        reason = "dormant service API; hosted activation remains refused"
    )]
    pub(super) fn main_native_released(&self) -> Result<(), ServiceRefusal> {
        let mut state = self.state();
        if state.main_os_thread != Some(std::thread::current().id()) {
            return Err(ServiceRefusal::WrongMainThread);
        }
        if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Running) {
            return Err(ServiceRefusal::WrongServiceState);
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        Ok(())
    }

    /// Cancellation before dispatch must first commit a durable terminal
    /// result in the World registry. In-flight native service keeps its
    /// reserved return path until main has finished with the owned request.
    #[allow(
        dead_code,
        reason = "dormant service API; hosted activation remains refused"
    )]
    pub(super) fn cancel_before_service(
        &self,
        slice: ReservedServiceSlice,
        settled: SettledHostCall,
    ) -> Result<(), ServiceCancelRefusal> {
        self.cancel_service(slice, settled, CancelPlacement::OrdinaryCompletion)
    }

    /// A refused/queued transfer left the requester holding the Baton. Its
    /// existing restore path resumes directly, without another FIFO turn.
    pub(super) fn cancel_current_service(
        &self,
        slice: ReservedServiceSlice,
        settled: SettledHostCall,
        _baton: &Baton,
    ) -> Result<(), ServiceCancelRefusal> {
        self.cancel_service(slice, settled, CancelPlacement::CurrentBatonOwner)
    }

    fn cancel_service(
        &self,
        slice: ReservedServiceSlice,
        settled: SettledHostCall,
        placement: CancelPlacement,
    ) -> Result<(), ServiceCancelRefusal> {
        let mut state = self.state();
        let matches = settled.key() == slice.key
            && settled.owner() == slice.requester
            && state.services.get(&slice.key).is_some_and(|metadata| {
                metadata.requester == slice.requester && metadata.phase == ServicePhase::Reserved
            });
        if !matches {
            return Err(ServiceCancelRefusal {
                reason: ServiceRefusal::WrongServiceState,
                slice,
                settled,
            });
        }
        let was_parked = match state.waits.get(&slice.requester).copied() {
            Some(Wait::HostRequest(key)) if key == slice.key => true,
            Some(Wait::Runnable) => false,
            Some(
                Wait::HostRequest(_)
                | Wait::Join(_)
                | Wait::Mutex(_)
                | Wait::Condition(_)
                | Wait::Select { .. }
                | Wait::Exited,
            )
            | None => {
                return Err(ServiceCancelRefusal {
                    reason: ServiceRefusal::WrongServiceState,
                    slice,
                    settled,
                });
            }
        };
        state.waits.insert(slice.requester, Wait::Runnable);
        match placement {
            CancelPlacement::CurrentBatonOwner => {
                state.ready.retain(|key| *key != slice.requester);
                state
                    .physical
                    .insert(slice.requester, OwnerLocation::Running);
            }
            CancelPlacement::OrdinaryCompletion if was_parked => {
                state.ready.push_back(slice.requester);
            }
            CancelPlacement::OrdinaryCompletion => {}
        }
        state.services.remove(&slice.key);
        state.pending_services.retain(|key| *key != slice.key);
        drop(state);
        self.notify_select();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn after_next_select_park_for_test(
        &self,
        owner: ThreadKey,
        expectation: SelectParkExpectation,
        callback: impl FnOnce(SelectParked) + Send + 'static,
    ) {
        let mut slot = self
            .after_next_select_park
            .lock()
            .expect("select park gate lock");
        assert!(slot.is_none(), "select park gate already installed");
        *slot = Some(SelectParkGate::Armed {
            owner,
            expectation,
            callback: Box::new(callback),
        });
    }

    /// The live owner prepares only native facts immediately before its
    /// parked-root/Baton transfer. No process operation occurs during that
    /// transfer; the token itself is minted only after Select is committed.
    #[cfg(test)]
    pub(crate) fn prepare_select_park_for_test(
        &self,
        owner: ThreadKey,
        processes: &crate::emacs_core::process::ProcessManager,
    ) -> Result<(), SelectParkObservationError> {
        let mut slot = self
            .after_next_select_park
            .lock()
            .map_err(|_| SelectParkObservationError::GatePoisoned)?;
        let Some(gate) = slot.take() else {
            return Ok(());
        };
        match gate {
            SelectParkGate::Armed {
                owner: expected,
                expectation,
                callback,
            } => {
                if owner != expected {
                    *slot = Some(SelectParkGate::Armed {
                        owner: expected,
                        expectation,
                        callback,
                    });
                    return Ok(());
                }
                let facts = match expectation {
                    SelectParkExpectation::Ordinary => Ok(SelectParkFacts::Ordinary),
                    SelectParkExpectation::MainConnectPending(id) => match owner {
                        ThreadKey::Main => processes
                            .main_connect_pending_for_test(id)
                            .map(SelectParkFacts::MainConnectPending)
                            .map_err(SelectParkObservationError::PendingConnect),
                        ThreadKey::Worker(_) => Err(SelectParkObservationError::NotMain),
                    },
                };
                match facts {
                    Ok(facts) => {
                        *slot = Some(SelectParkGate::Prepared {
                            owner,
                            facts,
                            callback,
                        })
                    }
                    Err(error) => {
                        *slot = Some(SelectParkGate::Armed {
                            owner: expected,
                            expectation,
                            callback,
                        });
                        return Err(error);
                    }
                }
            }
            SelectParkGate::Prepared {
                owner: expected,
                facts,
                callback,
            } => {
                *slot = Some(SelectParkGate::Prepared {
                    owner: expected,
                    facts,
                    callback,
                });
                if owner == expected {
                    return Err(SelectParkObservationError::AlreadyPrepared);
                }
            }
        }
        Ok(())
    }

    #[cfg(test)]
    fn observe_select_park_for_test(&self, owner: ThreadKey, wait_end: Instant) {
        let callback = {
            let mut slot = self
                .after_next_select_park
                .lock()
                .expect("select park gate lock");
            match slot.take() {
                Some(SelectParkGate::Prepared {
                    owner: expected,
                    facts,
                    callback,
                }) => {
                    if owner == expected {
                        Some((
                            callback,
                            SelectParked {
                                owner,
                                wait_end,
                                facts,
                            },
                        ))
                    } else {
                        *slot = Some(SelectParkGate::Prepared {
                            owner: expected,
                            facts,
                            callback,
                        });
                        None
                    }
                }
                Some(SelectParkGate::Armed {
                    owner,
                    expectation,
                    callback,
                }) => {
                    *slot = Some(SelectParkGate::Armed {
                        owner,
                        expectation,
                        callback,
                    });
                    None
                }
                None => None,
            }
        };
        // Never hold Control.state or the gate mutex while waiting for the
        // native writer's consumed acknowledgement. The Baton stays local.
        if let Some((callback, witness)) = callback {
            callback(witness);
        }
    }

    /// Register the main thread, which holds the Baton: its channel.
    pub(super) fn register_main(&self) -> mpsc::Receiver<Wake> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut state = self.state();
        state.senders.insert(ThreadKey::Main, sender);
        state.waits.insert(ThreadKey::Main, Wait::Runnable);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Running);
        state.main_os_thread = Some(std::thread::current().id());
        receiver
    }

    /// Register the new thread KEY, ready to run in its turn: its channel.
    pub(super) fn register_worker(&self, key: ThreadKey) -> mpsc::Receiver<Wake> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut state = self.state();
        state.senders.insert(key, sender);
        state.waits.insert(key, Wait::Runnable);
        state.physical.insert(key, OwnerLocation::Receiving);
        state.ready.push_back(key);
        receiver
    }

    /// Whether KEY has ended.
    pub(crate) fn exited(&self, key: ThreadKey) -> bool {
        matches!(self.state().waits.get(&key), Some(Wait::Exited) | None)
    }

    /// Whether a thread other than ME is ready to run, or will be at a
    /// deadline.
    pub(crate) fn others_could_run(&self, me: ThreadKey) -> bool {
        let state = self.state();
        Self::service_ready(&state)
            || state.waits.iter().any(|(key, wait)| {
                *key != me && matches!(wait, Wait::Runnable | Wait::Select { .. })
            })
    }

    /// What the threads other than ME are doing at NOW.
    pub(crate) fn peers(&self, me: ThreadKey, now: Instant) -> Peers {
        let state = self.state();
        if Self::service_ready(&state) {
            return Peers::Ready;
        }
        let mut earliest: Option<Instant> = None;
        for (key, wait) in &state.waits {
            if *key == me {
                continue;
            }
            match *wait {
                Wait::Runnable => return Peers::Ready,
                Wait::Select { wait_end, .. } if wait_end <= now => return Peers::Ready,
                Wait::Select {
                    wait_end: deadline, ..
                } => {
                    earliest = Some(earliest.map_or(deadline, |e| e.min(deadline)));
                }
                Wait::Join(_)
                | Wait::Mutex(_)
                | Wait::Condition(_)
                | Wait::HostRequest(_)
                | Wait::Exited => {}
            }
        }
        earliest.map_or(Peers::Blocked, Peers::Sleeping)
    }

    /// Poll the selectors at a yield safepoint without taking an owner
    /// off the CPU. Preserve FIFO order: only readiness changes queue state.
    pub(crate) fn yield_peer_ready(&self, me: ThreadKey) -> bool {
        let mut state = self.state();
        if state.closing
            || Self::service_ready(&state)
            || state
                .ready
                .iter()
                .any(|key| *key != me && state.waits.get(key) == Some(&Wait::Runnable))
        {
            // The ensuing transfer handles I/O/deadline readiness. Avoid
            // scanning sleeping owners twice on an actual handoff.
            return true;
        }
        Self::expire_deadlines(&mut state, Instant::now());
        if state
            .waits
            .values()
            .any(|wait| matches!(wait, Wait::Select { .. }))
            && let Some(Some(select)) = self.select.get()
            && select.wait_until(Instant::now())
        {
            Self::wake_selects(&mut state);
        }
        state
            .ready
            .iter()
            .any(|key| *key != me && state.waits.get(key) == Some(&Wait::Runnable))
    }

    /// Make KEY the next thread to run, whatever it waits for.
    pub(crate) fn prioritize(&self, key: ThreadKey) {
        let mut state = self.state();
        if matches!(
            state.waits.get(&key),
            Some(Wait::Exited | Wait::HostRequest(_)) | None
        ) {
            return;
        }
        state.ready.retain(|ready| *ready != key);
        state.waits.insert(key, Wait::Runnable);
        state.ready.push_front(key);
    }

    /// What KEY waits for.
    pub(crate) fn wait_of(&self, key: ThreadKey) -> Option<Wait> {
        self.state().waits.get(&key).copied()
    }

    /// Whether the runtime is shutting down.
    pub(crate) fn lifecycle(&self) -> Lifecycle {
        if self.state().closing {
            Lifecycle::Closing
        } else {
            Lifecycle::Running
        }
    }

    /// Make KEY runnable again if it waits for something.
    pub(crate) fn wake(&self, key: ThreadKey) {
        let mut state = self.state();
        Self::wake_locked(&mut state, key);
        drop(state);
        self.notify_select();
    }

    /// Make every thread waiting for WAIT runnable (a mutex release, a
    /// notification to all, a thread's end).
    pub(crate) fn wake_waiting_for(&self, wait: Wait) -> usize {
        let mut state = self.state();
        let keys: Vec<ThreadKey> = state
            .waits
            .iter()
            .filter(|(_, w)| **w == wait)
            .map(|(key, _)| *key)
            .collect();
        for key in &keys {
            Self::wake_locked(&mut state, *key);
        }
        keys.len()
    }

    /// Make the first thread (in key order) waiting for WAIT runnable.
    pub(crate) fn wake_one_waiting_for(&self, wait: Wait) -> bool {
        let mut state = self.state();
        let key = state
            .waits
            .iter()
            .find(|(_, w)| **w == wait)
            .map(|(key, _)| *key);
        match key {
            Some(key) => {
                Self::wake_locked(&mut state, key);
                true
            }
            None => false,
        }
    }

    fn wake_locked(state: &mut ControlState, key: ThreadKey) {
        match state.waits.get(&key) {
            Some(Wait::Runnable | Wait::Exited | Wait::HostRequest(_)) | None => {}
            Some(Wait::Join(_) | Wait::Mutex(_) | Wait::Condition(_) | Wait::Select { .. }) => {
                state.waits.insert(key, Wait::Runnable);
                state.ready.push_back(key);
            }
        }
    }

    /// Move threads whose deadline passed into the ready queue.
    fn expire_deadlines(state: &mut ControlState, now: Instant) {
        let expired: Vec<ThreadKey> = state
            .waits
            .iter()
            .filter(|(_, wait)| matches!(wait, Wait::Select { wait_end } if *wait_end <= now))
            .map(|(key, _)| *key)
            .collect();
        for key in expired {
            Self::wake_locked(state, key);
        }
    }

    /// Readiness wakes all selecting owners. They inspect their own wait
    /// policy and poll live descriptors only after reacquiring the Baton.
    fn wake_selects(state: &mut ControlState) {
        let selecting: Vec<_> = state
            .waits
            .iter()
            .filter_map(|(key, wait)| matches!(wait, Wait::Select { .. }).then_some(*key))
            .collect();
        for key in selecting {
            Self::wake_locked(state, key);
        }
    }

    /// Native acquire/service turns never complete main's semantic Wait.
    fn pick_service(state: &mut ControlState) -> Option<HostRequestKey> {
        if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Receiving) {
            return None;
        }
        while let Some(key) = state.pending_acquires.pop_front() {
            let Some(metadata) = state.services.get_mut(&key) else {
                continue;
            };
            match metadata.phase {
                ServicePhase::Reacquiring => {
                    metadata.phase = ServicePhase::ServingAfterWait;
                    state
                        .physical
                        .insert(ThreadKey::Main, OwnerLocation::Serving(key));
                    return Some(key);
                }
                ServicePhase::Reserved
                | ServicePhase::Serving
                | ServicePhase::RealWait
                | ServicePhase::ServingAfterWait => {}
            }
        }
        while let Some(key) = state.pending_services.pop_front() {
            let Some(metadata) = state.services.get_mut(&key) else {
                continue;
            };
            match metadata.phase {
                ServicePhase::Reserved => {
                    metadata.phase = ServicePhase::Serving;
                    state
                        .physical
                        .insert(ThreadKey::Main, OwnerLocation::Serving(key));
                    return Some(key);
                }
                ServicePhase::Serving
                | ServicePhase::RealWait
                | ServicePhase::Reacquiring
                | ServicePhase::ServingAfterWait => {}
            }
        }
        None
    }

    fn service_ready(state: &ControlState) -> bool {
        state.physical.get(&ThreadKey::Main) == Some(&OwnerLocation::Receiving)
            && (state.pending_acquires.iter().any(|key| {
                state
                    .services
                    .get(key)
                    .is_some_and(|metadata| metadata.phase == ServicePhase::Reacquiring)
            }) || state.pending_services.iter().any(|key| {
                state
                    .services
                    .get(key)
                    .is_some_and(|metadata| metadata.phase == ServicePhase::Reserved)
            }))
    }

    fn wake_for(next: NextOwner, baton: Baton) -> (ThreadKey, Wake) {
        match next {
            NextOwner::Resume(key) => (key, Wake::Resume(baton)),
            NextOwner::Service(key) => (ThreadKey::Main, Wake::ServiceTurn { key, baton }),
        }
    }

    fn intercept_ready(state: &mut ControlState, me: ThreadKey) -> Option<HostRequestKey> {
        if me == ThreadKey::Main {
            state.physical.insert(me, OwnerLocation::Receiving);
        }
        if let Some(key) = Self::pick_service(state) {
            if me != ThreadKey::Main {
                state.physical.insert(me, OwnerLocation::Receiving);
                if state.waits.get(&me) == Some(&Wait::Runnable) && !state.ready.contains(&me) {
                    // Its selected turn was already removed from FIFO. A
                    // native-only interception must not forfeit that turn.
                    state.ready.push_front(me);
                }
            }
            Some(key)
        } else {
            state.physical.insert(me, OwnerLocation::Running);
            None
        }
    }

    /// A Resume may already be in flight when main announces its native
    /// acquire. Run that service first, before restoring any Context/JIT state.
    /// The loop holds scheduler metadata only while forwarding the Baton.
    pub(super) fn intercept_acquire(
        &self,
        me: ThreadKey,
        mut wake: Wake,
        receiver: &mpsc::Receiver<Wake>,
    ) -> Wake {
        loop {
            match wake {
                Wake::ServiceTurn { key, baton } => {
                    return Wake::ServiceTurn { key, baton };
                }
                Wake::Resume(baton) => {
                    let key = Self::intercept_ready(&mut self.state(), me);
                    match key {
                        None => return Wake::Resume(baton),
                        Some(key) if me == ThreadKey::Main => {
                            return Wake::ServiceTurn { key, baton };
                        }
                        Some(key) => {
                            self.send_wake(ThreadKey::Main, Wake::ServiceTurn { key, baton });
                            wake = match receiver.recv() {
                                Ok(wake) => {
                                    wake.baton().enter();
                                    wake
                                }
                                Err(_) => std::process::abort(),
                            };
                        }
                    }
                }
            }
        }
    }

    fn send_wake(&self, target: ThreadKey, wake: Wake) {
        let sender = self.state().senders.get(&target).cloned();
        let Some(sender) = sender else {
            std::process::abort()
        };
        // There is exactly one Baton. A closed receiver or occupied slot is
        // an ownership failure, never a Lisp-level request failure.
        if sender.try_send(wake).is_err() {
            std::process::abort()
        }
    }

    /// Choose an owner, blocking only while every owner is parked. The
    /// Baton stays local and no World cell is read during this OS wait.
    fn pick_next(&self) -> Option<NextOwner> {
        loop {
            // Register BEFORE checking scheduler predicates. A transition
            // between this check and the OS block advances the same snapshot;
            // arming after dropping state would discard that durable nudge.
            let registration = self
                .select
                .get()
                .and_then(Option::as_ref)
                .map(GreenSelectHandle::arm);
            let deadline = {
                let mut state = self.state();
                if state
                    .waits
                    .values()
                    .any(|wait| matches!(wait, Wait::Select { .. }))
                    && let Some(Some(select)) = self.select.get()
                    && select.wait_until(Instant::now())
                {
                    Self::wake_selects(&mut state);
                }
                if let Some(key) = Self::pick_service(&mut state) {
                    return Some(NextOwner::Service(key));
                }
                match Self::pick(&mut state) {
                    Some((key, None)) => return Some(NextOwner::Resume(key)),
                    Some((_, Some(deadline))) => deadline,
                    None => {
                        if !state.closing
                            && state.physical.get(&ThreadKey::Main)
                                == Some(&OwnerLocation::NativeReleased)
                            && (!state.pending_services.is_empty()
                                || state
                                    .services
                                    .values()
                                    .any(|metadata| metadata.phase == ServicePhase::RealWait))
                        {
                            // No ordinary owner can run, but main's actual
                            // native wait can still complete. Keep the Baton
                            // out of Lisp until its acquire is announced.
                            drop(
                                self.metadata_changed
                                    .wait(state)
                                    .unwrap_or_else(PoisonError::into_inner),
                            );
                            continue;
                        }
                        return None;
                    }
                }
            };
            let activity = if let Some(Some(select)) = self.select.get() {
                match &registration {
                    Some(Ok(registration)) => select.wait_registered(registration, deadline),
                    Some(Err(_)) => true,
                    None => false,
                }
            } else {
                let mut state = self.state();
                // Recheck under the same lock used by the notifier: an acquire
                // between the initial scan and this wait must not be lost.
                if let Some(key) = Self::pick_service(&mut state) {
                    return Some(NextOwner::Service(key));
                }
                if let Some((key, None)) = Self::pick(&mut state) {
                    return Some(NextOwner::Resume(key));
                }
                drop(
                    self.metadata_changed
                        .wait_timeout(state, deadline.saturating_duration_since(Instant::now()))
                        .unwrap_or_else(PoisonError::into_inner),
                );
                false
            };
            if activity {
                Self::wake_selects(&mut self.state());
            }
            // An eventless/spurious return never expires a deadline early.
            // pick's monotonic check is the sole clock-based wake.
        }
    }

    /// The next thread to hold the Baton, if any can run; with no ready
    /// thread but one waiting for a deadline, the earliest deadline.
    fn pick(state: &mut ControlState) -> Option<(ThreadKey, Option<Instant>)> {
        Self::expire_deadlines(state, Instant::now());
        // Keep a suspended main's exact FIFO position while selecting peers.
        // Its ordinary turn becomes eligible only after native completion.
        state
            .ready
            .retain(|key| state.waits.get(key) == Some(&Wait::Runnable));
        if let Some(index) = state.ready.iter().position(|key| {
            *key != ThreadKey::Main
                || !matches!(
                    state.physical.get(key),
                    Some(OwnerLocation::NativeReleased | OwnerLocation::Serving(_))
                )
        }) {
            if let Some(key) = state.ready.remove(index) {
                return Some((key, None));
            }
        }
        state
            .waits
            .iter()
            .filter(|(key, _)| {
                **key != ThreadKey::Main
                    || !matches!(
                        state.physical.get(key),
                        Some(OwnerLocation::NativeReleased | OwnerLocation::Serving(_))
                    )
            })
            .filter_map(|(key, wait)| match wait {
                Wait::Select { wait_end } => Some((*key, *wait_end)),
                Wait::Runnable
                | Wait::Join(_)
                | Wait::Mutex(_)
                | Wait::Condition(_)
                | Wait::HostRequest(_)
                | Wait::Exited => None,
            })
            .min_by_key(|(_, deadline)| *deadline)
            .map(|(key, deadline)| (key, Some(deadline)))
    }

    /// Hand the Baton on from ME, which waits for WAIT, and block until it
    /// comes back. Returns it at once when ME is the next to run.
    pub(super) fn transfer(
        &self,
        me: ThreadKey,
        wait: Wait,
        baton: Baton,
        receiver: &mpsc::Receiver<Wake>,
    ) -> Result<Wake, Handoff> {
        if matches!(wait, Wait::HostRequest(_)) {
            // Host parks require their linear reservation and publication
            // plan. In particular, NoRunnable must not revive a host waiter.
            return Err(Handoff::NoRunnable(baton));
        }
        {
            let mut state = self.state();
            state.waits.insert(me, wait);
            state.physical.insert(me, OwnerLocation::Receiving);
            if wait == Wait::Runnable {
                state.ready.push_back(me);
            }
        }
        #[cfg(test)]
        if let Wait::Select { wait_end } = wait {
            self.observe_select_park_for_test(me, wait_end);
        }
        let Some(next) = self.pick_next() else {
            let mut state = self.state();
            state.waits.insert(me, Wait::Runnable);
            state.physical.insert(me, OwnerLocation::Running);
            return Err(Handoff::NoRunnable(baton));
        };
        let (target, wake) = Self::wake_for(next, baton);
        if target == me {
            if matches!(&wake, Wake::Resume(_)) {
                let mut state = self.state();
                state.waits.insert(me, Wait::Runnable);
                state.physical.insert(me, OwnerLocation::Running);
            }
            return Ok(wake);
        }
        self.send_wake(target, wake);
        match receiver.recv() {
            Ok(wake) => {
                wake.baton().enter();
                if matches!(&wake, Wake::Resume(_)) {
                    self.state().physical.insert(me, OwnerLocation::Running);
                }
                Ok(wake)
            }
            Err(_) => std::process::abort(),
        }
    }

    /// ME ended: hand the Baton to the next thread, wake its joiners, and
    /// return without waiting. With no thread able to run, the runtime is
    /// closing: every waiting thread is woken (it signals) and gets the
    /// Baton in turn.
    pub(crate) fn finish(&self, me: ThreadKey, baton: Baton) {
        {
            let mut state = self.state();
            state.waits.insert(me, Wait::Exited);
            state.physical.remove(&me);
            let joiners: Vec<ThreadKey> = state
                .waits
                .iter()
                .filter(|(_, wait)| **wait == Wait::Join(me))
                .map(|(key, _)| *key)
                .collect();
            for key in joiners {
                Self::wake_locked(&mut state, key);
            }
        }
        let next = self.pick_next().or_else(|| {
            let mut state = self.state();
            state.closing = true;
            let blocked: Vec<ThreadKey> = state
                .waits
                .iter()
                .filter(|(_, wait)| !matches!(wait, Wait::Exited))
                .map(|(key, _)| *key)
                .collect();
            for key in blocked {
                Self::wake_locked(&mut state, key);
            }
            Self::pick(&mut state).map(|(next, _)| NextOwner::Resume(next))
        });
        match next {
            Some(next) => {
                let (target, wake) = Self::wake_for(next, baton);
                self.send_wake(target, wake);
            }
            // Nothing else lives: the Baton and facade epoch end together.
            None => drop(baton),
        }
    }

    /// Start closing: every waiting thread is woken, to signal and end.
    pub(crate) fn close(&self) {
        let mut state = self.state();
        state.closing = true;
        let blocked: Vec<ThreadKey> = state
            .waits
            .iter()
            .filter(|(_, wait)| !matches!(wait, Wait::Exited | Wait::Runnable))
            .map(|(key, _)| *key)
            .collect();
        for key in blocked {
            Self::wake_locked(&mut state, key);
        }
        drop(state);
        self.notify_select();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host_key() -> HostRequestKey {
        HostRequestKey::for_test(NonZeroU64::MIN)
    }

    #[test]
    fn in_flight_resume_is_intercepted_before_worker_restore() {
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let later = ThreadKey::Worker(NonZeroU64::new(2).expect("nonzero"));
        let original_main_wait = Wait::Join(worker);
        let mut state = ControlState::default();
        state.waits.insert(ThreadKey::Main, original_main_wait);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        state.waits.insert(worker, Wait::Runnable);
        state.physical.insert(worker, OwnerLocation::Running);
        state.waits.insert(later, Wait::Runnable);
        state.ready.push_back(later);
        state.services.insert(
            host_key(),
            ServiceMetadata {
                requester: later,
                phase: ServicePhase::Reacquiring,
            },
        );
        state.pending_acquires.push_back(host_key());
        assert_eq!(
            Control::intercept_ready(&mut state, worker),
            Some(host_key())
        );
        assert_eq!(state.physical.get(&worker), Some(&OwnerLocation::Receiving));
        assert_eq!(
            state.ready.iter().copied().collect::<Vec<_>>(),
            vec![worker, later]
        );
        assert_eq!(state.waits.get(&ThreadKey::Main), Some(&original_main_wait));
    }

    #[test]
    fn ordinary_acquire_without_a_service_does_not_enqueue_itself() {
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        state.waits.insert(worker, Wait::Runnable);
        state.physical.insert(worker, OwnerLocation::Receiving);
        assert_eq!(Control::intercept_ready(&mut state, worker), None);
        assert_eq!(state.physical.get(&worker), Some(&OwnerLocation::Running));
        assert!(state.ready.is_empty());
    }

    #[test]
    fn native_completion_acquire_precedes_fifo_without_completing_main_wait() {
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let unrelated = ThreadKey::Worker(NonZeroU64::new(2).expect("nonzero"));
        let original = Wait::Join(requester);
        let mut state = ControlState::default();
        state.waits.insert(ThreadKey::Main, original);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        state.waits.insert(unrelated, Wait::Runnable);
        state.ready.push_back(unrelated);
        state.services.insert(
            host_key(),
            ServiceMetadata {
                requester,
                phase: ServicePhase::Reacquiring,
            },
        );
        state.pending_acquires.push_back(host_key());
        assert!(Control::service_ready(&state));
        assert_eq!(Control::pick_service(&mut state), Some(host_key()));
        assert_eq!(state.waits.get(&ThreadKey::Main), Some(&original));
        assert_eq!(state.ready.front(), Some(&unrelated));
        assert_eq!(
            state
                .services
                .get(&host_key())
                .map(|metadata| metadata.phase),
            Some(ServicePhase::ServingAfterWait)
        );
    }

    #[test]
    fn suspended_native_main_retains_its_exact_fifo_place() {
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let later = ThreadKey::Worker(NonZeroU64::new(2).expect("nonzero"));
        let mut state = ControlState::default();
        for key in [ThreadKey::Main, worker, later] {
            state.waits.insert(key, Wait::Runnable);
            state.ready.push_back(key);
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        assert_eq!(Control::pick(&mut state), Some((worker, None)));
        assert_eq!(
            state.ready.iter().copied().collect::<Vec<_>>(),
            vec![ThreadKey::Main, later]
        );
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        assert_eq!(Control::pick(&mut state), Some((ThreadKey::Main, None)));
    }

    #[test]
    fn native_reacquire_precedes_a_later_reserved_host_request() {
        let key = host_key();
        let later = HostRequestKey::for_test(NonZeroU64::new(2).expect("nonzero"));
        let mut state = ControlState::default();
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        for (key, phase) in [
            (key, ServicePhase::Reacquiring),
            (later, ServicePhase::Reserved),
        ] {
            state.services.insert(
                key,
                ServiceMetadata {
                    requester: ThreadKey::Worker(NonZeroU64::MIN),
                    phase,
                },
            );
        }
        state.pending_acquires.push_back(key);
        state.pending_services.push_back(later);
        assert_eq!(Control::pick_service(&mut state), Some(key));
        assert_eq!(state.pending_services.front(), Some(&later));
        assert_eq!(
            Control::pick_service(&mut state),
            None,
            "one native main service at a time"
        );
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        assert_eq!(Control::pick_service(&mut state), Some(later));
    }

    #[test]
    fn ordinary_wakeup_cannot_complete_a_host_request() {
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        state.waits.insert(requester, Wait::HostRequest(host_key()));
        Control::wake_locked(&mut state, requester);
        assert_eq!(
            state.waits.get(&requester),
            Some(&Wait::HostRequest(host_key()))
        );
        assert!(state.ready.is_empty());
    }

    #[test]
    fn signal_priority_cannot_turn_a_host_park_into_a_lisp_resume() {
        let control = Control::default();
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        control
            .state()
            .waits
            .insert(requester, Wait::HostRequest(host_key()));
        control.prioritize(requester);
        assert_eq!(
            control.wait_of(requester),
            Some(Wait::HostRequest(host_key()))
        );
        assert!(control.state().ready.is_empty());
    }

    #[test]
    fn host_wait_has_no_clock_or_fifo_readiness() {
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        state.waits.insert(requester, Wait::HostRequest(host_key()));
        // A stale FIFO entry is not a completion certificate.
        state.ready.push_back(requester);
        assert_eq!(Control::pick(&mut state), None);
        assert_eq!(
            state.waits.get(&requester),
            Some(&Wait::HostRequest(host_key()))
        );
    }

    #[test]
    fn native_release_preserves_the_semantic_wait_and_requires_the_running_main() {
        let control = Control::default();
        let _receiver = control.register_main();
        let requester = ThreadKey::Worker(NonZeroU64::MIN);
        let original = Wait::Join(requester);
        control.state().waits.insert(ThreadKey::Main, original);
        assert_eq!(control.main_native_released(), Ok(()));
        assert_eq!(control.wait_of(ThreadKey::Main), Some(original));
        assert_eq!(
            control.main_native_released(),
            Err(ServiceRefusal::WrongServiceState)
        );
        assert!(control.state().ready.is_empty());
    }

    #[test]
    fn native_release_cannot_be_announced_from_a_worker_os_thread() {
        let control = std::sync::Arc::new(Control::default());
        let _receiver = control.register_main();
        let worker_control = std::sync::Arc::clone(&control);
        let refusal = std::thread::spawn(move || worker_control.main_native_released())
            .join()
            .expect("worker finished");
        assert_eq!(refusal, Err(ServiceRefusal::WrongMainThread));
        assert_eq!(
            control.state().physical.get(&ThreadKey::Main),
            Some(&OwnerLocation::Running)
        );
    }

    #[test]
    fn select_deadline_becomes_ready_at_its_exact_fine_instant() {
        let now = Instant::now();
        let deadline = now + std::time::Duration::from_millis(50);
        let mut state = ControlState::default();
        state
            .waits
            .insert(ThreadKey::Main, Wait::Select { wait_end: deadline });
        Control::expire_deadlines(&mut state, deadline - std::time::Duration::from_nanos(1));
        assert_eq!(
            state.waits.get(&ThreadKey::Main),
            Some(&Wait::Select { wait_end: deadline })
        );
        assert!(state.ready.is_empty());
        Control::expire_deadlines(&mut state, deadline);
        assert_eq!(state.waits.get(&ThreadKey::Main), Some(&Wait::Runnable));
        assert_eq!(state.ready.pop_front(), Some(ThreadKey::Main));
    }
}
