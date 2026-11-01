//! Cold native service turns. No Context/World view crosses a native callback.

use super::super::super::world::Baton;
use super::super::scheduler::{
    Control, NativeRelease, ReservedServiceSlice, ServiceReturn, ServiceTransfer,
    ServiceTransferPlan, Wake,
};
use super::transport::{
    self, Acknowledgement, CancelReason, CloseProgress, CompletionDisposition, DeliveryStatus,
    DispatchPoll, DispatchTicket, HostClient, HostWaiter, NativeDispatch, NativeOutcome,
    NativeSettlement, Publication, ServiceTransport, TerminalRead, TerminalReceipt,
};
use super::{HostRequestKey, HostRootError, PreparedHostCall, RootsRetired};
use crate::host_effect::contracts::OwnedNativeData;
use crate::host_effect::native_pending::{NativeAdapterStep, PendingNativeOperation};
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::mpsc::Receiver;

use crate::host_effect::wake::native_wait::NativeWaitHandoff;
pub(in crate::emacs_core::eval::green) use crate::host_effect::wake::native_wait::RealWaitPermit;

#[derive(Default)]
pub(in crate::emacs_core::eval::green) enum ServiceRuntime {
    #[default]
    Dormant,
    Open(Box<ServiceSession>),
}

pub(in crate::emacs_core::eval::green) struct ServiceSession {
    client: HostClient,
    transport: ServiceTransport,
    waiters: BTreeMap<HostRequestKey, HostWaiter>,
    reservations: BTreeMap<HostRequestKey, ReservedServiceSlice>,
    deferred_terminals: BTreeMap<HostRequestKey, HostTerminal>,
    /// Linear actor-issued obligations; no derived host state is cached.
    dispatched: BTreeMap<HostRequestKey, NativeDispatch>,
    #[cfg(test)]
    driver: FixtureDriver,
    #[cfg(test)]
    completion: FixtureCompletion,
}

impl std::fmt::Debug for ServiceRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = match self {
            Self::Dormant => "dormant",
            Self::Open(_) => "open",
        };
        f.debug_struct("ServiceRuntime")
            .field("state", &state)
            .finish_non_exhaustive()
    }
}

pub(in crate::emacs_core::eval::green) struct HostTerminal {
    pub(in crate::emacs_core::eval::green) key: HostRequestKey,
    pub(in crate::emacs_core::eval::green) settlement: NativeSettlement,
    receipt: TerminalReceipt,
}

/// Local publication refusal retains the unconsumed scheduler obligation.
pub(in crate::emacs_core::eval::green) struct PublishRefusal {
    pub(in crate::emacs_core::eval::green) reservation: ReservedServiceSlice,
    pub(in crate::emacs_core::eval::green) error: HostRootError,
}

/// Neither the roots-retired receipt nor the observed terminal may disappear
/// when transport metadata retirement refuses its transition.
pub(in crate::emacs_core::eval::green) struct RetirementRefusal {
    pub(in crate::emacs_core::eval::green) roots: RootsRetired,
    pub(in crate::emacs_core::eval::green) terminal: HostTerminal,
    pub(in crate::emacs_core::eval::green) error: HostRootError,
}

pub(in crate::emacs_core::eval::green) enum HostResume {
    Actor(HostRequestKey),
    Refused {
        key: HostRequestKey,
        settlement: NativeSettlement,
    },
}

impl HostResume {
    pub(in crate::emacs_core::eval::green) fn key(&self) -> HostRequestKey {
        match self {
            Self::Actor(key) | Self::Refused { key, .. } => *key,
        }
    }
}

impl ServiceRuntime {
    fn session(&mut self) -> &mut ServiceSession {
        if matches!(self, Self::Dormant) {
            let (client, transport) = transport::channel();
            *self = Self::Open(Box::new(ServiceSession {
                client,
                transport,
                waiters: BTreeMap::new(),
                reservations: BTreeMap::new(),
                deferred_terminals: BTreeMap::new(),
                dispatched: BTreeMap::new(),
                #[cfg(test)]
                driver: FixtureDriver::Unavailable,
                #[cfg(test)]
                completion: FixtureCompletion::Unavailable,
            }));
        }
        match self {
            Self::Open(session) => session,
            Self::Dormant => unreachable_service(),
        }
    }

