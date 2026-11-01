//! Owned threading payloads shared by the runtime and the tagged collector.
//!
//! Native identities do not root an object. All Lisp edges belong to the
//! payload's child schema; transport leases and scheduler state stay outside
//! these collectible owners.

use super::LispString;
use crate::buffer::BufferId;
use crate::tagged::gc::HeapIdentity;
use crate::tagged::header::MutexObj;
use crate::tagged::value::TaggedValue;
use std::num::{NonZeroU32, NonZeroU64};

macro_rules! native_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub(crate) struct $name {
            heap: HeapIdentity,
            ordinal: u64,
        }

        impl $name {
            pub(crate) const fn heap_identity(self) -> HeapIdentity {
                self.heap
            }

            /// Conversion only for the existing scheduler/native-key boundary.
            pub(crate) const fn native_key(self) -> u64 {
                self.ordinal
            }
        }
    };
}

native_id!(ThreadId);
native_id!(MutexId);
native_id!(CondVarId);

/// Exhaustion never wraps a native identity onto an earlier object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum NativeIdExhausted {
    #[error("thread identity space exhausted")]
    Thread,
    #[error("mutex identity space exhausted")]
    Mutex,
    #[error("condition-variable identity space exhausted")]
    CondVar,
}

/// The single identity issuer owned by one World's thread registry.
///
/// This is intentionally not Clone: copying an issuer would reuse ordinals.
/// The runtime creates it once for the existing World heap lifetime.
/// The last word retains the legacy thread-creation counter's scalar layout.
#[repr(C)]
#[derive(Debug)]
pub(crate) struct ThreadIds {
    heap: HeapIdentity,
    next_mutex: Option<NonZeroU64>,
    next_condvar: Option<NonZeroU64>,
    next_thread: ThreadOrdinalCounter,
}

/// MAX is an exhausted sentinel, never an issued thread identity. Keeping a
/// scalar next ordinal preserves the existing hot `next_id > 1` predicate.
/// Private construction starts at one and checked advancement never wraps;
/// the backing word carries no NonZero range metadata into that predicate.
#[repr(transparent)]
#[derive(Debug)]
struct ThreadOrdinalCounter(u64);

impl ThreadOrdinalCounter {
    #[inline(always)]
    const fn get(&self) -> u64 {
        self.0
    }

    fn take(&mut self) -> Option<u64> {
        let ordinal = self.get();
        let next = ordinal.checked_add(1)?;
        self.0 = next;
        Some(ordinal)
    }
}

const _: () = assert!(size_of::<ThreadIds>() == 32);
const _: () = assert!(std::mem::offset_of!(ThreadIds, next_thread) == 24);

impl ThreadIds {
    pub(crate) const NEXT_THREAD_OFFSET: usize = std::mem::offset_of!(Self, next_thread);

    pub(crate) fn new(heap: HeapIdentity) -> Self {
        Self {
            heap,
            next_mutex: Some(NonZeroU64::MIN),
            next_condvar: Some(NonZeroU64::MIN),
            next_thread: ThreadOrdinalCounter(1),
        }
    }

    pub(crate) const fn heap_identity(&self) -> HeapIdentity {
        self.heap
    }

    #[inline(always)]
    pub(crate) fn any_thread_created(&self) -> bool {
        self.next_thread.get() > 1
    }

    pub(crate) const fn main_thread(&self) -> ThreadId {
        ThreadId {
            heap: self.heap,
            ordinal: 0,
        }
    }

    pub(crate) fn issue_thread(&mut self) -> Result<ThreadId, NativeIdExhausted> {
        let ordinal = self.next_thread.take().ok_or(NativeIdExhausted::Thread)?;
        Ok(ThreadId {
            heap: self.heap,
            ordinal,
        })
    }

    pub(crate) fn issue_mutex(&mut self) -> Result<MutexId, NativeIdExhausted> {
        let ordinal = Self::take(&mut self.next_mutex).ok_or(NativeIdExhausted::Mutex)?;
        Ok(MutexId {
            heap: self.heap,
            ordinal,
        })
    }

    pub(crate) fn issue_condvar(&mut self) -> Result<CondVarId, NativeIdExhausted> {
        let ordinal = Self::take(&mut self.next_condvar).ok_or(NativeIdExhausted::CondVar)?;
        Ok(CondVarId {
            heap: self.heap,
            ordinal,
        })
    }

    /// Re-type only an identity issued by this World. This validates native
    /// numbering, not object liveness or permission to access a payload.
    pub(crate) fn thread_from_native_key(&self, key: u64) -> Option<ThreadId> {
        (key < self.next_thread.get()).then_some(ThreadId {
            heap: self.heap,
            ordinal: key,
        })
    }

    pub(crate) fn mutex_from_native_key(&self, key: u64) -> Option<MutexId> {
        Self::issued_nonzero(self.next_mutex, key).then_some(MutexId {
            heap: self.heap,
            ordinal: key,
        })
    }

