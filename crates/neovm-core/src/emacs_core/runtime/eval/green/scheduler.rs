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
    ServiceTurn {
        key: HostRequestKey,
        baton: Baton,
    },
    /// Reenter the owned native driver without restoring main's Lisp owner.
    NativePump {
        baton: Baton,
    },
}

impl Wake {
    fn baton(&self) -> &Baton {
        match self {
            Self::Resume(baton) | Self::ServiceTurn { baton, .. } | Self::NativePump { baton } => {
                baton
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OwnerLocation {
    Running,
    Receiving,
    NativeReleased,
    NativeDriving,
    Serving(HostRequestKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServicePhase {
    Reserved,
    Serving,
    PendingNative,
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
    Native(NativeTurn),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeTurn {
    Service(HostRequestKey),
    Pump,
}

enum NativePumpChoice {
    Native(NativeTurn),
    OriginalMain,
}

/// A driver acquire is the one physical cycle's return, not a request key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeAcquire {
    Service(HostRequestKey),
    Pump,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativeWork {
    Ready,
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativePumpPurpose {
    Serving,
    Draining,
}

enum ReadySelection {
    NextEligible,
    /// Main already owns the idle Baton: consume only its old ready turn.
    NativeMainAcquire,
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
    pending_acquires: VecDeque<NativeAcquire>,
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

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainNativeWaitEvent {
    Released,
    PhysicalBlockEntered,
    PhysicalBlockReturned,
    Reacquired,
}

/// The scheduler, shared by every thread's seat.
#[derive(Default)]
pub(crate) struct Control {
    state: Mutex<ControlState>,
    #[cfg(test)]
    main_native_events: Mutex<Vec<MainNativeWaitEvent>>,
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
    #[cfg(test)]
    pub(crate) fn record_main_native_event_for_test(&self, event: MainNativeWaitEvent) {
        self.main_native_events.lock().unwrap().push(event);
    }
    #[cfg(test)]
    pub(crate) fn main_native_events_for_test(&self) -> Vec<MainNativeWaitEvent> {
        self.main_native_events.lock().unwrap().clone()
    }

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
            // A released native main owns a distinct registration. Its
            // durable scheduler state may become ready before or during the
            // OS wait; the idle-selector epoch alone cannot wake that role.
            // Drop the metadata guard before publishing to the paired source.
            let main_native =
                self.state().physical.get(&ThreadKey::Main) == Some(&OwnerLocation::NativeReleased);
            if main_native {
                select.nudge(
                    crate::host_effect::wake::ControlDestination::NativeMain,
                    crate::host_effect::wake::ControlNudge::SchedulerReady,
                );
            }
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
                let Some(turn) = Self::pick_service(&mut state) else {
                    return Err(ServiceTransferRefusal {
                        reason: ServiceRefusal::WrongServiceState,
                        baton,
                    });
                };
                drop(state);
                self.send_wake(ThreadKey::Main, Self::native_wake(turn, baton));
                Ok(ServiceTransfer::Dispatched)
            }
            Some(OwnerLocation::NativeReleased) => {
                state.pending_services.push_back(slice.key);
                drop(state);
                self.nudge_main(crate::host_effect::wake::ControlNudge::HostServiceReady);
                Ok(ServiceTransfer::Queued(baton))
            }
            Some(
                OwnerLocation::Running | OwnerLocation::NativeDriving | OwnerLocation::Serving(_),
            )
            | None => {
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
        if let Some(turn) = Self::pick_service(&mut state) {
            return Ok(Self::native_wake(turn, baton));
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
                    Wake::NativePump { baton } => {
                        self.send_wake(target, Wake::NativePump { baton });
                        Ok(ServiceReturn::Sent)
                    }
                }
            }
            Some(
                ServicePhase::Reserved
                | ServicePhase::PendingNative
                | ServicePhase::RealWait
                | ServicePhase::Reacquiring,
            )
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
                state
                    .pending_acquires
                    .push_back(NativeAcquire::Service(slice.key));
                drop(state);
                self.metadata_changed.notify_all();
                self.notify_select();
                match receiver.recv() {
                    Ok(Wake::ServiceTurn { key, baton }) if key == slice.key => {
                        baton.enter();
                        Ok(baton)
                    }
                    Ok(
                        Wake::Resume(baton)
                        | Wake::ServiceTurn { baton, .. }
                        | Wake::NativePump { baton },
                    ) => Err(ServiceAcquireRefusal {
                        reason: ServiceRefusal::WrongServiceState,
                        release: NativeRelease::Idle(baton),
                    }),
                    // Losing the sole Baton is an ownership failure. Native
                    // request cancellation itself never closes this channel.
                    Err(_) => std::process::abort(),
                }
            }
        }
    }

    /// Preserve this request's linear reservation while the owned driver
    /// serves another request. Deferral alone admits no ordinary Lisp owner.
    pub(super) fn defer_native_service(
        &self,
        slice: &ReservedServiceSlice,
        baton: Baton,
    ) -> Result<Baton, ServiceTransferRefusal> {
        let mut state = self.state();
        let result = if state.main_os_thread != Some(std::thread::current().id()) {
            Err(ServiceRefusal::WrongMainThread)
        } else {
            Self::defer_native_metadata(&mut state, slice)
        };
        match result {
            Ok(()) => Ok(baton),
            Err(reason) => Err(ServiceTransferRefusal { reason, baton }),
        }
    }

    fn defer_native_metadata(
        state: &mut ControlState,
        slice: &ReservedServiceSlice,
    ) -> Result<(), ServiceRefusal> {
        if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Serving(slice.key)) {
            return Err(ServiceRefusal::WrongServiceState);
        }
        let Some(metadata) = state.services.get_mut(&slice.key) else {
            return Err(ServiceRefusal::WrongServiceState);
        };
        if metadata.requester != slice.requester {
            return Err(ServiceRefusal::WrongServiceState);
        }
        metadata.phase = match metadata.phase {
            ServicePhase::Serving => ServicePhase::PendingNative,
            ServicePhase::ServingAfterWait => ServicePhase::RealWait,
            ServicePhase::Reserved
            | ServicePhase::PendingNative
            | ServicePhase::RealWait
            | ServicePhase::Reacquiring => return Err(ServiceRefusal::WrongServiceState),
        };
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
        Ok(())
    }

    /// Called only after the owned native record has an actual completion.
    /// The record retains the request's dispatch and cancellation obligations.
    pub(super) fn enter_native_completion(
        &self,
        slice: &ReservedServiceSlice,
        baton: Baton,
    ) -> Result<Baton, ServiceTransferRefusal> {
        let mut state = self.state();
        let result = if state.main_os_thread != Some(std::thread::current().id()) {
            Err(ServiceRefusal::WrongMainThread)
        } else {
            Self::enter_native_completion_metadata(&mut state, slice)
        };
        match result {
            Ok(()) => Ok(baton),
            Err(reason) => Err(ServiceTransferRefusal { reason, baton }),
        }
    }

    fn enter_native_completion_metadata(
        state: &mut ControlState,
        slice: &ReservedServiceSlice,
    ) -> Result<(), ServiceRefusal> {
        if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::NativeDriving) {
            return Err(ServiceRefusal::WrongServiceState);
        }
        let Some(metadata) = state.services.get_mut(&slice.key) else {
            return Err(ServiceRefusal::WrongServiceState);
        };
        if metadata.requester != slice.requester {
            return Err(ServiceRefusal::WrongServiceState);
        }
        metadata.phase = match metadata.phase {
            ServicePhase::PendingNative => ServicePhase::Serving,
            ServicePhase::RealWait => ServicePhase::ServingAfterWait,
            ServicePhase::Reserved
            | ServicePhase::Serving
            | ServicePhase::Reacquiring
            | ServicePhase::ServingAfterWait => return Err(ServiceRefusal::WrongServiceState),
        };
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Serving(slice.key));
        Ok(())
    }

    /// The sole physical executor lends ordinary eligibility to every newly
    /// pending call together. The driver owns one release, never one per key.
    pub(super) fn release_native_pump(
        &self,
        _permit: RealWaitPermit,
        baton: Baton,
    ) -> Result<NativeRelease, ServiceTransferRefusal> {
        let mut state = self.state();
        let reason = if state.main_os_thread != Some(std::thread::current().id()) {
            Some(ServiceRefusal::WrongMainThread)
        } else if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::NativeDriving)
            || Self::pending_native(&state) == NativeWork::Idle
        {
            Some(ServiceRefusal::WrongServiceState)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(ServiceTransferRefusal { reason, baton });
        }
        for metadata in state.services.values_mut() {
            if metadata.phase == ServicePhase::PendingNative {
                metadata.phase = ServicePhase::RealWait;
            }
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

    /// Physical completion first reacquires the native driver, not whichever
    /// request happened to arm the poll. A service wake for another key is valid.
    pub(super) fn reacquire_native_pump(
        &self,
        release: NativeRelease,
        receiver: &mpsc::Receiver<Wake>,
    ) -> Result<Wake, ServiceAcquireRefusal> {
        let mut state = self.state();
        let reason = if state.main_os_thread != Some(std::thread::current().id()) {
            Some(ServiceRefusal::WrongMainThread)
        } else if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::NativeReleased) {
            Some(ServiceRefusal::WrongServiceState)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(ServiceAcquireRefusal { reason, release });
        }
        match release {
            NativeRelease::Idle(baton) => {
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
                drop(state);
                baton.enter();
                Ok(Wake::NativePump { baton })
            }
            NativeRelease::Transferred => {
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Receiving);
                if !state.pending_acquires.contains(&NativeAcquire::Pump) {
                    state.pending_acquires.push_back(NativeAcquire::Pump);
                }
                drop(state);
                self.notify_select();
                match receiver.recv() {
                    Ok(wake) => {
                        wake.baton().enter();
                        Ok(wake)
                    }
                    Err(_) => std::process::abort(),
                }
            }
        }
    }

    /// Service-only choices preserve main's original wait. A still-reserved
    /// PendingNative call cannot admit main's unrelated Lisp continuation.
    pub(super) fn choose_native_pump_turn(
        &self,
        baton: Baton,
        purpose: NativePumpPurpose,
    ) -> Result<Wake, ServiceTransferRefusal> {
        let mut state = self.state();
        let reason = if state.main_os_thread != Some(std::thread::current().id()) {
            Some(ServiceRefusal::WrongMainThread)
        } else if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::NativeDriving) {
            Some(ServiceRefusal::WrongServiceState)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(ServiceTransferRefusal { reason, baton });
        }
        match Self::choose_native_metadata(&mut state, purpose) {
            NativePumpChoice::Native(turn) => Ok(Self::native_wake(turn, baton)),
            NativePumpChoice::OriginalMain => Ok(Wake::Resume(baton)),
        }
    }

