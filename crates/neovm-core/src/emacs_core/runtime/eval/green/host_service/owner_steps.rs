//! Request-local continuation authority, separate from terminal host replies.
//!
//! The held serial Baton protects this registry. Native receivers see only the
//! step key; a live restored requester must claim its linear port. All root
//! readers end before the requester obtains a mutable evaluator again.

use super::*;
use crate::emacs_core::eval::{Context, VmRootScopeState};
use crate::emacs_core::value::Value;
use crate::host_effect::native_outcome::CancelReason;
use crate::host_effect::owner_step::{
    ClaimedOwnerStep, OwnerFrameRecordSlot, OwnerFrameSchema, OwnerInvocationId,
    OwnerStepCompletion, OwnerStepKey, OwnerStepSequence, PendingOwnerStep, RegisteredOwnerFlow,
};

pub(in crate::emacs_core::eval::green) type StepKey = OwnerStepKey<ThreadKey>;
type PendingStep = PendingOwnerStep<'static, ThreadKey>;
pub(super) type StepClaim = ClaimedOwnerStep<'static, ThreadKey>;
type StepCompletion = OwnerStepCompletion<'static, ThreadKey>;

/// Exactly one successor may be prepared by the current requester claim.
pub(super) enum OwnerSuccessorPreparation {
    Unused,
    Issued(StepKey),
    Prepared(PendingStep),
}

/// An incomplete issued successor cannot accompany an accepted completion.
pub(super) enum OwnerPreparedSuccessor {
    Absent,
    Prepared(PendingStep),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::emacs_core::eval::green) enum OwnerNextStep {
    Absent,
    Prepared(StepKey),
}

/// Durable intent only. The requester retains its original Flow and the
/// native attempt must still perform and acknowledge its actual cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::emacs_core::eval::green) enum OwnerInterruption {
    Uninterrupted,
    Requested(CancelReason),
}

pub(super) struct OwnerStepReady {
    pub(super) completion: StepCompletion,
    pub(super) next: OwnerNextStep,
    pub(super) interruption: OwnerInterruption,
}

#[derive(Debug, thiserror::Error)]
pub(in crate::emacs_core::eval::green) enum OwnerStepError {
    #[error(transparent)]
    Root(#[from] HostRootError),
    #[error("owner step identity space exhausted")]
    IdentityExhausted,
    #[error("owner step does not match its live continuation")]
    StaleStep,
    #[error("owner continuation is not at this lifecycle stage")]
    WrongStage,
    #[error("owner continuation has been revoked")]
    Revoked,
    #[error("owner frame root schema is not supported")]
    UnsupportedSchema,
    #[error("owner frame root count overflow")]
    RootCountOverflow,
}

/// The parent remains Published in every nonterminal phase. No phase holds a
/// root reader, mutable Context, native engine, Baton or raw Lisp Flow.
pub(super) enum OwnerStepState {
    Idle,
    Issued(StepKey),
    Prepared(PendingStep),
    AwaitingOwner(PendingStep),
    Claimed {
        key: StepKey,
        successor: OwnerSuccessorPreparation,
    },
    Completed {
        completion: StepCompletion,
        successor: OwnerPreparedSuccessor,
    },
    Revoked(RevokedOwnerStep),
}

/// Revocation retains the exact consuming ports and completion metadata until
/// the parent actor has acknowledged actual native cleanup. Construction
/// excludes nested revocation and inactive states.
pub(super) struct RevokedOwnerStep {
    key: StepKey,
    retained: Box<OwnerStepState>,
}

impl RevokedOwnerStep {
    fn new(key: StepKey, retained: OwnerStepState) -> Result<Self, OwnerStepState> {
        if retained.unrevoked_key() != Some(key) {
            return Err(retained);
        }
        Ok(Self {
            key,
            retained: Box::new(retained),
        })
    }
}

impl OwnerStepState {
    fn unrevoked_key(&self) -> Option<StepKey> {
        match self {
            Self::Issued(key) => Some(*key),
            Self::Prepared(pending) | Self::AwaitingOwner(pending) => Some(pending.key()),
            Self::Claimed { key, successor: _ } => Some(*key),
            Self::Completed {
                completion,
                successor: _,
            } => Some(completion.key()),
            Self::Idle | Self::Revoked(_) => None,
        }
    }
}

impl std::fmt::Debug for OwnerStepState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let phase = match self {
            Self::Idle => "idle",
            Self::Issued(_) => "issued",
            Self::Prepared(_) => "prepared",
            Self::AwaitingOwner(_) => "awaiting-owner",
            Self::Claimed { .. } => "claimed",
            Self::Completed { .. } => "completed",
            Self::Revoked(_) => "revoked",
        };
        f.debug_tuple("OwnerStepState").field(&phase).finish()
    }
}

impl Default for OwnerStepState {
    fn default() -> Self {
        Self::Idle
    }
}

/// Checked sequencing is per parent request and never wraps across a step.
pub(super) struct OwnerSteps {
    sequence: OwnerStepSequence,
    state: OwnerStepState,
    interruption: OwnerInterruption,
}

impl std::fmt::Debug for OwnerSteps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnerSteps")
            .field("state", &self.state)
            .field("interruption", &self.interruption)
            .finish()
    }
}

impl Default for OwnerSteps {
    fn default() -> Self {
        Self {
            sequence: OwnerStepSequence::new(),
            state: OwnerStepState::Idle,
            interruption: OwnerInterruption::Uninterrupted,
        }
    }
}

impl OwnerSteps {
    #[cfg(test)]
    pub(super) fn state_for_test(&self) -> &OwnerStepState {
        &self.state
    }

