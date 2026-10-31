//! World-owned host-call roots and dormant native transport.
//!
//! Host endpoints carry metadata only. Every live root publication remains in
//! the World's existing root owner, including when no Lisp owner is parked.
//! This module installs no native adapter and does not admit hosted Green.

use super::super::world::{Baton, WorldLease};
use super::scheduler::ThreadKey;
pub(super) use crate::host_effect::contracts::HostRequestKey;
use crate::host_effect::contracts::{
    HostRequestGeneration, HostRequestId, HostSessionId, RecordKind, RegisteredRecord,
    RegisteredRootRef, RootRecordId,
};
use crate::tagged::gc::{HeapIdentity, QuiescentCollector, TaggedHeap};
use crate::tagged::transport::{RootBatchError, RootBatchLease};
use crate::tagged::value::TaggedValue;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

mod transport;

/// Process-wide correlation allocation, not host authority or a TLS slot.
struct SessionSequence(AtomicU64);
static SESSIONS: SessionSequence = SessionSequence(AtomicU64::new(1));

impl SessionSequence {
    fn issue(&self) -> Result<HostSessionId, HostRootError> {
        let value = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| HostRootError::IdentityExhausted)?;
        NonZeroU64::new(value)
            .map(HostSessionId::from_owner_id)
            .ok_or(HostRootError::IdentityExhausted)
    }
}

#[derive(Debug)]
struct RequestSequence(NonZeroU64);
impl RequestSequence {
    fn issue(&mut self) -> Result<HostRequestId, HostRootError> {
        let current = self.0;
        self.0 = current
            .checked_add(1)
            .ok_or(HostRootError::IdentityExhausted)?;
        Ok(HostRequestId::from_owner_id(current))
    }
}

#[derive(Debug)]
struct OpenSession {
    id: HostSessionId,
    requests: RequestSequence,
}

#[derive(Debug, Default)]
enum SessionState {
    #[default]
    Dormant,
    Open(OpenSession),
    Closing,
    Closed,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum HostRootError {
    #[error("host request identity space exhausted")]
    IdentityExhausted,
    #[error("display host is closed")]
    Closed,
    #[error("host request no longer belongs to this session")]
    UnknownRequest,
    #[error("host request belongs to another owner")]
    ForeignOwner,
    #[error("host request operation requires the current World Baton")]
    BatonRequired,
    #[error("host request operation requires its published parked owner")]
    OwnerNotParked,
    #[error("host request has not reached this lifecycle stage")]
    WrongStage,
    #[error("registered root record does not match this request or record kind")]
    ForeignRecord,
    #[error("host root registry of {expected:?} used with heap {actual:?}")]
    ForeignHeap {
        expected: HeapIdentity,
        actual: HeapIdentity,
    },
    #[error(transparent)]
    Root(#[from] RootBatchError),
    #[error(transparent)]
    Collector(#[from] crate::tagged::gc::CollectorQuiescenceError),
}

/// A registry entry's root ownership, independent of native completion.
#[derive(Debug)]
enum RecordState {
    Prepared,
    Published(HostParkRegistration),
    TerminalAwaitingOwner,
    OwnerAcknowledged,
}

#[derive(Debug)]
struct Record {
    owner: ThreadKey,
    state: RecordState,
    roots: Vec<RootBatchLease>,
    registered: BTreeMap<RootRecordId, RegisteredSlot>,
    next_record: NonZeroU64,
}

/// A slot indexes the leases retained by one request, never a Lisp position.
#[derive(Debug, Clone, Copy)]
struct RootSlot(usize);

#[derive(Debug)]
struct RegisteredSlot {
    kind: RecordKind,
    slot: RootSlot,
}

/// Authority borrowed from the actual restored owner, not a caller-supplied id.
/// Its Context borrow prevents another park during acknowledgement.
pub(super) struct RestoredHostOwner<'owner> {
    context: &'owner super::super::Context,
    baton: &'owner Baton,
    owner: ThreadKey,
}

impl std::fmt::Debug for RestoredHostOwner<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RestoredHostOwner")
            .field("owner", &self.owner)
            .finish_non_exhaustive()
    }
}