    pub(in crate::emacs_core::eval::green) fn publish(
        &mut self,
        key: HostRequestKey,
        native: OwnedNativeData,
        reservation: ReservedServiceSlice,
    ) -> Result<(), PublishRefusal> {
        let session = self.session();
        if key != reservation.key() || session.reservations.contains_key(&key) {
            return Err(PublishRefusal {
                reservation,
                error: HostRootError::WrongStage,
            });
        }
        match session.client.publish(key, native) {
            Publication::Delivered(waiter) => {
                session.reservations.insert(key, reservation);
                session.waiters.insert(key, waiter);
                Ok(())
            }
            Publication::ReceiverClosed { .. } => Err(PublishRefusal {
                reservation,
                error: HostRootError::Closed,
            }),
        }
    }

    pub(in crate::emacs_core::eval::green) fn cancel(
        &mut self,
        key: HostRequestKey,
        reason: CancelReason,
    ) -> Result<(), HostRootError> {
        let session = match self {
            Self::Open(session) => session,
            Self::Dormant => return Err(HostRootError::UnknownRequest),
        };
        match session.client.cancel(key, reason) {
            DeliveryStatus::Delivered => Ok(()),
            DeliveryStatus::ReceiverClosed => Err(HostRootError::Closed),
        }
    }

    pub(in crate::emacs_core::eval::green) fn close(&mut self) -> Result<(), HostRootError> {
        let session = match self {
            Self::Open(session) => session,
            Self::Dormant => return Ok(()),
        };
        match session.client.close() {
            DeliveryStatus::Delivered => Ok(()),
            DeliveryStatus::ReceiverClosed => Err(HostRootError::Closed),
        }
    }

    /// Close admission before the quiescent root drain. Undispatched owned
    /// tickets settle without invoking native code; active native work retains
    /// every registry lease until its callback has completed.
    pub(in crate::emacs_core::eval::green) fn prepare_shutdown(
        &mut self,
    ) -> Result<(), HostRootError> {
        let session = match self {
            Self::Dormant => return Ok(()),
            Self::Open(session) => session,
        };
        session.client.close();
        session.transport.drain_commands();
        let preserved = std::mem::take(&mut session.dispatched);
        for (_, dispatch) in preserved {
            let (ticket, native) = dispatch.into_parts();
            drop(native);
            match session
                .transport
                .complete(ticket.settle(NativeOutcome::HostClosed))
            {
                CompletionDisposition::Settled(_) => {}
                CompletionDisposition::Rejected { .. } => return Err(HostRootError::WrongStage),
            }
        }
        match session.transport.close_and_drain() {
            CloseProgress::Drained => Ok(()),
            CloseProgress::AwaitingNativeSettlement(_) => Err(HostRootError::WrongStage),
        }
    }

    /// The caller has successfully drained every host RootBatch under full
    /// collector quiescence. Drop-only native metadata has no callbacks/waits.
    pub(in crate::emacs_core::eval::green) fn finish_shutdown(&mut self) {
        if let Self::Open(session) = self {
            for (_, waiter) in std::mem::take(&mut session.waiters) {
                waiter.finish();
            }
        }
        *self = Self::Dormant;
    }

    pub(in crate::emacs_core::eval::green) fn transfer_plan(
        &self,
        key: HostRequestKey,
        control: &Control,
    ) -> Result<ServiceTransferPlan, HostRootError> {
        let session = match self {
            Self::Open(session) => session,
            Self::Dormant => return Err(HostRootError::UnknownRequest),
        };
        let slice = session
            .reservations
            .get(&key)
            .ok_or(HostRootError::UnknownRequest)?;
        control
            .prepare_service_transfer(slice)
            .map_err(|_| HostRootError::WrongStage)
    }