    pub(super) fn can_settle(&self) -> bool {
        matches!(self.state, OwnerStepState::Idle)
    }

    /// Private state transition shared by both kinds of terminal authority.
    /// Neither caller may dispose a published or claimed owner port.
    fn discard_unused_after_terminal(&mut self) -> Result<(), OwnerStepError> {
        match self.state {
            OwnerStepState::Idle => Ok(()),
            OwnerStepState::Issued(_) | OwnerStepState::Prepared(_) => {
                self.state = OwnerStepState::Idle;
                Ok(())
            }
            OwnerStepState::AwaitingOwner(_)
            | OwnerStepState::Claimed { .. }
            | OwnerStepState::Completed { .. }
            | OwnerStepState::Revoked(_) => Err(OwnerStepError::WrongStage),
        }
    }

    pub(super) fn request_interruption(&mut self, reason: CancelReason) -> OwnerInterruption {
        match self.interruption {
            OwnerInterruption::Uninterrupted => {
                self.interruption = OwnerInterruption::Requested(reason);
            }
            OwnerInterruption::Requested(_) => {}
        }
        self.interruption
    }
}

/// Registration/read authority for an already-admitted continuation during
/// Closing. Private construction prevents this from reopening new admission.
pub(super) struct ContinuationAccess {
    key: StepKey,
    _owner: PhantomData<Rc<()>>,
}

/// Original Flow objects never enter the World registry or a native envelope.
/// Their private pins/handler/backtrace metadata stay on this requester's
/// existing receive owner and are destroyed only on that owner/carrier.
#[derive(Default)]
pub(in crate::emacs_core::eval::green) struct OwnerLocalFlows {
    flows: BTreeMap<StepKey, crate::emacs_core::error::Flow>,
}

impl OwnerLocalFlows {
    fn retain(
        &mut self,
        key: StepKey,
        flow: crate::emacs_core::error::Flow,
    ) -> Result<(), RejectedContinuation<crate::emacs_core::error::Flow>> {
        match self.flows.entry(key) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(flow);
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(_) => Err(RejectedContinuation {
                item: flow,
                error: OwnerStepError::WrongStage,
            }),
        }
    }

    pub(super) fn take(
        &mut self,
        key: StepKey,
    ) -> Result<crate::emacs_core::error::Flow, OwnerStepError> {
        self.flows.remove(&key).ok_or(OwnerStepError::StaleStep)
    }
}

static_assertions::assert_not_impl_any!(OwnerLocalFlows: Send, Sync, Clone, Copy);

#[derive(Clone, Copy, Debug)]
struct OwnerRootOffset(usize);
#[derive(Clone, Copy, Debug)]
struct OwnerRootCount(usize);
#[derive(Clone, Copy, Debug)]
struct OwnerRootSpan {
    offset: OwnerRootOffset,
    count: OwnerRootCount,
}

#[derive(Clone, Copy, Debug)]
struct OwnerFrameLayout {
    arguments: OwnerRootSpan,
    execution: OwnerRootSpan,
    restore: OwnerRootSpan,
}

/// A rejection returns the owned port/result; it never silently discards the
/// rightful completion when validation fails.
#[must_use]
pub(super) struct RejectedContinuation<T> {
    pub(super) item: T,
    pub(super) error: OwnerStepError,
}
impl<T> std::fmt::Debug for RejectedContinuation<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RejectedContinuation")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

fn schema_slots(schema: OwnerFrameSchema) -> Result<[OwnerFrameRecordSlot; 3], OwnerStepError> {
    // This is the actual lower schema, not a hand-copied callback enum. Adding
    // a real variant makes this conversion non-exhaustive.
    match schema {
        OwnerFrameSchema::Redisplay {
            arguments,
            frame,
            restore,
        }
        | OwnerFrameSchema::DisplayQuery {
            arguments,
            frame,
            restore,
        }
        | OwnerFrameSchema::Font {
            arguments,
            frame,
            restore,
        }
        | OwnerFrameSchema::Selection {
            arguments,
            frame,
            restore,
        }
        | OwnerFrameSchema::Callback {
            arguments,
            frame,
            restore,
        }
        | OwnerFrameSchema::Restore {
            arguments,
            frame,
            restore,
        } => Ok([arguments, frame, restore]),
        #[cfg(feature = "contract-fixtures")]
        OwnerFrameSchema::ContractInjected => Err(OwnerStepError::UnsupportedSchema),
    }
}

fn validate_owner_frame_records(
    record: &Record,
    pending: &PendingStep,
) -> Result<(), OwnerStepError> {
    for slot in schema_slots(pending.frame_schema())? {
        let registered = record
            .registered
            .get(&slot.record())
            .ok_or(HostRootError::ForeignRecord)?;
        if registered.kind != slot.kind() {
            return Err(HostRootError::ForeignRecord.into());
        }
    }
    Ok(())
}

impl HostRegistry {
    fn validate_owner_step_baton(&self, baton: &Baton) -> Result<(), OwnerStepError> {
        if !baton.held_here() {
            return Err(HostRootError::BatonRequired.into());
        }
        let actual = baton.parallel_heap_identity();
        if self.heap != actual {
            return Err(HostRootError::ForeignHeap {
                expected: self.heap,
                actual,
            }
            .into());
        }
        Ok(())
    }