impl<'owner> RestoredHostOwner<'owner> {
    pub(super) fn after_restore(
        context: &'owner mut super::super::Context,
        baton: &'owner Baton,
    ) -> Result<Self, HostRootError> {
        if !baton.held_here() {
            return Err(HostRootError::BatonRequired);
        }
        let owner = context.green_key().ok_or(HostRootError::ForeignOwner)?;
        if context.threads.current_thread_id() != owner.id() {
            return Err(HostRootError::ForeignOwner);
        }
        let actual = context.tagged_heap.heap_identity();
        let expected = baton.parallel_heap_identity();
        if actual != expected {
            return Err(HostRootError::ForeignHeap { expected, actual });
        }
        Ok(Self {
            context,
            baton,
            owner,
        })
    }

    pub(super) fn with_registry<R>(
        &self,
        use_registry: impl FnOnce(&mut HostRegistry, &Self) -> R,
    ) -> R {
        self.context
            .with_green_runtime(|runtime| use_registry(runtime.parked.borrow_mut().hosts(), self))
    }
}

/// Linear request admission; dropping it leaves its World-owned roots intact.
#[derive(Debug)]
#[must_use = "publish or explicitly abandon the prepared host call"]
pub(super) struct PreparedHostCall {
    key: HostRequestKey,
    owner: ThreadKey,
    _confined: PhantomData<Rc<()>>,
}

/// Minted only after the owner's ordinary parked census is published.
#[derive(Debug)]
struct PublishedHostCall {
    key: HostRequestKey,
    owner: ThreadKey,
}

/// Registry-owned linear scheduler registration, never sent through a channel.
#[derive(Debug)]
pub(super) struct HostParkRegistration {
    key: HostRequestKey,
    owner: ThreadKey,
    _confined: PhantomData<Rc<()>>,
}

impl PublishedHostCall {
    fn into_registration(self) -> HostParkRegistration {
        HostParkRegistration {
            key: self.key,
            owner: self.owner,
            _confined: PhantomData,
        }
    }
}

impl HostParkRegistration {
    pub(super) fn key(&self) -> HostRequestKey {
        self.key
    }
    pub(super) fn owner(&self) -> ThreadKey {
        self.owner
    }
}

impl PreparedHostCall {
    pub(super) fn key(&self) -> HostRequestKey {
        self.key
    }
}

/// Registry-issued terminal observation; a transport notification cannot mint it.
#[derive(Debug)]
#[must_use = "deliver terminal settlement to its scheduler reservation"]
pub(super) struct SettledHostCall {
    key: HostRequestKey,
    owner: ThreadKey,
    _confined: PhantomData<Rc<()>>,
}

impl SettledHostCall {
    pub(super) fn key(&self) -> HostRequestKey {
        self.key
    }
    pub(super) fn owner(&self) -> ThreadKey {
        self.owner
    }
}

#[derive(Debug)]
#[must_use = "root retirement must follow owner acknowledgement"]
pub(super) struct OwnerAcknowledged<'owner> {
    key: HostRequestKey,
    restored: &'owner RestoredHostOwner<'owner>,
}

#[derive(Debug)]
pub(super) struct RootsRetired {
    key: HostRequestKey,
}

impl RootsRetired {
    pub(super) fn key(&self) -> HostRequestKey {
        self.key
    }
}

#[derive(Debug)]
#[must_use = "preserve acknowledgement and roots until retirement succeeds"]
pub(super) struct RetirementBlocked<'owner> {
    ack: OwnerAcknowledged<'owner>,
    error: HostRootError,
}

