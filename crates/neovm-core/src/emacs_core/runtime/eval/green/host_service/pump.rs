//! Main-only owned native progress. No owner or root-reader borrow is suspended.
//!
//! This is the Baton-serial E5 `NativeWaitPolicy::Released` mechanism: prepare
//! owned native interests, block through the single lower executor, then drive
//! owned observations. A control wake never identifies a completed request.

use super::super::super::world::Baton;
use super::super::scheduler::{
    Control, EndedServiceOwner, IntermediateReturn, NativePumpPurpose, NativeRelease, NativeWork,
    Peers, ReservedServiceSlice, ServiceReturn, ThreadKey, Wake,
};
use super::HostRequestKey;
#[cfg(test)]
use super::owner_steps::{OwnerInterruption, OwnerNextStep};
use super::owner_steps::{OwnerLocalFlows, OwnerStepError, OwnerStepReady, StepKey};
use super::pending::{
    ContinuationInsertion, NativeDrainProgress, NativePendingCalls, NativeReadyOutcome,
    PendingReservation, ReadyPending,
};
use super::scheduling::{
    MainServiceTurn, OwnerContinuation, ServiceStep, advance_native_step,
    complete_ended_native_turn,
};
use super::transport::{CancelResolution, ControlObservation, DispatchTicket, NativeOutcome};
use crate::host_effect::contracts::{OwnedNativeData, ReadDeadline};
use crate::host_effect::native_pending::{NativeAdapterStep, NativeCompletionRoute};
use crate::host_effect::owner_step::OwnerStepCompletion;
use crate::host_effect::wake::native_wait::{
    NativeWaitContinuation, NativeWaitHandoff, NativeWaitPreparationError, PreparedNativeWait,
    RealWaitPermit,
};
use crate::host_effect::wake::{ControlWake, ControlWakeOutcome, NativePollOwner};
use std::collections::{BTreeMap, VecDeque};
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

/// Test-only semantic backend events. Their completions contain registered
/// metadata only; the original !Send Flow remains with its requester.
#[cfg(test)]
pub(in crate::emacs_core::eval::green) enum FixtureSemanticEvent {
    Start {
        key: HostRequestKey,
        native: OwnedNativeData,
    },
    Native {
        key: HostRequestKey,
        outcome: NativeOutcome,
    },
    Owner {
        key: HostRequestKey,
        completion: OwnerStepCompletion<'static, ThreadKey>,
        next: OwnerNextStep,
        interruption: OwnerInterruption,
    },
}

#[cfg(test)]
pub(in crate::emacs_core::eval::green) type FixtureSemanticDriver = Box<
    dyn FnMut(FixtureSemanticEvent, NativeCompletionRoute) -> NativeAdapterStep<StepKey> + Send,
>;

/// Cleanup callbacks can produce native work only: their reply type has no
/// inhabited RequiresOwner arm. They never materialize a Lisp value.
#[cfg(test)]
pub(super) enum FixtureCleanupEvent {
    OwnerEnded {
        key: HostRequestKey,
        step: StepKey,
    },
    Native {
        key: HostRequestKey,
        outcome: NativeOutcome,
    },
}

#[cfg(test)]
pub(super) type FixtureCleanupDriver =
    Box<dyn FnMut(FixtureCleanupEvent, NativeCompletionRoute) -> NativeAdapterStep + Send>;

#[derive(Clone, Copy)]
enum SemanticContinuation {
    NativeOnly,
    Semantic,
}

enum NativeBackend {
    Closed,
    #[cfg(test)]
    Fixture(FixtureNativeDriver),
    #[cfg(test)]
    Semantic(FixtureSemanticDriver),
    #[cfg(test)]
    SemanticCleanup {
        driver: FixtureSemanticDriver,
        cleanup: FixtureCleanupDriver,
    },
}

impl NativeBackend {
    fn start(
        &mut self,
        key: HostRequestKey,
        native: OwnedNativeData,
        route: NativeCompletionRoute,
    ) -> NativeAdapterStep<StepKey> {
        match self {
            Self::Closed => {
                let _ = (key, native, route);
                NativeAdapterStep::Complete(NativeOutcome::HostClosed)
            }
            #[cfg(test)]
            Self::Fixture(driver) => match driver(key, native, route) {
                NativeAdapterStep::Complete(outcome) => NativeAdapterStep::Complete(outcome),
                NativeAdapterStep::Pending(operation) => NativeAdapterStep::Pending(operation),
                NativeAdapterStep::RequiresOwner(never) => match never {},
            },
            #[cfg(test)]
            Self::Semantic(driver) => driver(FixtureSemanticEvent::Start { key, native }, route),
            #[cfg(test)]
            Self::SemanticCleanup { driver, .. } => {
                driver(FixtureSemanticEvent::Start { key, native }, route)
            }
        }
    }