    pub(super) fn request_owner_interruption(
        &mut self,
        baton: &Baton,
        parent: HostRequestKey,
        reason: CancelReason,
    ) -> Result<OwnerInterruption, OwnerStepError> {
        self.validate_owner_step_baton(baton)?;
        let record = self.record_mut(parent)?;
        if !matches!(record.state, RecordState::Published(_)) {
            return Err(OwnerStepError::WrongStage);
        }
        Ok(record.steps.request_interruption(reason))
    }

    pub(super) fn owner_interruption(
        &self,
        baton: &Baton,
        key: StepKey,
    ) -> Result<OwnerInterruption, OwnerStepError> {
        self.validate_owner_step_baton(baton)?;
        let record = self.continuation_record(key)?;
        let current = match &record.steps.state {
            OwnerStepState::Revoked(revoked) => Some(revoked.key),
            OwnerStepState::Idle
            | OwnerStepState::Issued(_)
            | OwnerStepState::Prepared(_)
            | OwnerStepState::AwaitingOwner(_)
            | OwnerStepState::Claimed {
                key: _,
                successor: _,
            }
            | OwnerStepState::Completed {
                completion: _,
                successor: _,
            } => record.steps.state.unrevoked_key(),
        };
        if current != Some(key) {
            return Err(OwnerStepError::StaleStep);
        }
        Ok(record.steps.interruption)
    }

    fn continuation_record(&self, key: StepKey) -> Result<&Record, OwnerStepError> {
        let record = self
            .records
            .get(&key.parent())
            .ok_or(HostRootError::UnknownRequest)?;
        if record.owner != key.owner() {
            return Err(HostRootError::ForeignOwner.into());
        }
        if !matches!(record.state, RecordState::Published(_)) {
            return Err(OwnerStepError::WrongStage);
        }
        Ok(record)
    }

    /// Issue before the original requester publishes its first park, or while
    /// a restored requester prepares its next semantic stop.
    pub(super) fn issue_owner_step(
        &mut self,
        baton: &Baton,
        heap: &TaggedHeap,
        parent: HostRequestKey,
        owner: ThreadKey,
        invocation: OwnerInvocationId,
    ) -> Result<StepKey, OwnerStepError> {
        self.validate_access(baton, heap)?;
        if matches!(self.session, SessionState::Closing | SessionState::Closed) {
            return Err(HostRootError::Closed.into());
        }
        let record = self.record_mut(parent)?;
        if record.owner != owner {
            return Err(HostRootError::ForeignOwner.into());
        }
        if !matches!(
            record.state,
            RecordState::Prepared | RecordState::Published(_)
        ) || !matches!(record.steps.state, OwnerStepState::Idle)
        {
            return Err(OwnerStepError::WrongStage);
        }
        let nonce = record
            .steps
            .sequence
            .issue()
            .map_err(|_| OwnerStepError::IdentityExhausted)?;
        let key = OwnerStepKey::new(parent, owner, invocation, nonce);
        record.steps.state = OwnerStepState::Issued(key);
        Ok(key)
    }

    pub(super) fn prepare_owner_step(
        &mut self,
        baton: &Baton,
        heap: &TaggedHeap,
        pending: PendingStep,
    ) -> Result<(), RejectedContinuation<PendingStep>> {
        let validate = || -> Result<(), OwnerStepError> {
            self.validate_access(baton, heap)?;
            if matches!(self.session, SessionState::Closing | SessionState::Closed) {
                return Err(HostRootError::Closed.into());
            }
            let key = pending.key();
            let record = self
                .records
                .get(&key.parent())
                .ok_or(HostRootError::UnknownRequest)?;
            if record.owner != key.owner() {
                return Err(HostRootError::ForeignOwner.into());
            }
            if !matches!(record.steps.state, OwnerStepState::Issued(expected) if expected == key) {
                return Err(OwnerStepError::StaleStep);
            }
            validate_owner_frame_records(record, &pending)
        };
        if let Err(error) = validate() {
            return Err(RejectedContinuation {
                item: pending,
                error,
            });
        }
        // Validated above with no callback or handoff between accesses.
        match self.records.get_mut(&pending.key().parent()) {
            Some(record) => {
                record.steps.state = OwnerStepState::Prepared(pending);
                Ok(())
            }
            None => Err(RejectedContinuation {
                item: pending,
                error: HostRootError::UnknownRequest.into(),
            }),
        }
    }

    /// An admitted requester can reserve one next semantic stop, including
    /// cleanup during Closing, while its current consuming claim remains live.
    pub(super) fn issue_successor(
        &mut self,
        baton: &Baton,
        heap: &TaggedHeap,
        access: &ContinuationAccess,
    ) -> Result<StepKey, OwnerStepError> {
        self.validate_continuation_access(baton, heap, access)?;
        let record = self.record_mut(access.key.parent())?;
        let OwnerStepState::Claimed {
            key,
            successor: OwnerSuccessorPreparation::Unused,
        } = &record.steps.state
        else {
            return Err(OwnerStepError::WrongStage);
        };
        let current = *key;
        let nonce = record
            .steps
            .sequence
            .issue()
            .map_err(|_| OwnerStepError::IdentityExhausted)?;
        let next = OwnerStepKey::new(
            current.parent(),
            current.owner(),
            current.invocation(),
            nonce,
        );
        record.steps.state = OwnerStepState::Claimed {
            key: current,
            successor: OwnerSuccessorPreparation::Issued(next),
        };
        Ok(next)
    }

