//! Owned host-service transport. Only the service actor mutates request state.
//!
//! The transport neither publishes roots nor grants scheduler authority. Its
//! caller must publish a parked request before enqueueing it, and retain the
//! corresponding World-owned roots until the registry acknowledges retirement.
//! A terminal notification and a consumed native reply are not that retirement.

use super::HostRequestKey;
use crate::host_effect::contracts::OwnedNativeData;
use crossbeam_channel::{Receiver, Sender, TryRecvError, unbounded};
use std::collections::{BTreeMap, VecDeque};

/// Why the requesting owner no longer wants ordinary native completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::emacs_core::eval::green) enum CancelReason {
    TargetSignal,
    TargetQuit,
    OwnerEnded,
    SessionClosing,
}

/// Enqueue succeeded once, or the actor no longer exists. Never retry Delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeliveryStatus {
    Delivered,
    ReceiverClosed,
}

/// Native-only payloads. Registered Lisp errors remain in the root registry.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::emacs_core::eval::green) enum NativeReply {
    Returned(OwnedNativeData),
    LispError(OwnedNativeData),
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::emacs_core::eval::green) enum NativeOutcome {
    Reply(NativeReply),
    Interrupted(CancelReason),
    HostClosed,
}

/// Cancellation after dispatch never claims that the native effect rolled back.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::emacs_core::eval::green) enum NativeSettlement {
    Completed(NativeOutcome),
    CompletedAfterCancellation {
        outcome: NativeOutcome,
        cancellation: CancelReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CancelResolution {
    SuppressedBeforeDispatch,
    AwaitNativeSettlement,
    AlreadyTerminal,
    UnknownRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PublicationRejection {
    DuplicateKey,
}

/// The actor retains these observations until its registry consumes them.
#[derive(Debug)]
pub(super) enum ControlObservation {
    Cancellation {
        key: HostRequestKey,
        reason: CancelReason,
        resolution: CancelResolution,
    },
    RejectedPublication {
        key: HostRequestKey,
        reason: PublicationRejection,
        native: OwnedNativeData,
    },
    Closed,
}

/// A notification is metadata only; the terminal outcome stays in the actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SettlementNotification {
    Settled(HostRequestKey),
    Rejected(HostRequestKey, PublicationRejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WaitObservation {
    Pending,
    Notified(SettlementNotification),
    ChannelClosed,
}

enum Command {
    Publish {
        key: HostRequestKey,
        native: OwnedNativeData,
        notification: Sender<SettlementNotification>,
        observation_identity: Receiver<SettlementNotification>,
    },
    Cancel {
        key: HostRequestKey,
        reason: CancelReason,
        source: CancelSource,
    },
    Close,
}

enum CancelSource {
    Registry,
    Waiter(Receiver<SettlementNotification>),
}

/// There is one FIFO for publish, cancellation and session close.
#[derive(Clone)]
pub(super) struct HostClient {
    commands: Sender<Command>,
}

impl std::fmt::Debug for HostClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("HostClient").finish_non_exhaustive()
    }
}

pub(super) enum Publication {
    Delivered(HostWaiter),
    ReceiverClosed { key: HostRequestKey },
}

impl HostClient {
    /// Called only after the registry's successful park/publication sequence.
    pub(super) fn publish(&self, key: HostRequestKey, native: OwnedNativeData) -> Publication {
        let (notification, observed) = unbounded();
        match self.commands.send(Command::Publish {
            key,
            native,
            notification,
            observation_identity: observed.clone(),
        }) {
            Ok(()) => Publication::Delivered(HostWaiter {
                key,
                observed,
                commands: self.commands.clone(),
                drop_action: WaiterDropAction::Cancel,
            }),
            Err(_) => Publication::ReceiverClosed { key },
        }
    }

    pub(super) fn cancel(&self, key: HostRequestKey, reason: CancelReason) -> DeliveryStatus {
        delivery(self.commands.send(Command::Cancel {
            key,
            reason,
            source: CancelSource::Registry,
        }))
    }

    pub(super) fn close(&self) -> DeliveryStatus {
        delivery(self.commands.send(Command::Close))
    }
}

fn delivery<T>(result: Result<(), crossbeam_channel::SendError<T>>) -> DeliveryStatus {
    match result {
        Ok(()) => DeliveryStatus::Delivered,
        Err(_) => DeliveryStatus::ReceiverClosed,
    }
}

enum WaiterDropAction {
    Cancel,
    RegistryAcknowledged,
}

/// Dropping this endpoint never waits, joins, retires roots, or invokes Lisp.
pub(super) struct HostWaiter {
    key: HostRequestKey,
    observed: Receiver<SettlementNotification>,
    commands: Sender<Command>,
    drop_action: WaiterDropAction,
}

impl HostWaiter {
    pub(super) fn key(&self) -> HostRequestKey {
        self.key
    }

    pub(super) fn observe(&self) -> WaitObservation {
        match self.observed.try_recv() {
            Ok(notification) => WaitObservation::Notified(notification),
            Err(TryRecvError::Empty) => WaitObservation::Pending,
            Err(TryRecvError::Disconnected) => WaitObservation::ChannelClosed,
        }
    }

    /// This native-only wait does not itself release or reacquire the Baton.
    /// The scheduler must establish its park/restore authority around it.
    pub(super) fn wait(&self) -> WaitObservation {
        match self.observed.recv() {
            Ok(notification) => WaitObservation::Notified(notification),
            Err(_) => WaitObservation::ChannelClosed,
        }
    }

    /// Registry acknowledgement is separate from terminal observation.
    pub(super) fn finish(mut self) {
        self.drop_action = WaiterDropAction::RegistryAcknowledged;
    }
}

impl Drop for HostWaiter {
    fn drop(&mut self) {
        match self.drop_action {
            WaiterDropAction::Cancel => {
                let _ = self.commands.send(Command::Cancel {
                    key: self.key,
                    reason: CancelReason::OwnerEnded,
                    source: CancelSource::Waiter(self.observed.clone()),
                });
            }
            WaiterDropAction::RegistryAcknowledged => {}
        }
    }
}

struct Entry {
    state: EntryState,
    notification: Sender<SettlementNotification>,
    observation_identity: Receiver<SettlementNotification>,
}

enum EntryState {
    Queued(OwnedNativeData),
    Serving(CancellationState),
    Terminal(NativeSettlement),
    Observed,
}

#[derive(Clone, Copy)]
enum CancellationState {
    Uninterrupted,
    Requested(CancelReason),
}

enum Admission {
    Open,
    Closed,
}

/// The one mutable actor serializes cancellation, dispatch and completion.
/// No actor lock is held across native work or an owner wait.
pub(super) struct ServiceTransport {
    commands: Receiver<Command>,
    entries: BTreeMap<HostRequestKey, Entry>,
    queued: VecDeque<HostRequestKey>,
    observations: VecDeque<ControlObservation>,
    admission: Admission,
}

pub(super) fn channel() -> (HostClient, ServiceTransport) {
    let (sender, receiver) = unbounded();
    (
        HostClient { commands: sender },
        ServiceTransport {
            commands: receiver,
            entries: BTreeMap::new(),
            queued: VecDeque::new(),
            observations: VecDeque::new(),
            admission: Admission::Open,
        },
    )
}

pub(super) enum DispatchPoll {
    Ready(NativeDispatch),
    Idle,
}

/// Neither this envelope nor its ticket can be cloned or recreated by callers.
pub(super) struct NativeDispatch {
    ticket: DispatchTicket,
    native: OwnedNativeData,
}

impl NativeDispatch {
    pub(super) fn key(&self) -> HostRequestKey {
        self.ticket.key
    }

    pub(super) fn into_parts(self) -> (DispatchTicket, OwnedNativeData) {
        (self.ticket, self.native)
    }
}

pub(super) struct DispatchTicket {
    key: HostRequestKey,
}

impl DispatchTicket {
    pub(super) fn settle(self, outcome: NativeOutcome) -> NativeCompletion {
        NativeCompletion {
            key: self.key,
            outcome,
        }
    }
}

pub(super) struct NativeCompletion {
    key: HostRequestKey,
    outcome: NativeOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CompletionRejection {
    UnknownRequest,
    NotDispatched,
    AlreadyTerminal,
}

pub(super) enum CompletionDisposition {
    Settled(HostRequestKey),
    Rejected {
        completion: NativeCompletion,
        reason: CompletionRejection,
    },
}

/// Linear permission to retire *transport metadata*. Root retirement must have
/// happened under the registry's owner/collector proof before this is consumed.
pub(super) struct TerminalReceipt {
    key: HostRequestKey,
}

impl TerminalReceipt {
    pub(super) fn key(&self) -> HostRequestKey {
        self.key
    }
}

pub(super) enum TerminalRead {
    Ready {
        receipt: TerminalReceipt,
        settlement: NativeSettlement,
    },
    Pending,
    AlreadyObserved,
    UnknownRequest,
}

pub(super) enum Acknowledgement {
    Retired(HostRequestKey),
    Rejected(TerminalReceipt),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CloseProgress {
    Drained,
    AwaitingNativeSettlement(Vec<HostRequestKey>),
}

impl ServiceTransport {
    /// Process every earlier control message before selecting native work.
    pub(super) fn dispatch_next(&mut self) -> DispatchPoll {
        self.drain_commands();
        while let Some(key) = self.queued.pop_front() {
            let Some(entry) = self.entries.get_mut(&key) else {
                continue;
            };
            match std::mem::replace(
                &mut entry.state,
                EntryState::Serving(CancellationState::Uninterrupted),
            ) {
                EntryState::Queued(native) => {
                    return DispatchPoll::Ready(NativeDispatch {
                        ticket: DispatchTicket { key },
                        native,
                    });
                }
                state @ (EntryState::Serving(_)
                | EntryState::Terminal(_)
                | EntryState::Observed) => entry.state = state,
            }
        }
        DispatchPoll::Idle
    }

    pub(super) fn drain_commands(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::Publish {
                    key,
                    native,
                    notification,
                    observation_identity,
                } => self.admit(key, native, notification, observation_identity),
                Command::Cancel {
                    key,
                    reason,
                    source,
                } => {
                    let resolution = match source {
                        CancelSource::Registry => self.cancel(key, reason),
                        CancelSource::Waiter(observation) => match self.entries.get(&key) {
                            Some(entry)
                                if entry.observation_identity.same_channel(&observation) =>
                            {
                                self.cancel(key, reason)
                            }
                            Some(_) | None => CancelResolution::UnknownRequest,
                        },
                    };
                    self.observations
                        .push_back(ControlObservation::Cancellation {
                            key,
                            reason,
                            resolution,
                        });
                }
                Command::Close => {
                    self.close();
                    self.observations.push_back(ControlObservation::Closed);
                }
            }
        }
    }

    pub(super) fn next_observation(&mut self) -> Option<ControlObservation> {
        self.observations.pop_front()
    }

    fn admit(
        &mut self,
        key: HostRequestKey,
        native: OwnedNativeData,
        notification: Sender<SettlementNotification>,
        observation_identity: Receiver<SettlementNotification>,
    ) {
        use std::collections::btree_map::Entry as MapEntry;
        match self.entries.entry(key) {
            MapEntry::Occupied(_) => {
                self.observations
                    .push_back(ControlObservation::RejectedPublication {
                        key,
                        reason: PublicationRejection::DuplicateKey,
                        native,
                    });
                let _ = notification.send(SettlementNotification::Rejected(
                    key,
                    PublicationRejection::DuplicateKey,
                ));
            }
            MapEntry::Vacant(slot) => {
                let state = match self.admission {
                    Admission::Open => EntryState::Queued(native),
                    Admission::Closed => {
                        EntryState::Terminal(NativeSettlement::Completed(NativeOutcome::HostClosed))
                    }
                };
                let entry = slot.insert(Entry {
                    state,
                    notification,
                    observation_identity,
                });
                match entry.state {
                    EntryState::Queued(_) => self.queued.push_back(key),
                    EntryState::Terminal(_) => {
                        // Store the terminal state before making it observable.
                        let _ = entry
                            .notification
                            .send(SettlementNotification::Settled(key));
                    }
                    EntryState::Serving(_) | EntryState::Observed => {}
                }
            }
        }
    }

    fn cancel(&mut self, key: HostRequestKey, reason: CancelReason) -> CancelResolution {
        let Some(entry) = self.entries.get_mut(&key) else {
            return CancelResolution::UnknownRequest;
        };
        match &mut entry.state {
            EntryState::Queued(_) => {
                entry.state = EntryState::Terminal(NativeSettlement::Completed(
                    NativeOutcome::Interrupted(reason),
                ));
                let _ = entry
                    .notification
                    .send(SettlementNotification::Settled(key));
                CancelResolution::SuppressedBeforeDispatch
            }
            EntryState::Serving(cancellation) => {
                match cancellation {
                    CancellationState::Uninterrupted => {
                        *cancellation = CancellationState::Requested(reason);
                    }
                    CancellationState::Requested(_) => {}
                }
                CancelResolution::AwaitNativeSettlement
            }
            EntryState::Terminal(_) | EntryState::Observed => CancelResolution::AlreadyTerminal,
        }
    }

    /// Native completion and cancellation linearize in this same actor turn.
    pub(super) fn complete(&mut self, completion: NativeCompletion) -> CompletionDisposition {
        self.drain_commands();
        let Some(entry) = self.entries.get_mut(&completion.key) else {
            return CompletionDisposition::Rejected {
                completion,
                reason: CompletionRejection::UnknownRequest,
            };
        };
        let cancellation = match &entry.state {
            EntryState::Serving(cancellation) => *cancellation,
            EntryState::Queued(_) => {
                return CompletionDisposition::Rejected {
                    completion,
                    reason: CompletionRejection::NotDispatched,
                };
            }
            EntryState::Terminal(_) | EntryState::Observed => {
                return CompletionDisposition::Rejected {
                    completion,
                    reason: CompletionRejection::AlreadyTerminal,
                };
            }
        };
        let settlement = match cancellation {
            CancellationState::Uninterrupted => NativeSettlement::Completed(completion.outcome),
            CancellationState::Requested(cancellation) => {
                NativeSettlement::CompletedAfterCancellation {
                    outcome: completion.outcome,
                    cancellation,
                }
            }
        };
        entry.state = EntryState::Terminal(settlement);
        let _ = entry
            .notification
            .send(SettlementNotification::Settled(completion.key));
        CompletionDisposition::Settled(completion.key)
    }

    /// Consuming a reply leaves a tombstone until explicit registry retirement.
    pub(super) fn take_terminal(&mut self, key: HostRequestKey) -> TerminalRead {
        self.drain_commands();
        let Some(entry) = self.entries.get_mut(&key) else {
            return TerminalRead::UnknownRequest;
        };
        match std::mem::replace(&mut entry.state, EntryState::Observed) {
            EntryState::Terminal(settlement) => TerminalRead::Ready {
                receipt: TerminalReceipt { key },
                settlement,
            },
            state @ (EntryState::Queued(_) | EntryState::Serving(_)) => {
                entry.state = state;
                TerminalRead::Pending
            }
            EntryState::Observed => TerminalRead::AlreadyObserved,
        }
    }

    /// Called only after the matching owner/collector retirement proof succeeds.
    pub(super) fn acknowledge(&mut self, receipt: TerminalReceipt) -> Acknowledgement {
        match self.entries.get(&receipt.key) {
            Some(Entry {
                state: EntryState::Observed,
                ..
            }) => {
                self.entries.remove(&receipt.key);
                Acknowledgement::Retired(receipt.key)
            }
            Some(Entry {
                state: EntryState::Queued(_) | EntryState::Serving(_) | EntryState::Terminal(_),
                ..
            })
            | None => Acknowledgement::Rejected(receipt),
        }
    }

    fn close(&mut self) {
        self.admission = Admission::Closed;
        for (&key, entry) in &mut self.entries {
            match &mut entry.state {
                EntryState::Queued(_) => {
                    entry.state = EntryState::Terminal(NativeSettlement::Completed(
                        NativeOutcome::HostClosed,
                    ));
                    let _ = entry
                        .notification
                        .send(SettlementNotification::Settled(key));
                }
                EntryState::Serving(cancellation) => match cancellation {
                    CancellationState::Uninterrupted => {
                        *cancellation = CancellationState::Requested(CancelReason::SessionClosing);
                    }
                    CancellationState::Requested(_) => {}
                },
                EntryState::Terminal(_) | EntryState::Observed => {}
            }
        }
    }

    /// Closing never removes terminal records or an in-flight native obligation.
    pub(super) fn close_and_drain(&mut self) -> CloseProgress {
        self.drain_commands();
        self.close();
        let pending: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(&key, entry)| match entry.state {
                EntryState::Serving(_) => Some(key),
                EntryState::Queued(_) | EntryState::Terminal(_) | EntryState::Observed => None,
            })
            .collect();
        match pending.as_slice() {
            [] => CloseProgress::Drained,
            [_first, ..] => CloseProgress::AwaitingNativeSettlement(pending),
        }
    }
}

static_assertions::assert_impl_all!(HostClient: Send, Sync, Clone);
static_assertions::assert_impl_all!(HostWaiter: Send);
static_assertions::assert_not_impl_any!(HostWaiter: Clone, Copy);
static_assertions::assert_impl_all!(NativeDispatch: Send);
static_assertions::assert_not_impl_any!(NativeDispatch: Clone, Copy);
static_assertions::assert_not_impl_any!(DispatchTicket: Clone, Copy);
static_assertions::assert_not_impl_any!(NativeCompletion: Clone, Copy);
static_assertions::assert_not_impl_any!(TerminalReceipt: Clone, Copy);

#[cfg(test)]
#[path = "tests/transport.rs"]
mod tests;