impl<'owner> RetirementBlocked<'owner> {
    pub(super) fn into_parts(self) -> (OwnerAcknowledged<'owner>, HostRootError) {
        (self.ack, self.error)
    }
}

#[derive(Debug)]
pub(super) struct DrainReceipt;

/// Allocation retention, not execution authority. A blocked caller must retain
/// or explicitly quarantine this obligation; it cannot destroy the World early.
#[derive(Debug)]
#[must_use = "unfinished roots must retain the World allocation"]
pub(super) struct DrainBlocked {
    retention: WorldLease,
    error: HostRootError,
}

impl DrainBlocked {
    pub(super) fn new(retention: WorldLease, error: HostRootError) -> Self {
        Self { retention, error }
    }

    pub(super) fn error(&self) -> &HostRootError {
        &self.error
    }

    /// Used only when shutdown has no remaining owner able to retire roots.
    /// Keep allocation backing, report the refusal, and never block in Drop.
    pub(super) fn retain_for_shutdown(self) {
        tracing::error!(error = %self.error, "retaining World with unfinished host roots");
        std::mem::forget(self.retention);
    }
}

/// The existing root-owner field now owns BOTH kinds of World publication.
/// No Deref: emptiness must include host-only obligations for World Drop's
/// existing quiescent-drain guard to be complete.
#[derive(Debug)]
pub(in crate::emacs_core::eval) struct WorldRootRegistry {
    owners: FxHashMap<ThreadKey, RootBatchLease>,
    hosts: HostRegistry,
}

impl WorldRootRegistry {
    pub(super) fn new(heap: HeapIdentity) -> Self {
        Self {
            owners: FxHashMap::default(),
            hosts: HostRegistry::new(heap),
        }
    }
    pub(in crate::emacs_core::eval) fn is_empty(&self) -> bool {
        self.owners.is_empty() && self.hosts.records.is_empty()
    }
    pub(super) fn len(&self) -> usize {
        self.owners.len()
    }
    pub(super) fn values(&self) -> impl Iterator<Item = &RootBatchLease> {
        self.owners.values()
    }
    pub(super) fn first_owner_key(&self) -> Option<ThreadKey> {
        self.owners.keys().next().copied()
    }
    pub(super) fn contains_key(&self, key: &ThreadKey) -> bool {
        self.owners.contains_key(key)
    }
    pub(super) fn insert(
        &mut self,
        key: ThreadKey,
        roots: RootBatchLease,
    ) -> Option<RootBatchLease> {
        self.owners.insert(key, roots)
    }
    pub(super) fn remove(&mut self, key: &ThreadKey) -> Option<RootBatchLease> {
        self.owners.remove(key)
    }

    /// Issue Ready only after the ordinary owner census has committed.
    pub(super) fn publish_host(
        &mut self,
        prepared: PreparedHostCall,
    ) -> Result<HostRequestKey, HostRootError> {
        if !self.owners.contains_key(&prepared.owner) {
            return Err(HostRootError::OwnerNotParked);
        }
        let record = self.hosts.record_mut(prepared.key)?;
        if record.owner != prepared.owner {
            return Err(HostRootError::ForeignOwner);
        }
        match record.state {
            RecordState::Prepared => {}
            RecordState::Published(_)
            | RecordState::TerminalAwaitingOwner
            | RecordState::OwnerAcknowledged => return Err(HostRootError::WrongStage),
        }
        let key = prepared.key;
        record.state = RecordState::Published(
            PublishedHostCall {
                key,
                owner: prepared.owner,
            }
            .into_registration(),
        );
        Ok(key)
    }

    /// Native settlement precedes collector-authorized dead-owner root drain.
    /// Refusal preserves the entire registry and its existing World retention.
    pub(super) fn close_and_drain(
        &mut self,
        collector: &QuiescentCollector<'_>,
    ) -> Result<DrainReceipt, HostRootError> {
        let receipt = self.hosts.close_and_drain(collector)?;
        Ok(receipt)
    }