    pub(crate) fn condvar_from_native_key(&self, key: u64) -> Option<CondVarId> {
        Self::issued_nonzero(self.next_condvar, key).then_some(CondVarId {
            heap: self.heap,
            ordinal: key,
        })
    }

    fn issued_nonzero(next: Option<NonZeroU64>, key: u64) -> bool {
        key != 0 && next.is_none_or(|next| key < next.get())
    }

    fn take(next: &mut Option<NonZeroU64>) -> Option<u64> {
        let ordinal = next.as_ref()?.get();
        *next = ordinal.checked_add(1).and_then(NonZeroU64::new);
        Some(ordinal)
    }
}

/// A checked string-or-nil name retaining the original Lisp object identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ThreadName(TaggedValue);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InvalidThreadName;

impl ThreadName {
    pub(crate) const UNNAMED: Self = Self(TaggedValue::NIL);

    pub(crate) fn from_value(value: TaggedValue) -> Result<Self, InvalidThreadName> {
        if value.is_nil() || value.is_string() {
            Ok(Self(value))
        } else {
            Err(InvalidThreadName)
        }
    }

    pub(crate) fn is_unnamed(self) -> bool {
        self.0.is_nil()
    }

    pub(crate) const fn stored(self) -> TaggedValue {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunState {
    Running,
    Blocked,
}

/// The terminal category; values remain in the payload's canonical result,
/// last-error and signal slots, never in a second untraced copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThreadOutcome {
    Returned,
    InternalError,
    Signaled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThreadPhase {
    New,
    Active(RunState),
    Completing(ThreadOutcome),
    Terminal(ThreadOutcome),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JoinObservation {
    Unobserved,
    Observed,
}

/// An event after its same-heap transport handoff. These are ordinary heap
/// edges, not independently registered roots, so mailbox cycles can die.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StoredThreadEvent {
    pub(crate) sender: TaggedValue,
    pub(crate) error_symbol: TaggedValue,
    pub(crate) data: TaggedValue,
}

impl StoredThreadEvent {
    fn visit_children(&self, visit: &mut impl FnMut(TaggedValue)) {
        let Self {
            sender,
            error_symbol,
            data,
        } = self;
        visit(*sender);
        visit(*error_symbol);
        visit(*data);
    }
}

/// Thread-owned state. Access and mutation require the runtime's rooted,
/// same-heap authority; no reference to this payload may cross GC or yield.
#[derive(Debug)]
pub(crate) struct ThreadPayload {
    pub(crate) id: ThreadId,
    pub(crate) name: ThreadName,
    pub(crate) function: TaggedValue,
    pub(crate) phase: ThreadPhase,
    pub(crate) result: TaggedValue,
    pub(crate) last_error: Option<TaggedValue>,
    pub(crate) join_observation: JoinObservation,
    pub(crate) buffer_disposition: TaggedValue,
    pub(crate) current_buffer: Option<BufferId>,
    pub(crate) event_object: TaggedValue,
    pub(crate) error_symbol: TaggedValue,
    pub(crate) error_data: TaggedValue,
    pub(crate) blocked_remaining_forms: TaggedValue,
    pub(crate) stored_events: Vec<StoredThreadEvent>,
}

impl ThreadPayload {
    /// One exhaustive child schema for mark, preimage, minors and verification.
    pub(crate) fn visit_children(&self, visit: &mut impl FnMut(TaggedValue)) {
        let Self {
            id: _,
            name,
            function,
            phase: _,
            result,
            last_error,
            join_observation: _,
            buffer_disposition,
            current_buffer: _,
            event_object,
            error_symbol,
            error_data,
            blocked_remaining_forms,
            stored_events,
        } = self;
        visit(name.stored());
        visit(*function);
        visit(*result);
        if let Some(error) = last_error {
            visit(*error);
        }
        visit(*buffer_disposition);
        visit(*event_object);
        visit(*error_symbol);
        visit(*error_data);
        visit(*blocked_remaining_forms);
        for event in stored_events {
            event.visit_children(visit);
        }
    }

    /// Directly owned backing capacity, excluding this inline payload and the
    /// separately allocated Lisp objects reached by its child words.
    pub(crate) fn backing_bytes(&self) -> usize {
        self.backing_layout().1
    }

    pub(crate) fn logical_backing_bytes(&self) -> usize {
        self.backing_layout().0
    }

    fn backing_layout(&self) -> (usize, usize) {
        let Self {
            id: _,
            name: _,
            function: _,
            phase: _,
            result: _,
            last_error: _,
            join_observation: _,
            buffer_disposition: _,
            current_buffer: _,
            event_object: _,
            error_symbol: _,
            error_data: _,
            blocked_remaining_forms: _,
            stored_events,
        } = self;
        (
            stored_events
                .len()
                .saturating_mul(size_of::<StoredThreadEvent>()),
            stored_events
                .capacity()
                .saturating_mul(size_of::<StoredThreadEvent>()),
        )
    }
}

/// A locked mutex always has a positive recursive depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RecursionDepth(NonZeroU32);

impl RecursionDepth {
    pub(crate) const ONE: Self = Self(NonZeroU32::MIN);

    pub(crate) const fn new(count: u32) -> Option<Self> {
        match NonZeroU32::new(count) {
            Some(depth) => Some(Self(depth)),
            None => None,
        }
    }

    pub(crate) const fn get(self) -> u32 {
        self.0.get()
    }

    pub(crate) fn checked_increment(self) -> Option<Self> {
        self.get().checked_add(1).and_then(Self::new)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MutexOwnership {
    Unlocked,
    Locked {
        owner: ThreadId,
        depth: RecursionDepth,
    },
}

#[derive(Debug)]
pub(crate) struct MutexPayload {
    pub(crate) id: MutexId,
    pub(crate) name: Option<LispString>,
    pub(crate) ownership: MutexOwnership,
}

impl MutexPayload {
    pub(crate) fn visit_children(&self, visit: &mut impl FnMut(TaggedValue)) {
        let Self {
            id: _,
            name,
            ownership: _,
        } = self;
        visit_name_children(name, visit);
    }

    pub(crate) fn backing_bytes(&self) -> usize {
        self.backing_layout().1
    }

    pub(crate) fn logical_backing_bytes(&self) -> usize {
        self.backing_layout().0
    }

    fn backing_layout(&self) -> (usize, usize) {
        let Self {
            id: _,
            name,
            ownership: _,
        } = self;
        name_backing_layout(name)
    }
}

/// A condition's mutex identity and Lisp edge are validated together and can
/// never be mutated independently.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MutexReference {
    id: MutexId,
    value: TaggedValue,
}

impl MutexReference {
    /// The caller keeps `value` rooted while borrowing its actual object and
    /// while allocating the condition that will own the returned heap edge.
    pub(crate) fn from_mutex(value: TaggedValue, object: &MutexObj) -> Option<Self> {
        let header = value.as_veclike_ptr()?;
        if object.header.type_tag != crate::tagged::header::VecLikeType::Mutex
            || !std::ptr::eq(header, std::ptr::addr_of!(object.header))
        {
            return None;
        }
        Some(Self {
            id: object.payload.id,
            value,
        })
    }

    pub(crate) const fn id(self) -> MutexId {
        self.id
    }

    pub(crate) const fn value(self) -> TaggedValue {
        self.value
    }
}

#[derive(Debug)]
pub(crate) struct CondVarPayload {
    pub(crate) id: CondVarId,
    pub(crate) name: Option<LispString>,
    association: MutexReference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("condition variable and mutex belong to different heaps")]
pub(crate) struct ForeignMutexAssociation;

impl CondVarPayload {
    pub(crate) fn new(
        id: CondVarId,
        name: Option<LispString>,
        association: MutexReference,
    ) -> Result<Self, ForeignMutexAssociation> {
        if id.heap_identity() != association.id().heap_identity() {
            return Err(ForeignMutexAssociation);
        }
        Ok(Self {
            id,
            name,
            association,
        })
    }

    pub(crate) const fn association(&self) -> MutexReference {
        self.association
    }

    pub(crate) fn visit_children(&self, visit: &mut impl FnMut(TaggedValue)) {
        let Self {
            id: _,
            name,
            association,
        } = self;
        visit(association.value());
        visit_name_children(name, visit);
    }

    pub(crate) fn backing_bytes(&self) -> usize {
        self.backing_layout().1
    }

    pub(crate) fn logical_backing_bytes(&self) -> usize {
        self.backing_layout().0
    }

    fn backing_layout(&self) -> (usize, usize) {
        let Self {
            id: _,
            name,
            association: _,
        } = self;
        name_backing_layout(name)
    }
}

fn visit_name_children(name: &Option<LispString>, visit: &mut impl FnMut(TaggedValue)) {
    if let Some(name) = name {
        name.intervals().for_each_root(visit);
    }
}

fn name_backing_layout(name: &Option<LispString>) -> (usize, usize) {
    match name {
        Some(name) if name.has_owned_storage() => {
            (name.byte_len().saturating_add(1), name.owned_capacity())
        }
        Some(_) | None => (0, 0),
    }
}

static_assertions::assert_impl_all!(ThreadId: Send, Sync, Copy);
static_assertions::assert_impl_all!(MutexId: Send, Sync, Copy);
static_assertions::assert_impl_all!(CondVarId: Send, Sync, Copy);
static_assertions::assert_not_impl_any!(ThreadPayload: Send, Sync);
static_assertions::assert_not_impl_any!(MutexPayload: Send, Sync);
static_assertions::assert_not_impl_any!(CondVarPayload: Send, Sync);