    pub(super) fn prepare_successor(
        &mut self,
        baton: &Baton,
        heap: &TaggedHeap,
        access: &ContinuationAccess,
        pending: PendingStep,
    ) -> Result<(), RejectedContinuation<PendingStep>> {
        let validate = || -> Result<(), OwnerStepError> {
            self.validate_continuation_access(baton, heap, access)?;
            let record = self.continuation_record(access.key)?;
            if !matches!(
                &record.steps.state,
                OwnerStepState::Claimed {
                    successor: OwnerSuccessorPreparation::Issued(expected),
                    ..
                } if *expected == pending.key()
            ) {
                return Err(OwnerStepError::StaleStep);
            }
            validate_owner_frame_records(record, &pending)
        };
        if let Err(error) = validate() {
            return Err(RejectedContinuation {
                item: pending,
                error,
            });
        }
        match self.records.get_mut(&access.key.parent()) {
            Some(record) => {
                record.steps.state = OwnerStepState::Claimed {
                    key: access.key,
                    successor: OwnerSuccessorPreparation::Prepared(pending),
                };
                Ok(())
            }
            None => Err(RejectedContinuation {
                item: pending,
                error: HostRootError::UnknownRequest.into(),
            }),
        }
    }

    /// Native main can publish only the step already rooted by its requester.
    /// This mutates metadata, never dereferences a worker Lisp object.
    pub(super) fn publish_owner_step(
        &mut self,
        baton: &Baton,
        key: StepKey,
    ) -> Result<(), OwnerStepError> {
        if !baton.held_here() {
            return Err(HostRootError::BatonRequired.into());
        }
        self.continuation_record(key)?;
        let record = self.record_mut(key.parent())?;
        if !matches!(&record.steps.state, OwnerStepState::Prepared(step) if step.key() == key) {
            return Err(OwnerStepError::StaleStep);
        }
        let OwnerStepState::Prepared(pending) =
            std::mem::replace(&mut record.steps.state, OwnerStepState::Idle)
        else {
            return Err(OwnerStepError::WrongStage);
        };
        record.steps.state = OwnerStepState::AwaitingOwner(pending);
        Ok(())
    }

    pub(in crate::emacs_core::eval::green) fn awaiting_owner_step(
        &self,
        parent: HostRequestKey,
    ) -> Result<StepKey, OwnerStepError> {
        let record = self
            .records
            .get(&parent)
            .ok_or(HostRootError::UnknownRequest)?;
        match &record.steps.state {
            OwnerStepState::AwaitingOwner(pending) => Ok(pending.key()),
            OwnerStepState::Revoked(_) => Err(OwnerStepError::Revoked),
            OwnerStepState::Idle
            | OwnerStepState::Issued(_)
            | OwnerStepState::Prepared(_)
            | OwnerStepState::Claimed { .. }
            | OwnerStepState::Completed { .. } => Err(OwnerStepError::WrongStage),
        }
    }

    pub(super) fn claim_owner_step(
        &mut self,
        restored: &RestoredHostOwner<'_>,
        key: StepKey,
    ) -> Result<(StepClaim, ContinuationAccess), OwnerStepError> {
        self.validate_access(restored.baton, &restored.context.tagged_heap)?;
        if restored.owner != key.owner() {
            return Err(HostRootError::ForeignOwner.into());
        }
        self.continuation_record(key)?;
        let record = self.record_mut(key.parent())?;
        if !matches!(&record.steps.state, OwnerStepState::AwaitingOwner(pending) if pending.key() == key)
        {
            return Err(OwnerStepError::StaleStep);
        }
        let OwnerStepState::AwaitingOwner(pending) = std::mem::replace(
            &mut record.steps.state,
            OwnerStepState::Claimed {
                key,
                successor: OwnerSuccessorPreparation::Unused,
            },
        ) else {
            return Err(OwnerStepError::WrongStage);
        };
        match pending.claim(key) {
            Ok(claim) => Ok((
                claim,
                ContinuationAccess {
                    key,
                    _owner: PhantomData,
                },
            )),
            Err(rejected) => {
                let (pending, _) = rejected.into_parts();
                record.steps.state = OwnerStepState::AwaitingOwner(pending);
                Err(OwnerStepError::StaleStep)
            }
        }
    }

    fn validate_continuation_access(
        &self,
        baton: &Baton,
        heap: &TaggedHeap,
        access: &ContinuationAccess,
    ) -> Result<(), OwnerStepError> {
        self.validate_access(baton, heap)?;
        let record = self.continuation_record(access.key)?;
        if !matches!(record.steps.state, OwnerStepState::Claimed { key, .. } if key == access.key) {
            return Err(OwnerStepError::StaleStep);
        }
        Ok(())
    }