    fn continuation(&self) -> SemanticContinuation {
        match self {
            #[cfg(test)]
            Self::Semantic(_) | Self::SemanticCleanup { .. } => SemanticContinuation::Semantic,
            Self::Closed => SemanticContinuation::NativeOnly,
            #[cfg(test)]
            Self::Fixture(_) => SemanticContinuation::NativeOnly,
        }
    }

    fn native_complete(
        &mut self,
        key: HostRequestKey,
        outcome: NativeOutcome,
        continuation: SemanticContinuation,
        route: NativeCompletionRoute,
    ) -> NativeAdapterStep<StepKey> {
        match continuation {
            SemanticContinuation::NativeOnly => NativeAdapterStep::Complete(outcome),
            SemanticContinuation::Semantic => match self {
                #[cfg(test)]
                Self::Semantic(driver) | Self::SemanticCleanup { driver, .. } => {
                    driver(FixtureSemanticEvent::Native { key, outcome }, route)
                }
                Self::Closed => {
                    let _ = (key, outcome, route);
                    fatal_pump()
                }
                #[cfg(test)]
                Self::Fixture(_) => fatal_pump(),
            },
        }
    }

    fn owner_complete(
        &mut self,
        key: HostRequestKey,
        ready: OwnerStepReady,
        route: NativeCompletionRoute,
    ) -> NativeAdapterStep<StepKey> {
        match self {
            #[cfg(test)]
            Self::Semantic(driver) | Self::SemanticCleanup { driver, .. } => driver(
                FixtureSemanticEvent::Owner {
                    key,
                    completion: ready.completion,
                    next: ready.next,
                    interruption: ready.interruption,
                },
                route,
            ),
            Self::Closed => {
                let _ = (key, ready, route);
                fatal_pump()
            }
            #[cfg(test)]
            Self::Fixture(_) => fatal_pump(),
        }
    }

    fn owner_ended(&mut self, step: StepKey, route: NativeCompletionRoute) -> NativeAdapterStep {
        match self {
            #[cfg(test)]
            Self::SemanticCleanup { cleanup, .. } => cleanup(
                FixtureCleanupEvent::OwnerEnded {
                    key: step.parent(),
                    step,
                },
                route,
            ),
            Self::Closed => {
                let _ = (step, route);
                fatal_pump()
            }
            #[cfg(test)]
            Self::Fixture(_) | Self::Semantic(_) => fatal_pump(),
        }
    }

    fn cleanup_complete(
        &mut self,
        key: HostRequestKey,
        outcome: NativeOutcome,
        route: NativeCompletionRoute,
    ) -> NativeAdapterStep {
        match self {
            #[cfg(test)]
            Self::SemanticCleanup { cleanup, .. } => {
                cleanup(FixtureCleanupEvent::Native { key, outcome }, route)
            }
            Self::Closed => {
                let _ = (key, outcome, route);
                fatal_pump()
            }
            #[cfg(test)]
            Self::Fixture(_) | Self::Semantic(_) => fatal_pump(),
        }
    }
}

enum PendingContinuation {
    Requester(SemanticContinuation),
    Ended(EndedServiceOwner),
}

struct PendingCallReservation {
    slice: ReservedServiceSlice,
    ticket: DispatchTicket,
    continuation: PendingContinuation,
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
    owner_attempts: BTreeMap<HostRequestKey, SemanticContinuation>,
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
            owner_attempts: BTreeMap::new(),
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

    #[cfg(test)]
    pub(super) fn install_semantic_fixture_driver(&mut self, driver: FixtureSemanticDriver) {
        self.backend = NativeBackend::Semantic(driver);
    }

    #[cfg(test)]
    pub(super) fn install_cleanup_fixture_driver(
        &mut self,
        driver: FixtureSemanticDriver,
        cleanup: FixtureCleanupDriver,
    ) {
        self.backend = NativeBackend::SemanticCleanup { driver, cleanup };
    }

