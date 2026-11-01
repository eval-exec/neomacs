//! Closed owner-continuation vocabulary. It carries only registered references
//! and native correlation metadata, never evaluator/engine borrows or raw Flow.
//!
//! The publication owner retains every RootBatch. A numeric key grants no
//! execution authority: the registry must validate heap/Baton/current owner and
//! durable lifecycle before consuming a port on the requesting continuation.

use super::contracts::*;
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct OwnerInvocationId(NonZeroU64);
impl OwnerInvocationId {
    pub(crate) const fn from_owner_id(value: NonZeroU64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct OwnerStepNonce(NonZeroU64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OwnerStepIdentityExhausted;

enum SequenceState {
    Available(NonZeroU64),
    Exhausted,
}
pub(crate) struct OwnerStepSequence {
    state: SequenceState,
}
impl Default for OwnerStepSequence {
    fn default() -> Self {
        Self::new()
    }
}
impl OwnerStepSequence {
    pub(crate) fn new() -> Self {
        Self {
            state: SequenceState::Available(NonZeroU64::MIN),
        }
    }
    pub(crate) fn issue(&mut self) -> Result<OwnerStepNonce, OwnerStepIdentityExhausted> {
        match self.state {
            SequenceState::Available(current) => {
                self.state = match current.checked_add(1) {
                    Some(next) => SequenceState::Available(next),
                    None => SequenceState::Exhausted,
                };
                Ok(OwnerStepNonce(current))
            }
            SequenceState::Exhausted => Err(OwnerStepIdentityExhausted),
        }
    }
}

/// O is the upper owner's canonical key, e.g. Green ThreadKey. Keeping it
/// generic avoids a lower-layer dependency on evaluator/scheduler vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct OwnerStepKey<O> {
    parent: HostRequestKey,
    owner: O,
    invocation: OwnerInvocationId,
    nonce: OwnerStepNonce,
}
impl<O: Copy> OwnerStepKey<O> {
    pub(crate) fn new(
        parent: HostRequestKey,
        owner: O,
        invocation: OwnerInvocationId,
        nonce: OwnerStepNonce,
    ) -> Self {
        Self {
            parent,
            owner,
            invocation,
            nonce,
        }
    }
    pub(crate) fn parent(self) -> HostRequestKey {
        self.parent
    }
    pub(crate) fn owner(self) -> O {
        self.owner
    }
    pub(crate) fn invocation(self) -> OwnerInvocationId {
        self.invocation
    }
    pub(crate) fn nonce(self) -> OwnerStepNonce {
        self.nonce
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnerStepRefusal {
    WrongParent,
    WrongOwner,
    WrongInvocation,
    WrongNonce,
    WrongRecord,
    WrongFamily,
}
pub(crate) struct RejectedOwnerStep<T> {
    item: T,
    reason: OwnerStepRefusal,
}
impl<T> std::fmt::Debug for RejectedOwnerStep<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RejectedOwnerStep")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
impl<T> RejectedOwnerStep<T> {
    pub(crate) fn into_parts(self) -> (T, OwnerStepRefusal) {
        (self.item, self.reason)
    }
    pub(crate) fn reason(&self) -> OwnerStepRefusal {
        self.reason
    }
}
fn compare_key<O: Copy + Eq>(
    actual: OwnerStepKey<O>,
    expected: OwnerStepKey<O>,
) -> Result<(), OwnerStepRefusal> {
    if actual.parent != expected.parent {
        Err(OwnerStepRefusal::WrongParent)
    } else if actual.owner != expected.owner {
        Err(OwnerStepRefusal::WrongOwner)
    } else if actual.invocation != expected.invocation {
        Err(OwnerStepRefusal::WrongInvocation)
    } else if actual.nonce != expected.nonce {
        Err(OwnerStepRefusal::WrongNonce)
    } else {
        Ok(())
    }
}

/// All four references are opaque; private Flow policies/pins stay on requester.
pub(crate) enum RegisteredOwnerFlow<'s> {
    Signal(RegisteredRootRef<'s, OwnerSignalFlowRecord>),
    Throw(RegisteredRootRef<'s, OwnerThrowFlowRecord>),
    ThreadBlocked(RegisteredRootRef<'s, OwnerBlockedFlowRecord>),
    Shutdown(RegisteredRootRef<'s, OwnerShutdownFlowRecord>),
}
impl RegisteredOwnerFlow<'_> {
    fn result_slot(&self) -> OwnerFrameRecordSlot {
        match self {
            Self::Signal(record) => OwnerFrameRecordSlot::from_reference(record),
            Self::Throw(record) => OwnerFrameRecordSlot::from_reference(record),
            Self::ThreadBlocked(record) => OwnerFrameRecordSlot::from_reference(record),
            Self::Shutdown(record) => OwnerFrameRecordSlot::from_reference(record),
        }
    }
    fn matches(&self, parent: HostRequestKey) -> bool {
        match self {
            Self::Signal(record) => record.matches(parent),
            Self::Throw(record) => record.matches(parent),
            Self::ThreadBlocked(record) => record.matches(parent),
            Self::Shutdown(record) => record.matches(parent),
        }
    }
}

mod sealed {
    pub trait Kind {}
}
pub(crate) trait OwnerStepKind: sealed::Kind {
    type Arguments: RegisteredRecord;
    type Result: RegisteredRecord;
    const FAMILY: OwnerStepFamily;
}

pub(crate) struct PublishedOwnerStep<'s, K: OwnerStepKind, O> {
    key: OwnerStepKey<O>,
    arguments: RegisteredRootRef<'s, K::Arguments>,
    frame: RegisteredRootRef<'s, OwnerExecutionFrameRecord>,
    restore: RegisteredRootRef<'s, OwnerRestoreFrameRecord>,
}
impl<'s, K: OwnerStepKind, O: Copy> PublishedOwnerStep<'s, K, O> {
    pub(crate) fn key(&self) -> OwnerStepKey<O> {
        self.key
    }
    pub(crate) fn arguments(&self) -> &RegisteredRootRef<'s, K::Arguments> {
        &self.arguments
    }
    pub(crate) fn frame(&self) -> &RegisteredRootRef<'s, OwnerExecutionFrameRecord> {
        &self.frame
    }
    pub(crate) fn restore(&self) -> &RegisteredRootRef<'s, OwnerRestoreFrameRecord> {
        &self.restore
    }
}

pub(crate) struct OwnerResumePort<K: OwnerStepKind, O> {
    key: OwnerStepKey<O>,
    _kind: PhantomData<fn() -> K>,
    _requester: PhantomData<Rc<()>>,
}
pub(crate) struct OwnerStepEntry<'s, K: OwnerStepKind, O> {
    published: PublishedOwnerStep<'s, K, O>,
    port: OwnerResumePort<K, O>,
}
pub(crate) struct OwnerStepClaim<'s, K: OwnerStepKind, O> {
    published: PublishedOwnerStep<'s, K, O>,
    port: OwnerResumePort<K, O>,
}
impl<'s, K: OwnerStepKind, O> OwnerStepEntry<'s, K, O> {
    pub(crate) fn published(&self) -> &PublishedOwnerStep<'s, K, O> {
        &self.published
    }
}
impl<'s, K: OwnerStepKind, O> OwnerStepClaim<'s, K, O> {
    pub(crate) fn published(&self) -> &PublishedOwnerStep<'s, K, O> {
        &self.published
    }
}