    fn take_turn(&mut self, key: HostRequestKey) -> Result<NativeTurn, HostRootError> {
        let session = match self {
            Self::Open(session) => session,
            Self::Dormant => return Err(HostRootError::UnknownRequest),
        };
        if !session.reservations.contains_key(&key) {
            return Err(HostRootError::UnknownRequest);
        }
        if session.deferred_terminals.contains_key(&key) {
            let slice = session
                .reservations
                .remove(&key)
                .ok_or(HostRootError::UnknownRequest)?;
            return Ok(NativeTurn::AlreadySettled { slice });
        }
        let dispatch = match session.dispatched.remove(&key) {
            Some(dispatch) => Some(dispatch),
            None => loop {
                match session.transport.dispatch_next() {
                    DispatchPoll::Ready(dispatch) if dispatch.key() == key => break Some(dispatch),
                    DispatchPoll::Ready(dispatch) => {
                        // Preserve another request's linear ticket. No native
                        // callback runs until its own reserved service turn.
                        session.dispatched.insert(dispatch.key(), dispatch);
                    }
                    DispatchPoll::Idle => break None,
                }
            },
        };
        let turn = match dispatch {
            Some(dispatch) => {
                let (ticket, native) = dispatch.into_parts();
                let slice = session
                    .reservations
                    .remove(&key)
                    .ok_or(HostRootError::UnknownRequest)?;
                NativeTurn::Dispatch {
                    slice,
                    ticket,
                    native,
                }
            }
            None => {
                // Cancellation/close may have committed a terminal without a
                // dispatch. It must return the reserved slice without running
                // the native callback or manufacturing a dispatch ticket.
                let terminal = match session.transport.take_terminal(key) {
                    TerminalRead::Ready {
                        receipt,
                        settlement,
                    } => HostTerminal {
                        key,
                        receipt,
                        settlement,
                    },
                    TerminalRead::Pending | TerminalRead::AlreadyObserved => {
                        return Err(HostRootError::WrongStage);
                    }
                    TerminalRead::UnknownRequest => return Err(HostRootError::UnknownRequest),
                };
                session.deferred_terminals.insert(key, terminal);
                let slice = session
                    .reservations
                    .remove(&key)
                    .ok_or(HostRootError::UnknownRequest)?;
                NativeTurn::AlreadySettled { slice }
            }
        };
        Ok(turn)
    }

    fn complete(
        &mut self,
        ticket: DispatchTicket,
        outcome: NativeOutcome,
    ) -> Result<(), HostRootError> {
        let session = match self {
            Self::Open(session) => session,
            Self::Dormant => return Err(HostRootError::UnknownRequest),
        };
        match session.transport.complete(ticket.settle(outcome)) {
            CompletionDisposition::Settled(_) => Ok(()),
            CompletionDisposition::Rejected { .. } => Err(HostRootError::WrongStage),
        }
    }

    pub(super) fn native_observations(&mut self) -> Vec<transport::ControlObservation> {
        let session = match self {
            Self::Dormant => return Vec::new(),
            Self::Open(session) => session,
        };
        session.transport.drain_commands();
        let mut observations = Vec::new();
        while let Some(observation) = session.transport.next_observation() {
            observations.push(observation);
        }
        observations
    }

    pub(in crate::emacs_core::eval::green) fn take_terminal(
        &mut self,
        key: HostRequestKey,
    ) -> Result<HostTerminal, HostRootError> {
        let session = match self {
            Self::Open(session) => session,
            Self::Dormant => return Err(HostRootError::UnknownRequest),
        };
        if let Some(terminal) = session.deferred_terminals.remove(&key) {
            return Ok(terminal);
        }
        match session.transport.take_terminal(key) {
            TerminalRead::Ready {
                receipt,
                settlement,
            } => Ok(HostTerminal {
                key,
                receipt,
                settlement,
            }),
            TerminalRead::Pending | TerminalRead::AlreadyObserved => Err(HostRootError::WrongStage),
            TerminalRead::UnknownRequest => Err(HostRootError::UnknownRequest),
        }
    }

    pub(in crate::emacs_core::eval::green) fn defer_terminal(
        &mut self,
        terminal: HostTerminal,
    ) -> Result<(), HostTerminal> {
        let session = match self {
            Self::Open(session) => session,
            Self::Dormant => return Err(terminal),
        };
        if session.deferred_terminals.contains_key(&terminal.key) {
            return Err(terminal);
        }
        session.deferred_terminals.insert(terminal.key, terminal);
        Ok(())
    }

