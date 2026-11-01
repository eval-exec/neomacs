//! Main-owned native pending obligations. This module never acquires the Baton,
//! accesses the World, settles a native dispatch ticket, or retires Lisp roots.

use super::HostRequestKey;
use crate::host_effect::contracts::ReadDeadline;
use crate::host_effect::native_outcome::{CancelReason, NativeOutcome};
use crate::host_effect::native_pending::{
    NativeCancelProgress, NativeDeadlineProgress, NativePendingObservation, PendingNativeOperation,
};
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::time::Instant;

pub(super) trait PendingReservation {
    fn key(&self) -> HostRequestKey;
}

struct PendingEntry<R> {
    operation: PendingNativeOperation,
    reservation: R,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Admission {
    Open,
    Closed,
}

pub(super) struct NativePendingCalls<R: PendingReservation> {
    entries: BTreeMap<HostRequestKey, PendingEntry<R>>,
    admission: Admission,
    _main_owner: PhantomData<Rc<()>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PendingInsertRefusal {
    WrongKey,
    DuplicateKey,
    Closed,
}

pub(super) struct RejectedPending<R> {
    operation: PendingNativeOperation,
    reservation: R,
    reason: PendingInsertRefusal,
}

impl<R> RejectedPending<R> {
    pub(super) fn into_parts(self) -> (PendingNativeOperation, R, PendingInsertRefusal) {
        (self.operation, self.reservation, self.reason)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum NativeReadyOutcome {
    Completed(NativeOutcome),
    ProducerClosed,
}

pub(super) struct ReadyPending<R> {
    key: HostRequestKey,
    outcome: NativeReadyOutcome,
    reservation: R,
}

impl<R> ReadyPending<R> {
    pub(super) fn key(&self) -> HostRequestKey {
        self.key
    }

    pub(super) fn into_parts(self) -> (HostRequestKey, NativeReadyOutcome, R) {
        (self.key, self.outcome, self.reservation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PendingCancelProgress {
    UnknownRequest,
    Requested(NativeCancelProgress),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativeDrainProgress {
    Drained,
    AwaitingNativeSettlement(NonZeroUsize),
}

impl<R: PendingReservation> Default for NativePendingCalls<R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: PendingReservation> NativePendingCalls<R> {
    pub(super) fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            admission: Admission::Open,
            _main_owner: PhantomData,
        }
    }

    pub(super) fn insert(
        &mut self,
        operation: PendingNativeOperation,
        reservation: R,
    ) -> Result<(), RejectedPending<R>> {
        let key = operation.key();
        let refusal = if key != reservation.key() {
            Some(PendingInsertRefusal::WrongKey)
        } else {
            match self.admission {
                Admission::Closed => Some(PendingInsertRefusal::Closed),
                Admission::Open if self.entries.contains_key(&key) => {
                    Some(PendingInsertRefusal::DuplicateKey)
                }
                Admission::Open => None,
            }
        };
        if let Some(reason) = refusal {
            return Err(RejectedPending {
                operation,
                reservation,
                reason,
            });
        }
        self.entries.insert(
            key,
            PendingEntry {
                operation,
                reservation,
            },
        );
        Ok(())
    }

    /// Pending logical interests carry deadlines, never physical poll leases.
    pub(super) fn next_deadline(&self) -> ReadDeadline {
        self.entries
            .values()
            .fold(ReadDeadline::Unbounded, |next, entry| {
                match (next, entry.operation.next_wait_deadline()) {
                    (ReadDeadline::Immediate, _) | (_, ReadDeadline::Immediate) => {
                        ReadDeadline::Immediate
                    }
                    (ReadDeadline::At(a), ReadDeadline::At(b)) => ReadDeadline::At(a.min(b)),
                    (ReadDeadline::At(a), ReadDeadline::Unbounded)
                    | (ReadDeadline::Unbounded, ReadDeadline::At(a)) => ReadDeadline::At(a),
                    (ReadDeadline::Unbounded, ReadDeadline::Unbounded) => ReadDeadline::Unbounded,
                }
            })
    }

    /// Moving the ready record transfers its reservation intact. Actual actor
    /// completion occurs only after the caller reacquires this specific key.
    pub(super) fn observe_ready(&mut self) -> Vec<ReadyPending<R>> {
        let mut observations = Vec::new();
        for (&key, entry) in &mut self.entries {
            match entry.operation.observe() {
                NativePendingObservation::Pending | NativePendingObservation::AlreadyObserved => {}
                NativePendingObservation::Complete(completion) => {
                    // The private completion channel is bound to this operation's
                    // key, validated with the reservation once on insertion.
                    let (_, outcome) = completion.into_parts();
                    observations.push((key, NativeReadyOutcome::Completed(outcome)));
                }
                NativePendingObservation::Closed(_) => {
                    observations.push((key, NativeReadyOutcome::ProducerClosed));
                }
            }
        }
        let mut ready = Vec::with_capacity(observations.len());
        for (key, outcome) in observations {
            if let Some(entry) = self.entries.remove(&key) {
                ready.push(ReadyPending {
                    key,
                    outcome,
                    reservation: entry.reservation,
                });
            }
        }
        ready
    }

    pub(super) fn request_cancel(
        &mut self,
        key: HostRequestKey,
        reason: CancelReason,
    ) -> PendingCancelProgress {
        match self.entries.get_mut(&key) {
            Some(entry) => PendingCancelProgress::Requested(entry.operation.request_cancel(reason)),
            None => PendingCancelProgress::UnknownRequest,
        }
    }

    pub(super) fn expire_deadlines(
        &mut self,
        now: Instant,
    ) -> Vec<(HostRequestKey, NativeDeadlineProgress)> {
        self.entries
            .iter_mut()
            .map(|(&key, entry)| (key, entry.operation.expire_deadline(now)))
            .collect()
    }

    pub(super) fn request_all_cancel(&mut self, reason: CancelReason) -> NativeDrainProgress {
        for entry in self.entries.values_mut() {
            entry.operation.request_cancel(reason);
        }
        self.drain_progress()
    }

    pub(super) fn close(&mut self) -> NativeDrainProgress {
        self.admission = Admission::Closed;
        self.request_all_cancel(CancelReason::SessionClosing)
    }

    pub(super) fn drain_progress(&self) -> NativeDrainProgress {
        match NonZeroUsize::new(self.entries.len()) {
            Some(pending) => NativeDrainProgress::AwaitingNativeSettlement(pending),
            None => NativeDrainProgress::Drained,
        }
    }
}

static_assertions::assert_not_impl_any!(NativePendingCalls<ContractReservation>: Send, Sync);

/// A type-only control proves confinement even when a reservation is Send/Sync.
struct ContractReservation(HostRequestKey);
impl PendingReservation for ContractReservation {
    fn key(&self) -> HostRequestKey {
        self.0
    }
}

#[cfg(test)]
#[path = "tests/pending.rs"]
mod tests;
