//! Main-only owned native progress. No owner or root-reader borrow is suspended.
//!
//! This is the Baton-serial E5 `NativeWaitPolicy::Released` mechanism: prepare
//! owned native interests, block through the single lower executor, then drive
//! owned observations. A control wake never identifies a completed request.

use super::super::super::world::Baton;
use super::super::scheduler::{
    Control, NativePumpPurpose, NativeRelease, NativeWork, Peers, ReservedServiceSlice,
    ServiceReturn, ThreadKey, Wake,
};
use super::HostRequestKey;
use super::pending::{
    NativeDrainProgress, NativePendingCalls, NativeReadyOutcome, PendingReservation, ReadyPending,
};
use super::scheduling::{MainServiceTurn, ServiceStep, complete_native_turn};
use super::transport::{CancelResolution, ControlObservation, DispatchTicket, NativeOutcome};
use crate::host_effect::contracts::{OwnedNativeData, ReadDeadline};
use crate::host_effect::native_pending::{NativeAdapterStep, NativeCompletionRoute};
use crate::host_effect::wake::native_wait::{
    NativeWaitContinuation, NativeWaitHandoff, NativeWaitPreparationError, PreparedNativeWait,
    RealWaitPermit,
};
use crate::host_effect::wake::{ControlWake, ControlWakeOutcome, NativePollOwner};
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Instant;

/// Native-only test backend. Production projection adapters remain uninstalled.
#[cfg(test)]
pub(in crate::emacs_core::eval::green) type FixtureNativeDriver = Box<
    dyn FnMut(HostRequestKey, OwnedNativeData, NativeCompletionRoute) -> NativeAdapterStep + Send,
>;

enum NativeBackend {
    Closed,
    #[cfg(test)]
    Fixture(FixtureNativeDriver),
}

impl NativeBackend {
    fn start(
        &mut self,
        key: HostRequestKey,
        native: OwnedNativeData,
        route: NativeCompletionRoute,
    ) -> NativeAdapterStep {
        match self {
            Self::Closed => {
                let _ = (key, native, route);
                NativeAdapterStep::Complete(NativeOutcome::HostClosed)
            }
            #[cfg(test)]
            Self::Fixture(driver) => driver(key, native, route),
        }
    }
}

struct PendingCallReservation {
    slice: ReservedServiceSlice,
    ticket: DispatchTicket,
}

impl PendingReservation for PendingCallReservation {
    fn key(&self) -> HostRequestKey {
        self.slice.key()
    }
}

/// The existing boxed GreenSeat receiver owns this object, never the World.
/// Pending native records own tickets/reservations, not heap readers or Batons.
pub(in crate::emacs_core::eval::green) struct MainNativePump {
    pending: NativePendingCalls<PendingCallReservation>,
    ready: VecDeque<ReadyPending<PendingCallReservation>>,
    backend: NativeBackend,
    lifecycle: PumpLifecycle,
    // Private, condition-backed settlement source. It outlives every published
    // producer and cannot be retired by the failed/replaced native endpoint.
    settlement: Arc<ControlWake>,
    _host_main: PhantomData<Rc<()>>,
}

impl Default for MainNativePump {
    fn default() -> Self {
        Self {
            pending: NativePendingCalls::new(),
            ready: VecDeque::new(),
            backend: NativeBackend::Closed,
            lifecycle: PumpLifecycle::Open,
            settlement: ControlWake::paired().0,
            _host_main: PhantomData,
        }
    }
}

static_assertions::assert_not_impl_any!(MainNativePump: Send, Sync, Clone, Copy);

impl MainNativePump {
    #[cfg(test)]
    pub(super) fn install_fixture_driver(&mut self, driver: FixtureNativeDriver) {
        self.backend = NativeBackend::Fixture(driver);
    }

    pub(super) fn close(&mut self) -> MainNativeDrain {
        // Dropping an owned backend commits actual producer abandonment before
        // its wake. Externally running operations remain until they acknowledge.
        self.backend = NativeBackend::Closed;
        self.pending.close();
        if self.lifecycle == PumpLifecycle::Open {
            self.lifecycle = PumpLifecycle::CloseRequested;
        }
        self.drain_progress()
    }

    fn drain_progress(&self) -> MainNativeDrain {
        match (self.pending.drain_progress(), self.ready.is_empty()) {
            (NativeDrainProgress::Drained, true) => MainNativeDrain::Drained,
            (NativeDrainProgress::Drained, false)
            | (NativeDrainProgress::AwaitingNativeSettlement(_), _) => MainNativeDrain::Pending,
        }
    }