    pub(super) fn close(&mut self) -> MainNativeDrain {
        // Dropping an owned backend commits actual producer abandonment before
        // its wake. Externally running operations remain until they acknowledge.
        // Admitted semantic attempts may still require affine cleanup. Retain
        // their backend until every native/owner continuation has settled.
        match self.backend {
            #[cfg(test)]
            NativeBackend::Semantic(_) | NativeBackend::SemanticCleanup { .. } => {}
            NativeBackend::Closed => {}
            #[cfg(test)]
            NativeBackend::Fixture(_) => self.backend = NativeBackend::Closed,
        }
        self.pending.close();
        if self.lifecycle == PumpLifecycle::Open {
            self.lifecycle = PumpLifecycle::CloseRequested;
        }
        self.drain_progress()
    }

    fn drain_progress(&self) -> MainNativeDrain {
        if !self.owner_attempts.is_empty() {
            return MainNativeDrain::Pending;
        }
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
            let mut registry = runtime.parked.borrow_mut();
            let _ = registry.services.close();
            if registry.hosts().begin_closing(baton).is_err() {
                fatal_pump();
            }
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

    fn continue_owner_step(
        &mut self,
        parent: HostRequestKey,
        baton: Baton,
        control: &Control,
        receiver: &Receiver<Wake>,
    ) -> ServiceReturn {
        self.run(DriverTurn::Continue { parent, baton }, control, receiver)
    }

    fn route(&self, control: &Control) -> NativeCompletionRoute {
        let source = match control.native_pump_source() {
            Ok(source) => source,
            Err(_) => fatal_pump(),
        };
        NativeCompletionRoute::new(source.handle(), self.settlement.handle())
    }

    fn advance(
        &mut self,
        step: ServiceStep,
        continuation: SemanticContinuation,
        origin: PendingOrigin,
        control: &Control,
    ) -> PumpProgress {
        match step {
            ServiceStep::Complete(served) => {
                PumpProgress::Returned(served.return_to_requester(control))
            }
            ServiceStep::Pending {
                baton,
                slice,
                ticket,
                operation,
            } => {
                let admission = ContinuationInsertion::from_service(&slice);
                let baton = match control.defer_native_service(&slice, baton) {
                    Ok(baton) => baton,
                    Err(_) => fatal_pump(),
                };
                let reservation = PendingCallReservation {
                    slice,
                    ticket,
                    continuation: PendingContinuation::Requester(continuation),
                };
                let inserted = match origin {
                    PendingOrigin::New => self.pending.insert(operation, reservation),
                    PendingOrigin::Continuation => {
                        self.pending
                            .insert_continuation(operation, reservation, admission)
                    }
                };
                if let Err(rejected) = inserted {
                    let _preserved = rejected.into_parts();
                    fatal_pump();
                }
                PumpProgress::Driving(baton)
            }
            ServiceStep::Owner {
                baton,
                continuation: saved,
            } => {
                let parent = saved.step.parent();
                self.close_admission(&baton);
                self.observe_controls(&baton);
                let plan = match control.prepare_intermediate_return(&saved.slice) {
                    Ok(plan) => plan,
                    Err(_) => fatal_pump(),
                };
                {
                    let runtime = baton.green_runtime();
                    let Some(runtime) = runtime.as_ref() else {
                        fatal_pump()
                    };
                    let mut registry = runtime.parked.borrow_mut();
                    if registry
                        .hosts()
                        .publish_owner_step(&baton, saved.step)
                        .is_err()
                    {
                        fatal_pump();
                    }
                    if let Err(preserved) = registry.services.save_owner_continuation(saved) {
                        let _preserved = preserved;
                        fatal_pump();
                    }
                }
                if self.owner_attempts.insert(parent, continuation).is_some() {
                    fatal_pump();
                }
                // Registry borrows ended and both linear obligations are saved
                // before a peer can receive the unique Baton.
                match control.return_prepared_intermediate(plan, baton) {
                    Ok(IntermediateReturn::Sent) => PumpProgress::Returned(ServiceReturn::Sent),
                    Ok(IntermediateReturn::LocalMain { key, baton }) => {
                        PumpProgress::Returned(ServiceReturn::ResumeOwnerStep { key, baton })
                    }
                    Err(_) => fatal_pump(),
                }
            }
        }
    }

    fn take_ended_owner_step(
        &mut self,
        baton: &Baton,
        control: &Control,
    ) -> Option<(OwnerContinuation, EndedServiceOwner)> {
        let runtime = baton.green_runtime();
        let Some(runtime) = runtime.as_ref() else {
            fatal_pump()
        };
        let mut registry = runtime.parked.borrow_mut();
        let (saved, ended) = registry
            .services
            .take_ended_owner_continuation(control, baton)?;
        if registry
            .hosts()
            .revoke_owner_step(baton, &ended, saved.step)
            .is_err()
        {
            fatal_pump();
        }
        if self.owner_attempts.remove(&saved.step.parent()).is_none() {
            fatal_pump();
        }
        Some((saved, ended))
    }

    /// The ended route retains its exact exit proof and dispatch through every
    /// physical continuation. It cannot enter the live-owner service transitions.
    fn advance_ended(
        &mut self,
        baton: Baton,
        slice: ReservedServiceSlice,
        ticket: DispatchTicket,
        ended: EndedServiceOwner,
        step: NativeAdapterStep,
        control: &Control,
    ) -> Baton {
        match step {
            NativeAdapterStep::Complete(outcome) => {
                complete_ended_native_turn(baton, slice, ticket, ended, outcome, control)
            }
            NativeAdapterStep::Pending(operation) => {
                let admission = ContinuationInsertion::from_service(&slice);
                let reservation = PendingCallReservation {
                    slice,
                    ticket,
                    continuation: PendingContinuation::Ended(ended),
                };
                if let Err(rejected) =
                    self.pending
                        .insert_continuation(operation, reservation, admission)
                {
                    let _preserved = rejected.into_parts();
                    fatal_pump();
                }
                baton
            }
            NativeAdapterStep::RequiresOwner(never) => match never {},
        }
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
                    let route = self.route(control);
                    let continuation = self.backend.continuation();
                    let step =
                        service.serve_step(|key, native| self.backend.start(key, native, route));
                    match self.advance(step, continuation, PendingOrigin::New, control) {
                        PumpProgress::Returned(returned) => return returned,
                        PumpProgress::Driving(baton) => baton,
                    }
                }
                DriverTurn::Continue { parent, baton } => {
                    baton.enter();
                    // Read the actor's durable intent before resuming semantic
                    // native work; that work may finish without another loop.
                    self.close_admission(&baton);
                    self.observe_controls(&baton);
                    let (saved, completion) = {
                        let runtime = baton.green_runtime();
                        let Some(runtime) = runtime.as_ref() else {
                            fatal_pump()
                        };
                        let mut registry = runtime.parked.borrow_mut();
                        let saved = match registry.services.take_owner_continuation(parent) {
                            Ok(saved) => saved,
                            Err(_) => fatal_pump(),
                        };
                        let completion =
                            match registry.hosts().take_owner_completion(&baton, saved.step) {
                                Ok(completion) => completion,
                                Err(_) => fatal_pump(),
                            };
                        (saved, completion)
                    };
                    let Some(continuation) = self.owner_attempts.remove(&parent) else {
                        fatal_pump()
                    };
                    let route = self.route(control);
                    let step = self.backend.owner_complete(parent, completion, route);
                    let step = advance_native_step(baton, saved.slice, saved.ticket, step);
                    match self.advance(step, continuation, PendingOrigin::Continuation, control) {
                        PumpProgress::Returned(returned) => return returned,
                        PumpProgress::Driving(baton) => baton,
                    }
                }
                DriverTurn::Drive(baton) => baton,
            };
            baton.enter();
            self.close_admission(&baton);
            self.observe_controls(&baton);
            if let Some((saved, ended)) = self.take_ended_owner_step(&baton, control) {
                // The registry borrow and all root readers have ended. Only
                // owned native data and a sealed exit proof cross this call.
                let route = self.route(control);
                let step = self.backend.owner_ended(saved.step, route);
                let baton =
                    self.advance_ended(baton, saved.slice, saved.ticket, ended, step, control);
                turn = DriverTurn::Drive(baton);
                continue;
            }
            self.pending.expire_deadlines(Instant::now());
            self.ready.extend(self.pending.observe_ready());
            if let Some(ready) = self.ready.pop_front() {
                let (key, outcome, reservation) = ready.into_parts();
                let PendingCallReservation {
                    slice,
                    ticket,
                    continuation,
                } = reservation;
                if key != slice.key() {
                    fatal_pump();
                }
                let outcome = match outcome {
                    NativeReadyOutcome::Completed(outcome) => outcome,
                    NativeReadyOutcome::ProducerClosed => NativeOutcome::HostClosed,
                };
                let route = self.route(control);
                let continuation = match continuation {
                    PendingContinuation::Requester(continuation) => continuation,
                    PendingContinuation::Ended(ended) => {
                        let step = self.backend.cleanup_complete(key, outcome, route);
                        let baton = self.advance_ended(baton, slice, ticket, ended, step, control);
                        turn = DriverTurn::Drive(baton);
                        continue;
                    }
                };
                // A control-only return never enters this transition: the
                // consumed record proves this request has native settlement.
                let baton = match control.enter_native_completion(&slice, baton) {
                    Ok(baton) => baton,
                    Err(_) => fatal_pump(),
                };
                let step = self
                    .backend
                    .native_complete(key, outcome, continuation, route);
                let step = advance_native_step(baton, slice, ticket, step);
                match self.advance(step, continuation, PendingOrigin::Continuation, control) {
                    PumpProgress::Returned(returned) => return returned,
                    PumpProgress::Driving(baton) => {
                        turn = DriverTurn::Drive(baton);
                        continue;
                    }
                }
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
                Wake::ContinuationTurn { key, baton } => {
                    turn = DriverTurn::Continue { parent: key, baton };
                    continue;
                }
                Wake::OwnerStep { .. } => fatal_pump(),
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
                Wake::ContinuationTurn { key, baton } => {
                    DriverTurn::Continue { parent: key, baton }
                }
                Wake::OwnerStep { .. } => fatal_pump(),
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
                        // A suspended owner continuation has no pending native
                        // record. Keep its intent in the same published parent
                        // before asking an actual native operation to cancel.
                        // Neither action revokes a live requester or injects Lisp.
                        let runtime = baton.green_runtime();
                        let Some(runtime) = runtime.as_ref() else {
                            fatal_pump()
                        };
                        let recorded = runtime
                            .parked
                            .borrow_mut()
                            .hosts()
                            .request_owner_interruption(baton, key, reason);
                        match recorded {
                            Ok(_) => {}
                            // Actor completion drains earlier commands before it
                            // settles; their observations may outlive the final
                            // owner return. They cannot reopen a finished record.
                            Err(
                                OwnerStepError::WrongStage
                                | OwnerStepError::Root(super::HostRootError::UnknownRequest),
                            ) => {}
                            Err(
                                OwnerStepError::Root(_)
                                | OwnerStepError::IdentityExhausted
                                | OwnerStepError::StaleStep
                                | OwnerStepError::Revoked
                                | OwnerStepError::UnsupportedSchema
                                | OwnerStepError::RootCountOverflow,
                            ) => fatal_pump(),
                        }
                        self.pending.request_cancel(key, reason);
                    }
                    CancelResolution::SuppressedBeforeDispatch
                    | CancelResolution::AlreadyTerminal
                    | CancelResolution::UnknownRequest => {}
                },
                ControlObservation::Closed => {
                    self.close();
                    self.close_admission(baton);
                    let runtime = baton.green_runtime();
                    let Some(runtime) = runtime.as_ref() else {
                        fatal_pump()
                    };
                    if runtime
                        .parked
                        .borrow_mut()
                        .hosts()
                        .observe_session_closing(baton)
                        .is_err()
                    {
                        fatal_pump();
                    }
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
    Start {
        key: HostRequestKey,
        baton: Baton,
    },
    Continue {
        parent: HostRequestKey,
        baton: Baton,
    },
    Drive(Baton),
}