    pub(super) fn hosts(&mut self) -> &mut HostRegistry {
        &mut self.hosts
    }
}

#[derive(Debug)]
pub(super) struct HostRegistry {
    heap: HeapIdentity,
    session: SessionState,
    records: BTreeMap<HostRequestKey, Record>,
}

impl HostRegistry {
    fn new(heap: HeapIdentity) -> Self {
        Self {
            heap,
            session: SessionState::Dormant,
            records: BTreeMap::new(),
        }
    }
    fn next_key(&mut self) -> Result<HostRequestKey, HostRootError> {
        if matches!(self.session, SessionState::Dormant) {
            self.session = SessionState::Open(OpenSession {
                id: SESSIONS.issue()?,
                requests: RequestSequence(NonZeroU64::MIN),
            });
        }
        match &mut self.session {
            SessionState::Open(open) => Ok(HostRequestKey::from_owner_identity(
                open.id,
                open.requests.issue()?,
                HostRequestGeneration::from_owner_id(NonZeroU64::MIN),
            )),
            SessionState::Dormant | SessionState::Closing | SessionState::Closed => {
                Err(HostRootError::Closed)
            }
        }
    }

    fn record_mut(&mut self, key: HostRequestKey) -> Result<&mut Record, HostRootError> {
        self.records
            .get_mut(&key)
            .ok_or(HostRootError::UnknownRequest)
    }

    /// Register live original-owner roots; native endpoints receive only key.
    /// The caller's root slice stays live; reserve/fill/publish executes no
    /// Lisp, safepoint, collection or ownership handoff.
    ///
    /// # Safety
    /// Every input is a live root of this original same-heap owner. Keep the
    /// slice live until this publication returns; the metadata heap check
    /// cannot prove provenance of arbitrary raw Lisp words.
    pub(super) unsafe fn prepare(
        &mut self,
        baton: &Baton,
        heap: &TaggedHeap,
        owner: ThreadKey,
        roots: &[TaggedValue],
    ) -> Result<PreparedHostCall, HostRootError> {
        if !baton.held_here() {
            return Err(HostRootError::BatonRequired);
        }
        for actual in [heap.heap_identity(), baton.parallel_heap_identity()] {
            if actual != self.heap {
                return Err(HostRootError::ForeignHeap {
                    expected: self.heap,
                    actual,
                });
            }
        }
        let key = self.next_key()?;
        let prepared = baton.prepare_parked_batch(heap, roots.len())?;
        // SAFETY: this private original-owner boundary holds the unique Baton;
        // inputs are live same-heap owner roots and stay live through publish.
        // No callback, GC or handoff occurs during this fill/publish extent.
        let finalized = unsafe { prepared.finalize(roots) }.map_err(|failure| {
            let (_prepared, error) = failure.into_parts();
            HostRootError::Root(error)
        })?;
        let lease = finalized.publish().map_err(|failure| {
            let (_finalized, error) = failure.into_parts();
            HostRootError::Root(error)
        })?;
        self.records.insert(
            key,
            Record {
                owner,
                state: RecordState::Prepared,
                roots: vec![lease],
                registered: BTreeMap::new(),
                next_record: NonZeroU64::MIN,
            },
        );
        Ok(PreparedHostCall {
            key,
            owner,
            _confined: PhantomData,
        })
    }

