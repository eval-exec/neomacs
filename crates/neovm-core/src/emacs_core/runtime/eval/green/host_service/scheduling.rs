//! Cold native service turns. No Context/World view crosses a native callback.

use super::super::super::world::Baton;
use super::super::scheduler::{
    Control, EndedServiceOwner, NativeRelease, OwnerResumePlan, ReservedServiceSlice,
    ServiceRefusal, ServiceReturn, ServiceTransfer, ServiceTransferPlan, ThreadKey, Wake,
};
use super::owner_steps::StepKey;
use super::transport::{
    self, Acknowledgement, CancelReason, CloseProgress, CompletionDisposition, DeliveryStatus,
    DispatchPoll, DispatchTicket, HostClient, HostWaiter, NativeDispatch, NativeOutcome,
    NativeSettlement, Publication, ServiceTransport, TerminalRead, TerminalReceipt,
};
use super::{HostRequestKey, HostRootError, PreparedHostCall, RootsRetired, SettledHostCall};
use crate::host_effect::contracts::OwnedNativeData;
use crate::host_effect::native_pending::{NativeAdapterStep, PendingNativeOperation};
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::mpsc::Receiver;

use crate::host_effect::wake::native_wait::NativeWaitHandoff;
pub(in crate::emacs_core::eval::green) use crate::host_effect::wake::native_wait::RealWaitPermit;

impl super::pending::ContinuationReservation for ReservedServiceSlice {
    fn continuation_request(&self) -> HostRequestKey {
        self.key()
    }
}

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
    /// Saved linear service authority while its original owner executes.
    /// Native affine attempt storage remains in main's receive-owned pump.
    owner_continuations: BTreeMap<HostRequestKey, OwnerContinuation>,
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

pub(super) struct OwnerContinuation {
    pub(super) step: StepKey,
    pub(super) slice: ReservedServiceSlice,
    pub(super) ticket: DispatchTicket,
}

/// Issued only after consuming the actor's actual dispatch completion. A key
/// or cancellation hint cannot authorize revoking a live root obligation.
#[must_use]
pub(super) struct NativeCleanupSettled {
    key: HostRequestKey,
}

impl NativeCleanupSettled {
    pub(super) fn key(&self) -> HostRequestKey {
        self.key
    }
}

/// Linear proof that admission/transport reached a terminal before any native
/// callback ran. Construction stays private to those authoritative outcomes.
#[must_use]
pub(super) struct UnstartedHostCall {
    key: HostRequestKey,
}