    /// Closing is permitted only for this live, already-admitted claim.
    /// New prepare/issue APIs remain closed.
    ///
    /// # Safety
    /// Roots must be live same-heap roots of the restored original requester;
    /// no GC, callback or handoff occurs through registration.
    pub(super) unsafe fn register_continuation<K: RegisteredRecord>(
        &mut self,
        baton: &Baton,
        heap: &TaggedHeap,
        access: &ContinuationAccess,
        roots: &[TaggedValue],
    ) -> Result<RegisteredRootRef<'static, K>, OwnerStepError> {
        self.validate_continuation_access(baton, heap, access)?;
        // SAFETY: the caller owns the live same-heap roots; the validated claim
        // supplies only this parent's admission during Closing, not new work.
        let reference = unsafe {
            self.register_record(baton, heap, access.key.owner(), access.key.parent(), roots)
        }?;
        let record = self.record_mut(access.key.parent())?;
        let slot = record
            .registered
            .get_mut(&reference.record())
            .ok_or(HostRootError::ForeignRecord)?;
        slot.origin = RecordOrigin::OwnerStep(access.key);
        Ok(reference)
    }

    pub(super) fn complete_owner_step(
        &mut self,
        baton: &Baton,
        heap: &TaggedHeap,
        completion: StepCompletion,
    ) -> Result<(), RejectedContinuation<StepCompletion>> {
        let key = completion.key();
        let validate = || -> Result<(), OwnerStepError> {
            self.validate_access(baton, heap)?;
            let record = self.continuation_record(key)?;
            match &record.steps.state {
                OwnerStepState::Claimed {
                    key: expected,
                    successor,
                } if *expected == key => {
                    if matches!(successor, OwnerSuccessorPreparation::Issued(_)) {
                        return Err(OwnerStepError::WrongStage);
                    }
                }
                OwnerStepState::Idle
                | OwnerStepState::Issued(_)
                | OwnerStepState::Prepared(_)
                | OwnerStepState::AwaitingOwner(_)
                | OwnerStepState::Claimed {
                    key: _,
                    successor: _,
                }
                | OwnerStepState::Completed {
                    completion: _,
                    successor: _,
                }
                | OwnerStepState::Revoked(_) => return Err(OwnerStepError::StaleStep),
            }
            let slot = completion.result_slot();
            let registered = record
                .registered
                .get(&slot.record())
                .ok_or(HostRootError::ForeignRecord)?;
            if registered.kind != slot.kind() || registered.origin != RecordOrigin::OwnerStep(key) {
                return Err(OwnerStepError::StaleStep);
            }
            Ok(())
        };
        if let Err(error) = validate() {
            return Err(RejectedContinuation {
                item: completion,
                error,
            });
        }
        match self.records.get_mut(&key.parent()) {
            Some(record) => {
                let previous = std::mem::replace(&mut record.steps.state, OwnerStepState::Idle);
                match previous {
                    OwnerStepState::Claimed {
                        key: current,
                        successor,
                    } if current == key => {
                        let successor = match successor {
                            OwnerSuccessorPreparation::Unused => OwnerPreparedSuccessor::Absent,
                            OwnerSuccessorPreparation::Prepared(pending) => {
                                OwnerPreparedSuccessor::Prepared(pending)
                            }
                            OwnerSuccessorPreparation::Issued(next) => {
                                record.steps.state = OwnerStepState::Claimed {
                                    key: current,
                                    successor: OwnerSuccessorPreparation::Issued(next),
                                };
                                return Err(RejectedContinuation {
                                    item: completion,
                                    error: OwnerStepError::WrongStage,
                                });
                            }
                        };
                        record.steps.state = OwnerStepState::Completed {
                            completion,
                            successor,
                        };
                        Ok(())
                    }
                    previous @ (OwnerStepState::Idle
                    | OwnerStepState::Issued(_)
                    | OwnerStepState::Prepared(_)
                    | OwnerStepState::AwaitingOwner(_)
                    | OwnerStepState::Claimed {
                        key: _,
                        successor: _,
                    }
                    | OwnerStepState::Completed {
                        completion: _,
                        successor: _,
                    }
                    | OwnerStepState::Revoked(_)) => {
                        record.steps.state = previous;
                        Err(RejectedContinuation {
                            item: completion,
                            error: OwnerStepError::StaleStep,
                        })
                    }
                }
            }
            None => Err(RejectedContinuation {
                item: completion,
                error: HostRootError::UnknownRequest.into(),
            }),
        }
    }

    pub(super) fn take_owner_completion(
        &mut self,
        baton: &Baton,
        key: StepKey,
    ) -> Result<OwnerStepReady, OwnerStepError> {
        if !baton.held_here() {
            return Err(HostRootError::BatonRequired.into());
        }
        self.continuation_record(key)?;
        let record = self.record_mut(key.parent())?;
        if !matches!(&record.steps.state, OwnerStepState::Completed { completion, .. } if completion.key() == key)
        {
            return Err(OwnerStepError::StaleStep);
        }
        let OwnerStepState::Completed {
            completion,
            successor,
        } = std::mem::replace(&mut record.steps.state, OwnerStepState::Idle)
        else {
            return Err(OwnerStepError::WrongStage);
        };
        let next = match successor {
            OwnerPreparedSuccessor::Absent => OwnerNextStep::Absent,
            OwnerPreparedSuccessor::Prepared(pending) => {
                let next = pending.key();
                record.steps.state = OwnerStepState::Prepared(pending);
                OwnerNextStep::Prepared(next)
            }
        };
        Ok(OwnerStepReady {
            completion,
            next,
            interruption: record.steps.interruption,
        })
    }

    pub(super) fn validate_owner_completion(
        &self,
        baton: &Baton,
        key: StepKey,
    ) -> Result<(), OwnerStepError> {
        if !baton.held_here() {
            return Err(HostRootError::BatonRequired.into());
        }
        let record = self.continuation_record(key)?;
        if matches!(&record.steps.state, OwnerStepState::Completed { completion, .. } if completion.key() == key)
        {
            Ok(())
        } else {
            Err(OwnerStepError::StaleStep)
        }
    }

    pub(super) fn finish_native_owner_step(
        &mut self,
        baton: &Baton,
        settled: super::scheduling::NativeCleanupSettled,
    ) -> Result<(), OwnerStepError> {
        self.validate_owner_step_baton(baton)?;
        let record = self.record_mut(settled.key())?;
        if !matches!(record.state, RecordState::Published(_)) {
            return Err(OwnerStepError::WrongStage);
        }
        match record.steps.state {
            OwnerStepState::Revoked(_) => {
                record.steps.state = OwnerStepState::Idle;
                Ok(())
            }
            OwnerStepState::Idle
            | OwnerStepState::Issued(_)
            | OwnerStepState::Prepared(_)
            | OwnerStepState::AwaitingOwner(_)
            | OwnerStepState::Claimed { .. }
            | OwnerStepState::Completed { .. } => record.steps.discard_unused_after_terminal(),
        }
    }

    /// Only the transport/admission owner can mint this undispatched proof.
    /// Active owner authority is never disposed through this transition.
    pub(super) fn finish_unstarted_owner_step(
        &mut self,
        baton: &Baton,
        unstarted: super::scheduling::UnstartedHostCall,
    ) -> Result<(), OwnerStepError> {
        self.validate_owner_step_baton(baton)?;
        let record = self.record_mut(unstarted.key())?;
        if !matches!(record.state, RecordState::Published(_)) {
            return Err(OwnerStepError::WrongStage);
        }
        record.steps.discard_unused_after_terminal()
    }

    pub(super) fn revoke_owner_step(
        &mut self,
        baton: &Baton,
        ended: &super::super::scheduler::EndedServiceOwner,
        key: StepKey,
    ) -> Result<(), OwnerStepError> {
        self.validate_owner_step_baton(baton)?;
        if ended.key() != key.parent() {
            return Err(OwnerStepError::StaleStep);
        }
        if ended.requester() != key.owner() {
            return Err(HostRootError::ForeignOwner.into());
        }
        let record = self.continuation_record(key)?;
        if let OwnerStepState::Revoked(revoked) = &record.steps.state {
            // Repeating the same ended-owner proof is bounded and idempotent;
            // it neither nests another retained state nor drops this one.
            return if revoked.key == key {
                Ok(())
            } else {
                Err(OwnerStepError::StaleStep)
            };
        }
        if record.steps.state.unrevoked_key() != Some(key) {
            return Err(OwnerStepError::StaleStep);
        }
        let record = self.record_mut(key.parent())?;
        let retained = std::mem::replace(&mut record.steps.state, OwnerStepState::Idle);
        match RevokedOwnerStep::new(key, retained) {
            Ok(revoked) => {
                record.steps.state = OwnerStepState::Revoked(revoked);
                Ok(())
            }
            Err(retained) => {
                record.steps.state = retained;
                Err(OwnerStepError::StaleStep)
            }
        }
    }

    fn owner_frame_layout(
        &self,
        key: StepKey,
    ) -> Result<(OwnerFrameLayout, [OwnerFrameRecordSlot; 3]), OwnerStepError> {
        let record = self.continuation_record(key)?;
        let pending = match &record.steps.state {
            OwnerStepState::AwaitingOwner(pending) if pending.key() == key => pending,
            OwnerStepState::Idle
            | OwnerStepState::Issued(_)
            | OwnerStepState::Prepared(_)
            | OwnerStepState::AwaitingOwner(_)
            | OwnerStepState::Claimed { .. }
            | OwnerStepState::Completed { .. }
            | OwnerStepState::Revoked(_) => return Err(OwnerStepError::StaleStep),
        };
        let slots = schema_slots(pending.frame_schema())?;
        let mut total = 0usize;
        let mut span = |slot: OwnerFrameRecordSlot| -> Result<OwnerRootSpan, OwnerStepError> {
            let registered = record
                .registered
                .get(&slot.record())
                .ok_or(HostRootError::ForeignRecord)?;
            if registered.kind != slot.kind() {
                return Err(HostRootError::ForeignRecord.into());
            }
            record
                .roots
                .get(registered.slot.0)
                .ok_or(HostRootError::ForeignRecord)?;
            let count = registered.payload.len();
            let offset = total;
            total = total
                .checked_add(count)
                .ok_or(OwnerStepError::RootCountOverflow)?;
            Ok(OwnerRootSpan {
                offset: OwnerRootOffset(offset),
                count: OwnerRootCount(count),
            })
        };
        Ok((
            OwnerFrameLayout {
                arguments: span(slots[0])?,
                execution: span(slots[1])?,
                restore: span(slots[2])?,
            },
            slots,
        ))
    }

    fn copy_owner_frame(
        &self,
        baton: &Baton,
        heap: &TaggedHeap,
        key: StepKey,
        slots: [OwnerFrameRecordSlot; 3],
        scratch: &mut Vec<TaggedValue>,
        output: &mut [TaggedValue],
    ) -> Result<(), OwnerStepError> {
        self.validate_access(baton, heap)?;
        let record = self.continuation_record(key)?;
        let mut offset = 0usize;
        for slot in slots {
            let registered = record
                .registered
                .get(&slot.record())
                .ok_or(HostRootError::ForeignRecord)?;
            if registered.kind != slot.kind() {
                return Err(HostRootError::ForeignRecord.into());
            }
            let lease = record
                .roots
                .get(registered.slot.0)
                .ok_or(HostRootError::ForeignRecord)?;
            let end = offset
                .checked_add(registered.payload.len())
                .ok_or(OwnerStepError::RootCountOverflow)?;
            let destination = output
                .get_mut(offset..end)
                .ok_or(OwnerStepError::RootCountOverflow)?;
            // All Rust storage was reserved before any reader. Scratch never
            // escapes this read extent; the ordinary owner frame is filled
            // before reader completion, and no callback or GC occurs here.
            scratch.clear();
            let reader = lease.reader(heap).map_err(HostRootError::from)?;
            reader.append_roots(scratch);
            let decoded = registered.payload.copy_into(scratch, destination);
            reader.finish().map_err(HostRootError::from)?;
            decoded?;
            offset = end;
        }
        Ok(())
    }
}