/// Typed frame record positions, not reusable raw array offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OwnerFrameRecordSlot {
    record: RootRecordId,
    kind: RecordKind,
}
impl OwnerFrameRecordSlot {
    fn from_reference<K: RegisteredRecord>(record: &RegisteredRootRef<'_, K>) -> Self {
        Self {
            record: record.record(),
            kind: record.kind(),
        }
    }
    pub(crate) fn record(self) -> RootRecordId {
        self.record
    }
    pub(crate) fn kind(self) -> RecordKind {
        self.kind
    }
}

pub(crate) struct CompletedOwnerStep<'s, O> {
    key: OwnerStepKey<O>,
    result: OwnerStepResult<'s>,
    _requester: PhantomData<Rc<()>>,
}
impl<'s, O: Copy> CompletedOwnerStep<'s, O> {
    pub(crate) fn key(&self) -> OwnerStepKey<O> {
        self.key
    }
    pub(crate) fn result_slot(&self) -> OwnerFrameRecordSlot {
        self.result.result_slot()
    }
    pub(crate) fn into_parts(self) -> (OwnerStepKey<O>, OwnerStepResult<'s>) {
        (self.key, self.result)
    }
}

macro_rules! step_families {
    ($(($variant:ident, $constructor:ident, $kind:ident, $arguments:ident, $result:ident)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum OwnerStepFamily { $($variant,)+ }
        $(
            pub(crate) enum $kind {}
            impl sealed::Kind for $kind {}
            impl OwnerStepKind for $kind {
                type Arguments = $arguments;
                type Result = $result;
                const FAMILY: OwnerStepFamily = OwnerStepFamily::$variant;
            }
        )+
        /// Published metadata is separate from owner-confined consuming ports.
        pub(crate) enum OwnerStepEnvelope<'s, O> { $($variant(PublishedOwnerStep<'s, $kind, O>),)+ }
        pub(crate) enum PendingOwnerStep<'s, O> { $($variant(OwnerStepEntry<'s, $kind, O>),)+ }
        pub(crate) enum ClaimedOwnerStep<'s, O> { $($variant(OwnerStepClaim<'s, $kind, O>),)+ }
        pub(crate) enum OwnerStepCompletion<'s, O> { $($variant(CompletedOwnerStep<'s, O>),)+ }
        pub(crate) enum OwnerStepResult<'s> {
            $($variant(RegisteredRootRef<'s, $result>),)+
            Flow(RegisteredOwnerFlow<'s>),
            // Mutate the actual result vocabulary for compiler association
            // contracts, without adding a production semantic alternative.
            #[cfg(feature = "contract-fixtures")]
            ContractInjected(RegisteredRootRef<'s, CallbackStepResult>),
        }

        /// Actual lower schema inspected by frame-coverage contracts. Each
        /// producer destructures every PublishedOwnerStep field, without `..`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum OwnerFrameSchema {
            $($variant { arguments: OwnerFrameRecordSlot, frame: OwnerFrameRecordSlot, restore: OwnerFrameRecordSlot },)+
            #[cfg(feature = "contract-fixtures")]
            ContractInjected,
        }

        enum ResultAssociation {
            Family(OwnerStepFamily),
            OwnerFlow,
        }
        impl<'s> OwnerStepResult<'s> {
            pub(crate) fn result_slot(&self) -> OwnerFrameRecordSlot {
                match self {
                    $(Self::$variant(record) => OwnerFrameRecordSlot::from_reference(record),)+
                    Self::Flow(flow) => flow.result_slot(),
                    #[cfg(feature = "contract-fixtures")]
                    Self::ContractInjected(record) => OwnerFrameRecordSlot::from_reference(record),
                }
            }
            fn association(&self) -> ResultAssociation {
                match self {
                    $(Self::$variant(_) => ResultAssociation::Family(OwnerStepFamily::$variant),)+
                    Self::Flow(_) => ResultAssociation::OwnerFlow,
                    #[cfg(feature = "contract-fixtures")]
                    Self::ContractInjected(_) => ResultAssociation::Family(OwnerStepFamily::Callback),
                }
            }
            fn matches(&self, parent: HostRequestKey) -> bool {
                match self {
                    $(Self::$variant(record) => record.matches(parent),)+
                    Self::Flow(flow) => flow.matches(parent),
                    #[cfg(feature = "contract-fixtures")]
                    Self::ContractInjected(record) => record.matches(parent),
                }
            }
        }
        impl<'s, O: Copy> OwnerStepEnvelope<'s, O> {
            pub(crate) fn key(&self) -> OwnerStepKey<O> { match self { $(Self::$variant(step) => step.key,)+ } }
            pub(crate) fn family(&self) -> OwnerStepFamily { match self { $(Self::$variant(_) => OwnerStepFamily::$variant,)+ } }
        }
        impl<'s, O: Copy + Eq> PendingOwnerStep<'s, O> {
            $(pub(crate) fn $constructor(
                key: OwnerStepKey<O>,
                arguments: RegisteredRootRef<'s, $arguments>,
                frame: RegisteredRootRef<'s, OwnerExecutionFrameRecord>,
                restore: RegisteredRootRef<'s, OwnerRestoreFrameRecord>,
            ) -> Result<Self, RejectedOwnerStep<PublishedOwnerStep<'s, $kind, O>>> {
                let published = PublishedOwnerStep { key, arguments, frame, restore };
                if !published.arguments.matches(key.parent) || !published.frame.matches(key.parent) || !published.restore.matches(key.parent) {
                    return Err(RejectedOwnerStep { item: published, reason: OwnerStepRefusal::WrongRecord });
                }
                Ok(Self::$variant(OwnerStepEntry {
                    published,
                    port: OwnerResumePort { key, _kind: PhantomData, _requester: PhantomData },
                }))
            })+
            pub(crate) fn key(&self) -> OwnerStepKey<O> { match self { $(Self::$variant(entry) => entry.port.key,)+ } }
            pub(crate) fn family(&self) -> OwnerStepFamily { match self { $(Self::$variant(_) => OwnerStepFamily::$variant,)+ } }
            pub(crate) fn frame_schema(&self) -> OwnerFrameSchema {
                match self { $(Self::$variant(entry) => {
                    let PublishedOwnerStep { key: _, arguments, frame, restore } = &entry.published;
                    OwnerFrameSchema::$variant {
                        arguments: OwnerFrameRecordSlot::from_reference(arguments),
                        frame: OwnerFrameRecordSlot::from_reference(frame),
                        restore: OwnerFrameRecordSlot::from_reference(restore),
                    }
                },)+ }
            }
            /// The registry validates durable phase, live heap and restored
            /// requester before supplying its authoritative expected key.
            pub(crate) fn claim(self, expected: OwnerStepKey<O>) -> Result<ClaimedOwnerStep<'s, O>, RejectedOwnerStep<Self>> {
                if let Err(reason) = compare_key(self.key(), expected) {
                    return Err(RejectedOwnerStep { item: self, reason });
                }
                Ok(match self { $(Self::$variant(entry) => ClaimedOwnerStep::$variant(OwnerStepClaim { published: entry.published, port: entry.port }),)+ })
            }
        }
        impl<'s, O: Copy + Eq> ClaimedOwnerStep<'s, O> {
            pub(crate) fn key(&self) -> OwnerStepKey<O> { match self { $(Self::$variant(claim) => claim.port.key,)+ } }
            pub(crate) fn family(&self) -> OwnerStepFamily { match self { $(Self::$variant(_) => OwnerStepFamily::$variant,)+ } }
            pub(crate) fn complete(self, result: OwnerStepResult<'s>) -> Result<OwnerStepCompletion<'s, O>, RejectedOwnerStep<(Self, OwnerStepResult<'s>)>> {
                let key = self.key();
                match result.association() {
                    ResultAssociation::Family(family) if family != self.family() => {
                        return Err(RejectedOwnerStep { item: (self, result), reason: OwnerStepRefusal::WrongFamily });
                    }
                    ResultAssociation::Family(_) | ResultAssociation::OwnerFlow => {}
                }
                if !result.matches(key.parent) {
                    return Err(RejectedOwnerStep { item: (self, result), reason: OwnerStepRefusal::WrongRecord });
                }
                Ok(match self { $(Self::$variant(claim) => {
                    let OwnerStepClaim { published: _, port } = claim;
                    let OwnerResumePort { key, _kind: _, _requester: _ } = port;
                    OwnerStepCompletion::$variant(CompletedOwnerStep { key, result, _requester: PhantomData })
                },)+ })
            }
        }
        impl<'s, O: Copy> OwnerStepCompletion<'s, O> {
            pub(crate) fn key(&self) -> OwnerStepKey<O> { match self { $(Self::$variant(done) => done.key,)+ } }
            pub(crate) fn result_slot(&self) -> OwnerFrameRecordSlot { match self { $(Self::$variant(done) => done.result_slot(),)+ } }
            pub(crate) fn family(&self) -> OwnerStepFamily { match self { $(Self::$variant(_) => OwnerStepFamily::$variant,)+ } }
            pub(crate) fn into_parts(self) -> (OwnerStepKey<O>, OwnerStepResult<'s>) { match self { $(Self::$variant(done) => done.into_parts(),)+ } }
        }
    };
}

step_families!(
    (
        Redisplay,
        redisplay,
        RedisplayStep,
        RedisplayStepArguments,
        RedisplayStepResult
    ),
    (
        DisplayQuery,
        display_query,
        DisplayQueryStep,
        DisplayQueryStepArguments,
        DisplayQueryStepResult
    ),
    (Font, font, FontStep, FontStepArguments, FontStepResult),
    (
        Selection,
        selection,
        SelectionStep,
        SelectionStepArguments,
        SelectionStepResult
    ),
    (
        Callback,
        callback,
        CallbackStep,
        CallbackStepArguments,
        CallbackStepResult
    ),
    (
        Restore,
        restore,
        RestoreStep,
        RestoreStepArguments,
        RestoreStepResult
    ),
);

static_assertions::assert_not_impl_any!(OwnerResumePort<CallbackStep, u64>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(PendingOwnerStep<'static, u64>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ClaimedOwnerStep<'static, u64>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(OwnerStepCompletion<'static, u64>: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(OwnerStepEnvelope<'static, u64>: Send);

#[cfg(test)]
#[path = "tests/owner_step_test.rs"]
mod tests;