impl UnstartedHostCall {
    pub(super) fn key(&self) -> HostRequestKey {
        self.key
    }
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
                owner_continuations: BTreeMap::new(),
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
            return Ok(NativeTurn::AlreadySettled {
                slice,
                unstarted: UnstartedHostCall { key },
            });
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
                NativeTurn::AlreadySettled {
                    slice,
                    unstarted: UnstartedHostCall { key },
                }
            }
        };
        Ok(turn)
    }

    fn complete(
        &mut self,
        ticket: DispatchTicket,
        outcome: NativeOutcome,
    ) -> Result<NativeCleanupSettled, HostRootError> {
        let session = match self {
            Self::Open(session) => session,
            Self::Dormant => return Err(HostRootError::UnknownRequest),
        };
        match session.transport.complete(ticket.settle(outcome)) {
            CompletionDisposition::Settled(key) => Ok(NativeCleanupSettled { key }),
            CompletionDisposition::Rejected { .. } => Err(HostRootError::WrongStage),
        }
    }

    pub(super) fn save_owner_continuation(
        &mut self,
        continuation: OwnerContinuation,
    ) -> Result<(), OwnerContinuation> {
        let Self::Open(session) = self else {
            return Err(continuation);
        };
        let key = continuation.step.parent();
        if key != continuation.slice.key() || session.owner_continuations.contains_key(&key) {
            return Err(continuation);
        }
        session.owner_continuations.insert(key, continuation);
        Ok(())
    }

    pub(super) fn take_owner_continuation(
        &mut self,
        parent: HostRequestKey,
    ) -> Result<OwnerContinuation, HostRootError> {
        let Self::Open(session) = self else {
            return Err(HostRootError::UnknownRequest);
        };
        session
            .owner_continuations
            .remove(&parent)
            .ok_or(HostRootError::UnknownRequest)
    }

    pub(super) fn owner_continuation_keys(&self) -> Vec<HostRequestKey> {
        match self {
            Self::Dormant => Vec::new(),
            Self::Open(session) => session.owner_continuations.keys().copied().collect(),
        }
    }

    /// Consume a saved continuation only with its owning scheduler's explicit
    /// exit proof. An unknown/live requester retains all linear obligations.
    pub(super) fn take_ended_owner_continuation(
        &mut self,
        control: &Control,
        baton: &Baton,
    ) -> Option<(OwnerContinuation, EndedServiceOwner)> {
        let Self::Open(session) = self else {
            return None;
        };
        let (key, ended) = session
            .owner_continuations
            .iter()
            .find_map(|(key, saved)| {
                control
                    .ended_service_owner(&saved.slice, baton)
                    .ok()
                    .map(|ended| (*key, ended))
            })?;
        let saved = session.owner_continuations.remove(&key)?;
        Some((saved, ended))
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
        unstarted: UnstartedHostCall,
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
            key: _,
            baton,
            control,
            receiver,
            turn,
        } = self;
        match turn {
            NativeTurn::AlreadySettled { slice, unstarted } => {
                finish_unstarted_native_turn(baton, slice, unstarted)
            }
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
                complete_native_turn(baton, slice, ticket, outcome)
            }
        }
    }

    /// The owned path returns a pending native record, never a borrowed scope.
    pub(super) fn serve_step(
        self,
        start: impl FnOnce(HostRequestKey, OwnedNativeData) -> NativeAdapterStep<StepKey>,
    ) -> ServiceStep {
        let Self {
            key,
            baton,
            control: _,
            receiver: _,
            turn,
        } = self;
        match turn {
            NativeTurn::AlreadySettled { slice, unstarted } => {
                ServiceStep::Complete(finish_unstarted_native_turn(baton, slice, unstarted))
            }
            NativeTurn::Dispatch {
                slice,
                ticket,
                native,
            } => advance_native_step(baton, slice, ticket, start(key, native)),
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
    Owner {
        baton: Baton,
        continuation: OwnerContinuation,
    },
}

/// One classification for initial dispatch and every physical/owner resume.
/// Only the Complete arm consumes the dispatch ticket and settles the parent.
pub(super) fn advance_native_step(
    baton: Baton,
    slice: ReservedServiceSlice,
    ticket: DispatchTicket,
    step: NativeAdapterStep<StepKey>,
) -> ServiceStep {
    match step {
        NativeAdapterStep::Complete(outcome) => {
            ServiceStep::Complete(complete_native_turn(baton, slice, ticket, outcome))
        }
        NativeAdapterStep::Pending(operation) => ServiceStep::Pending {
            baton,
            slice,
            ticket,
            operation,
        },
        NativeAdapterStep::RequiresOwner(step) => {
            if step.parent() != slice.key() || step.owner() != slice.requester() {
                unreachable_service();
            }
            ServiceStep::Owner {
                baton,
                continuation: OwnerContinuation {
                    step,
                    slice,
                    ticket,
                },
            }
        }
    }
}

pub(super) fn complete_native_turn(
    baton: Baton,
    slice: ReservedServiceSlice,
    ticket: DispatchTicket,
    outcome: NativeOutcome,
) -> ServedSlice {
    drop(settle_native_dispatch(&baton, &slice, ticket, outcome));
    ServedSlice { baton, slice }
}

/// One settlement rule for live and ended continuations. The consuming receipt
/// chooses the scheduler return path; neither path can bypass actor completion.
fn settle_native_dispatch(
    baton: &Baton,
    slice: &ReservedServiceSlice,
    ticket: DispatchTicket,
    outcome: NativeOutcome,
) -> SettledHostCall {
    let runtime = baton.green_runtime();
    let Some(runtime) = runtime.as_ref() else {
        unreachable_service()
    };
    let mut registry = runtime.parked.borrow_mut();
    let settled = match registry.services.complete(ticket, outcome) {
        Ok(settled) => settled,
        Err(error) => fatal_service(error),
    };
    if settled.key() != slice.key()
        || registry
            .hosts()
            .finish_native_owner_step(baton, settled)
            .is_err()
    {
        unreachable_service();
    }
    match registry.hosts().settle(slice.key()) {
        Ok(settled) => settled,
        Err(error) => fatal_service(error),
    }
}

/// Dead-owner cleanup has no requester return or acknowledgement. The actual
/// actor settlement consumes revocation first; roots remain for quiescent drain.
pub(super) fn complete_ended_native_turn(
    baton: Baton,
    slice: ReservedServiceSlice,
    ticket: DispatchTicket,
    ended: EndedServiceOwner,
    outcome: NativeOutcome,
    control: &Control,
) -> Baton {
    if ended.key() != slice.key() || ended.requester() != slice.requester() {
        unreachable_service();
    }
    let settled = settle_native_dispatch(&baton, &slice, ticket, outcome);
    if control
        .finish_ended_continuation(slice, settled, &baton)
        .is_err()
    {
        unreachable_service();
    }
    baton
}

/// Called only after the requester has canonically republished its newest
/// owner image. Validation borrows end before the scheduler can wait or send.
pub(in crate::emacs_core::eval::green) fn resume_owner_step(
    parent: HostRequestKey,
    baton: Baton,
    control: &Control,
    receiver: &Receiver<Wake>,
) -> Wake {
    enum Route {
        Local(OwnerResumePlan),
        Routed(OwnerResumePlan),
    }
    let route = {
        let runtime = baton.green_runtime();
        let Some(runtime) = runtime.as_ref() else {
            unreachable_service()
        };
        let registry = runtime.parked.borrow();
        let ServiceRuntime::Open(session) = &registry.services else {
            unreachable_service()
        };
        let Some(saved) = session.owner_continuations.get(&parent) else {
            unreachable_service()
        };
        if registry
            .hosts
            .validate_owner_completion(&baton, saved.step)
            .is_err()
        {
            unreachable_service();
        }
        let plan = match control.prepare_owner_resume(&saved.slice) {
            Ok(plan) => plan,
            Err(_) => unreachable_service(),
        };
        match saved.slice.requester() {
            super::super::scheduler::ThreadKey::Main => Route::Local(plan),
            super::super::scheduler::ThreadKey::Worker(_) => Route::Routed(plan),
        }
    };
    match route {
        Route::Local(plan) => match control.resume_prepared_local_owner_step(plan, baton) {
            Ok(wake) => wake,
            Err(_) => unreachable_service(),
        },
        Route::Routed(plan) => {
            match control.resume_prepared_owner_step(plan, baton) {
                Ok(ServiceTransfer::Dispatched) => {}
                Ok(ServiceTransfer::Queued(_)) | Err(_) => unreachable_service(),
            }
            match receiver.recv() {
                Ok(wake) => wake,
                Err(_) => unreachable_service(),
            }
        }
    }
}

fn settle_unstarted_host_call(
    registry: &mut super::WorldRootRegistry,
    baton: &Baton,
    unstarted: UnstartedHostCall,
) -> SettledHostCall {
    let key = unstarted.key();
    if registry
        .hosts()
        .finish_unstarted_owner_step(baton, unstarted)
        .is_err()
    {
        unreachable_service();
    }
    match registry.hosts().settle(key) {
        Ok(settled) => settled,
        Err(error) => fatal_service(error),
    }
}

fn finish_unstarted_native_turn(
    baton: Baton,
    slice: ReservedServiceSlice,
    unstarted: UnstartedHostCall,
) -> ServedSlice {
    if slice.key() != unstarted.key() {
        unreachable_service();
    }
    {
        let runtime = baton.green_runtime();
        let Some(runtime) = runtime.as_ref() else {
            unreachable_service()
        };
        drop(settle_unstarted_host_call(
            &mut runtime.parked.borrow_mut(),
            &baton,
            unstarted,
        ));
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
        let returned = if self.slice.requester() == super::super::scheduler::ThreadKey::Main {
            control.return_from_local_service(self.slice, self.baton)
        } else {
            control.return_from_service(self.slice, self.baton)
        };
        match returned {
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
    Semantic(super::pump::FixtureSemanticDriver),
    SemanticCleanup {
        driver: super::pump::FixtureSemanticDriver,
        cleanup: super::pump::FixtureCleanupDriver,
    },
}

#[cfg(test)]
impl ServiceRuntime {
    /// Preserve an actual actor-issued dispatch and waiter for lifecycle controls.
    pub(super) fn native_dispatch_for_test(
        &mut self,
        key: HostRequestKey,
    ) -> (DispatchTicket, HostWaiter) {
        let session = self.session();
        let Publication::Delivered(waiter) = session.client.publish(key, OwnedNativeData::None)
        else {
            panic!("test actor publication was closed")
        };
        session.transport.drain_commands();
        let DispatchPoll::Ready(dispatch) = session.transport.dispatch_next() else {
            panic!("test actor did not dispatch its publication")
        };
        let (ticket, native) = dispatch.into_parts();
        drop(native);
        (ticket, waiter)
    }

    pub(super) fn complete_native_dispatch_for_test(
        &mut self,
        ticket: DispatchTicket,
        outcome: NativeOutcome,
    ) -> NativeCleanupSettled {
        self.complete(ticket, outcome).unwrap()
    }

    /// Exercise actual actor publication, dispatch and completion rather than
    /// constructing the native-settlement proof from a correlation key.
    pub(super) fn native_cleanup_settled_for_test(
        &mut self,
        key: HostRequestKey,
    ) -> NativeCleanupSettled {
        let (ticket, waiter) = self.native_dispatch_for_test(key);
        let settled = self.complete_native_dispatch_for_test(ticket, NativeOutcome::HostClosed);
        waiter.finish();
        settled
    }

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

    pub(in crate::emacs_core::eval::green) fn install_semantic_fixture_driver(
        &mut self,
        driver: super::pump::FixtureSemanticDriver,
    ) {
        self.session().driver = FixtureDriver::Semantic(driver);
    }

    pub(super) fn install_cleanup_fixture_driver(
        &mut self,
        driver: super::pump::FixtureSemanticDriver,
        cleanup: super::pump::FixtureCleanupDriver,
    ) {
        self.session().driver = FixtureDriver::SemanticCleanup { driver, cleanup };
    }

    /// Fault injection retires the actual publication receiver before any
    /// request exists. The stored client then reports ReceiverClosed naturally.
    pub(super) fn retire_publication_receiver_for_test(&mut self) {
        let session = self.session();
        assert!(session.reservations.is_empty());
        assert!(session.waiters.is_empty());
        assert!(session.deferred_terminals.is_empty());
        assert!(session.dispatched.is_empty());
        assert!(session.owner_continuations.is_empty());
        let (_, replacement) = transport::channel();
        session.transport = replacement;
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
            FixtureDriver::Semantic(driver) => pump.install_semantic_fixture_driver(driver),
            FixtureDriver::SemanticCleanup { driver, cleanup } => {
                pump.install_cleanup_fixture_driver(driver, cleanup)
            }
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
            Ok(registration) => match registration.owner() {
                ThreadKey::Main => control.reserve_local_service(registration),
                ThreadKey::Worker(_) => control.reserve_service(registration),
            },
            Err(error) => fatal_service(error),
        };
        match reservation {
            Err(ServiceRefusal::DuplicateRequest) => unreachable_service(),
            Err(
                ServiceRefusal::Closing
                | ServiceRefusal::MainUnavailable
                | ServiceRefusal::MainIsRequester
                | ServiceRefusal::RequesterUnavailable
                | ServiceRefusal::WrongServiceState
                | ServiceRefusal::WrongMainThread,
            ) => {
                // No scheduler reservation or actor admission exists. Roots
                // still receive a durable terminal before the owner restores.
                drop(settle_unstarted_host_call(
                    &mut registry,
                    &baton,
                    UnstartedHostCall { key },
                ));
                Err(HostResume::Refused {
                    key,
                    settlement: NativeSettlement::Completed(NativeOutcome::HostClosed),
                })
            }
            Ok(reservation) => match registry.services.publish(key, native, reservation) {
                Err(refusal) => {
                    if !matches!(refusal.error, HostRootError::Closed) {
                        fatal_service(refusal.error);
                    }
                    let settled = settle_unstarted_host_call(
                        &mut registry,
                        &baton,
                        UnstartedHostCall { key },
                    );
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
    // The typed plan is consumed after the registry borrow has ended. Main's
    // local route enters the same native receive driver without a self-send.
    match plan.requester() {
        ThreadKey::Main => {
            return match control.transfer_prepared_local_service(plan, baton) {
                Ok(wake) => (wake, HostResume::Actor(key)),
                Err(refusal) => {
                    tracing::debug!(?refusal.reason, "local host transfer refused before native dispatch");
                    suppress_with_baton(&refusal.baton, key, control);
                    (Wake::Resume(refusal.baton), HostResume::Actor(key))
                }
            };
        }
        ThreadKey::Worker(_) => {}
    }
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
    let settled = settle_unstarted_host_call(registry, baton, UnstartedHostCall { key });
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

static_assertions::assert_not_impl_any!(UnstartedHostCall: Clone, Copy);
static_assertions::assert_not_impl_any!(PublishRefusal: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(RetirementRefusal: Clone, Copy);
static_assertions::assert_not_impl_any!(HostTerminal: Clone, Copy);
static_assertions::assert_not_impl_any!(RealWaitPermit: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(MainServiceTurn<'static>: Send, Sync, Clone, Copy);

static_assertions::assert_not_impl_any!(ServedSlice: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(NativeServiceScope<'static>: Send, Sync, Clone, Copy);

#[cfg(test)]
#[path = "tests/unstarted.rs"]
mod unstarted_tests;

#[cfg(test)]
#[path = "tests/local_main.rs"]
mod local_main_tests;