/// An execution frame lives only on its original requester. Its roots occupy
/// the existing ordinary VM frame, traced by every collection and nested park.
/// Drop restores the root scope without Lisp, blocking or a borrowed reader.
pub(super) struct OwnerExecutionFrame<'owner> {
    context: &'owner mut Context,
    scope: VmRootScopeState,
    key: StepKey,
    base: OwnerRootOffset,
    layout: OwnerFrameLayout,
}

/// A snapshot borrows only the existing seat and restores its unique Baton on
/// every return or unwind, without needing another evaluator borrow.
struct OwnerBatonLoan<'seat> {
    seat: &'seat mut super::super::GreenSeat,
    baton: Option<Baton>,
}

impl<'seat> OwnerBatonLoan<'seat> {
    #[cold]
    #[inline(never)]
    fn new(seat: &'seat mut super::super::GreenSeat) -> Result<Self, HostRootError> {
        let baton = seat.take_owner_step_baton()?;
        Ok(Self {
            seat,
            baton: Some(baton),
        })
    }

    fn baton(&self) -> Result<&Baton, HostRootError> {
        self.baton.as_ref().ok_or(HostRootError::BatonRequired)
    }
}

impl Drop for OwnerBatonLoan<'_> {
    #[cold]
    #[inline(never)]
    fn drop(&mut self) {
        match self.baton.take() {
            Some(baton) => self.seat.restore_owner_step_baton(baton),
            None => {
                tracing::error!("owner Baton loan lost its unique authority");
                std::process::abort();
            }
        }
    }
}