    fn choose_native_metadata(
        state: &mut ControlState,
        purpose: NativePumpPurpose,
    ) -> NativePumpChoice {
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        if let Some(turn) = Self::pick_queued_service(state) {
            return NativePumpChoice::Native(turn);
        }
        let released = state
            .services
            .values()
            .any(|metadata| metadata.phase == ServicePhase::RealWait);
        let reserved = state
            .services
            .values()
            .any(|metadata| metadata.phase == ServicePhase::PendingNative);
        match purpose {
            NativePumpPurpose::Serving => {
                if released
                    && !reserved
                    && state.waits.get(&ThreadKey::Main) == Some(&Wait::Runnable)
                {
                    let _ = Self::pop_ready(state, ReadySelection::NativeMainAcquire);
                    state
                        .physical
                        .insert(ThreadKey::Main, OwnerLocation::Running);
                    return NativePumpChoice::OriginalMain;
                }
            }
            NativePumpPurpose::Draining => {}
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
        NativePumpChoice::Native(NativeTurn::Pump)
    }

    /// Existing owned process source; no host object or World borrow escapes.
    pub(super) fn native_pump_source(
        &self,
    ) -> Result<std::sync::Arc<crate::host_effect::wake::ControlWake>, ServiceRefusal> {
        self.select
            .get()
            .and_then(Option::as_ref)
            .map(GreenSelectHandle::control_source)
            .ok_or(ServiceRefusal::MainUnavailable)
    }

    /// Recheck after registration, before committing to an actual OS wait.
    pub(super) fn native_service_queued(&self) -> NativeWork {
        if Self::queued_native_work(&self.state()) == NativeWork::Ready {
            NativeWork::Ready
        } else {
            NativeWork::Idle
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
            if let Some(turn) = Self::pick_service(&mut state) {
                drop(state);
                self.send_wake(ThreadKey::Main, Self::native_wake(turn, baton));
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

    /// Main has published its canonical parked roots and reached an actual
    /// native wait. Its Select remains distinct from its physical location:
    /// peers may run, but cannot resume main before the native observer returns.
    pub(super) fn release_main_wait(
        &self,
        _permit: RealWaitPermit,
        baton: Baton,
        wait_end: Instant,
    ) -> NativeRelease {
        let mut state = self.state();
        if state.main_os_thread != Some(std::thread::current().id())
            || state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Running)
        {
            std::process::abort();
        }
        #[cfg(test)]
        self.main_native_events
            .lock()
            .unwrap()
            .push(MainNativeWaitEvent::Released);
        Self::set_wait(&mut state, ThreadKey::Main, Wait::Select { wait_end });
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        let next = match Self::pick(&mut state) {
            Some((key, None)) => Some(key),
            Some((_, Some(_))) | None => None,
        };
        drop(state);
        #[cfg(test)]
        self.observe_select_park_for_test(ThreadKey::Main, wait_end);
        match next {
            Some(key) => {
                self.send_wake(key, Wake::Resume(baton));
                NativeRelease::Transferred
            }
            None => NativeRelease::Idle(baton),
        }
    }

    /// The physical wait has returned. Publish main's readiness before waking
    /// an idle scheduler; owner restoration and all Lisp remain in switch.rs.
    pub(super) fn reacquire_main_wait(
        &self,
        release: NativeRelease,
        receiver: &mpsc::Receiver<Wake>,
    ) -> Wake {
        let mut state = self.state();
        if state.main_os_thread != Some(std::thread::current().id())
            || state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::NativeReleased)
        {
            std::process::abort();
        }
        #[cfg(test)]
        self.main_native_events
            .lock()
            .unwrap()
            .push(MainNativeWaitEvent::Reacquired);
        match release {
            NativeRelease::Idle(baton) => {
                Self::finish_idle_main_acquire(&mut state);
                drop(state);
                baton.enter();
                Wake::Resume(baton)
            }
            NativeRelease::Transferred => {
                Self::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::Receiving);
                Self::enqueue_ready(&mut state, ThreadKey::Main);
                drop(state);
                self.notify_select();
                match receiver.recv() {
                    Ok(wake) => {
                        wake.baton().enter();
                        wake
                    }
                    Err(_) => std::process::abort(),
                }
            }
        }
    }

    fn finish_idle_main_acquire(state: &mut ControlState) {
        Self::set_wait(state, ThreadKey::Main, Wait::Runnable);
        // Readiness may have queued main while the physical wait excluded it.
        // Returning its idle Baton consumes that turn without selecting a peer.
        let _ = Self::pop_ready(state, ReadySelection::NativeMainAcquire);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Running);
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

    /// Undo `register_worker` for a worker whose carrier OS thread never
    /// started (`make-thread` spawn refusal). The worker never held the
    /// Baton and its Lisp handle was never returned, so no joiner, mutex,
    /// condition, host request, or service can name it.
    ///
    /// The ready turn is invalidated through the seam: `set_wait` publishes
    /// the typed exit and `retire_exited` removes the record, so `pop_ready`'s
    /// retain drops the stale turn. A stale Runnable turn would be picked and
    /// block the scheduler on a carrier that does not exist.
    pub(super) fn unregister_worker(&self, key: ThreadKey) {
        let mut state = self.state();
        Self::set_wait(&mut state, key, Wait::Exited);
        // Whole-key removal goes through the seam; this path no longer writes
        // waits/senders/physical directly beside `set_wait`.
        Self::remove_key(&mut state, key);
    }

    /// Test-only: whether any scheduler record still names KEY.
    #[cfg(test)]
    pub(super) fn worker_registered_for_test(&self, key: ThreadKey) -> bool {
        let state = self.state();
        state.senders.contains_key(&key)
            || state.waits.contains_key(&key)
            || state.physical.contains_key(&key)
    }

    /// Test-only: how many threads the registry currently charges (wait
    /// entries, wake senders).
    #[cfg(test)]
    pub(super) fn registry_sizes_for_test(&self) -> (usize, usize) {
        let state = self.state();
        (state.waits.len(), state.senders.len())
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

    /// Move threads whose deadline passed into the ready queue, EARLIEST
    /// DEADLINE FIRST. `waits` is keyed by `ThreadKey`, and `Main` orders
    /// before every worker, so expiring in map order let a single coarse
    /// host wakeup that left several deadlines expired together resume the
    /// LATER deadline first — Main's `sleep-for` before the peer whose
    /// earlier timer had already passed (macOS arm64, CI run 41283917816:
    /// the green release-point deopt tests lost the redefinition that way).
    /// The single-expiry wake already runs the earliest deadline
    /// (`pick`'s `min_by_key`); this keeps that order when several expire
    /// at once. Identical deadlines keep `ThreadKey` order: simultaneous
    /// deadlines have no order of their own to preserve.
    fn expire_deadlines(state: &mut ControlState, now: Instant) {
        let mut expired: Vec<(ThreadKey, Instant)> = state
            .waits
            .iter()
            .filter_map(|(key, wait)| match wait {
                Wait::Select { wait_end } if *wait_end <= now => Some((*key, *wait_end)),
                _ => None,
            })
            .collect();
        expired.sort_by_key(|&(_, wait_end)| wait_end);
        for (key, _) in expired {
            Self::wake_locked(state, key);
        }
    }

    /// Readiness wakes all selecting owners. They inspect their own wait
    /// policy and poll live descriptors only after reacquiring the Baton.
    /// Both readiness call sites run BEFORE `pick` (the next-owner scan and
    /// the post-wait activity wake), so a readiness event arriving while
    /// several deadlines have already passed must admit those in deadline
    /// order too — key order here would resume Main before a worker whose
    /// earlier timer had expired, the same inversion `expire_deadlines`
    /// fixes on the pure timer path. The still-waiting selectors follow in
    /// key order. A monotonic `Instant::now()` never expires a deadline
    /// early, so this stays a readiness-gated wake, not a clock one.
    fn wake_selects(state: &mut ControlState) {
        Self::expire_deadlines(state, Instant::now());
        let selecting: Vec<_> = state
            .waits
            .iter()
            .filter_map(|(key, wait)| matches!(wait, Wait::Select { .. }).then_some(*key))
            .collect();
        for key in selecting {
            Self::wake_locked(state, key);
        }
    }

    fn pick_acquire(state: &mut ControlState) -> Option<NativeTurn> {
        while let Some(acquire) = state.pending_acquires.pop_front() {
            match acquire {
                NativeAcquire::Pump => {
                    if Self::pending_native(state) == NativeWork::Idle {
                        continue;
                    }
                    state
                        .physical
                        .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
                    return Some(NativeTurn::Pump);
                }
                NativeAcquire::Service(key) => {
                    let Some(metadata) = state.services.get_mut(&key) else {
                        continue;
                    };
                    match metadata.phase {
                        ServicePhase::Reacquiring => {
                            metadata.phase = ServicePhase::ServingAfterWait;
                            state
                                .physical
                                .insert(ThreadKey::Main, OwnerLocation::Serving(key));
                            return Some(NativeTurn::Service(key));
                        }
                        ServicePhase::Reserved
                        | ServicePhase::Serving
                        | ServicePhase::PendingNative
                        | ServicePhase::RealWait
                        | ServicePhase::ServingAfterWait => {}
                    }
                }
            }
        }
        None
    }

    fn pick_reserved_service(state: &mut ControlState) -> Option<NativeTurn> {
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
                    return Some(NativeTurn::Service(key));
                }
                ServicePhase::Serving
                | ServicePhase::PendingNative
                | ServicePhase::RealWait
                | ServicePhase::Reacquiring
                | ServicePhase::ServingAfterWait => {}
            }
        }
        None
    }

    fn pending_native(state: &ControlState) -> NativeWork {
        if state.services.values().any(|metadata| {
            matches!(
                metadata.phase,
                ServicePhase::PendingNative | ServicePhase::RealWait
            )
        }) {
            NativeWork::Ready
        } else {
            NativeWork::Idle
        }
    }

    fn pick_queued_service(state: &mut ControlState) -> Option<NativeTurn> {
        if state.physical.get(&ThreadKey::Main) != Some(&OwnerLocation::Receiving) {
            return None;
        }
        Self::pick_acquire(state).or_else(|| Self::pick_reserved_service(state))
    }

    /// Native acquire/service turns never complete main's semantic Wait.
    fn pick_service(state: &mut ControlState) -> Option<NativeTurn> {
        if let Some(turn) = Self::pick_queued_service(state) {
            return Some(turn);
        }
        if state.physical.get(&ThreadKey::Main) == Some(&OwnerLocation::Receiving)
            && Self::pending_native(state) == NativeWork::Ready
        {
            // Pending native work remains a progress source even when no Lisp
            // owner is runnable. Reenter only the driver, never main's Lisp.
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
            return Some(NativeTurn::Pump);
        }
        None
    }

    fn queued_native_work(state: &ControlState) -> NativeWork {
        if state.pending_acquires.iter().any(|acquire| match acquire {
            NativeAcquire::Pump => Self::pending_native(state) == NativeWork::Ready,
            NativeAcquire::Service(key) => state
                .services
                .get(key)
                .is_some_and(|metadata| metadata.phase == ServicePhase::Reacquiring),
        }) || state.pending_services.iter().any(|key| {
            state
                .services
                .get(key)
                .is_some_and(|metadata| metadata.phase == ServicePhase::Reserved)
        }) {
            NativeWork::Ready
        } else {
            NativeWork::Idle
        }
    }

    fn service_ready(state: &ControlState) -> bool {
        state.physical.get(&ThreadKey::Main) == Some(&OwnerLocation::Receiving)
            && Self::queued_native_work(state) == NativeWork::Ready
    }

    fn native_wake(turn: NativeTurn, baton: Baton) -> Wake {
        match turn {
            NativeTurn::Service(key) => Wake::ServiceTurn { key, baton },
            NativeTurn::Pump => Wake::NativePump { baton },
        }
    }

    fn wake_for(next: NextOwner, baton: Baton) -> (ThreadKey, Wake) {
        match next {
            NextOwner::Resume(key) => (key, Wake::Resume(baton)),
            NextOwner::Native(turn) => (ThreadKey::Main, Self::native_wake(turn, baton)),
        }
    }

    fn intercept_ready(state: &mut ControlState, me: ThreadKey) -> Option<NativeTurn> {
        if me == ThreadKey::Main {
            state.physical.insert(me, OwnerLocation::Receiving);
        }
        // A pending-only driver is a scheduling fallback, not authority to
        // intercept a terminal's direct return to its reserved requester.
        if let Some(turn) = Self::pick_queued_service(state) {
            if me != ThreadKey::Main {
                state.physical.insert(me, OwnerLocation::Receiving);
                if state.waits.get(&me) == Some(&Wait::Runnable) && !state.ready.contains(&me) {
                    // Its selected turn was already removed from FIFO. A
                    // native-only interception must not forfeit that turn.
                    state.ready.push_front(me);
                }
            }
            Some(turn)
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
                Wake::NativePump { baton } => return Wake::NativePump { baton },
                Wake::Resume(baton) => {
                    let key = Self::intercept_ready(&mut self.state(), me);
                    match key {
                        None => return Wake::Resume(baton),
                        Some(turn) if me == ThreadKey::Main => {
                            return Self::native_wake(turn, baton);
                        }
                        Some(turn) => {
                            self.send_wake(ThreadKey::Main, Self::native_wake(turn, baton));
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
                if let Some(turn) = Self::pick_service(&mut state) {
                    return Some(NextOwner::Native(turn));
                }
                match Self::pick(&mut state) {
                    Some((key, None)) => return Some(NextOwner::Resume(key)),
                    Some((_, Some(deadline))) => deadline,
                    None => {
                        if state.physical.get(&ThreadKey::Main)
                            == Some(&OwnerLocation::NativeReleased)
                            && (!state.closing || Self::pending_native(&state) == NativeWork::Ready)
                        {
                            // No ordinary owner can run, but main's actual
                            // native wait can still complete. Closing cannot
                            // drop the last Baton while native settlements and
                            // their parked roots remain live. Keep it out of
                            // Lisp until the driver's acquire is announced.
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
                if let Some(turn) = Self::pick_service(&mut state) {
                    return Some(NextOwner::Native(turn));
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
            // Activity wakes may expire deadlines (`wake_selects`), but only
            // ones a monotonic now shows already passed; pick's monotonic
            // check remains the sole clock-based wake.
        }
    }

    /// Maintenance seams shared with the scheduler-index implementation.
    /// New transitions must not mutate the wait map or ready queue elsewhere.
    fn set_wait(state: &mut ControlState, key: ThreadKey, wait: Wait) {
        state.waits.insert(key, wait);
    }

    /// Remove every scheduler record that names an ended thread. This is the
    /// single whole-key removal seam: `unregister_worker` and `finish` call it
    /// only after publishing `Wait::Exited` through [`Self::set_wait`], so the
    /// ended fact stays typed (`exited` reads an absent wait as ended) and
    /// joiners keep observing it. Whole-key removal must stay here, never as
    /// direct map writes beside `set_wait`: when the scheduler scan indexes
    /// land, this is where their counts and per-object waiter sets are
    /// released after the `Exited` transition.
    fn remove_key(state: &mut ControlState, key: ThreadKey) {
        debug_assert!(
            matches!(state.waits.get(&key), Some(Wait::Exited) | None),
            "remove_key follows the exit publication, not a pause"
        );
        state.waits.remove(&key);
        state.senders.remove(&key);
        state.physical.remove(&key);
        // No ready-queue edit is needed: `pop_ready` drops a stale turn whose
        // wait is not `Runnable` (or whose key no longer has one).
    }

    fn enqueue_ready(state: &mut ControlState, key: ThreadKey) {
        // A native main can already have been woken while its physical wait
        // still excluded it. Preserve that FIFO place without a duplicate turn.
        if !state.ready.contains(&key) {
            state.ready.push_back(key);
        }
    }

    fn pop_ready(state: &mut ControlState, selection: ReadySelection) -> Option<ThreadKey> {
        match selection {
            ReadySelection::NativeMainAcquire => {
                let index = state.ready.iter().position(|key| *key == ThreadKey::Main)?;
                return state.ready.remove(index);
            }
            ReadySelection::NextEligible => {}
        }
        // Keep a suspended main's exact FIFO position while selecting peers.
        // Its ordinary turn becomes eligible only after native completion.
        state
            .ready
            .retain(|key| state.waits.get(key) == Some(&Wait::Runnable));
        if let Some(index) = state.ready.iter().position(|key| {
            *key != ThreadKey::Main
                || !matches!(
                    state.physical.get(key),
                    Some(
                        OwnerLocation::NativeReleased
                            | OwnerLocation::NativeDriving
                            | OwnerLocation::Serving(_)
                    )
                )
        }) {
            if let Some(key) = state.ready.remove(index) {
                return Some(key);
            }
        }
        None
    }

    /// The next thread to hold the Baton, if any can run; with no ready
    /// thread but one waiting for a deadline, the earliest deadline.
    fn pick(state: &mut ControlState) -> Option<(ThreadKey, Option<Instant>)> {
        Self::expire_deadlines(state, Instant::now());
        if let Some(key) = Self::pop_ready(state, ReadySelection::NextEligible) {
            return Some((key, None));
        }
        state
            .waits
            .iter()
            .filter(|(key, _)| {
                **key != ThreadKey::Main
                    || !matches!(
                        state.physical.get(key),
                        Some(
                            OwnerLocation::NativeReleased
                                | OwnerLocation::NativeDriving
                                | OwnerLocation::Serving(_)
                        )
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

    /// Publish exit and join readiness before entering a scheduler/native wait.
    /// A released native main may be the only owner that can make progress.
    fn publish_owner_exit(&self, me: ThreadKey) {
        {
            let mut state = self.state();
            Self::set_wait(&mut state, me, Wait::Exited);
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
        // NativeMain observes a different arm from the idle selector. Wake it
        // before pick_next can wait for that owner's physical reacquisition.
        self.notify_select();
    }

    /// ME ended: hand the Baton to the next thread, wake its joiners, and
    /// return without waiting. With no thread able to run, the runtime is
    /// closing: every waiting thread is woken (it signals) and gets the
    /// Baton in turn.
    pub(crate) fn finish(&self, me: ThreadKey, baton: Baton) {
        self.publish_owner_exit(me);
        // A finished thread must stop charging the scheduler (P11 soak
        // finding #2): the joiners and the native main already observed the
        // exit publication above, and the record goes before the Baton can
        // move, so the next owner never selects or scans a dead entry.
        {
            let mut state = self.state();
            Self::remove_key(&mut state, me);
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
        state
            .pending_acquires
            .push_back(NativeAcquire::Service(host_key()));
        assert_eq!(
            Control::intercept_ready(&mut state, worker),
            Some(NativeTurn::Service(host_key()))
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
        state
            .pending_acquires
            .push_back(NativeAcquire::Service(host_key()));
        assert!(Control::service_ready(&state));
        assert_eq!(
            Control::pick_service(&mut state),
            Some(NativeTurn::Service(host_key()))
        );
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
        state
            .pending_acquires
            .push_back(NativeAcquire::Service(key));
        state.pending_services.push_back(later);
        assert_eq!(
            Control::pick_service(&mut state),
            Some(NativeTurn::Service(key))
        );
        assert_eq!(state.pending_services.front(), Some(&later));
        assert_eq!(
            Control::pick_service(&mut state),
            None,
            "one native main service at a time"
        );
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        assert_eq!(
            Control::pick_service(&mut state),
            Some(NativeTurn::Service(later))
        );
    }

    #[test]
    fn native_pump_serves_b_without_completing_pending_a_or_main_wait() {
        let a = host_key();
        let b = HostRequestKey::for_test(NonZeroU64::new(2).unwrap());
        let owner_a = ThreadKey::Worker(NonZeroU64::MIN);
        let owner_b = ThreadKey::Worker(NonZeroU64::new(2).unwrap());
        let original_wait = Wait::Join(owner_a);
        let mut state = ControlState::default();
        Control::set_wait(&mut state, ThreadKey::Main, original_wait);
        Control::set_wait(&mut state, owner_a, Wait::HostRequest(a));
        Control::set_wait(&mut state, owner_b, Wait::HostRequest(b));
        for (key, requester, phase) in [
            (a, owner_a, ServicePhase::RealWait),
            (b, owner_b, ServicePhase::Reserved),
        ] {
            state
                .services
                .insert(key, ServiceMetadata { requester, phase });
        }
        state.pending_services.push_back(b);
        state.pending_acquires.push_back(NativeAcquire::Pump);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        assert_eq!(Control::pick_service(&mut state), Some(NativeTurn::Pump));
        assert_eq!(state.services[&a].phase, ServicePhase::RealWait);
        assert!(
            matches!(Control::choose_native_metadata(&mut state, NativePumpPurpose::Serving), NativePumpChoice::Native(NativeTurn::Service(key)) if key == b)
        );
        assert_eq!(state.services[&a].phase, ServicePhase::RealWait);
        assert_eq!(state.services[&b].phase, ServicePhase::Serving);
        assert_eq!(state.waits[&ThreadKey::Main], original_wait);

        let slice_b = ReservedServiceSlice {
            key: b,
            requester: owner_b,
            _confined: PhantomData,
        };
        assert_eq!(Control::defer_native_metadata(&mut state, &slice_b), Ok(()));
        assert_eq!(state.services[&a].phase, ServicePhase::RealWait);
        assert_eq!(state.services[&b].phase, ServicePhase::PendingNative);
        let slice_a = ReservedServiceSlice {
            key: a,
            requester: owner_a,
            _confined: PhantomData,
        };
        assert_eq!(
            Control::enter_native_completion_metadata(&mut state, &slice_a),
            Ok(())
        );
        assert_eq!(state.services[&a].phase, ServicePhase::ServingAfterWait);
        assert_eq!(state.services[&b].phase, ServicePhase::PendingNative);
        assert_eq!(state.waits[&ThreadKey::Main], original_wait);
        assert!(
            state.ready.is_empty(),
            "native hops cannot enqueue a Lisp continuation"
        );
    }

    #[test]
    fn pending_native_record_keeps_no_runnable_scheduler_in_the_driver() {
        let key = host_key();
        let owner = ThreadKey::Worker(NonZeroU64::MIN);
        let original_wait = Wait::Join(owner);
        for phase in [ServicePhase::PendingNative, ServicePhase::RealWait] {
            let mut state = ControlState::default();
            Control::set_wait(&mut state, ThreadKey::Main, original_wait);
            Control::set_wait(&mut state, owner, Wait::HostRequest(key));
            state.services.insert(
                key,
                ServiceMetadata {
                    requester: owner,
                    phase,
                },
            );
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::Receiving);
            assert_eq!(Control::pick(&mut state), None);
            assert_eq!(Control::queued_native_work(&state), NativeWork::Idle);
            assert!(!Control::service_ready(&state));
            assert_eq!(Control::pick_service(&mut state), Some(NativeTurn::Pump));
            assert_eq!(state.services[&key].phase, phase);
            assert_eq!(
                state.physical[&ThreadKey::Main],
                OwnerLocation::NativeDriving
            );
            assert_eq!(state.waits[&ThreadKey::Main], original_wait);
            assert!(state.ready.is_empty());
        }
    }

    #[test]
    fn stale_native_pump_acquire_does_not_revive_a_drained_record() {
        let owner = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        let original = Wait::Join(owner);
        Control::set_wait(&mut state, ThreadKey::Main, original);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        state.pending_acquires.push_back(NativeAcquire::Pump);
        assert_eq!(Control::queued_native_work(&state), NativeWork::Idle);
        assert!(!Control::service_ready(&state));
        assert_eq!(Control::pick_service(&mut state), None);
        assert!(state.pending_acquires.is_empty());
        assert_eq!(state.waits[&ThreadKey::Main], original);
        assert_eq!(state.physical[&ThreadKey::Main], OwnerLocation::Receiving);
    }

    #[test]
    fn pending_only_driver_cannot_intercept_a_reserved_requester_return() {
        let owner_a = ThreadKey::Worker(NonZeroU64::MIN);
        let returning_b = ThreadKey::Worker(NonZeroU64::new(2).unwrap());
        let mut state = ControlState::default();
        Control::set_wait(&mut state, ThreadKey::Main, Wait::Join(owner_a));
        Control::set_wait(&mut state, returning_b, Wait::Runnable);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        state.physical.insert(returning_b, OwnerLocation::Receiving);
        state.services.insert(
            host_key(),
            ServiceMetadata {
                requester: owner_a,
                phase: ServicePhase::RealWait,
            },
        );
        assert!(!Control::service_ready(&state));
        assert_eq!(Control::intercept_ready(&mut state, returning_b), None);
        assert_eq!(state.physical[&returning_b], OwnerLocation::Running);
        assert_eq!(state.physical[&ThreadKey::Main], OwnerLocation::Receiving);
        assert_eq!(state.services[&host_key()].phase, ServicePhase::RealWait);
        assert!(state.ready.is_empty());
    }

    #[test]
    fn a_new_pending_native_reservation_cannot_resume_main_on_an_older_release() {
        let a = host_key();
        let b = HostRequestKey::for_test(NonZeroU64::new(2).unwrap());
        let mut state = ControlState::default();
        Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
        Control::enqueue_ready(&mut state, ThreadKey::Main);
        for (key, phase) in [
            (a, ServicePhase::RealWait),
            (b, ServicePhase::PendingNative),
        ] {
            state.services.insert(
                key,
                ServiceMetadata {
                    requester: ThreadKey::Worker(NonZeroU64::MIN),
                    phase,
                },
            );
        }
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
        assert!(matches!(
            Control::choose_native_metadata(&mut state, NativePumpPurpose::Serving),
            NativePumpChoice::Native(NativeTurn::Pump)
        ));
        assert_eq!(state.ready.front(), Some(&ThreadKey::Main));
        assert_eq!(state.services[&a].phase, ServicePhase::RealWait);
        assert_eq!(state.services[&b].phase, ServicePhase::PendingNative);
    }

    #[test]
    fn explicit_native_drain_does_not_resume_main_before_settlement() {
        for purpose in [NativePumpPurpose::Serving, NativePumpPurpose::Draining] {
            let key = host_key();
            let mut state = ControlState::default();
            Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
            Control::enqueue_ready(&mut state, ThreadKey::Main);
            state.services.insert(
                key,
                ServiceMetadata {
                    requester: ThreadKey::Worker(NonZeroU64::MIN),
                    phase: ServicePhase::RealWait,
                },
            );
            state
                .physical
                .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
            let choice = Control::choose_native_metadata(&mut state, purpose);
            match purpose {
                NativePumpPurpose::Serving => {
                    assert!(matches!(choice, NativePumpChoice::OriginalMain));
                    assert_eq!(state.physical[&ThreadKey::Main], OwnerLocation::Running);
                    assert!(state.ready.is_empty());
                }
                NativePumpPurpose::Draining => {
                    assert!(matches!(choice, NativePumpChoice::Native(NativeTurn::Pump)));
                    assert_eq!(
                        state.physical[&ThreadKey::Main],
                        OwnerLocation::NativeDriving
                    );
                    assert_eq!(state.ready.front(), Some(&ThreadKey::Main));
                }
            }
            assert_eq!(state.waits[&ThreadKey::Main], Wait::Runnable);
            assert_eq!(state.services[&key].phase, ServicePhase::RealWait);
        }
    }

    #[test]
    fn pending_native_completion_refuses_another_owner_without_changing_any_phase() {
        let key = host_key();
        let owner = ThreadKey::Worker(NonZeroU64::MIN);
        let other = ThreadKey::Worker(NonZeroU64::new(2).unwrap());
        let mut state = ControlState::default();
        state.services.insert(
            key,
            ServiceMetadata {
                requester: owner,
                phase: ServicePhase::RealWait,
            },
        );
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeDriving);
        let wrong = ReservedServiceSlice {
            key,
            requester: other,
            _confined: PhantomData,
        };
        assert_eq!(
            Control::enter_native_completion_metadata(&mut state, &wrong),
            Err(ServiceRefusal::WrongServiceState)
        );
        assert_eq!(state.services[&key].phase, ServicePhase::RealWait);
        assert_eq!(
            state.physical[&ThreadKey::Main],
            OwnerLocation::NativeDriving
        );
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
    fn pending_native_owner_exit_publishes_readiness_before_native_reacquire() {
        use crate::host_effect::wake::{ControlDestination, ControlNudge, ControlWakeOutcome};
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        for original in [Wait::Runnable, Wait::Join(worker)] {
            let manager = crate::emacs_core::process::ProcessManager::new();
            let control = Control::default();
            let _main = control.register_main();
            let _worker = control.register_worker(worker);
            control.install_select(manager.green_select_handle());
            {
                let mut state = control.state();
                Control::set_wait(&mut state, ThreadKey::Main, original);
                state
                    .physical
                    .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
            }
            let source = control.native_pump_source().unwrap();
            let armed = source.arm(ControlDestination::NativeMain).unwrap();
            assert_eq!(armed.poll(), None);
            control.publish_owner_exit(worker);
            assert_eq!(control.wait_of(worker), Some(Wait::Exited));
            assert_eq!(control.wait_of(ThreadKey::Main), Some(Wait::Runnable));
            assert_eq!(
                armed.poll(),
                Some(ControlWakeOutcome::ControlReady(
                    ControlNudge::SchedulerReady
                )),
                "worker exit must wake main before waiting for its native acquire"
            );
            assert_eq!(
                control.state().physical.get(&ThreadKey::Main),
                Some(&OwnerLocation::NativeReleased),
                "a wake hint does not fabricate native reacquisition"
            );
        }
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
    fn native_main_readiness_retains_one_fifo_turn_until_physical_acquire() {
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        let deadline = Instant::now();
        Control::set_wait(
            &mut state,
            ThreadKey::Main,
            Wait::Select { wait_end: deadline },
        );
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        Control::expire_deadlines(&mut state, deadline);
        assert_eq!(
            Control::pop_ready(&mut state, ReadySelection::NextEligible),
            None
        );
        Control::set_wait(&mut state, worker, Wait::Runnable);
        Control::enqueue_ready(&mut state, worker);
        Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        Control::enqueue_ready(&mut state, ThreadKey::Main);
        assert_eq!(
            Control::pop_ready(&mut state, ReadySelection::NextEligible),
            Some(ThreadKey::Main)
        );
        assert_eq!(
            Control::pop_ready(&mut state, ReadySelection::NextEligible),
            Some(worker)
        );
        assert_eq!(
            Control::pop_ready(&mut state, ReadySelection::NextEligible),
            None
        );
    }

    #[test]
    fn idle_main_acquire_consumes_its_woken_turn_before_the_next_yield() {
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        Control::set_wait(
            &mut state,
            ThreadKey::Main,
            Wait::Select {
                wait_end: Instant::now() + std::time::Duration::from_secs(30),
            },
        );
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        Control::wake_locked(&mut state, ThreadKey::Main);
        Control::finish_idle_main_acquire(&mut state);
        assert_eq!(Control::intercept_ready(&mut state, ThreadKey::Main), None);

        Control::set_wait(&mut state, worker, Wait::Runnable);
        Control::enqueue_ready(&mut state, worker);
        // The metadata prelude of main's next Runnable transfer.
        Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        Control::enqueue_ready(&mut state, ThreadKey::Main);
        assert_eq!(Control::pick(&mut state), Some((worker, None)));
        assert_eq!(Control::pick(&mut state), Some((ThreadKey::Main, None)));
        assert_eq!(Control::pick(&mut state), None);
    }

    #[test]
    fn idle_main_acquire_does_not_consume_an_earlier_peer_turn() {
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let mut state = ControlState::default();
        Control::set_wait(
            &mut state,
            ThreadKey::Main,
            Wait::Select {
                wait_end: Instant::now() + std::time::Duration::from_secs(30),
            },
        );
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::NativeReleased);
        Control::set_wait(&mut state, worker, Wait::Runnable);
        Control::enqueue_ready(&mut state, worker);
        Control::wake_locked(&mut state, ThreadKey::Main);
        Control::finish_idle_main_acquire(&mut state);
        assert_eq!(Control::intercept_ready(&mut state, ThreadKey::Main), None);
        assert_eq!(
            state.ready.iter().copied().collect::<Vec<_>>(),
            vec![worker]
        );

        Control::set_wait(&mut state, ThreadKey::Main, Wait::Runnable);
        state
            .physical
            .insert(ThreadKey::Main, OwnerLocation::Receiving);
        Control::enqueue_ready(&mut state, ThreadKey::Main);
        assert_eq!(Control::pick(&mut state), Some((worker, None)));
        assert_eq!(Control::pick(&mut state), Some((ThreadKey::Main, None)));
        assert_eq!(Control::pick(&mut state), None);
    }

    #[test]
    fn idle_native_wait_must_use_earlier_peer_select_deadline() {
        let control = Control::default();
        let _main_receiver = control.register_main();
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let _worker_receiver = control.register_worker(worker);
        let now = Instant::now();
        let peer_deadline = now + std::time::Duration::from_secs(1);
        let main_deadline = now + std::time::Duration::from_secs(30);
        {
            let mut state = control.state();
            Control::set_wait(
                &mut state,
                worker,
                Wait::Select {
                    wait_end: peer_deadline,
                },
            );
        }
        assert_eq!(
            control.peers(ThreadKey::Main, now),
            Peers::Sleeping(peer_deadline),
            "the executor must cap its physical wait while retaining main's semantic deadline"
        );
        assert!(peer_deadline < main_deadline);
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

    /// Several deadlines expired by one coarse host wakeup admit in
    /// DEADLINE order, not `ThreadKey` order: `waits` is a BTreeMap and
    /// `Main` orders before every worker, so map order resumed Main's
    /// later `sleep-for` before a peer whose earlier timer had already
    /// passed (macOS arm64, CI run 41283917816: the green release-point
    /// tests lost the redefinition that way). `pick` runs the earliest
    /// deadline while exactly one is expired; expiry must not invert
    /// that order when several expire together.
    #[test]
    fn expired_select_waits_admit_in_deadline_order_not_key_order() {
        let now = Instant::now();
        let early = now + std::time::Duration::from_micros(5);
        let middle = now + std::time::Duration::from_micros(20);
        let late = now + std::time::Duration::from_micros(30);
        // Key order is Main, Worker(2), Worker(5); deadline order is the
        // opposite, so a key-ordered expiry and a deadline-ordered one
        // differ at every position.
        let worker2 = ThreadKey::Worker(NonZeroU64::new(2).expect("nonzero"));
        let worker5 = ThreadKey::Worker(NonZeroU64::new(5).expect("nonzero"));
        let mut state = ControlState::default();
        state
            .waits
            .insert(ThreadKey::Main, Wait::Select { wait_end: late });
        state
            .waits
            .insert(worker2, Wait::Select { wait_end: middle });
        state
            .waits
            .insert(worker5, Wait::Select { wait_end: early });
        Control::expire_deadlines(&mut state, late);
        assert_eq!(
            state.ready.pop_front(),
            Some(worker5),
            "the earliest deadline admits first"
        );
        assert_eq!(
            state.ready.pop_front(),
            Some(worker2),
            "the middle deadline admits second"
        );
        assert_eq!(
            state.ready.pop_front(),
            Some(ThreadKey::Main),
            "the latest deadline admits last, key order notwithstanding"
        );
        assert!(state.ready.is_empty());
    }

    /// The readiness path admits expired timers in deadline order too.
    /// `wake_selects` runs before `pick` both in the next-owner scan and
    /// after a physical wait with activity, so fd/loopback readiness
    /// arriving while several deadlines have already passed must not let
    /// key order (Main first) resume the later deadline first: the
    /// readiness twin of the timer regression above.
    #[test]
    fn readiness_wake_admits_expired_selects_in_deadline_order() {
        // `wake_selects` samples its own `Instant::now()`, so the expired
        // three are PAST offsets — however many microseconds the prologue
        // takes, every sample expires them, and only the order can vary.
        // (Future microsecond offsets made the test racy on slower hosts:
        // a sample between `early` and `middle` expired one deadline and
        // let key order in behind it.)
        let now = Instant::now();
        let early = now - std::time::Duration::from_micros(30);
        let middle = now - std::time::Duration::from_micros(20);
        let late = now - std::time::Duration::from_micros(5);
        let unexpired = now + std::time::Duration::from_secs(3600);
        // Key order is Main, Worker(2), Worker(5), Worker(7); deadline
        // order is the opposite for the expired three, and Worker(7) has
        // not expired at all.
        let worker2 = ThreadKey::Worker(NonZeroU64::new(2).expect("nonzero"));
        let worker5 = ThreadKey::Worker(NonZeroU64::new(5).expect("nonzero"));
        let worker7 = ThreadKey::Worker(NonZeroU64::new(7).expect("nonzero"));
        let mut state = ControlState::default();
        state
            .waits
            .insert(ThreadKey::Main, Wait::Select { wait_end: late });
        state
            .waits
            .insert(worker2, Wait::Select { wait_end: middle });
        state
            .waits
            .insert(worker5, Wait::Select { wait_end: early });
        state.waits.insert(
            worker7,
            Wait::Select {
                wait_end: unexpired,
            },
        );
        Control::wake_selects(&mut state);
        assert_eq!(
            state.ready.pop_front(),
            Some(worker5),
            "the earliest expired deadline admits first"
        );
        assert_eq!(
            state.ready.pop_front(),
            Some(worker2),
            "the middle expired deadline admits second"
        );
        assert_eq!(
            state.ready.pop_front(),
            Some(ThreadKey::Main),
            "the latest expired deadline admits after the earlier ones"
        );
        assert_eq!(
            state.ready.pop_front(),
            Some(worker7),
            "the unexpired selector admits last, key order among the waiting"
        );
        assert!(state.ready.is_empty());
    }

    #[test]
    fn a_retired_worker_is_exited_and_leaves_no_record_or_turn() {
        let control = Control::default();
        let _main_receiver = control.register_main();
        let worker = ThreadKey::Worker(NonZeroU64::MIN);
        let _worker_receiver = control.register_worker(worker);
        {
            let mut state = control.state();
            Control::set_wait(&mut state, worker, Wait::Exited);
            Control::remove_key(&mut state, worker);
        }
        assert!(control.exited(worker), "absence keeps the ended fact typed");
        assert!(!control.worker_registered_for_test(worker));
        assert_eq!(
            control.registry_sizes_for_test(),
            (1, 1),
            "only main remains charged after retirement"
        );
        assert_eq!(
            Control::pick(&mut control.state()),
            None,
            "the retired worker's stale Runnable turn must not be selected"
        );
        assert!(
            control.state().ready.is_empty(),
            "pop_ready dropped the retired worker's stale turn"
        );
    }
}