    pub(in crate::emacs_core::eval::green) fn retire(
        &mut self,
        roots: RootsRetired,
        terminal: HostTerminal,
    ) -> Result<NativeSettlement, RetirementRefusal> {
        let error = if roots.key() != terminal.key {
            Some(HostRootError::ForeignRecord)
        } else if matches!(self, Self::Dormant) {
            Some(HostRootError::UnknownRequest)
        } else {
            None
        };
        if let Some(error) = error {
            return Err(RetirementRefusal {
                roots,
                terminal,
                error,
            });
        }
        let Self::Open(session) = self else {
            unreachable_service()
        };
        let HostTerminal {
            key,
            settlement,
            receipt,
        } = terminal;
        match session.transport.acknowledge(receipt) {
            Acknowledgement::Retired(_) => {
                if let Some(waiter) = session.waiters.remove(&key) {
                    waiter.finish();
                }
                Ok(settlement)
            }
            Acknowledgement::Rejected(receipt) => Err(RetirementRefusal {
                roots,
                terminal: HostTerminal {
                    key,
                    settlement,
                    receipt,
                },
                error: HostRootError::WrongStage,
            }),
        }
    }

    fn suppress_before_transfer(
        &mut self,
        key: HostRequestKey,
    ) -> Result<ReservedServiceSlice, HostRootError> {
        let session = match self {
            Self::Open(session) => session,
            Self::Dormant => return Err(HostRootError::UnknownRequest),
        };
        session.client.cancel(key, CancelReason::SessionClosing);
        session.transport.drain_commands();
        if let Some(dispatch) = session.dispatched.remove(&key) {
            // A ticket preserved from actor ordering has not run native code.
            // Complete that owned obligation explicitly before observing it.
            let (ticket, native) = dispatch.into_parts();
            drop(native);
            match session
                .transport
                .complete(ticket.settle(NativeOutcome::HostClosed))
            {
                CompletionDisposition::Settled(_) => {}
                CompletionDisposition::Rejected { .. } => return Err(HostRootError::WrongStage),
            }
        }
        if session.deferred_terminals.contains_key(&key) {
            return session
                .reservations
                .remove(&key)
                .ok_or(HostRootError::UnknownRequest);
        }
        let terminal = match session.transport.take_terminal(key) {
            TerminalRead::Ready {
                receipt,
                settlement,
            } => HostTerminal {
                key,
                receipt,
                settlement,
            },
            TerminalRead::Pending | TerminalRead::AlreadyObserved => {
                return Err(HostRootError::WrongStage);
            }
            TerminalRead::UnknownRequest => return Err(HostRootError::UnknownRequest),
        };
        session.deferred_terminals.insert(key, terminal);
        session
            .reservations
            .remove(&key)
            .ok_or(HostRootError::UnknownRequest)
    }
}

enum NativeTurn {
    Dispatch {
        slice: ReservedServiceSlice,
        ticket: DispatchTicket,
        native: OwnedNativeData,
    },
    AlreadySettled {
        slice: ReservedServiceSlice,
    },
}

/// Only the parked-main receive path constructs this service capability.
/// The owned Baton is never exposed to the native operation.
pub(in crate::emacs_core::eval::green) struct MainServiceTurn<'receive> {
    key: HostRequestKey,
    baton: Baton,
    control: &'receive Control,
    receiver: &'receive Receiver<Wake>,
    turn: NativeTurn,
}

impl<'receive> MainServiceTurn<'receive> {
    pub(in crate::emacs_core::eval::green) fn from_main_wake(
        key: HostRequestKey,
        baton: Baton,
        control: &'receive Control,
        receiver: &'receive Receiver<Wake>,
    ) -> Self {
        baton.enter();
        let turn = {
            let runtime = baton.green_runtime();
            let Some(runtime) = runtime.as_ref() else {
                unreachable_service()
            };
            let result = runtime.parked.borrow_mut().services.take_turn(key);
            match result {
                Ok(turn) => turn,
                Err(error) => fatal_service(error),
            }
        };
        Self {
            key,
            baton,
            control,
            receiver,
            turn,
        }
    }