enum PendingOrigin {
    New,
    Continuation,
}

enum PumpProgress {
    Returned(ServiceReturn),
    Driving(Baton),
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

#[cfg(test)]
#[path = "tests/semantic_pump.rs"]
mod semantic_tests;

/// Owner-selected receiving state occupies the existing boxed seat field.
/// Main alone carries native service storage; a pooled worker carries no host.
pub(in crate::emacs_core::eval::green) struct OwnerReceive {
    state: ReceiveState,
    owner_flows: OwnerLocalFlows,
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
            owner_flows: OwnerLocalFlows::default(),
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
        // Retained !Send exits are destroyed on this actual owner before its
        // transport channel may enter an idle carrier pool.
        drop(self.owner_flows);
        match self.state {
            ReceiveState::Worker(channel) => channel,
            ReceiveState::Main { .. } => fatal_pump(),
        }
    }

    pub(in crate::emacs_core::eval::green) fn owner_flows_mut(&mut self) -> &mut OwnerLocalFlows {
        &mut self.owner_flows
    }

    pub(in crate::emacs_core::eval::green) fn continue_owner_step(
        &mut self,
        parent: HostRequestKey,
        baton: Baton,
        control: &Control,
    ) -> ServiceReturn {
        match &mut self.state {
            ReceiveState::Main { channel, pump } => {
                pump.continue_owner_step(parent, baton, control, channel)
            }
            ReceiveState::Worker(_) => fatal_pump(),
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