    fn close_admission(&mut self, baton: &Baton) {
        if self.lifecycle == PumpLifecycle::CloseRequested {
            let runtime = baton.green_runtime();
            let Some(runtime) = runtime.as_ref() else {
                fatal_pump()
            };
            // Actor close precedes consuming native settlement; no unstarted
            // host work can enter the failed backend afterwards.
            let _ = runtime.parked.borrow_mut().services.close();
            self.lifecycle = PumpLifecycle::Draining;
        }
    }

    fn settlement_wait(&self, deadline: ReadDeadline) -> PreparedNativeWait {
        // The same owned plan and exclusive lease guard the condition-backed
        // drain. Its physical wait still goes through BlockingNativeWait.
        match PreparedNativeWait::new(&self.settlement, NativePollOwner::NativeMain, deadline) {
            Ok(wait) => wait,
            Err(
                NativeWaitPreparationError::Closed(_) | NativeWaitPreparationError::PollBusy(_),
            ) => {
                // Only this pump owns the source; neither retirement before
                // drain nor a second simultaneous lease is a recoverable I/O failure.
                fatal_pump()
            }
        }
    }

    fn prepare_wait(
        &mut self,
        source: &Arc<ControlWake>,
        deadline: ReadDeadline,
    ) -> PreparedNativeWait {
        match self.lifecycle {
            PumpLifecycle::CloseRequested | PumpLifecycle::Draining => {
                self.settlement_wait(deadline)
            }
            PumpLifecycle::Open => {
                match PreparedNativeWait::new(source, NativePollOwner::NativeMain, deadline) {
                    Ok(wait) => wait,
                    Err(NativeWaitPreparationError::Closed(_)) => {
                        self.close();
                        self.settlement_wait(deadline)
                    }
                    Err(NativeWaitPreparationError::PollBusy(_)) => fatal_pump(),
                }
            }
        }
    }

    pub(super) fn serve(
        &mut self,
        key: HostRequestKey,
        baton: Baton,
        control: &Control,
        receiver: &Receiver<Wake>,
    ) -> ServiceReturn {
        self.run(DriverTurn::Start { key, baton }, control, receiver)
    }

    pub(in crate::emacs_core::eval::green) fn drive(
        &mut self,
        baton: Baton,
        control: &Control,
        receiver: &Receiver<Wake>,
    ) -> ServiceReturn {
        self.run(DriverTurn::Drive(baton), control, receiver)
    }

