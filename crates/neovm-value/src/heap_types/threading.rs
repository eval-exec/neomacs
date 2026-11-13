//! Owned threading payloads shared by the runtime and the tagged collector.
//!
//! Native identities do not root an object. All Lisp edges belong to the
//! payload's child schema; transport leases and scheduler state stay outside
//! these collectible owners.

use super::LispString;
use crate::tagged::gc::{HeapIdentity, TaggedHeap};
use crate::tagged::header::MutexObj;
use crate::tagged::transport::{SharedRoot, SharedRootError};
use crate::tagged::value::TaggedValue;
use crate::value::buffer_vocab::BufferId;
use std::num::{NonZeroU32, NonZeroU64};
use std::time::Instant;

macro_rules! native_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[doc(hidden)]
        pub struct $name {
            heap: HeapIdentity,
            ordinal: u64,
        }

        impl $name {
            #[doc(hidden)]
            pub const fn heap_identity(self) -> HeapIdentity {
                self.heap
            }

            /// Conversion only for the existing scheduler/native-key boundary.
            #[doc(hidden)]
            pub const fn native_key(self) -> u64 {
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
#[doc(hidden)]
pub enum NativeIdExhausted {
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
#[doc(hidden)]
pub struct ThreadIds {
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
    #[doc(hidden)]
    pub const NEXT_THREAD_OFFSET: usize = std::mem::offset_of!(Self, next_thread);

    #[doc(hidden)]
    pub fn new(heap: HeapIdentity) -> Self {
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
    #[doc(hidden)]
    pub fn any_thread_created(&self) -> bool {
        self.next_thread.get() > 1
    }

    #[doc(hidden)]
    pub const fn main_thread(&self) -> ThreadId {
        ThreadId {
            heap: self.heap,
            ordinal: 0,
        }
    }

    #[doc(hidden)]
    pub fn issue_thread(&mut self) -> Result<ThreadId, NativeIdExhausted> {
        let ordinal = self.next_thread.take().ok_or(NativeIdExhausted::Thread)?;
        Ok(ThreadId {
            heap: self.heap,
            ordinal,
        })
    }

    #[doc(hidden)]
    pub fn issue_mutex(&mut self) -> Result<MutexId, NativeIdExhausted> {
        let ordinal = Self::take(&mut self.next_mutex).ok_or(NativeIdExhausted::Mutex)?;
        Ok(MutexId {
            heap: self.heap,
            ordinal,
        })
    }

    #[doc(hidden)]
    pub fn issue_condvar(&mut self) -> Result<CondVarId, NativeIdExhausted> {
        let ordinal = Self::take(&mut self.next_condvar).ok_or(NativeIdExhausted::CondVar)?;
        Ok(CondVarId {
            heap: self.heap,
            ordinal,
        })
    }

    /// Re-type only an identity issued by this World. This validates native
    /// numbering, not object liveness or permission to access a payload.
    #[doc(hidden)]
    pub fn thread_from_native_key(&self, key: u64) -> Option<ThreadId> {
        (key < self.next_thread.get()).then_some(ThreadId {
            heap: self.heap,
            ordinal: key,
        })
    }

    #[doc(hidden)]
    pub fn mutex_from_native_key(&self, key: u64) -> Option<MutexId> {
        Self::issued_nonzero(self.next_mutex, key).then_some(MutexId {
            heap: self.heap,
            ordinal: key,
        })
    }

    #[doc(hidden)]
    pub fn condvar_from_native_key(&self, key: u64) -> Option<CondVarId> {
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
#[doc(hidden)]
pub struct ThreadName(TaggedValue);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct InvalidThreadName;

impl ThreadName {
    #[doc(hidden)]
    pub const UNNAMED: Self = Self(TaggedValue::NIL);

    #[doc(hidden)]
    pub fn from_value(value: TaggedValue) -> Result<Self, InvalidThreadName> {
        if value.is_nil() || value.is_string() {
            Ok(Self(value))
        } else {
            Err(InvalidThreadName)
        }
    }

    #[doc(hidden)]
    pub fn is_unnamed(self) -> bool {
        self.0.is_nil()
    }

    #[doc(hidden)]
    pub const fn stored(self) -> TaggedValue {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum RunState {
    Running,
    Blocked,
}

/// The terminal category; values remain in the payload's canonical result,
/// last-error and signal slots, never in a second untraced copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum ThreadOutcome {
    Returned,
    InternalError,
    Signaled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum ThreadPhase {
    New,
    Active(RunState),
    Completing(ThreadOutcome),
    Terminal(ThreadOutcome),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum JoinObservation {
    Unobserved,
    Observed,
}

/// An event after its same-heap transport handoff. These are ordinary heap
/// edges, not independently registered roots, so mailbox cycles can die.
#[derive(Clone, Copy, Debug)]
#[doc(hidden)]
pub struct StoredThreadEvent {
    sender: TaggedValue,
    error_symbol: TaggedValue,
    data: TaggedValue,
}

impl StoredThreadEvent {
    /// Admit all three transport roots to the target's heap before producing
    /// ordinary child words. A foreign sender, symbol or data leaves the
    /// target untouched; the caller still owns every transport lease.
    #[doc(hidden)]
    pub fn try_from_roots(
        heap: &TaggedHeap,
        roots: &[SharedRoot; 3],
    ) -> Result<Self, SharedRootError> {
        let sender = roots[0].materialize(heap)?;
        let error_symbol = roots[1].materialize(heap)?;
        let data = roots[2].materialize(heap)?;
        Ok(Self {
            sender: sender.value(),
            error_symbol: error_symbol.value(),
            data: data.value(),
        })
    }

    #[doc(hidden)]
    pub fn values(self) -> [TaggedValue; 3] {
        let Self {
            sender,
            error_symbol,
            data,
        } = self;
        [sender, error_symbol, data]
    }

    fn visit_children(&self, visit: &mut impl FnMut(TaggedValue)) {
        for value in self.values() {
            visit(value);
        }
    }
}

/// What a suspended simulated thread must re-run before its saved
/// continuation.
///
/// GNU keeps this information on the waiting thread's C stack: a contended
/// `lisp_mutex_lock_for_thread` re-checks the mutex when it is woken
/// (`src/thread.c:212-246`), and `condition_wait_callback` reacquires the
/// condition's mutex with the lock count saved before the wait
/// (`src/thread.c:446-476`). The simulated backend suspends the Lisp stack
/// instead, so the same state is carried in the thread's native slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum BlockedWait {
    /// Retry `mutex-lock` on the object recorded in `event_object`.
    MutexLock,
    /// Reacquire the mutex associated with the object in `event_object` at
    /// this recursion count, then continue.
    ConditionWait(RecursionDepth),
    /// A `thread_select` wait (`sleep-for`, `accept-process-output`,
    /// `sit-for`). GNU does not broadcast this wait from `thread-signal`
    /// (`src/thread.c:974-982`); it ends at `deadline`.
    Select { deadline: Instant },
}

/// Whether `run_thread` has installed its top-level handler.
///
/// GNU `run_thread` calls `acquire_global_lock` (`src/thread.c:805`) before
/// it installs `handlerlist_sentinel` (`:810-815`). `post_acquire_global_lock`
/// delivers a pending signal only when `handlerlist` is non-NULL (`:141-154`),
/// and that sentinel — not the user's `condition-case` — is what makes the
/// guard true. `Absent` is the first acquisition. `Installed` is every
/// reacquire after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum HandlerList {
    Absent,
    Installed,
}

/// Scalar runtime state that carries no Lisp children or owned storage. Its
/// mutable view cannot change heap identity or introduce an unbarriered edge.
#[derive(Clone, Copy, Debug)]
#[doc(hidden)]
pub struct ThreadNativeState {
    pub(crate) phase: ThreadPhase,
    pub(crate) join_observation: JoinObservation,
    pub(crate) current_buffer: Option<BufferId>,
    /// The typed operation to re-run when a suspended wait is resumed.
    pub(crate) blocked_wait: Option<BlockedWait>,
    /// The wait condition fired (mutex released, condition notified, or an
    /// injected signal). GNU's equivalent is the waiter waking from
    /// `sys_cond_wait`; `event_object` stays set until the waiter itself
    /// clears it, so this cannot be derived from that field.
    pub(crate) woken: bool,
    /// This carrier is currently executing on the simulation's OS stack.
    ///
    /// `ThreadPhase::Active(RunState::Running)` alone cannot distinguish a
    /// worker that has been started but has not run from one whose body is
    /// on the stack right now; a release point reached from a running
    /// worker must not re-enter another running worker as "created".
    pub(crate) executing: bool,
    /// `run_thread`'s top-level handler (`src/thread.c:810-815`).
    pub(crate) handlerlist: HandlerList,
}

impl ThreadNativeState {
    #[doc(hidden)]
    pub const fn new(phase: ThreadPhase, current_buffer: Option<BufferId>) -> Self {
        Self {
            phase,
            join_observation: JoinObservation::Unobserved,
            current_buffer,
            blocked_wait: None,
            woken: false,
            executing: false,
            handlerlist: HandlerList::Absent,
        }
    }
}

/// All Lisp child words and their directly owned storage. Only this view is
/// exposed by the pre-store SATB/remembered-owner mutation wrapper.
#[derive(Debug)]
#[doc(hidden)]
pub struct ThreadChildren {
    pub(crate) name: ThreadName,
    pub(crate) function: TaggedValue,
    pub(crate) result: TaggedValue,
    pub(crate) last_error: Option<TaggedValue>,
    pub(crate) buffer_disposition: TaggedValue,
    pub(crate) event_object: TaggedValue,
    pub(crate) error_symbol: TaggedValue,
    pub(crate) error_data: TaggedValue,
    pub(crate) blocked_remaining_forms: TaggedValue,
    pub(crate) stored_events: Vec<StoredThreadEvent>,
}

/// Thread-owned state. Access and mutation require the runtime's rooted,
/// same-heap authority; no reference to either view may cross GC or yield.
#[derive(Debug)]
#[doc(hidden)]
pub struct ThreadPayload {
    id: ThreadId,
    pub(crate) native: ThreadNativeState,
    pub(crate) children: ThreadChildren,
}

impl ThreadPayload {
    #[doc(hidden)]
    pub const fn new(id: ThreadId, native: ThreadNativeState, children: ThreadChildren) -> Self {
        Self {
            id,
            native,
            children,
        }
    }

    #[doc(hidden)]
    pub const fn id(&self) -> ThreadId {
        self.id
    }

    /// One exhaustive child schema for mark, preimage, minors and verification.
    pub(crate) fn visit_children(&self, visit: &mut impl FnMut(TaggedValue)) {
        let Self {
            id: _,
            native,
            children,
        } = self;
        // Every native field is deliberately non-traced. Adding a new field
        // requires classifying it here as well as in the native-view type.
        let ThreadNativeState {
            phase: _,
            join_observation: _,
            current_buffer: _,
            blocked_wait: _,
            woken: _,
            executing: _,
            handlerlist: _,
        } = native;
        children.visit_children(visit);
    }

    /// Directly owned backing capacity, excluding this inline payload and the
    /// separately allocated Lisp objects reached by its child words.
    pub(crate) fn backing_bytes(&self) -> usize {
        self.children.backing_layout().1
    }

    pub(crate) fn logical_backing_bytes(&self) -> usize {
        self.children.backing_layout().0
    }
}

impl ThreadChildren {
    fn visit_children(&self, visit: &mut impl FnMut(TaggedValue)) {
        let Self {
            name,
            function,
            result,
            last_error,
            buffer_disposition,
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

    fn backing_layout(&self) -> (usize, usize) {
        let Self {
            name: _,
            function: _,
            result: _,
            last_error: _,
            buffer_disposition: _,
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
#[doc(hidden)]
pub struct RecursionDepth(NonZeroU32);

impl RecursionDepth {
    #[doc(hidden)]
    pub const ONE: Self = Self(NonZeroU32::MIN);

    #[doc(hidden)]
    pub const fn new(count: u32) -> Option<Self> {
        match NonZeroU32::new(count) {
            Some(depth) => Some(Self(depth)),
            None => None,
        }
    }

    #[doc(hidden)]
    pub const fn get(self) -> u32 {
        self.0.get()
    }

    #[doc(hidden)]
    pub fn checked_increment(self) -> Option<Self> {
        self.get().checked_add(1).and_then(Self::new)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum MutexOwnership {
    Unlocked,
    Locked {
        owner: ThreadId,
        depth: RecursionDepth,
    },
}

#[derive(Debug)]
#[doc(hidden)]
pub struct MutexPayload {
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
#[doc(hidden)]
pub struct MutexReference {
    id: MutexId,
    value: TaggedValue,
}

impl MutexReference {
    /// The caller keeps `value` rooted while borrowing its actual object and
    /// while allocating the condition that will own the returned heap edge.
    #[doc(hidden)]
    pub fn from_mutex(value: TaggedValue, object: &MutexObj) -> Option<Self> {
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

    #[doc(hidden)]
    pub const fn id(self) -> MutexId {
        self.id
    }

    #[doc(hidden)]
    pub const fn value(self) -> TaggedValue {
        self.value
    }
}

#[derive(Debug)]
#[doc(hidden)]
pub struct CondVarPayload {
    pub(crate) id: CondVarId,
    pub(crate) name: Option<LispString>,
    association: MutexReference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("condition variable and mutex belong to different heaps")]
#[doc(hidden)]
pub struct ForeignMutexAssociation;

impl CondVarPayload {
    #[doc(hidden)]
    pub fn new(
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

    #[doc(hidden)]
    pub const fn association(&self) -> MutexReference {
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
// Copy excludes owned backing; Send + Sync excludes TaggedValue child words.
static_assertions::assert_impl_all!(ThreadNativeState: Send, Sync, Copy);
static_assertions::assert_not_impl_any!(ThreadChildren: Send, Sync);
static_assertions::assert_not_impl_any!(ThreadPayload: Send, Sync);
static_assertions::assert_not_impl_any!(MutexPayload: Send, Sync);
static_assertions::assert_not_impl_any!(CondVarPayload: Send, Sync);

impl CondVarPayload {
    #[inline]
    #[doc(hidden)]
    pub fn name(&self) -> &Option<LispString> {
        &self.name
    }
}

impl MutexPayload {
    #[inline]
    #[doc(hidden)]
    pub fn name(&self) -> &Option<LispString> {
        &self.name
    }
    #[inline]
    #[doc(hidden)]
    pub fn ownership(&self) -> MutexOwnership {
        self.ownership
    }
    #[inline]
    #[doc(hidden)]
    pub fn ownership_mut(&mut self) -> &mut MutexOwnership {
        &mut self.ownership
    }
}

impl ThreadChildren {
    /// The function object `make-thread` stored for this thread.
    ///
    /// GNU `Fmake_thread` keeps the function in the thread state
    /// (`new_thread->function`, src/thread.c:897) and the worker runs it only
    /// after it acquires the global lock, so the creating evaluator must be
    /// able to hand it back at a release point.
    #[inline]
    #[doc(hidden)]
    pub fn function(&self) -> TaggedValue {
        self.function
    }
    #[inline]
    #[doc(hidden)]
    pub fn blocked_remaining_forms(&self) -> TaggedValue {
        self.blocked_remaining_forms
    }
    #[inline]
    #[doc(hidden)]
    pub fn blocked_remaining_forms_mut(&mut self) -> &mut TaggedValue {
        &mut self.blocked_remaining_forms
    }
    #[inline]
    #[doc(hidden)]
    pub fn buffer_disposition(&self) -> TaggedValue {
        self.buffer_disposition
    }
    #[inline]
    #[doc(hidden)]
    pub fn buffer_disposition_mut(&mut self) -> &mut TaggedValue {
        &mut self.buffer_disposition
    }
    #[inline]
    #[doc(hidden)]
    pub fn error_data(&self) -> TaggedValue {
        self.error_data
    }
    #[inline]
    #[doc(hidden)]
    pub fn error_data_mut(&mut self) -> &mut TaggedValue {
        &mut self.error_data
    }
    #[inline]
    #[doc(hidden)]
    pub fn error_symbol(&self) -> TaggedValue {
        self.error_symbol
    }
    #[inline]
    #[doc(hidden)]
    pub fn error_symbol_mut(&mut self) -> &mut TaggedValue {
        &mut self.error_symbol
    }
    #[inline]
    #[doc(hidden)]
    pub fn event_object(&self) -> TaggedValue {
        self.event_object
    }
    #[inline]
    #[doc(hidden)]
    pub fn event_object_mut(&mut self) -> &mut TaggedValue {
        &mut self.event_object
    }
    #[inline]
    #[doc(hidden)]
    pub fn last_error(&self) -> Option<TaggedValue> {
        self.last_error
    }
    #[inline]
    #[doc(hidden)]
    pub fn last_error_mut(&mut self) -> &mut Option<TaggedValue> {
        &mut self.last_error
    }
    #[inline]
    #[doc(hidden)]
    pub fn result(&self) -> TaggedValue {
        self.result
    }
    #[inline]
    #[doc(hidden)]
    pub fn result_mut(&mut self) -> &mut TaggedValue {
        &mut self.result
    }
    #[inline]
    #[doc(hidden)]
    pub fn stored_events(&self) -> &Vec<StoredThreadEvent> {
        &self.stored_events
    }
    #[inline]
    #[doc(hidden)]
    pub fn stored_events_mut(&mut self) -> &mut Vec<StoredThreadEvent> {
        &mut self.stored_events
    }
}

impl ThreadNativeState {
    #[inline]
    #[doc(hidden)]
    pub fn current_buffer(&self) -> Option<BufferId> {
        self.current_buffer
    }
    #[inline]
    #[doc(hidden)]
    pub fn current_buffer_mut(&mut self) -> &mut Option<BufferId> {
        &mut self.current_buffer
    }
    #[inline]
    #[doc(hidden)]
    pub fn join_observation(&self) -> JoinObservation {
        self.join_observation
    }
    #[inline]
    #[doc(hidden)]
    pub fn join_observation_mut(&mut self) -> &mut JoinObservation {
        &mut self.join_observation
    }
    #[inline]
    #[doc(hidden)]
    pub fn phase(&self) -> ThreadPhase {
        self.phase
    }
    #[inline]
    #[doc(hidden)]
    pub fn phase_mut(&mut self) -> &mut ThreadPhase {
        &mut self.phase
    }
    #[inline]
    #[doc(hidden)]
    pub fn blocked_wait(&self) -> Option<BlockedWait> {
        self.blocked_wait
    }
    #[inline]
    #[doc(hidden)]
    pub fn blocked_wait_mut(&mut self) -> &mut Option<BlockedWait> {
        &mut self.blocked_wait
    }
    #[inline]
    #[doc(hidden)]
    pub fn woken(&self) -> bool {
        self.woken
    }
    #[inline]
    #[doc(hidden)]
    pub fn woken_mut(&mut self) -> &mut bool {
        &mut self.woken
    }
    #[inline]
    #[doc(hidden)]
    pub fn executing(&self) -> bool {
        self.executing
    }
    #[inline]
    #[doc(hidden)]
    pub fn executing_mut(&mut self) -> &mut bool {
        &mut self.executing
    }
    #[inline]
    #[doc(hidden)]
    pub fn handlerlist(&self) -> HandlerList {
        self.handlerlist
    }
    #[inline]
    #[doc(hidden)]
    pub fn handlerlist_mut(&mut self) -> &mut HandlerList {
        &mut self.handlerlist
    }
}

impl ThreadPayload {
    #[inline]
    #[doc(hidden)]
    pub fn children(&self) -> &ThreadChildren {
        &self.children
    }
    #[inline]
    #[doc(hidden)]
    pub fn native(&self) -> ThreadNativeState {
        self.native
    }
}

impl ThreadChildren {
    #[inline]
    #[doc(hidden)]
    pub fn name(&self) -> ThreadName {
        self.name
    }
}

impl CondVarPayload {
    #[inline]
    #[doc(hidden)]
    pub fn id(&self) -> CondVarId {
        self.id
    }
}

impl MutexPayload {
    #[inline]
    #[doc(hidden)]
    pub fn id(&self) -> MutexId {
        self.id
    }
}

impl ThreadChildren {
    #[doc(hidden)]
    pub fn new(
        name: ThreadName,
        function: TaggedValue,
        result: TaggedValue,
        last_error: Option<TaggedValue>,
        buffer_disposition: TaggedValue,
        event_object: TaggedValue,
        error_symbol: TaggedValue,
        error_data: TaggedValue,
        blocked_remaining_forms: TaggedValue,
        stored_events: Vec<StoredThreadEvent>,
    ) -> Self {
        Self {
            name,
            function,
            result,
            last_error,
            buffer_disposition,
            event_object,
            error_symbol,
            error_data,
            blocked_remaining_forms,
            stored_events,
        }
    }
}

impl MutexPayload {
    /// A new mutex is always unlocked: the payload owns no lock state until
    /// the mutator that acquires it records ownership.
    #[doc(hidden)]
    pub fn new(id: MutexId, name: Option<LispString>) -> Self {
        Self {
            id,
            name,
            ownership: MutexOwnership::Unlocked,
        }
    }
}