impl OwnerExecutionFrame<'_> {
    pub(super) fn key(&self) -> StepKey {
        self.key
    }

    /// Read current intent after any nested wait. This snapshot owns no root
    /// reader or registry borrow, and the seat holds its Baton again on return.
    #[cold]
    #[inline(never)]
    pub(super) fn interruption(&mut self) -> Result<OwnerInterruption, OwnerStepError> {
        let key = self.key;
        let seat = self
            .context
            .green_seat()
            .ok_or(HostRootError::ForeignOwner)?;
        if seat.key != key.owner() {
            return Err(HostRootError::ForeignOwner.into());
        }
        let loan = OwnerBatonLoan::new(seat)?;
        {
            let baton = loan.baton()?;
            let runtime = baton.green_runtime();
            let runtime = runtime.as_ref().ok_or(HostRootError::BatonRequired)?;
            let registry = runtime.parked.borrow();
            registry.hosts.owner_interruption(baton, key)
        }
    }

    fn span(&self, span: OwnerRootSpan) -> Result<&[Value], OwnerStepError> {
        let offset = self
            .base
            .0
            .checked_add(span.offset.0)
            .ok_or(OwnerStepError::RootCountOverflow)?;
        self.context
            .vm_frame_root_slots_checked(offset, span.count.0)
            .ok_or(OwnerStepError::WrongStage)
    }
    pub(super) fn arguments(&self) -> Result<&[Value], OwnerStepError> {
        self.span(self.layout.arguments)
    }
    pub(super) fn execution_roots(&self) -> Result<&[Value], OwnerStepError> {
        self.span(self.layout.execution)
    }
    pub(super) fn restore_roots(&self) -> Result<&[Value], OwnerStepError> {
        self.span(self.layout.restore)
    }

    /// The adapter is an original-owner call-site operation, never a function
    /// pointer or arbitrary closure received from native transport.
    pub(super) fn execute<R>(&mut self, execute: impl FnOnce(&mut Context) -> R) -> R {
        execute(self.context)
    }

    /// Keep the exact requester Flow and all its private state. The canonical
    /// in-flight registry walk includes deferred bytecode-backtrace pins; no
    /// transport-side copy of SignalData/ThrowData is maintained here.
    pub(super) fn retain_flow(
        &mut self,
        access: &ContinuationAccess,
        flow: crate::emacs_core::error::Flow,
    ) -> Result<RegisteredOwnerFlow<'static>, RejectedContinuation<crate::emacs_core::error::Flow>>
    {
        if access.key != self.key {
            return Err(RejectedContinuation {
                item: flow,
                error: OwnerStepError::StaleStep,
            });
        }
        let retained = (|| -> Result<RegisteredOwnerFlow<'static>, OwnerStepError> {
            let seat = self
                .context
                .green_seat()
                .ok_or(HostRootError::ForeignOwner)?;
            let baton = seat.take_owner_step_baton()?;
            let registered = (|| {
                let mut roots = Vec::new();
                crate::emacs_core::error::collect_in_flight_registry_gc_roots(
                    &mut roots,
                    &self.context.in_flight_registry,
                    self.context.tagged_heap.identity(),
                );
                let runtime = baton.green_runtime();
                let runtime = runtime.as_ref().ok_or(HostRootError::BatonRequired)?;
                let mut registry = runtime.parked.borrow_mut();
                let hosts = registry.hosts();
                use crate::emacs_core::error::FlowRef;
                // SAFETY: the existing original-owner InFlightRoots pins keep
                // every canonical word live throughout same-heap registration;
                // this scope runs no Lisp, GC, callback or handoff.
                unsafe {
                    match flow.kind() {
                        FlowRef::Signal(_) => hosts.register_continuation::<crate::host_effect::contracts::OwnerSignalFlowRecord>(&baton, &self.context.tagged_heap, access, &roots).map(RegisteredOwnerFlow::Signal),
                        FlowRef::Throw(_) => hosts.register_continuation::<crate::host_effect::contracts::OwnerThrowFlowRecord>(&baton, &self.context.tagged_heap, access, &roots).map(RegisteredOwnerFlow::Throw),
                        FlowRef::ThreadBlocked(_) => hosts.register_continuation::<crate::host_effect::contracts::OwnerBlockedFlowRecord>(&baton, &self.context.tagged_heap, access, &roots).map(RegisteredOwnerFlow::ThreadBlocked),
                        FlowRef::Shutdown(_) => hosts.register_continuation::<crate::host_effect::contracts::OwnerShutdownFlowRecord>(&baton, &self.context.tagged_heap, access, &roots).map(RegisteredOwnerFlow::Shutdown),
                    }
                }
            })();
            match self.context.green_seat() {
                Some(seat) => seat.restore_owner_step_baton(baton),
                None => {
                    tracing::error!("owner Flow publication lost its Green seat");
                    std::process::abort();
                }
            }
            registered
        })();
        let reference = match retained {
            Ok(reference) => reference,
            Err(error) => return Err(RejectedContinuation { item: flow, error }),
        };
        let receiver = match self
            .context
            .green_seat()
            .and_then(|seat| seat.owner_step_receive().ok())
        {
            Some(receiver) => receiver,
            None => {
                return Err(RejectedContinuation {
                    item: flow,
                    error: HostRootError::OwnerNotParked.into(),
                });
            }
        };
        receiver.owner_flows_mut().retain(self.key, flow)?;
        Ok(reference)
    }
}