    /// Publish an additional typed argument/result/error/signal record.
    /// The registry retains every lease independently of terminal settlement.
    ///
    /// # Safety
    /// These are the held Baton's original-owner live same-heap Lisp roots.
    /// Keep them live until publication completes, with no collection/handoff.
    pub(super) unsafe fn register<K: RegisteredRecord>(
        &mut self,
        baton: &Baton,
        heap: &TaggedHeap,
        owner: ThreadKey,
        key: HostRequestKey,
        roots: &[TaggedValue],
    ) -> Result<RegisteredRootRef<'static, K>, HostRootError> {
        self.validate_access(baton, heap)?;
        if matches!(self.session, SessionState::Closing | SessionState::Closed) {
            return Err(HostRootError::Closed);
        }
        let record = self.record_mut(key)?;
        if record.owner != owner {
            return Err(HostRootError::ForeignOwner);
        }
        if matches!(record.state, RecordState::OwnerAcknowledged) {
            return Err(HostRootError::WrongStage);
        }
        let id = RootRecordId::from_owner_id(record.next_record);
        let next = record
            .next_record
            .checked_add(1)
            .ok_or(HostRootError::IdentityExhausted)?;
        let prepared = baton.prepare_parked_batch(heap, roots.len())?;
        // SAFETY: original-owner input roots remain live under the held Baton;
        // the private caller promises no GC, callback or handoff in this extent.
        let finalized = unsafe { prepared.finalize(roots) }.map_err(|failure| {
            let (_prepared, error) = failure.into_parts();
            HostRootError::Root(error)
        })?;
        let lease = finalized.publish().map_err(|failure| {
            let (_finalized, error) = failure.into_parts();
            HostRootError::Root(error)
        })?;
        let slot = RootSlot(record.roots.len());
        record.roots.push(lease);
        // SAFETY: the just-published lease now belongs to this World registry,
        // and it remains retained through original-owner acknowledgement.
        let reference = unsafe { RegisteredRootRef::from_registered(key, id) };
        record.registered.insert(
            id,
            RegisteredSlot {
                kind: reference.kind(),
                slot,
            },
        );
        record.next_record = next;
        Ok(reference)
    }

    fn validate_access(&self, baton: &Baton, heap: &TaggedHeap) -> Result<(), HostRootError> {
        if !baton.held_here() {
            return Err(HostRootError::BatonRequired);
        }
        for actual in [heap.heap_identity(), baton.parallel_heap_identity()] {
            if actual != self.heap {
                return Err(HostRootError::ForeignHeap {
                    expected: self.heap,
                    actual,
                });
            }
        }
        Ok(())
    }

    /// Read only on the held same-World Baton. A native endpoint never receives
    /// the reader or the heap, only its registered reference metadata.
    pub(super) fn with_registered_roots<K: RegisteredRecord, R>(
        &self,
        restored: &RestoredHostOwner<'_>,
        key: HostRequestKey,
        reference: &RegisteredRootRef<'_, K>,
        read: impl FnOnce(&[TaggedValue]) -> R,
    ) -> Result<R, HostRootError> {
        let heap = &restored.context.tagged_heap;
        self.validate_access(restored.baton, heap)?;
        if matches!(self.session, SessionState::Closing | SessionState::Closed) {
            return Err(HostRootError::Closed);
        }
        if !reference.matches(key) {
            return Err(HostRootError::ForeignRecord);
        }
        let record = self
            .records
            .get(&key)
            .ok_or(HostRootError::UnknownRequest)?;
        if record.owner != restored.owner {
            return Err(HostRootError::ForeignOwner);
        }
        if matches!(record.state, RecordState::OwnerAcknowledged) {
            return Err(HostRootError::WrongStage);
        }
        let registration = record
            .registered
            .get(&reference.record())
            .ok_or(HostRootError::ForeignRecord)?;
        if registration.kind != reference.kind() {
            return Err(HostRootError::ForeignRecord);
        }
        let lease = record
            .roots
            .get(registration.slot.0)
            .ok_or(HostRootError::ForeignRecord)?;
        let reader = lease.reader(heap)?;
        let mut roots = Vec::with_capacity(lease.root_count());
        reader.append_roots(&mut roots);
        let result = read(&roots);
        reader.finish()?;
        Ok(result)
    }

    pub(super) fn registration(
        &self,
        key: HostRequestKey,
    ) -> Result<&HostParkRegistration, HostRootError> {
        match &self
            .records
            .get(&key)
            .ok_or(HostRootError::UnknownRequest)?
            .state
        {
            RecordState::Published(registration) => Ok(registration),
            RecordState::Prepared
            | RecordState::TerminalAwaitingOwner
            | RecordState::OwnerAcknowledged => Err(HostRootError::WrongStage),
        }
    }

    pub(super) fn settle(&mut self, key: HostRequestKey) -> Result<SettledHostCall, HostRootError> {
        let record = self.record_mut(key)?;
        match record.state {
            RecordState::Published(_) => {
                record.state = RecordState::TerminalAwaitingOwner;
                Ok(SettledHostCall {
                    key,
                    owner: record.owner,
                    _confined: PhantomData,
                })
            }
            RecordState::Prepared
            | RecordState::TerminalAwaitingOwner
            | RecordState::OwnerAcknowledged => Err(HostRootError::WrongStage),
        }
    }

    /// Called by the original owner's restore path, never by native service.
    pub(super) fn acknowledge<'owner>(
        &mut self,
        restored: &'owner RestoredHostOwner<'owner>,
        key: HostRequestKey,
    ) -> Result<OwnerAcknowledged<'owner>, HostRootError> {
        self.validate_access(restored.baton, &restored.context.tagged_heap)?;
        let record = self.record_mut(key)?;
        if record.owner != restored.owner {
            return Err(HostRootError::ForeignOwner);
        }
        match record.state {
            RecordState::TerminalAwaitingOwner => {
                record.state = RecordState::OwnerAcknowledged;
                Ok(OwnerAcknowledged { key, restored })
            }
            RecordState::Prepared | RecordState::Published(_) | RecordState::OwnerAcknowledged => {
                Err(HostRootError::WrongStage)
            }
        }
    }

    pub(super) fn retire<'owner>(
        &mut self,
        ack: OwnerAcknowledged<'owner>,
    ) -> Result<RootsRetired, RetirementBlocked<'owner>> {
        let retired = (|| {
            self.validate_access(ack.restored.baton, &ack.restored.context.tagged_heap)?;
            let record = self.record_mut(ack.key)?;
            if !matches!(record.state, RecordState::OwnerAcknowledged) {
                return Err(HostRootError::WrongStage);
            }
            Self::finish_roots(record)?;
            self.records.remove(&ack.key);
            Ok(RootsRetired { key: ack.key })
        })();
        retired.map_err(|error| RetirementBlocked { ack, error })
    }

    fn finish_roots(record: &mut Record) -> Result<(), HostRootError> {
        while let Some(lease) = record.roots.pop() {
            match super::root_registry::finish(lease) {
                Ok(()) => {}
                Err(failure) => {
                    let (lease, error) = failure.into_parts();
                    record.roots.push(lease);
                    return Err(HostRootError::Root(error));
                }
            }
        }
        Ok(())
    }

    /// Every continuation has ended; quiescence authorizes dead-owner retirement.
    pub(super) fn close_and_drain(
        &mut self,
        collector: &QuiescentCollector<'_>,
    ) -> Result<DrainReceipt, HostRootError> {
        self.session = SessionState::Closing;
        for record in self.records.values_mut() {
            for lease in &record.roots {
                if lease.heap_identity() != collector.heap_identity() {
                    return Err(HostRootError::ForeignHeap {
                        expected: lease.heap_identity(),
                        actual: collector.heap_identity(),
                    });
                }
            }
            Self::finish_roots(record)?;
        }
        self.records.clear();
        self.session = SessionState::Closed;
        Ok(DrainReceipt)
    }
}

static_assertions::assert_not_impl_any!(SettledHostCall: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(RestoredHostOwner<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(OwnerAcknowledged<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(RetirementBlocked<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(PreparedHostCall: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(HostParkRegistration: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(DrainBlocked: Send, Sync, Clone, Copy);

#[cfg(test)]
mod tests;