    pub(in crate::emacs_core::eval::green) fn serve(
        self,
        serve: impl FnOnce(&mut NativeServiceScope<'_>) -> NativeOutcome,
    ) -> ServedSlice {
        let Self {
            key,
            baton,
            control,
            receiver,
            turn,
        } = self;
        let (baton, slice) = match turn {
            NativeTurn::AlreadySettled { slice } => (baton, slice),
            NativeTurn::Dispatch {
                slice,
                ticket,
                native,
            } => {
                let mut scope = NativeServiceScope {
                    arguments: NativeArguments::Available(native),
                    authority: ServiceAuthority::Held { baton, slice },
                    control,
                    receiver,
                };
                let outcome = serve(&mut scope);
                let (baton, slice) = scope.into_authority();
                {
                    let runtime = baton.green_runtime();
                    let Some(runtime) = runtime.as_ref() else {
                        unreachable_service()
                    };
                    let mut registry = runtime.parked.borrow_mut();
                    if let Err(error) = registry.services.complete(ticket, outcome) {
                        fatal_service(error);
                    }
                }
                (baton, slice)
            }
        };
        finish_native_turn(key, baton, slice)
    }

    /// The owned path returns a pending native record, never a borrowed scope.
    pub(super) fn serve_step(
        self,
        start: impl FnOnce(HostRequestKey, OwnedNativeData) -> NativeAdapterStep,
    ) -> ServiceStep {
        let Self {
            key,
            baton,
            control: _,
            receiver: _,
            turn,
        } = self;
        match turn {
            NativeTurn::AlreadySettled { slice } => {
                ServiceStep::Complete(finish_native_turn(key, baton, slice))
            }
            NativeTurn::Dispatch {
                slice,
                ticket,
                native,
            } => match start(key, native) {
                NativeAdapterStep::Complete(outcome) => {
                    ServiceStep::Complete(complete_native_turn(baton, slice, ticket, outcome))
                }
                NativeAdapterStep::Pending(operation) => ServiceStep::Pending {
                    baton,
                    slice,
                    ticket,
                    operation,
                },
                NativeAdapterStep::RequiresOwner(never) => match never {},
            },
        }
    }
}

pub(super) enum ServiceStep {
    Complete(ServedSlice),
    Pending {
        baton: Baton,
        slice: ReservedServiceSlice,
        ticket: DispatchTicket,
        operation: PendingNativeOperation,
    },
}

pub(super) fn complete_native_turn(
    baton: Baton,
    slice: ReservedServiceSlice,
    ticket: DispatchTicket,
    outcome: NativeOutcome,
) -> ServedSlice {
    let key = slice.key();
    {
        let runtime = baton.green_runtime();
        let Some(runtime) = runtime.as_ref() else {
            unreachable_service()
        };
        let mut registry = runtime.parked.borrow_mut();
        if let Err(error) = registry.services.complete(ticket, outcome) {
            fatal_service(error);
        }
    }
    finish_native_turn(key, baton, slice)
}

fn finish_native_turn(
    key: HostRequestKey,
    baton: Baton,
    slice: ReservedServiceSlice,
) -> ServedSlice {
    {
        let runtime = baton.green_runtime();
        let Some(runtime) = runtime.as_ref() else {
            unreachable_service()
        };
        let mut registry = runtime.parked.borrow_mut();
        match registry.hosts().settle(key) {
            Ok(settled) => drop(settled),
            Err(error) => fatal_service(error),
        }
    }
    ServedSlice { baton, slice }
}

pub(in crate::emacs_core::eval::green) struct ServedSlice {
    baton: Baton,
    slice: ReservedServiceSlice,
}

impl ServedSlice {
    pub(in crate::emacs_core::eval::green) fn return_to_requester(
        self,
        control: &Control,
    ) -> ServiceReturn {
        match control.return_from_service(self.slice, self.baton) {
            Ok(returned) => returned,
            Err(refusal) => {
                tracing::error!(?refusal.reason, "host service lost its scheduler reservation");
                unreachable_service()
            }
        }
    }
}

enum NativeArguments {
    Available(OwnedNativeData),
    Taken,
}
enum ServiceAuthority {
    Held {
        baton: Baton,
        slice: ReservedServiceSlice,
    },
    Released {
        release: NativeRelease,
        slice: ReservedServiceSlice,
    },
    Transition,
}

/// Native data and scheduler-owned channel handles only: no evaluator borrow,
/// Lisp continuation, World view, root reader, or native host TLS recovery.
pub(in crate::emacs_core::eval::green) struct NativeServiceScope<'receive> {
    arguments: NativeArguments,
    authority: ServiceAuthority,
    control: &'receive Control,
    receiver: &'receive Receiver<Wake>,
}