    fn run(
        &mut self,
        mut turn: DriverTurn,
        control: &Control,
        receiver: &Receiver<Wake>,
    ) -> ServiceReturn {
        loop {
            let baton = match turn {
                DriverTurn::Start { key, baton } => {
                    let service = MainServiceTurn::from_main_wake(key, baton, control, receiver);
                    let source = match control.native_pump_source() {
                        Ok(source) => source,
                        Err(_) => fatal_pump(),
                    };
                    let route =
                        NativeCompletionRoute::new(source.handle(), self.settlement.handle());
                    match service.serve_step(|key, native| self.backend.start(key, native, route)) {
                        ServiceStep::Complete(served) => {
                            return served.return_to_requester(control);
                        }
                        ServiceStep::Pending {
                            baton,
                            slice,
                            ticket,
                            operation,
                        } => {
                            let baton = match control.defer_native_service(&slice, baton) {
                                Ok(baton) => baton,
                                Err(_) => fatal_pump(),
                            };
                            if let Err(rejected) = self
                                .pending
                                .insert(operation, PendingCallReservation { slice, ticket })
                            {
                                // A bad adapter must not drop its published root/ticket
                                // obligations or resume a different requester.
                                let _preserved = rejected.into_parts();
                                fatal_pump();
                            }
                            baton
                        }
                    }
                }
                DriverTurn::Drive(baton) => baton,
            };
            baton.enter();
            self.close_admission(&baton);
            self.observe_controls(&baton);
            self.pending.expire_deadlines(Instant::now());
            self.ready.extend(self.pending.observe_ready());
            if let Some(ready) = self.ready.pop_front() {
                let (key, outcome, reservation) = ready.into_parts();
                let PendingCallReservation { slice, ticket } = reservation;
                if key != slice.key() {
                    fatal_pump();
                }
                // A control-only return never enters this transition: the
                // consumed record proves this request has native settlement.
                let baton = match control.enter_native_completion(&slice, baton) {
                    Ok(baton) => baton,
                    Err(_) => fatal_pump(),
                };
                let outcome = match outcome {
                    NativeReadyOutcome::Completed(outcome) => outcome,
                    NativeReadyOutcome::ProducerClosed => NativeOutcome::HostClosed,
                };
                return complete_native_turn(baton, slice, ticket, outcome)
                    .return_to_requester(control);
            }
            let purpose = match self.lifecycle {
                PumpLifecycle::Open => NativePumpPurpose::Serving,
                PumpLifecycle::CloseRequested | PumpLifecycle::Draining => {
                    NativePumpPurpose::Draining
                }
            };
            let wake = match control.choose_native_pump_turn(baton, purpose) {
                Ok(wake) => wake,
                Err(_) => fatal_pump(),
            };
            let baton = match wake {
                Wake::Resume(baton) => return ServiceReturn::ResumeMain(baton),
                Wake::ServiceTurn { key, baton } => {
                    turn = DriverTurn::Start { key, baton };
                    continue;
                }
                Wake::NativePump { baton } => baton,
            };
            let wake = match control.native_pump_source() {
                Ok(wake) => wake,
                Err(_) => fatal_pump(),
            };
            let deadline = cap_peer_deadline(self.pending.next_deadline(), control);
            let mut wait = self.prepare_wait(&wake, deadline);
            self.close_admission(&baton);
            // Register first, then recheck durable actor and native facts.
            self.observe_controls(&baton);
            self.pending.expire_deadlines(Instant::now());
            self.ready.extend(self.pending.observe_ready());
            if !self.ready.is_empty() || control.native_service_queued() == NativeWork::Ready {
                drop(wait);
                turn = DriverTurn::Drive(baton);
                continue;
            }
            let mut handoff = PumpHandoff {
                control,
                receiver,
                authority: PumpAuthority::Held(baton),
            };
            let result = wait.wait_with(&mut handoff);
            // Native records never retain this lease, including while another
            // request is served or original main becomes ordinarily eligible.
            drop(wait);
            match result {
                Ok(ControlWakeOutcome::HostClosed(_)) | Err(_) => {
                    self.close();
                }
                Ok(
                    ControlWakeOutcome::ControlReady(_)
                    | ControlWakeOutcome::CancellationRequested
                    | ControlWakeOutcome::NativeReady
                    | ControlWakeOutcome::DeadlineElapsed,
                ) => {}
            }
            turn = match handoff.into_wake() {
                Wake::Resume(baton) => return ServiceReturn::ResumeMain(baton),
                Wake::ServiceTurn { key, baton } => DriverTurn::Start { key, baton },
                Wake::NativePump { baton } => DriverTurn::Drive(baton),
            };
        }
    }