impl Context {
    /// Recover the original control-flow object only on its requester. Main's
    /// native service never receives this object or this method's authority.
    pub(in crate::emacs_core::eval::green) fn take_owner_step_flow(
        &mut self,
        key: StepKey,
    ) -> Result<crate::emacs_core::error::Flow, OwnerStepError> {
        let seat = self.green_seat().ok_or(HostRootError::ForeignOwner)?;
        if seat.key != key.owner() {
            return Err(HostRootError::ForeignOwner.into());
        }
        let baton = seat.take_owner_step_baton()?;
        let held = baton.held_here();
        seat.restore_owner_step_baton(baton);
        if !held {
            return Err(HostRootError::BatonRequired.into());
        }
        seat.owner_step_receive()?.owner_flows_mut().take(key)
    }

    /// Restore/materialize using the original requester, then release every
    /// reader before calling the owner-local executor. The seat again holds
    /// its Baton before execution, so a callback can enter a genuine wait.
    pub(in crate::emacs_core::eval::green) fn with_owner_execution_frame<R>(
        &mut self,
        key: StepKey,
        execute: impl FnOnce(&mut OwnerExecutionFrame<'_>, StepClaim, ContinuationAccess) -> R,
    ) -> Result<R, OwnerStepError> {
        let seat = self.green_seat().ok_or(HostRootError::ForeignOwner)?;
        let baton = seat.take_owner_step_baton()?;
        let prepared = (|| {
            let restored = RestoredHostOwner::after_restore(self, &baton)?;
            if restored.owner != key.owner() {
                return Err(HostRootError::ForeignOwner.into());
            }
            let (layout, slots) =
                restored.with_registry(|registry, _| registry.owner_frame_layout(key))?;
            let count = layout
                .restore
                .offset
                .0
                .checked_add(layout.restore.count.0)
                .ok_or(OwnerStepError::RootCountOverflow)?;
            // Rust-only reservation and initialized nil slots precede readers.
            let mut scratch = Vec::with_capacity(count);
            let scope = self.save_vm_roots();
            let base = self.reserve_vm_frame_root_slots(count);
            let materialized: Result<_, OwnerStepError> = (|| {
                let runtime = baton.green_runtime();
                let runtime = runtime.as_ref().ok_or(HostRootError::BatonRequired)?;
                let registry = runtime.parked.borrow();
                let frame = self
                    .vm_root_frames
                    .last_mut()
                    .ok_or(OwnerStepError::WrongStage)?;
                let output = frame
                    .roots
                    .get_mut(base..base + count)
                    .ok_or(OwnerStepError::WrongStage)?;
                registry.hosts.copy_owner_frame(
                    &baton,
                    &self.tagged_heap,
                    key,
                    slots,
                    &mut scratch,
                    output,
                )?;
                drop(registry);
                let restored = RestoredHostOwner::after_restore(self, &baton)?;
                let (claim, access) = restored
                    .with_registry(|registry, owner| registry.claim_owner_step(owner, key))?;
                Ok((scope, base, layout, claim, access))
            })();
            if materialized.is_err() {
                self.restore_vm_roots(scope);
            }
            materialized
        })();
        match self.green_seat() {
            Some(seat) => seat.restore_owner_step_baton(baton),
            None => {
                // The sealed materialization ran no Lisp and cannot remove its
                // seat. Preserve unique authority on an invariant failure.
                tracing::error!("owner-step materialization lost its Green seat");
                std::process::abort();
            }
        }
        let (scope, base, layout, claim, access) = prepared?;
        let mut frame = OwnerExecutionFrame {
            context: self,
            scope,
            key,
            base: OwnerRootOffset(base),
            layout,
        };
        Ok(execute(&mut frame, claim, access))
    }
}

impl Drop for OwnerExecutionFrame<'_> {
    fn drop(&mut self) {
        self.context.restore_vm_roots(self.scope);
    }
}

static_assertions::assert_not_impl_any!(ContinuationAccess: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(OwnerExecutionFrame<'static>: Send, Sync, Clone, Copy);

#[cfg(test)]
#[path = "tests/owner_steps.rs"]
pub(super) mod tests;