impl NativeServiceScope<'_> {
    pub(in crate::emacs_core::eval::green) fn take_arguments(
        &mut self,
    ) -> Result<OwnedNativeData, HostRootError> {
        match std::mem::replace(&mut self.arguments, NativeArguments::Taken) {
            NativeArguments::Available(data) => Ok(data),
            NativeArguments::Taken => Err(HostRootError::WrongStage),
        }
    }

    fn into_authority(self) -> (Baton, ReservedServiceSlice) {
        match self.authority {
            ServiceAuthority::Held { baton, slice } => (baton, slice),
            ServiceAuthority::Released { .. } | ServiceAuthority::Transition => {
                unreachable_service()
            }
        }
    }

    fn begin_native_release(&mut self, permit: RealWaitPermit) {
        let authority = std::mem::replace(&mut self.authority, ServiceAuthority::Transition);
        let (baton, slice) = match authority {
            ServiceAuthority::Held { baton, slice } => (baton, slice),
            ServiceAuthority::Released { .. } | ServiceAuthority::Transition => {
                unreachable_service()
            }
        };
        let release = match self.control.release_service(&slice, permit, baton) {
            Ok(release) => release,
            Err(refusal) => {
                self.authority = ServiceAuthority::Held {
                    baton: refusal.baton,
                    slice,
                };
                unreachable_service()
            }
        };
        self.authority = ServiceAuthority::Released { release, slice };
    }

    fn finish_native_release(&mut self) {
        let authority = std::mem::replace(&mut self.authority, ServiceAuthority::Transition);
        let (release, slice) = match authority {
            ServiceAuthority::Released { release, slice } => (release, slice),
            ServiceAuthority::Held { .. } | ServiceAuthority::Transition => unreachable_service(),
        };
        let baton = match self
            .control
            .reacquire_service(&slice, release, self.receiver)
        {
            Ok(baton) => baton,
            Err(refusal) => {
                tracing::error!(?refusal.reason, "native service cannot reacquire its Baton");
                unreachable_service()
            }
        };
        self.authority = ServiceAuthority::Held { baton, slice };
    }

    /// Fixtures use the same private physical executor as production. The
    /// forwarding thread owns only native data; no second permit mint exists.
    #[cfg(test)]
    pub(in crate::emacs_core::eval::green) fn receive_native<T: Send + 'static>(
        &mut self,
        receiver: Receiver<T>,
    ) -> Result<T, std::sync::mpsc::RecvError> {
        use crate::host_effect::contracts::ReadDeadline;
        use crate::host_effect::wake::native_wait::PreparedNativeWait;
        use crate::host_effect::wake::{
            ControlDestination, ControlNudge, ControlWake, NativePollOwner,
        };
        let poller = std::sync::Arc::new(polling::Poller::new().unwrap());
        let (wake, publisher) = ControlWake::paired_native(poller).unwrap();
        let mut wait =
            PreparedNativeWait::new(&wake, NativePollOwner::NativeMain, ReadDeadline::Unbounded)
                .unwrap();
        let (completed, completion) = std::sync::mpsc::sync_channel(1);
        let forwarder = std::thread::spawn(move || {
            let result = receiver.recv();
            completed.send(result).unwrap();
            let _ = publisher.nudge(
                ControlDestination::NativeMain,
                ControlNudge::HostServiceReady,
            );
        });
        wait.wait_with(self).unwrap();
        let result = completion
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        forwarder.join().unwrap();
        result
    }
}