    fn observe_controls(&mut self, baton: &Baton) {
        let observations = {
            let runtime = baton.green_runtime();
            let Some(runtime) = runtime.as_ref() else {
                fatal_pump()
            };
            runtime.parked.borrow_mut().services.native_observations()
        };
        for observation in observations {
            match observation {
                ControlObservation::Cancellation {
                    key,
                    reason,
                    resolution,
                } => match resolution {
                    CancelResolution::AwaitNativeSettlement => {
                        self.pending.request_cancel(key, reason);
                    }
                    CancelResolution::SuppressedBeforeDispatch
                    | CancelResolution::AlreadyTerminal
                    | CancelResolution::UnknownRequest => {}
                },
                ControlObservation::Closed => {
                    self.close();
                    self.lifecycle = PumpLifecycle::Draining;
                }
                ControlObservation::RejectedPublication {
                    key: _,
                    reason: _,
                    native,
                } => drop(native),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::emacs_core::eval::green) enum MainNativeDrain {
    Drained,
    Pending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PumpLifecycle {
    Open,
    CloseRequested,
    Draining,
}

enum DriverTurn {
    Start { key: HostRequestKey, baton: Baton },
    Drive(Baton),
}

enum PumpAuthority {
    Held(Baton),
    Released(NativeRelease),
    Acquired(Wake),
    Transition,
}

struct PumpHandoff<'native> {
    control: &'native Control,
    receiver: &'native Receiver<Wake>,
    authority: PumpAuthority,
}

impl NativeWaitHandoff for PumpHandoff<'_> {
    fn release(&mut self, permit: RealWaitPermit) {
        let PumpAuthority::Held(baton) =
            std::mem::replace(&mut self.authority, PumpAuthority::Transition)
        else {
            fatal_pump()
        };
        self.authority =
            PumpAuthority::Released(match self.control.release_native_pump(permit, baton) {
                Ok(release) => release,
                Err(_) => fatal_pump(),
            });
    }

    fn reacquire(&mut self) {
        let PumpAuthority::Released(release) =
            std::mem::replace(&mut self.authority, PumpAuthority::Transition)
        else {
            fatal_pump()
        };
        self.authority = PumpAuthority::Acquired(
            match self.control.reacquire_native_pump(release, self.receiver) {
                Ok(wake) => wake,
                Err(_) => fatal_pump(),
            },
        );
    }

    fn continuation(&self) -> NativeWaitContinuation {
        NativeWaitContinuation::ReturnToDriver
    }
}

impl PumpHandoff<'_> {
    fn into_wake(self) -> Wake {
        match self.authority {
            PumpAuthority::Acquired(wake) => wake,
            // A known-ready registration performs no physical release.
            PumpAuthority::Held(baton) => Wake::NativePump { baton },
            PumpAuthority::Released(_) | PumpAuthority::Transition => fatal_pump(),
        }
    }
}

fn cap_peer_deadline(deadline: ReadDeadline, control: &Control) -> ReadDeadline {
    match (deadline, control.peers(ThreadKey::Main, Instant::now())) {
        (ReadDeadline::Immediate, Peers::Ready | Peers::Sleeping(_) | Peers::Blocked) => {
            ReadDeadline::Immediate
        }
        (ReadDeadline::At(end), Peers::Sleeping(peer)) => ReadDeadline::At(end.min(peer)),
        (ReadDeadline::Unbounded, Peers::Sleeping(peer)) => ReadDeadline::At(peer),
        (original, Peers::Ready | Peers::Blocked) => original,
    }
}

fn fatal_pump() -> ! {
    tracing::error!("native pump lost its unique scheduler or settlement authority");
    std::process::abort()
}

/// Owner-selected receiving state occupies the existing boxed seat field.
/// Main alone carries native service storage; a pooled worker carries no host.
pub(in crate::emacs_core::eval::green) struct OwnerReceive {
    state: ReceiveState,
}

enum ReceiveState {
    Worker(Receiver<Wake>),
    Main {
        channel: Receiver<Wake>,
        pump: Box<MainNativePump>,
    },
}

impl OwnerReceive {
    pub(in crate::emacs_core::eval::green) fn new(key: ThreadKey, channel: Receiver<Wake>) -> Self {
        Self {
            state: match key {
                ThreadKey::Main => ReceiveState::Main {
                    channel,
                    pump: Box::default(),
                },
                ThreadKey::Worker(_) => ReceiveState::Worker(channel),
            },
        }
    }

    pub(in crate::emacs_core::eval::green) fn as_channel(&self) -> &Receiver<Wake> {
        match &self.state {
            ReceiveState::Worker(channel) | ReceiveState::Main { channel, .. } => channel,
        }
    }

    pub(in crate::emacs_core::eval::green) fn into_worker_channel(self) -> Receiver<Wake> {
        match self.state {
            ReceiveState::Worker(channel) => channel,
            ReceiveState::Main { .. } => fatal_pump(),
        }
    }

    pub(in crate::emacs_core::eval::green) fn serve(
        &mut self,
        key: HostRequestKey,
        baton: Baton,
        control: &Control,
    ) -> ServiceReturn {
        match &mut self.state {
            ReceiveState::Main { channel, pump } => {
                super::scheduling::serve_main_wake(key, baton, control, channel, pump)
            }
            ReceiveState::Worker(_) => fatal_pump(),
        }
    }

    pub(in crate::emacs_core::eval::green) fn drive(
        &mut self,
        baton: Baton,
        control: &Control,
    ) -> ServiceReturn {
        match &mut self.state {
            ReceiveState::Main { channel, pump } => pump.drive(baton, control, channel),
            ReceiveState::Worker(_) => fatal_pump(),
        }
    }

    pub(in crate::emacs_core::eval::green) fn close_native(&mut self) -> MainNativeDrain {
        match &mut self.state {
            ReceiveState::Main { pump, .. } => pump.close(),
            ReceiveState::Worker(_) => MainNativeDrain::Drained,
        }
    }

    pub(in crate::emacs_core::eval::green) fn native_drain(&self) -> MainNativeDrain {
        match &self.state {
            ReceiveState::Main { pump, .. } => pump.drain_progress(),
            ReceiveState::Worker(_) => MainNativeDrain::Drained,
        }
    }
}

static_assertions::assert_not_impl_any!(OwnerReceive: Send, Sync, Clone, Copy);