impl NativeWaitHandoff for NativeServiceScope<'_> {
    fn release(&mut self, permit: RealWaitPermit) {
        self.begin_native_release(permit);
    }

    fn reacquire(&mut self) {
        self.finish_native_release();
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(in crate::emacs_core::eval::green) struct FixtureGcCompleted {
    pub(in crate::emacs_core::eval::green) worker: std::thread::ThreadId,
}

#[cfg(test)]
enum FixtureCompletion {
    Unavailable,
    Pending {
        sender: std::sync::mpsc::Sender<FixtureGcCompleted>,
        count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    },
    Completed,
}

#[cfg(test)]
pub(in crate::emacs_core::eval::green) enum FixtureDriver {
    Unavailable,
    Installed(Box<dyn FnOnce(&mut NativeServiceScope<'_>) -> NativeOutcome + Send>),
    Owned(super::pump::FixtureNativeDriver),
}

#[cfg(test)]
impl ServiceRuntime {
    pub(in crate::emacs_core::eval::green) fn install_fixture_completion(
        &mut self,
        sender: std::sync::mpsc::Sender<FixtureGcCompleted>,
        count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        self.session().completion = FixtureCompletion::Pending { sender, count };
    }

    /// Called only by the test primitive after a completed exact collection.
    pub(in crate::emacs_core::eval::green) fn finish_fixture_gc(
        &mut self,
    ) -> Result<(), HostRootError> {
        let slot = std::mem::replace(&mut self.session().completion, FixtureCompletion::Completed);
        match slot {
            FixtureCompletion::Pending { sender, count } => {
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                sender
                    .send(FixtureGcCompleted {
                        worker: std::thread::current().id(),
                    })
                    .map_err(|_| HostRootError::Closed)
            }
            FixtureCompletion::Unavailable | FixtureCompletion::Completed => {
                Err(HostRootError::WrongStage)
            }
        }
    }

    pub(in crate::emacs_core::eval::green) fn install_fixture_driver(
        &mut self,
        driver: impl FnOnce(&mut NativeServiceScope<'_>) -> NativeOutcome + Send + 'static,
    ) {
        self.session().driver = FixtureDriver::Installed(Box::new(driver));
    }

    pub(in crate::emacs_core::eval::green) fn install_native_fixture_driver(
        &mut self,
        driver: super::pump::FixtureNativeDriver,
    ) {
        self.session().driver = FixtureDriver::Owned(driver);
    }

    pub(in crate::emacs_core::eval::green) fn take_fixture_driver(&mut self) -> FixtureDriver {
        std::mem::replace(&mut self.session().driver, FixtureDriver::Unavailable)
    }
}

pub(in crate::emacs_core::eval::green) fn serve_main_wake(
    key: HostRequestKey,
    baton: Baton,
    control: &Control,
    receiver: &Receiver<Wake>,
    pump: &mut super::pump::MainNativePump,
) -> ServiceReturn {
    baton.enter();
    #[cfg(test)]
    {
        let driver = {
            let runtime = baton.green_runtime();
            let Some(runtime) = runtime.as_ref() else {
                unreachable_service()
            };
            runtime.parked.borrow_mut().services.take_fixture_driver()
        };
        match driver {
            FixtureDriver::Installed(driver) => {
                let turn = MainServiceTurn::from_main_wake(key, baton, control, receiver);
                return turn.serve(driver).return_to_requester(control);
            }
            FixtureDriver::Owned(driver) => pump.install_fixture_driver(driver),
            FixtureDriver::Unavailable => {}
        }
    }
    pump.serve(key, baton, control, receiver)
}

/// Called only after switch.rs publishes the ordinary parked census. All
/// registry/actor borrows end before a Baton hand-off or channel wait.
pub(in crate::emacs_core::eval::green) fn dispatch_host(
    prepared: PreparedHostCall,
    native: OwnedNativeData,
    baton: Baton,
    control: &Control,
    receiver: &Receiver<Wake>,
) -> (Wake, HostResume) {
    let key = prepared.key();
    let preparation = {
        let runtime = baton.green_runtime();
        let Some(runtime) = runtime.as_ref() else {
            unreachable_service()
        };
        let mut registry = runtime.parked.borrow_mut();
        if let Err(error) = registry.publish_host(prepared) {
            fatal_service(error);
        }
        let reservation = match registry.hosts().registration(key) {
            Ok(registration) => control.reserve_service(registration),
            Err(error) => fatal_service(error),
        };
        match reservation {
            Err(_) => {
                // No scheduler reservation or actor admission exists. Roots
                // still receive a durable terminal before the owner restores.
                match registry.hosts().settle(key) {
                    Ok(settled) => drop(settled),
                    Err(error) => fatal_service(error),
                }
                Err(HostResume::Refused {
                    key,
                    settlement: NativeSettlement::Completed(NativeOutcome::HostClosed),
                })
            }
            Ok(reservation) => match registry.services.publish(key, native, reservation) {
                Err(refusal) => {
                    let settled = match registry.hosts().settle(key) {
                        Ok(settled) => settled,
                        Err(error) => fatal_service(error),
                    };
                    if let Err(refusal) =
                        control.cancel_current_service(refusal.reservation, settled, &baton)
                    {
                        tracing::error!(?refusal.reason, "host publication refusal cannot restore requester");
                        unreachable_service();
                    }
                    Err(HostResume::Refused {
                        key,
                        settlement: NativeSettlement::Completed(NativeOutcome::HostClosed),
                    })
                }
                Ok(()) => match registry.services.transfer_plan(key, control) {
                    Ok(plan) => Ok(plan),
                    Err(_) => {
                        suppress_reserved(&mut registry, key, control, &baton);
                        Err(HostResume::Actor(key))
                    }
                },
            },
        }
    };
    let plan = match preparation {
        Ok(plan) => plan,
        Err(resume) => return (Wake::Resume(baton), resume),
    };
    let transfer = match control.transfer_prepared_service(plan, baton) {
        Ok(ServiceTransfer::Queued(baton)) => {
            // Main may still be inside a real native release. R2 queues until
            // its acquire point; R3 supplies the value-free host-wait nudge.
            let plan = {
                let runtime = baton.green_runtime();
                let Some(runtime) = runtime.as_ref() else {
                    unreachable_service()
                };
                runtime.parked.borrow().services.transfer_plan(key, control)
            };
            match plan {
                Ok(plan) => control.wait_queued_prepared(plan, baton),
                Err(_) => {
                    suppress_with_baton(&baton, key, control);
                    return (Wake::Resume(baton), HostResume::Actor(key));
                }
            }
        }
        other => other,
    };
    match transfer {
        Ok(ServiceTransfer::Dispatched) => {
            let wake = match receiver.recv() {
                Ok(wake) => wake,
                Err(_) => unreachable_service(),
            };
            (wake, HostResume::Actor(key))
        }
        Ok(ServiceTransfer::Queued(baton)) => {
            // wait_queued_prepared returns only dispatched or a refusal.
            // Keep a defensive path that preserves roots and the owned Baton.
            suppress_with_baton(&baton, key, control);
            (Wake::Resume(baton), HostResume::Actor(key))
        }
        Err(refusal) => {
            tracing::debug!(?refusal.reason, "host transfer refused before native dispatch");
            suppress_with_baton(&refusal.baton, key, control);
            (Wake::Resume(refusal.baton), HostResume::Actor(key))
        }
    }
}

fn suppress_with_baton(baton: &Baton, key: HostRequestKey, control: &Control) {
    let runtime = baton.green_runtime();
    let Some(runtime) = runtime.as_ref() else {
        unreachable_service()
    };
    suppress_reserved(&mut runtime.parked.borrow_mut(), key, control, baton);
}

fn suppress_reserved(
    registry: &mut super::WorldRootRegistry,
    key: HostRequestKey,
    control: &Control,
    baton: &Baton,
) {
    let slice = match registry.services.suppress_before_transfer(key) {
        Ok(slice) => slice,
        Err(error) => fatal_service(error),
    };
    let settled = match registry.hosts().settle(key) {
        Ok(settled) => settled,
        Err(error) => fatal_service(error),
    };
    if let Err(refusal) = control.cancel_current_service(slice, settled, baton) {
        tracing::error!(?refusal.reason, "host transfer refusal cannot restore requester");
        unreachable_service();
    }
}

fn fatal_service(error: HostRootError) -> ! {
    tracing::error!(%error, "host service scheduler/root invariant failed");
    unreachable_service()
}

fn unreachable_service() -> ! {
    std::process::abort()
}

static_assertions::assert_not_impl_any!(PublishRefusal: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(RetirementRefusal: Clone, Copy);
static_assertions::assert_not_impl_any!(HostTerminal: Clone, Copy);
static_assertions::assert_not_impl_any!(RealWaitPermit: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(MainServiceTurn<'static>: Send, Sync, Clone, Copy);

static_assertions::assert_not_impl_any!(ServedSlice: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(NativeServiceScope<'static>: Send, Sync, Clone, Copy);
